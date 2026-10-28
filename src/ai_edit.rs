//! Parsing and previewing file edits produced by the AI chat.
//!
//! The model is asked to emit `se-file` / `se-replace` fences. The editor
//! parses those fences, builds a diff, and applies them only after the user
//! confirms. This keeps the protocol provider-agnostic (no tool-calling
//! requirement) while still letting the assistant operate on project files.

use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};
use std::path::Path;

use similar::{ChangeTag, TextDiff};

const MAX_DIFF_LINES: usize = 800;
const MAX_PREVIEW_CHARS: usize = 60;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AiEditKind {
    Write,
    Replace,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DiffTag {
    Context,
    Add,
    Remove,
    Meta,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DiffLine {
    pub tag: DiffTag,
    pub text: String,
}

#[derive(Clone, Debug)]
pub struct AiEdit {
    pub path: String,
    pub kind: AiEditKind,
    pub content: String,
    pub pairs: Vec<(String, String)>,
    pub diff: Vec<DiffLine>,
    /// Hash of the file when the diff was prepared. Used to avoid applying an
    /// edit after the file changed underneath the AI response.
    pub base_hash: Option<u64>,
    pub resolved: Option<String>,
    pub error: Option<String>,
}

#[derive(Clone, Debug, Default)]
pub struct ParsedEdits {
    pub text: String,
    pub edits: Vec<AiEdit>,
}

impl AiEdit {
    fn write(path: String, content: String) -> Self {
        Self {
            path,
            kind: AiEditKind::Write,
            content,
            pairs: Vec::new(),
            diff: Vec::new(),
            base_hash: None,
            resolved: None,
            error: None,
        }
    }

    fn replace(path: String, pairs: Vec<(String, String)>) -> Self {
        Self {
            path,
            kind: AiEditKind::Replace,
            content: String::new(),
            pairs,
            diff: Vec::new(),
            base_hash: None,
            resolved: None,
            error: None,
        }
    }

    /// Read the current file, validate the edit and build the preview diff.
    pub fn prepare(&mut self, root: &Path) {
        self.diff.clear();
        self.resolved = None;
        self.base_hash = None;
        self.error = None;
        let path = match normalize_rel_path(&self.path) {
            Ok(path) => path,
            Err(err) => {
                self.error = Some(err);
                return;
            }
        };
        self.path = path;
        let absolute = root.join(&self.path);
        let old = if absolute.is_file() {
            match crate::util::read_text_file(&absolute) {
                Ok(loaded) => loaded.text,
                Err(err) => {
                    self.error = Some(err);
                    return;
                }
            }
        } else {
            String::new()
        };
        if absolute.is_file() {
            self.base_hash = Some(hash_text(&old));
        }

        let new_text = match self.kind {
            AiEditKind::Write => self.content.clone(),
            AiEditKind::Replace => {
                if self.pairs.is_empty() {
                    self.error = Some("se-replace 没有 SEARCH/REPLACE 片段".to_owned());
                    return;
                }
                let mut text = old.clone();
                for (search, replacement) in &self.pairs {
                    let count = text.matches(search).count();
                    if count != 1 {
                        self.error = Some(format!(
                            "SEARCH 片段匹配到 {count} 处（需要恰好 1 处）：{}",
                            preview(search)
                        ));
                        return;
                    }
                    text = text.replacen(search, replacement, 1);
                }
                text
            }
        };
        self.diff = if old.len() + new_text.len() > 2_000_000 {
            vec![DiffLine {
                tag: DiffTag::Meta,
                text: "文件较大，未生成完整差异；应用前请确认路径并保留备份".to_owned(),
            }]
        } else {
            make_diff(&old, &new_text)
        };
        self.resolved = Some(new_text);
    }
}

/// Parse `se-file` and `se-replace` fences out of an assistant reply.
/// The returned `text` has those fences removed so the chat can render the
/// explanation while the edit card shows the files and diff.
pub fn parse_edits(text: &str) -> ParsedEdits {
    let mut cleaned = String::new();
    let mut edits = Vec::new();
    let mut lines = text.split_inclusive('\n');
    while let Some(line) = lines.next() {
        let trimmed = line.trim_start();
        let Some((tag, attrs)) = fence_header(trimmed) else {
            cleaned.push_str(line);
            continue;
        };
        if !matches!(tag, "se-file" | "se-write" | "se-replace") {
            cleaned.push_str(line);
            continue;
        }
        let mut body = String::new();
        let mut closed = false;
        for body_line in lines.by_ref() {
            if body_line.trim_start().starts_with("```") {
                closed = true;
                break;
            }
            body.push_str(body_line);
        }
        if !closed {
            // Do not guess where a malformed block ends; keep it visible and
            // let the user ask the model again instead of proposing a huge
            // accidental overwrite.
            cleaned.push_str(line);
            cleaned.push_str(&body);
            continue;
        }
        let path = parse_path_attr(attrs).unwrap_or_default();
        if tag == "se-replace" {
            edits.push(AiEdit::replace(path, parse_pairs(&body)));
        } else {
            edits.push(AiEdit::write(path, body));
        }
        if !cleaned.ends_with('\n') && !cleaned.is_empty() {
            cleaned.push('\n');
        }
    }
    ParsedEdits {
        text: cleaned,
        edits,
    }
}

fn fence_header(line: &str) -> Option<(&str, &str)> {
    let rest = line.strip_prefix("```")?;
    let rest = rest.trim_start();
    let end = rest.find(char::is_whitespace).unwrap_or(rest.len());
    let tag = &rest[..end];
    if !tag.starts_with("se-") {
        return None;
    }
    Some((tag, rest[end..].trim()))
}

fn parse_path_attr(attrs: &str) -> Option<String> {
    for key in ["path", "file"] {
        let Some(start) = attrs.find(key) else {
            continue;
        };
        let after = attrs[start + key.len()..].trim_start();
        let Some(after) = after.strip_prefix('=') else {
            continue;
        };
        let mut value = after.trim_start();
        if let Some(quote) = value
            .chars()
            .next()
            .filter(|ch| matches!(ch, '"' | '\'' | '`'))
        {
            value = &value[quote.len_utf8()..];
            let end = value.find(quote)?;
            return Some(value[..end].trim().to_owned());
        }
        let end = value.find(char::is_whitespace).unwrap_or(value.len());
        return Some(value[..end].trim().to_owned());
    }
    None
}

fn parse_pairs(body: &str) -> Vec<(String, String)> {
    let mut pairs = Vec::new();
    let mut state = 0u8;
    let mut search = String::new();
    let mut replacement = String::new();
    for line in body.split_inclusive('\n') {
        let marker = line.trim();
        if marker.starts_with("<<<<<<<") {
            state = 1;
            search.clear();
            replacement.clear();
            continue;
        }
        if state == 1 && marker.starts_with("=======") {
            state = 2;
            continue;
        }
        if state == 2 && marker.starts_with(">>>>>>>") {
            pairs.push((search.clone(), replacement.clone()));
            state = 0;
            continue;
        }
        match state {
            1 => search.push_str(line),
            2 => replacement.push_str(line),
            _ => {}
        }
    }
    pairs
}

pub fn normalize_rel_path(path: &str) -> Result<String, String> {
    let path = path
        .trim()
        .trim_matches('`')
        .trim_matches('"')
        .trim_matches('\'')
        .replace('\\', "/");
    if path.is_empty() {
        return Err("文件路径为空".to_owned());
    }
    if path.starts_with('/') || path.contains(':') {
        return Err(format!("只允许工程内相对路径：{path}"));
    }
    let mut parts = Vec::new();
    for part in path.split('/') {
        match part {
            "" | "." => continue,
            ".." => return Err(format!("路径不能包含 ..：{path}")),
            part => parts.push(part),
        }
    }
    if parts.is_empty() {
        return Err("文件路径为空".to_owned());
    }
    if parts.iter().any(|part| is_generated_dir(part)) {
        return Err(format!("不允许修改生成目录或配置目录：{path}"));
    }
    let lower = path.to_ascii_lowercase();
    if [
        ".bin", ".elf", ".axf", ".o", ".a", ".lib", ".exe", ".pack", ".png", ".jpg", ".jpeg",
        ".gif", ".bmp", ".ico", ".zip", ".pdf",
    ]
    .iter()
    .any(|extension| lower.ends_with(extension))
    {
        return Err(format!("不允许 AI 以文本方式修改二进制或归档文件：{path}"));
    }
    Ok(parts.join("/"))
}

fn is_generated_dir(name: &str) -> bool {
    matches!(
        name.to_ascii_lowercase().as_str(),
        ".git"
            | ".svn"
            | ".simple-embedded"
            | "build"
            | "debug"
            | "release"
            | "out"
            | "target"
            | "objects"
            | "listings"
            | "node_modules"
            | "tools"
            | "dist"
    )
}

pub fn hash_text(text: &str) -> u64 {
    let mut hasher = DefaultHasher::new();
    text.hash(&mut hasher);
    hasher.finish()
}

fn preview(text: &str) -> String {
    let one_line = text.replace('\n', "\\n");
    if one_line.chars().count() <= MAX_PREVIEW_CHARS {
        return one_line;
    }
    let mut result = one_line.chars().take(MAX_PREVIEW_CHARS).collect::<String>();
    result.push('…');
    result
}

fn make_diff(old: &str, new: &str) -> Vec<DiffLine> {
    let diff = TextDiff::from_lines(old, new);
    let mut lines = Vec::new();
    for change in diff.iter_all_changes() {
        if lines.len() >= MAX_DIFF_LINES {
            lines.push(DiffLine {
                tag: DiffTag::Meta,
                text: format!("差异过长，仅显示前 {MAX_DIFF_LINES} 行"),
            });
            break;
        }
        let tag = match change.tag() {
            ChangeTag::Delete => DiffTag::Remove,
            ChangeTag::Insert => DiffTag::Add,
            ChangeTag::Equal => DiffTag::Context,
        };
        let mut text = change.to_string();
        if text.ends_with('\n') {
            text.pop();
        }
        lines.push(DiffLine { tag, text });
    }
    lines
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::util::{write_text_file, LineEnding, TextEncoding};

    fn write(path: &Path, text: &str) {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).unwrap();
        }
        write_text_file(path, text, LineEnding::Lf, false, TextEncoding::Utf8).unwrap();
    }

    #[test]
    fn parses_file_and_replace_fences() {
        let reply = "准备修改。\n```se-file path=Core/Inc/board.h\n#pragma once\n```\n```se-replace path=\"Core/Src/main.c\"\n<<<<<<< SEARCH\nold();\n=======\nnew();\n>>>>>>> REPLACE\n```\n完成。\n";
        let parsed = parse_edits(reply);
        assert_eq!(parsed.edits.len(), 2);
        assert_eq!(parsed.edits[0].path, "Core/Inc/board.h");
        assert_eq!(parsed.edits[0].content, "#pragma once\n");
        assert_eq!(parsed.edits[1].path, "Core/Src/main.c");
        assert_eq!(
            parsed.edits[1].pairs,
            vec![("old();\n".to_owned(), "new();\n".to_owned())]
        );
        assert!(parsed.text.contains("准备修改"));
        assert!(parsed.text.contains("完成"));
        assert!(!parsed.text.contains("se-file"));
    }

    #[test]
    fn rejects_escaping_and_generated_paths() {
        assert!(normalize_rel_path("../secret.c").is_err());
        assert!(normalize_rel_path("C:\\secret.c").is_err());
        assert!(normalize_rel_path("build/out.c").is_err());
        assert!(normalize_rel_path("firmware.bin").is_err());
        assert!(normalize_rel_path("Core/Src/main.c").is_ok());
        assert_eq!(
            normalize_rel_path("./Core\\Src\\main.c").unwrap(),
            "Core/Src/main.c"
        );
    }

    #[test]
    fn prepares_replacement_and_diff() {
        let dir = tempfile::tempdir().unwrap();
        write(&dir.path().join("Core/Src/main.c"), "a\nb\n");
        let mut edit = AiEdit::replace(
            "Core/Src/main.c".to_owned(),
            vec![("a\n".to_owned(), "x\n".to_owned())],
        );
        edit.prepare(dir.path());
        assert!(edit.error.is_none());
        assert_eq!(edit.resolved.as_deref(), Some("x\nb\n"));
        assert!(edit.base_hash.is_some());
        assert!(edit.diff.iter().any(|line| line.tag == DiffTag::Add));
        assert!(edit.diff.iter().any(|line| line.tag == DiffTag::Remove));
    }

    #[test]
    fn reports_missing_search_text() {
        let dir = tempfile::tempdir().unwrap();
        write(&dir.path().join("main.c"), "a\n");
        let mut edit = AiEdit::replace(
            "main.c".to_owned(),
            vec![("missing\n".to_owned(), "x\n".to_owned())],
        );
        edit.prepare(dir.path());
        assert!(edit.error.is_some());
        assert!(edit.resolved.is_none());
    }

    #[test]
    fn prepares_new_file() {
        let dir = tempfile::tempdir().unwrap();
        let mut edit = AiEdit::write("hardware.dts".to_owned(), "/ {\n};\n".to_owned());
        edit.prepare(dir.path());
        assert!(edit.error.is_none());
        assert!(edit.base_hash.is_none());
        assert_eq!(edit.resolved.as_deref(), Some("/ {\n};\n"));
        assert!(edit.diff.iter().all(|line| line.tag == DiffTag::Add));
    }
}
