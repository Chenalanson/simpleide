//! Project-local AI issue log (`<project>.ai-issues.md`).
//!
//! The model emits `se-issue` blocks. The IDE merges them by title so an open
//! issue can later be updated with a root cause and solution.

use std::path::{Path, PathBuf};

use crate::util::{read_text_file, write_text_file, LineEnding, TextEncoding};

pub const ISSUE_SUFFIX: &str = ".ai-issues.md";
const ISSUE_START: &str = "<!-- simple-embedded-ai-issue";
const ISSUE_END: &str = "<!-- /simple-embedded-ai-issue -->";

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct AiIssue {
    pub title: String,
    pub resolved: bool,
    pub symptom: String,
    pub cause: String,
    pub solution: String,
    pub files: String,
    pub error: Option<String>,
}

pub fn issue_path(root: &Path, project_name: &str) -> PathBuf {
    let mut name = project_name.trim().to_owned();
    if name.is_empty() {
        name = "project".to_owned();
    }
    for ch in ['\\', '/', ':', '*', '?', '"', '<', '>', '|'] {
        name = name.replace(ch, "_");
    }
    root.join(format!("{name}{ISSUE_SUFFIX}"))
}

pub fn parse_issue(attrs: &str, body: &str) -> AiIssue {
    let attr_title = attr(attrs, "title");
    let attr_status = attr(attrs, "status");
    let mut issue = AiIssue::default();
    let mut current = 0u8;
    for line in body.lines() {
        let trimmed = line.trim();
        if issue.title.is_empty() {
            if let Some(title) = trimmed.strip_prefix("## ") {
                issue.title = title.trim().to_owned();
                current = 1;
                continue;
            }
        }
        if let Some((key, value)) = split_field(trimmed) {
            match key {
                "标题" | "问题" => {
                    issue.title = value.to_owned();
                    current = 1;
                }
                "状态" => {
                    issue.resolved = is_resolved(value);
                    current = 0;
                }
                "现象" | "问题描述" => {
                    issue.symptom = value.to_owned();
                    current = 2;
                }
                "原因" | "根因" => {
                    issue.cause = value.to_owned();
                    current = 3;
                }
                "解决方案" | "解决" | "修复" => {
                    issue.solution = value.to_owned();
                    current = 4;
                }
                "文件" | "相关文件" => {
                    issue.files = value.to_owned();
                    current = 5;
                }
                _ => current = 0,
            }
        } else if !trimmed.is_empty() && current != 0 {
            let target = match current {
                1 => &mut issue.title,
                2 => &mut issue.symptom,
                3 => &mut issue.cause,
                4 => &mut issue.solution,
                5 => &mut issue.files,
                _ => continue,
            };
            if !target.is_empty() {
                target.push('\n');
            }
            target.push_str(trimmed);
        }
    }
    if let Some(title) = attr_title {
        issue.title = title;
    }
    if let Some(status) = attr_status {
        issue.resolved = is_resolved(&status);
    }
    issue.title = truncate(&issue.title, 200);
    issue.symptom = truncate(&issue.symptom, 8_000);
    issue.cause = truncate(&issue.cause, 8_000);
    issue.solution = truncate(&issue.solution, 8_000);
    issue.files = truncate(&issue.files, 2_000);
    if issue.title.is_empty() {
        issue.error = Some("se-issue 缺少标题".to_owned());
    }
    issue
}

pub fn ensure_file(root: &Path, project_name: &str) -> Result<PathBuf, String> {
    let path = issue_path(root, project_name);
    if path.is_file() {
        return Ok(path);
    }
    write(&path, project_name, &[])?;
    Ok(path)
}

/// Merge new issues by title (case-insensitive) and write the Markdown file.
pub fn record(
    root: &Path,
    project_name: &str,
    new_issues: &[AiIssue],
) -> Result<(PathBuf, usize), String> {
    let path = issue_path(root, project_name);
    let mut issues = if path.is_file() {
        read_text_file(&path)
            .map(|loaded| parse(&loaded.text))
            .unwrap_or_default()
    } else {
        Vec::new()
    };
    let mut updated = 0usize;
    for issue in new_issues {
        if issue.error.is_some() || issue.title.trim().is_empty() {
            continue;
        }
        if let Some(existing) = issues
            .iter_mut()
            .find(|existing| existing.title.eq_ignore_ascii_case(&issue.title))
        {
            if !issue.symptom.is_empty() {
                existing.symptom = issue.symptom.clone();
            }
            if !issue.cause.is_empty() {
                existing.cause = issue.cause.clone();
            }
            if !issue.solution.is_empty() {
                existing.solution = issue.solution.clone();
            }
            if !issue.files.is_empty() {
                existing.files = issue.files.clone();
            }
            existing.resolved = issue.resolved;
            updated += 1;
        } else {
            issues.push(issue.clone());
            updated += 1;
        }
    }
    write(&path, project_name, &issues)?;
    Ok((path, updated))
}

fn write(path: &Path, project_name: &str, issues: &[AiIssue]) -> Result<(), String> {
    let mut text = String::new();
    text.push_str(&format!(
        "# Simple embedded AI 问题记录：{}\n\n",
        project_name.trim()
    ));
    text.push_str("> 本文件由 Simple embedded 自动维护，记录 AI 协作中遇到的问题和解决方案。\n");
    text.push_str("> 状态为“未解决”的问题会在后续对话中用同标题更新。\n\n");
    for issue in issues {
        text.push_str(ISSUE_START);
        text.push_str(" title=\"");
        text.push_str(&issue.title.replace('"', "'"));
        text.push_str("\" status=\"");
        text.push_str(if issue.resolved { "resolved" } else { "open" });
        text.push_str("\" -->\n");
        text.push_str(&format!("## {}\n\n", issue.title));
        text.push_str(if issue.resolved {
            "- 状态：已解决\n"
        } else {
            "- 状态：未解决\n"
        });
        if !issue.symptom.is_empty() {
            text.push_str(&format!(
                "- 现象：{}\n",
                issue.symptom.replace('\n', "\n  ")
            ));
        }
        if !issue.cause.is_empty() {
            text.push_str(&format!("- 原因：{}\n", issue.cause.replace('\n', "\n  ")));
        }
        if !issue.solution.is_empty() {
            text.push_str(&format!(
                "- 解决方案：{}\n",
                issue.solution.replace('\n', "\n  ")
            ));
        }
        if !issue.files.is_empty() {
            text.push_str(&format!(
                "- 相关文件：{}\n",
                issue.files.replace('\n', "\n  ")
            ));
        }
        text.push_str(ISSUE_END);
        text.push_str("\n\n");
    }
    write_text_file(path, &text, LineEnding::Lf, false, TextEncoding::Utf8)
}

fn parse(text: &str) -> Vec<AiIssue> {
    let mut issues = Vec::new();
    let mut rest = text;
    while let Some(start) = rest.find(ISSUE_START) {
        let block = &rest[start..];
        let Some(header_end) = block.find("-->") else {
            break;
        };
        let header = &block[..header_end];
        let body_start = &block[header_end + 3..];
        let Some(end) = body_start.find(ISSUE_END) else {
            break;
        };
        let body = &body_start[..end];
        rest = &body_start[end + ISSUE_END.len()..];
        let title = attr(header, "title").unwrap_or_default();
        let resolved = is_resolved(&attr(header, "status").unwrap_or_default());
        let mut issue = parse_issue("", body);
        if !title.is_empty() {
            issue.title = title;
            issue.error = None;
        }
        issue.resolved = resolved;
        if issue.error.is_none() {
            issues.push(issue);
        }
    }
    issues
}

fn attr(attrs: &str, key: &str) -> Option<String> {
    let needle = format!("{key}=");
    let start = attrs.find(&needle)? + needle.len();
    let value = attrs[start..].trim_start();
    if let Some(value) = value.strip_prefix('"') {
        let end = value.find('"')?;
        return Some(value[..end].to_owned());
    }
    let end = value.find(char::is_whitespace).unwrap_or(value.len());
    Some(value[..end].to_owned())
}

fn split_field(line: &str) -> Option<(&str, &str)> {
    let (key, value) = line.split_once('：').or_else(|| line.split_once(':'))?;
    let key = key.trim().trim_start_matches(['-', '*', '•']).trim();
    if matches!(
        key,
        "标题"
            | "问题"
            | "状态"
            | "现象"
            | "问题描述"
            | "原因"
            | "根因"
            | "解决方案"
            | "解决"
            | "修复"
            | "文件"
            | "相关文件"
    ) {
        Some((key, value.trim()))
    } else {
        None
    }
}

fn is_resolved(value: &str) -> bool {
    matches!(
        value.trim().to_ascii_lowercase().as_str(),
        "已解决" | "解决" | "resolved" | "fixed" | "closed" | "done"
    )
}

fn truncate(text: &str, limit: usize) -> String {
    if text.chars().count() <= limit {
        return text.to_owned();
    }
    let mut result = text.chars().take(limit).collect::<String>();
    result.push('…');
    result
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_issue_fields_and_status() {
        let issue = parse_issue(
            "status=open",
            "标题：串口回显乱码\n现象：收到的字节顺序错误\n原因：波特率不匹配\n相关文件：BSP/PERIPHERAL/bsp_usart_echo.c\n",
        );
        assert_eq!(issue.title, "串口回显乱码");
        assert!(!issue.resolved);
        assert!(issue.symptom.contains("字节顺序"));
        assert!(issue.cause.contains("波特率"));
        assert!(issue.files.contains("bsp_usart_echo.c"));
    }

    #[test]
    fn updates_existing_issue_by_title() {
        let dir = tempfile::tempdir().unwrap();
        let first = parse_issue("status=open", "标题：链接失败\n现象：undefined reference\n");
        let (path, count) = record(dir.path(), "demo", &[first]).unwrap();
        assert_eq!(count, 1);
        let second = parse_issue(
            "status=resolved",
            "标题：链接失败\n解决方案：把 bsp_led.c 加入 C_SOURCES\n",
        );
        record(dir.path(), "demo", &[second]).unwrap();
        let loaded = read_text_file(&path).unwrap();
        assert!(loaded.text.contains("已解决"));
        assert!(loaded.text.contains("C_SOURCES"));
        assert_eq!(parse(&loaded.text).len(), 1);
    }

    #[test]
    fn creates_header_and_issue_path() {
        let dir = tempfile::tempdir().unwrap();
        let path = ensure_file(dir.path(), "demo").unwrap();
        assert!(path.is_file());
        assert_eq!(path, dir.path().join("demo.ai-issues.md"));
    }

    #[test]
    fn rejects_issue_without_title() {
        let issue = parse_issue("", "现象：没有标题\n");
        assert!(issue.error.is_some());
    }
}
