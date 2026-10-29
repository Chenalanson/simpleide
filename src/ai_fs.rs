//! Read-only project file retrieval requested by the model.
//!
//! The AI can emit `se-list` / `se-read` blocks when the automatic context is
//! not enough. The IDE executes them against the project root and feeds the
//! real file contents into the next request. Nothing outside the project is
//! readable, and generated/configuration directories are rejected.

use std::path::Path;

pub const MAX_FS_REQUESTS: usize = 12;
pub const MAX_FS_REPORT_BYTES: usize = 48_000;
const MAX_FS_FILE_BYTES: usize = 24_000;
const MAX_FS_DIR_ENTRIES: usize = 300;
const MAX_FS_PREVIEW_FILE_BYTES: u64 = 2 * 1024 * 1024;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum AiFsKind {
    #[default]
    List,
    Read,
}

#[derive(Clone, Debug, Default)]
pub struct AiFsRequest {
    pub kind: AiFsKind,
    pub path: String,
    pub depth: usize,
    pub limit: usize,
    pub lines: Option<(usize, usize)>,
    pub error: Option<String>,
}

impl AiFsRequest {
    pub fn parse(read: bool, attrs: &str) -> Self {
        let mut request = Self {
            kind: if read { AiFsKind::Read } else { AiFsKind::List },
            depth: 1,
            limit: 200,
            ..Self::default()
        };
        request.path = crate::ai_edit::parse_path_attr(attrs).unwrap_or_default();
        if request.path.trim().is_empty() {
            if read {
                request.error = Some("se-read 缺少 path".to_owned());
            } else {
                request.path = ".".to_owned();
            }
        }
        if let Some(depth) = attr_number(attrs, "depth") {
            request.depth = depth.clamp(1, 3);
        }
        if let Some(limit) = attr_number(attrs, "limit") {
            request.limit = limit.clamp(1, MAX_FS_DIR_ENTRIES);
        }
        if let Some(value) = attr_value(attrs, "lines") {
            request.lines = parse_lines(&value);
            if request.lines.is_none() {
                request.error = Some(format!("lines 格式应为 1-120：{value}"));
            }
        } else {
            let start = attr_number(attrs, "start");
            let end = attr_number(attrs, "end");
            if start.is_some() || end.is_some() {
                let start = start.unwrap_or(1).max(1);
                let end = end.unwrap_or(usize::MAX);
                if end >= start {
                    request.lines = Some((start, end));
                } else {
                    request.error = Some("start 不能大于 end".to_owned());
                }
            }
        }
        request
    }

    pub fn label(&self) -> String {
        match self.kind {
            AiFsKind::List => {
                let path = if self.path == "." {
                    "工程根目录".to_owned()
                } else {
                    self.path.clone()
                };
                format!("列出 {path}（深度 {}）", self.depth)
            }
            AiFsKind::Read => match self.lines {
                Some((start, end)) => format!("读取 {} 第 {start}-{end} 行", self.path),
                None => format!("读取 {}", self.path),
            },
        }
    }
}

#[derive(Debug, Default)]
pub struct AiFsOutcome {
    pub report: String,
    pub summary: String,
    pub errors: Vec<String>,
    pub files: usize,
    pub dirs: usize,
    pub skipped: usize,
}

pub fn execute(root: &Path, requests: &[AiFsRequest]) -> AiFsOutcome {
    let mut outcome = AiFsOutcome::default();
    let canonical_root = match root.canonicalize() {
        Ok(path) => path,
        Err(err) => {
            outcome.errors.push(format!("工程根目录不可访问：{err}"));
            outcome.summary = "文件检索失败：工程根目录不可访问".to_owned();
            outcome.report = format!("[IDE 文件检索结果]\n\n读取失败：{err}\n");
            return outcome;
        }
    };
    let mut report = String::from(
        "[IDE 文件检索结果]\n\n以下内容由 IDE 从工程磁盘真实读取，不是模型猜测。请基于这些内容继续分析；需要更多文件时继续输出 se-list / se-read，不要对未读取的文件生成 se-replace。\n\n",
    );
    let limit = requests.len().min(MAX_FS_REQUESTS);
    for request in requests.iter().take(limit) {
        if report.len() >= MAX_FS_REPORT_BYTES {
            outcome.skipped += 1;
            continue;
        }
        let mut section = String::new();
        let result = match request.error.as_deref() {
            Some(error) => Err(error.to_owned()),
            None => match request.kind {
                AiFsKind::List => execute_list(root, &canonical_root, request, &mut section),
                AiFsKind::Read => execute_read(root, &canonical_root, request, &mut section),
            },
        };
        match result {
            Ok(()) => {
                match request.kind {
                    AiFsKind::List => outcome.dirs += 1,
                    AiFsKind::Read => outcome.files += 1,
                }
                if report.len() + section.len() > MAX_FS_REPORT_BYTES {
                    outcome.skipped += 1;
                    continue;
                }
                report.push_str(&section);
            }
            Err(error) => {
                outcome.errors.push(error.clone());
                report.push_str(&format!("### {}：失败\n\n{error}\n\n", request.label()));
            }
        }
    }
    if requests.len() > limit {
        outcome.skipped += requests.len() - limit;
    }
    if outcome.skipped > 0 {
        report.push_str(&format!(
            "[本轮检索达到上限，{} 个请求未执行；请减少范围或分轮继续。]\n\n",
            outcome.skipped
        ));
    }
    report.push_str(&format!(
        "[检索统计：{} 个文件，{} 个目录，{} 个失败，{} 个跳过]\n",
        outcome.files,
        outcome.dirs,
        outcome.errors.len(),
        outcome.skipped
    ));
    let mut summary = format!(
        "已读取 {} 个文件、列出 {} 个目录",
        outcome.files, outcome.dirs
    );
    if !outcome.errors.is_empty() {
        summary.push_str(&format!("，{} 个失败", outcome.errors.len()));
    }
    if outcome.skipped > 0 {
        summary.push_str(&format!("，{} 个跳过", outcome.skipped));
    }
    summary.push_str(&format!("（约 {} KB）", report.len() / 1024));
    outcome.summary = summary;
    outcome.report = report;
    outcome
}

fn execute_list(
    root: &Path,
    canonical_root: &Path,
    request: &AiFsRequest,
    section: &mut String,
) -> Result<(), String> {
    let relative = normalize_dir_path(&request.path)?;
    if relative.split('/').any(is_blocked_part) {
        return Err(format!("不允许读取该目录：{}", request.path));
    }
    let absolute = if relative.is_empty() {
        root.to_path_buf()
    } else {
        root.join(&relative)
    };
    ensure_inside(canonical_root, &absolute)?;
    let metadata = std::fs::symlink_metadata(&absolute)
        .map_err(|err| format!("{} 不存在或无法读取：{err}", request.path))?;
    if metadata.file_type().is_symlink() {
        return Err(format!("不允许读取符号链接：{}", request.path));
    }
    if !metadata.is_dir() {
        return Err(format!("{} 不是目录", request.path));
    }
    let title = if relative.is_empty() {
        "工程根目录".to_owned()
    } else {
        relative.clone()
    };
    section.push_str(&format!("### 目录 {title}\n\n```text\n"));
    let mut entries = Vec::new();
    collect_entries(
        root,
        &absolute,
        0,
        request.depth,
        request.limit,
        &mut entries,
    )?;
    let mut shown = 0usize;
    for entry in &entries {
        if shown >= request.limit {
            break;
        }
        section.push_str(&entry.line);
        section.push('\n');
        shown += 1;
    }
    if entries.len() > shown {
        section.push_str(&format!("... 还有 {} 项未列出\n", entries.len() - shown));
    }
    section.push_str("```\n\n");
    Ok(())
}

fn execute_read(
    root: &Path,
    canonical_root: &Path,
    request: &AiFsRequest,
    section: &mut String,
) -> Result<(), String> {
    let relative = crate::ai_edit::normalize_rel_path(&request.path)?;
    if relative.split('/').any(is_blocked_part) {
        return Err(format!("不允许读取该文件：{}", request.path));
    }
    let absolute = root.join(&relative);
    ensure_inside(canonical_root, &absolute)?;
    let metadata = std::fs::symlink_metadata(&absolute)
        .map_err(|err| format!("{} 不存在或无法读取：{err}", request.path))?;
    if metadata.file_type().is_symlink() {
        return Err(format!("不允许读取符号链接：{}", request.path));
    }
    if !metadata.is_file() {
        return Err(format!("{} 不是普通文件", request.path));
    }
    if metadata.len() > MAX_FS_PREVIEW_FILE_BYTES {
        return Err(format!(
            "{} 超过 2MB（{}），请改用 lines 读取局部",
            request.path,
            format_size(metadata.len())
        ));
    }
    let loaded = crate::util::read_text_file(&absolute)
        .map_err(|err| format!("{} 读取失败：{err}", request.path))?;
    let text = loaded.text;
    let total_lines = text.lines().count().max(1);
    let (body, range_label) = match request.lines {
        Some((start, end)) => {
            let start = start.max(1);
            let end = end.min(total_lines);
            if end < start {
                return Err(format!(
                    "{} 的行范围 {start}-{end} 超出文件范围（共 {total_lines} 行）",
                    request.path
                ));
            }
            let body = text
                .lines()
                .skip(start - 1)
                .take(end - start + 1)
                .collect::<Vec<_>>()
                .join("\n");
            (body, Some((start, end)))
        }
        None => (text, None),
    };
    let (body, cut) = clip_bytes(&body, MAX_FS_FILE_BYTES);
    section.push_str(&format!(
        "### 文件 {}（{}）\n\n",
        relative,
        match range_label {
            Some((start, end)) => format!("第 {start}-{end} 行，共 {total_lines} 行"),
            None => format!("共 {total_lines} 行"),
        }
    ));
    section.push_str("```text\n");
    section.push_str(body.trim_end_matches(['\r', '\n']));
    section.push_str("\n```\n\n");
    if cut {
        section.push_str(&format!(
            "[内容超过 {}，已截断；需要后续内容时用 se-read path={} lines={}-{}]\n\n",
            format_size(MAX_FS_FILE_BYTES as u64),
            relative,
            range_label.map(|(_, end)| end + 1).unwrap_or(1),
            range_label.map(|(_, end)| end + 200).unwrap_or(200)
        ));
    }
    Ok(())
}

fn collect_entries(
    root: &Path,
    directory: &Path,
    depth: usize,
    max_depth: usize,
    max_entries: usize,
    entries: &mut Vec<DirEntry>,
) -> Result<(), String> {
    if entries.len() >= max_entries {
        return Ok(());
    }
    let mut children = std::fs::read_dir(directory)
        .map_err(|err| format!("无法列出目录 {}：{err}", directory.display()))?
        .flatten()
        .collect::<Vec<_>>();
    children.sort_by_key(|entry| {
        let is_dir = entry.file_type().map(|kind| kind.is_dir()).unwrap_or(false);
        (
            !is_dir,
            entry.file_name().to_string_lossy().to_ascii_lowercase(),
        )
    });
    for child in children {
        if entries.len() >= max_entries {
            break;
        }
        let Ok(file_type) = child.file_type() else {
            continue;
        };
        if file_type.is_symlink() {
            continue;
        }
        let file_name = child.file_name();
        let name = file_name.to_string_lossy().into_owned();
        if is_skipped_name(&name) {
            continue;
        }
        let relative = child
            .path()
            .strip_prefix(root)
            .unwrap_or(&child.path())
            .to_string_lossy()
            .replace('\\', "/");
        if file_type.is_dir() {
            entries.push(DirEntry {
                line: format!("{}{}/", indent(depth), name),
            });
            if depth + 1 < max_depth {
                collect_entries(
                    root,
                    &child.path(),
                    depth + 1,
                    max_depth,
                    max_entries,
                    entries,
                )?;
            }
        } else if file_type.is_file() {
            let size = child.metadata().map(|meta| meta.len()).unwrap_or(0);
            entries.push(DirEntry {
                line: format!("{}{}  ({} B)  {}", indent(depth), name, size, relative),
            });
        }
    }
    Ok(())
}

struct DirEntry {
    line: String,
}

fn indent(depth: usize) -> String {
    "  ".repeat(depth)
}

fn normalize_dir_path(path: &str) -> Result<String, String> {
    let trimmed = path
        .trim()
        .trim_matches('`')
        .trim_matches('"')
        .trim_matches('\'');
    if trimmed.is_empty() || trimmed == "." || trimmed == "./" || trimmed == ".\\" {
        return Ok(String::new());
    }
    crate::ai_edit::normalize_rel_path(trimmed)
}

fn ensure_inside(canonical_root: &Path, path: &Path) -> Result<(), String> {
    let canonical = path
        .canonicalize()
        .map_err(|err| format!("路径不可访问：{err}"))?;
    if !canonical.starts_with(canonical_root) {
        return Err("只允许读取工程目录内的文件".to_owned());
    }
    Ok(())
}

fn is_blocked_part(part: &str) -> bool {
    let lower = part.to_ascii_lowercase();
    crate::ai_edit::is_generated_dir(part)
        || matches!(
            lower.as_str(),
            ".opencode" | ".vscode" | ".idea" | "__pycache__" | ".pytest_cache"
        )
}

fn is_skipped_name(name: &str) -> bool {
    let lower = name.to_ascii_lowercase();
    is_blocked_part(name)
        || matches!(lower.as_str(), ".ai-chat.md" | ".ai-issues.md")
        || lower.ends_with(".ai-chat.md")
        || lower.ends_with(".ai-issues.md")
}

fn attr_value(attrs: &str, key: &str) -> Option<String> {
    let mut search_from = 0usize;
    while let Some(found) = attrs[search_from..].find(key) {
        let start = search_from + found;
        let before = attrs[..start].chars().next_back();
        if before.is_some_and(|ch| ch.is_ascii_alphanumeric() || ch == '_') {
            search_from = start + key.len();
            continue;
        }
        let after = attrs[start + key.len()..].trim_start();
        let Some(after) = after.strip_prefix('=') else {
            search_from = start + key.len();
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

fn attr_number(attrs: &str, key: &str) -> Option<usize> {
    attr_value(attrs, key).and_then(|value| value.parse().ok())
}

fn parse_lines(value: &str) -> Option<(usize, usize)> {
    let value = value.trim();
    if let Some((start, end)) = value.split_once('-') {
        let start = start.trim().parse::<usize>().ok().unwrap_or(1).max(1);
        let end = end.trim().parse::<usize>().ok()?;
        (end >= start).then_some((start, end))
    } else {
        let line = value.parse::<usize>().ok()?;
        Some((line.max(1), line.max(1)))
    }
}

fn clip_bytes(text: &str, max_bytes: usize) -> (String, bool) {
    if text.len() <= max_bytes {
        return (text.to_owned(), false);
    }
    let mut end = max_bytes;
    while end > 0 && !text.is_char_boundary(end) {
        end -= 1;
    }
    (text[..end].to_owned(), true)
}

fn format_size(bytes: u64) -> String {
    if bytes >= 1024 * 1024 {
        format!("{:.1} MB", bytes as f64 / (1024.0 * 1024.0))
    } else if bytes >= 1024 {
        format!("{:.1} KB", bytes as f64 / 1024.0)
    } else {
        format!("{bytes} B")
    }
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
    fn parses_list_and_read_requests() {
        let list = AiFsRequest::parse(false, "path=Core/Src depth=2 limit=50");
        assert_eq!(list.kind, AiFsKind::List);
        assert_eq!(list.path, "Core/Src");
        assert_eq!(list.depth, 2);
        assert_eq!(list.limit, 50);
        assert!(list.error.is_none());

        let read = AiFsRequest::parse(true, "path=\"Core/Src/main.c\" lines=10-40");
        assert_eq!(read.kind, AiFsKind::Read);
        assert_eq!(read.path, "Core/Src/main.c");
        assert_eq!(read.lines, Some((10, 40)));
        assert!(read.error.is_none());

        let missing = AiFsRequest::parse(true, "");
        assert!(missing.error.is_some());
    }

    #[test]
    fn lists_directory_and_reads_file_range() {
        let dir = tempfile::tempdir().unwrap();
        write(&dir.path().join("Core/Src/main.c"), "line1\nline2\nline3\n");
        write(&dir.path().join("build/generated.c"), "generated\n");
        let requests = vec![
            AiFsRequest::parse(false, "path=. depth=2"),
            AiFsRequest::parse(true, "path=Core/Src/main.c lines=2-3"),
        ];
        let outcome = execute(dir.path(), &requests);
        assert!(outcome.report.contains("Core/Src/main.c"));
        assert!(!outcome.report.contains("generated.c"));
        assert!(outcome.report.contains("line2\nline3"));
        assert!(!outcome.report.contains("line1"));
        assert_eq!(outcome.files, 1);
        assert_eq!(outcome.dirs, 1);
        assert!(outcome.errors.is_empty());
    }

    #[test]
    fn rejects_escape_and_binary_paths() {
        let dir = tempfile::tempdir().unwrap();
        write(&dir.path().join("main.c"), "int main(void) {}\n");
        let requests = vec![
            AiFsRequest::parse(true, "path=../secret.c"),
            AiFsRequest::parse(true, "path=firmware.bin"),
            AiFsRequest::parse(true, "path=build/out.c"),
        ];
        let outcome = execute(dir.path(), &requests);
        assert!(!outcome.errors.is_empty());
        assert!(!outcome.report.contains("int main"));
    }
}
