//! Parsing and previewing file edits produced by the AI chat.
//!
//! The model is asked to emit `se-file` / `se-replace` fences. The editor
//! parses those fences, builds a diff, and applies them only after the user
//! confirms. This keeps the protocol provider-agnostic (no tool-calling
//! requirement) while still letting the assistant operate on project files.

use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};
use std::path::{Path, PathBuf};
use std::time::SystemTime;

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
    pub deletes: Vec<AiDelete>,
    pub asks: Vec<crate::ai_ask::AiAsk>,
    pub build: bool,
    pub evolutions: Vec<String>,
}

#[derive(Clone, Debug)]
pub struct AiDelete {
    pub path: String,
    pub reason: String,
    pub size: u64,
    pub modified: Option<SystemTime>,
    pub error: Option<String>,
}

impl AiDelete {
    fn new(path: String, reason: String) -> Self {
        Self {
            path,
            reason,
            size: 0,
            modified: None,
            error: None,
        }
    }

    /// Validate the target before showing the deletion card. The actual file is
    /// checked again by [`Self::verify`] immediately before removal so a file
    /// that changed after the AI response is not deleted silently.
    pub fn prepare(&mut self, root: &Path) {
        self.error = None;
        self.size = 0;
        self.modified = None;
        match inspect_delete_target(root, &self.path) {
            Ok((path, metadata)) => {
                self.path = path;
                self.size = metadata.len();
                self.modified = metadata.modified().ok();
            }
            Err(err) => self.error = Some(err),
        }
    }

    pub fn verify(&self, root: &Path) -> Result<PathBuf, String> {
        if let Some(err) = &self.error {
            return Err(err.clone());
        }
        let (path, metadata) = inspect_delete_target(root, &self.path)?;
        if metadata.len() != self.size || metadata.modified().ok() != self.modified {
            return Err(format!(
                "{} 在 AI 生成后被修改，请重新生成删除操作",
                self.path
            ));
        }
        Ok(root.join(path))
    }
}

fn inspect_delete_target(root: &Path, path: &str) -> Result<(String, std::fs::Metadata), String> {
    let path = normalize_delete_path(path)?;
    let absolute = root.join(&path);
    let metadata = std::fs::symlink_metadata(&absolute)
        .map_err(|err| format!("{path} 不存在或无法读取：{err}"))?;
    if metadata.file_type().is_symlink() {
        return Err(format!("不允许通过符号链接删除：{path}"));
    }
    if !metadata.is_file() {
        return Err(format!("只允许 AI 删除普通文件，不能删除目录：{path}"));
    }
    let root_canonical = root.canonicalize().unwrap_or_else(|_| root.to_path_buf());
    let absolute_canonical = absolute
        .canonicalize()
        .map_err(|err| format!("无法解析 {path}：{err}"))?;
    if !absolute_canonical.starts_with(&root_canonical) {
        return Err(format!("删除路径不在工程内：{path}"));
    }
    Ok((path, metadata))
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
/// Merge consecutive `se-replace` blocks for the same file so a model that
/// emits one block per hunk does not trip the duplicate-file guard. `se-file`
/// blocks (and mixed write+replace blocks) are kept as-is; the apply step
/// reports those as conflicts because their ordering is ambiguous.
pub fn merge_edits(edits: Vec<AiEdit>) -> Vec<AiEdit> {
    let mut merged: Vec<AiEdit> = Vec::new();
    let mut indexes = std::collections::HashMap::<String, usize>::new();
    for edit in edits {
        let key = normalize_rel_path(&edit.path)
            .map(|path| path.to_ascii_lowercase())
            .unwrap_or_else(|_| edit.path.to_ascii_lowercase());
        if let Some(&index) = indexes.get(&key) {
            let existing = &mut merged[index];
            if existing.kind == AiEditKind::Replace && edit.kind == AiEditKind::Replace {
                existing.pairs.extend(edit.pairs);
                continue;
            }
            merged.push(edit);
            continue;
        }
        indexes.insert(key, merged.len());
        merged.push(edit);
    }
    merged
}

/// Detect an edit fence that never received its closing ```. Providers that
/// stop at an output-token limit often leave the last file block half-written;
/// applying it would overwrite the file with truncated content.
pub fn incomplete_edit_block(text: &str) -> bool {
    let mut open_fence: Option<usize> = None;
    for line in text.split_inclusive('\n') {
        let trimmed = line.trim_start();
        let ticks = trimmed.chars().take_while(|ch| *ch == '`').count();
        if let Some(open) = open_fence {
            if ticks >= open && trimmed[ticks..].trim().is_empty() {
                open_fence = None;
            }
            continue;
        }
        if ticks < 3 {
            continue;
        }
        let tag = trimmed[ticks..].trim_start();
        let tag = tag.split_whitespace().next().unwrap_or("");
        if matches!(
            tag,
            "se-file"
                | "se-write"
                | "se-replace"
                | "se-delete"
                | "se-ask"
                | "se-build"
                | "se-evolve"
        ) {
            open_fence = Some(ticks);
        }
    }
    open_fence.is_some()
}

pub fn parse_edits(text: &str) -> ParsedEdits {
    let mut cleaned = String::new();
    let mut edits = Vec::new();
    let mut deletes = Vec::new();
    let mut asks = Vec::new();
    let mut build = false;
    let mut evolutions = Vec::new();
    let lines = text.split_inclusive('\n').collect::<Vec<_>>();
    let mut index = 0usize;
    while index < lines.len() {
        let line = lines[index];
        let trimmed = line.trim_start();
        let Some((fence_len, tag, attrs)) = fence_header(trimmed) else {
            cleaned.push_str(line);
            index += 1;
            continue;
        };
        if !matches!(
            tag,
            "se-file"
                | "se-write"
                | "se-replace"
                | "se-delete"
                | "se-ask"
                | "se-build"
                | "se-evolve"
        ) {
            cleaned.push_str(line);
            index += 1;
            continue;
        }
        let path = parse_path_attr(attrs).unwrap_or_default();
        // Markdown files commonly contain their own triple-backtick Mermaid
        // blocks. For the legacy three-backtick form, take the last fence
        // before the next edit block so an inner diagram does not cut the file.
        let close = if fence_len == 3 && path.to_ascii_lowercase().ends_with(".md") {
            find_last_fence(&lines, index + 1, fence_len)
        } else {
            find_first_fence(&lines, index + 1, fence_len)
        };
        let Some(close) = close else {
            // Do not guess where a malformed block ends; keep it visible and
            // let the user ask the model again instead of proposing a huge
            // accidental overwrite.
            for line in &lines[index..] {
                cleaned.push_str(line);
            }
            break;
        };
        let body = lines[index + 1..close].concat();
        if tag == "se-evolve" {
            evolutions.extend(parse_evolution_rules(&body));
        } else if tag == "se-build" {
            build = true;
        } else if tag == "se-ask" {
            asks.push(crate::ai_ask::parse_ask(attrs, &body));
        } else if tag == "se-delete" {
            let reason = body.trim().chars().take(500).collect::<String>();
            deletes.push(AiDelete::new(path, reason));
        } else if tag == "se-replace" {
            edits.push(AiEdit::replace(path, parse_pairs(&body)));
        } else {
            edits.push(AiEdit::write(path, body));
        }
        if !cleaned.ends_with('\n') && !cleaned.is_empty() {
            cleaned.push('\n');
        }
        index = close + 1;
    }
    ParsedEdits {
        text: cleaned,
        edits,
        deletes,
        asks,
        build,
        evolutions,
    }
}

fn parse_evolution_rules(body: &str) -> Vec<String> {
    body.lines()
        .map(str::trim)
        .filter(|line| !line.is_empty() && !line.starts_with('#'))
        .map(|line| line.trim_start_matches(['-', '*', '•']).trim().to_owned())
        .filter(|line| !line.is_empty())
        .take(5)
        .collect()
}

fn find_first_fence(lines: &[&str], start: usize, fence_len: usize) -> Option<usize> {
    (start..lines.len()).find(|index| is_closing_fence(lines[*index], fence_len))
}

fn find_last_fence(lines: &[&str], start: usize, fence_len: usize) -> Option<usize> {
    let limit = (start..lines.len())
        .find(|index| {
            fence_header(lines[*index].trim_start()).is_some_and(|(_, tag, _)| {
                matches!(
                    tag,
                    "se-file"
                        | "se-write"
                        | "se-replace"
                        | "se-delete"
                        | "se-ask"
                        | "se-build"
                        | "se-evolve"
                )
            })
        })
        .unwrap_or(lines.len());
    (start..limit)
        .rev()
        .find(|index| is_closing_fence(lines[*index], fence_len))
}

fn is_closing_fence(line: &str, fence_len: usize) -> bool {
    let trimmed = line.trim_start();
    let ticks = trimmed.chars().take_while(|ch| *ch == '`').count();
    ticks >= fence_len && trimmed[ticks..].trim().is_empty()
}

fn fence_header(line: &str) -> Option<(usize, &str, &str)> {
    let ticks = line.chars().take_while(|ch| *ch == '`').count();
    if ticks < 3 {
        return None;
    }
    let rest = line[ticks..].trim_start();
    let end = rest.find(char::is_whitespace).unwrap_or(rest.len());
    let tag = &rest[..end];
    if !tag.starts_with("se-") {
        return None;
    }
    Some((ticks, tag, rest[end..].trim()))
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
    if lower.ends_with(crate::ai_chat::CHAT_SUFFIX) {
        return Err(format!("AI 对话记录由 IDE 维护，不能修改：{path}"));
    }
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

fn normalize_delete_path(path: &str) -> Result<String, String> {
    let path = path
        .trim()
        .trim_matches('`')
        .trim_matches('"')
        .trim_matches('\'')
        .replace('\\', "/");
    if path.is_empty() {
        return Err("删除文件路径为空".to_owned());
    }
    if path.starts_with('/') || path.contains(':') {
        return Err(format!("只允许删除工程内相对路径：{path}"));
    }
    let mut parts = Vec::new();
    for part in path.split('/') {
        match part {
            "" | "." => continue,
            ".." => return Err(format!("删除路径不能包含 ..：{path}")),
            part => parts.push(part),
        }
    }
    if parts.is_empty() {
        return Err("删除文件路径为空".to_owned());
    }
    if parts.iter().any(|part| is_generated_dir(part)) {
        return Err(format!("不允许删除生成目录或配置目录：{path}"));
    }
    let lower = path.to_ascii_lowercase();
    if lower.ends_with(".se-workspace") {
        return Err(format!("不允许删除工作区文件：{path}"));
    }
    if lower.ends_with(crate::ai_chat::CHAT_SUFFIX) {
        return Err(format!("AI 对话记录由 IDE 维护，不能删除：{path}"));
    }
    Ok(parts.join("/"))
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
        assert!(normalize_rel_path("demo.ai-chat.md").is_err());
        assert!(normalize_rel_path("Core/Src/main.c").is_ok());
        assert_eq!(
            normalize_rel_path("./Core\\Src\\main.c").unwrap(),
            "Core/Src/main.c"
        );
    }

    #[test]
    fn merges_multiple_replaces_for_same_file() {
        let dir = tempfile::tempdir().unwrap();
        write(&dir.path().join("Core/Src/main.c"), "a\nb\nc\n");
        let first = AiEdit::replace(
            "Core/Src/main.c".to_owned(),
            vec![("a\n".to_owned(), "x\n".to_owned())],
        );
        let second = AiEdit::replace(
            "Core/Src/main.c".to_owned(),
            vec![("c\n".to_owned(), "z\n".to_owned())],
        );
        let mut merged = merge_edits(vec![first, second]);
        assert_eq!(merged.len(), 1);
        merged[0].prepare(dir.path());
        assert!(merged[0].error.is_none());
        assert_eq!(merged[0].resolved.as_deref(), Some("x\nb\nz\n"));
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

    #[test]
    fn detects_unclosed_edit_fence() {
        assert!(incomplete_edit_block(
            "```se-file path=framework.md\n# incomplete\n"
        ));
        assert!(incomplete_edit_block(
            "```se-replace path=main.c\n<<<<<<< SEARCH\na\n"
        ));
        assert!(!incomplete_edit_block(
            "```se-file path=main.c\nint main(void) {}\n```\n"
        ));
        assert!(!incomplete_edit_block("普通回复\n```c\nint x;\n```\n"));
    }

    #[test]
    fn legacy_three_backtick_markdown_keeps_inner_fences() {
        let reply = "```se-file path=docs/framework.md\n# Doc\n\n```mermaid\ngantt\n```\n\n## 2. 软件架构\n- 前后台\n```\n";
        let parsed = parse_edits(reply);
        assert_eq!(parsed.edits.len(), 1);
        assert!(parsed.edits[0].content.contains("## 2. 软件架构"));
        assert!(parsed.edits[0].content.contains("```mermaid"));
    }

    #[test]
    fn parses_evolution_block_without_file_edit() {
        let reply = "已记录。\n````se-evolve\n- 目录：应用任务放 APP/tasks/\n- 命名：外设驱动统一 bsp_<外设>.c/.h\n````\n";
        let parsed = parse_edits(reply);
        assert!(parsed.edits.is_empty());
        assert_eq!(parsed.evolutions.len(), 2);
        assert!(parsed.evolutions[0].contains("APP/tasks"));
        assert!(!parsed.text.contains("se-evolve"));
        assert!(parsed.text.contains("已记录"));
    }

    #[test]
    fn four_backtick_file_can_contain_markdown_fences() {
        let reply = "````se-file path=framework.md\n# Doc\n\n```mermaid\ngantt\n```\n\n````\n";
        let parsed = parse_edits(reply);
        assert_eq!(parsed.edits.len(), 1);
        assert_eq!(parsed.edits[0].path, "framework.md");
        assert!(parsed.edits[0].content.contains("```mermaid"));
        assert!(parsed.edits[0].content.contains("gantt"));
        assert!(!incomplete_edit_block(reply));
    }

    #[test]
    fn parses_build_request() {
        let reply = "修改完成。\n````se-build\n````\n";
        let parsed = parse_edits(reply);
        assert!(parsed.build);
        assert!(parsed.edits.is_empty());
        assert!(!parsed.text.contains("se-build"));
        assert!(parsed.text.contains("修改完成"));
        assert!(incomplete_edit_block("````se-build\n"));
    }

    #[test]
    fn parses_ask_block_with_options() {
        let reply =
            "需要确认。\n````se-ask multiple\n需要保留哪些功能？\n- 串口回显\n- DMA\n````\n";
        let parsed = parse_edits(reply);
        assert_eq!(parsed.asks.len(), 1);
        assert_eq!(parsed.asks[0].question, "需要保留哪些功能？");
        assert!(parsed.asks[0].multiple);
        assert_eq!(parsed.asks[0].options, vec!["串口回显", "DMA"]);
        assert!(!parsed.text.contains("se-ask"));
        assert!(parsed.text.contains("需要确认"));
        assert!(incomplete_edit_block(
            "````se-ask\n使用哪个串口？\n- USART1\n"
        ));
    }

    #[test]
    fn parses_delete_block_with_reason() {
        let reply = "已迁移。\n````se-delete path=Core/Src/bsp_usart_echo.c\n旧实现已迁移到 BSP/PERIPHERAL，保留会重复定义。\n````\n";
        let parsed = parse_edits(reply);
        assert_eq!(parsed.deletes.len(), 1);
        assert_eq!(parsed.deletes[0].path, "Core/Src/bsp_usart_echo.c");
        assert!(parsed.deletes[0].reason.contains("BSP/PERIPHERAL"));
        assert!(!parsed.text.contains("se-delete"));
        assert!(parsed.text.contains("已迁移"));
        assert!(incomplete_edit_block(
            "````se-delete path=Core/Src/bsp_usart_echo.c\n"
        ));
    }

    #[test]
    fn prepares_and_verifies_delete_target() {
        let dir = tempfile::tempdir().unwrap();
        write(&dir.path().join("Core/Src/old.c"), "int old;\n");
        let mut delete = AiDelete::new("Core/Src/old.c".to_owned(), "unused".to_owned());
        delete.prepare(dir.path());
        assert!(delete.error.is_none());
        assert_eq!(delete.size, 9);
        assert!(delete.verify(dir.path()).is_ok());
        std::fs::write(dir.path().join("Core/Src/old.c"), "int changed;\n").unwrap();
        assert!(delete.verify(dir.path()).is_err());
    }

    #[test]
    fn rejects_unsafe_delete_paths() {
        let dir = tempfile::tempdir().unwrap();
        write(&dir.path().join("Core/Src/old.c"), "int old;\n");
        let mut directory = AiDelete::new("Core".to_owned(), String::new());
        directory.prepare(dir.path());
        assert!(directory.error.is_some());
        for path in [
            "../secret.c",
            "build/out.c",
            ".simple-embedded/settings.json",
            "project.se-workspace",
            "demo.ai-chat.md",
            "missing.c",
        ] {
            let mut delete = AiDelete::new(path.to_owned(), String::new());
            delete.prepare(dir.path());
            assert!(delete.error.is_some(), "{path} should be rejected");
        }
        assert!(normalize_delete_path("Core/Src/old.c").is_ok());
    }
}
