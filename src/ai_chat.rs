//! Persistent AI conversation log stored in the project root.
//!
//! The file is human-readable Markdown with invisible HTML markers so parsing
//! does not depend on headings that the model may also use inside an answer.

use std::path::{Path, PathBuf};

use crate::util::{read_text_file, write_text_file, LineEnding, TextEncoding};

pub const CHAT_SUFFIX: &str = ".ai-chat.md";
const MAX_FILE_BYTES: usize = 2_000_000;
const MAX_CONTENT_CHARS: usize = 200_000;
const MAX_REASONING_CHARS: usize = 100_000;

const MESSAGE_START: &str = "<!-- simple-embedded-ai-chat-message";
const MESSAGE_END: &str = "<!-- /simple-embedded-ai-chat-message -->";
const REASONING_START: &str = "<!-- simple-embedded-ai-chat-reasoning -->";
const REASONING_END: &str = "<!-- /simple-embedded-ai-chat-reasoning -->";

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct StoredChatMessage {
    pub role: String,
    pub content: String,
    pub reasoning: String,
}

pub fn chat_path(root: &Path, project_name: &str) -> PathBuf {
    let mut name = project_name.trim().to_owned();
    if name.is_empty() {
        name = "project".to_owned();
    }
    for ch in ['\\', '/', ':', '*', '?', '"', '<', '>', '|'] {
        name = name.replace(ch, "_");
    }
    root.join(format!("{name}{CHAT_SUFFIX}"))
}

pub fn write(
    path: &Path,
    project_name: &str,
    messages: &[StoredChatMessage],
) -> Result<(), String> {
    let text = render_limited(project_name, messages);
    write_text_file(path, &text, LineEnding::Lf, false, TextEncoding::Utf8)
}

pub fn load(path: &Path) -> Result<Vec<StoredChatMessage>, String> {
    if !path.is_file() {
        return Ok(Vec::new());
    }
    let loaded = read_text_file(path)?;
    let messages = parse(&loaded.text);
    if messages.is_empty()
        && !loaded.text.trim().is_empty()
        && !loaded.text.contains(MESSAGE_START)
        && !loaded.text.contains("# Simple embedded AI 对话：")
    {
        return Err(format!(
            "无法识别对话记录格式：{}",
            crate::util::display_path(path)
        ));
    }
    Ok(messages)
}

fn render_limited(project_name: &str, messages: &[StoredChatMessage]) -> String {
    let mut first = 0usize;
    let mut text = render(project_name, messages);
    while text.len() > MAX_FILE_BYTES && first < messages.len() {
        first += 1;
        text = render(project_name, &messages[first..]);
    }
    if first > 0 {
        text = text.replacen(
            "> 本文件由 Simple embedded 自动维护，记录与 AI 的对话。",
            "> 本文件由 Simple embedded 自动维护，记录与 AI 的对话。\n> 较早的对话因文件过大已截断。",
            1,
        );
    }
    text
}

fn render(project_name: &str, messages: &[StoredChatMessage]) -> String {
    let mut text = String::new();
    text.push_str(&format!(
        "# Simple embedded AI 对话：{}\n\n",
        project_name.trim()
    ));
    text.push_str("> 本文件由 Simple embedded 自动维护，记录与 AI 的对话。\n");
    text.push_str("> 进入 AI 模式时会自动加载；删除本文件会清空下次加载的历史。\n\n");
    for message in messages {
        let user = message.role != "assistant";
        let role = if user { "user" } else { "assistant" };
        let label = if user { "用户" } else { "AI" };
        let content = truncate_chars(&message.content, MAX_CONTENT_CHARS);
        let reasoning = truncate_chars(&message.reasoning, MAX_REASONING_CHARS);
        if content.trim().is_empty() && reasoning.trim().is_empty() {
            continue;
        }
        text.push_str(&format!("{MESSAGE_START} role=\"{role}\" -->\n"));
        text.push_str(&format!("## {label}\n\n"));
        text.push_str(content.trim_end());
        text.push('\n');
        if !reasoning.trim().is_empty() {
            text.push_str("\n<details>\n<summary>思考过程</summary>\n\n");
            text.push_str(REASONING_START);
            text.push('\n');
            text.push_str(reasoning.trim());
            text.push('\n');
            text.push_str(REASONING_END);
            text.push_str("\n\n</details>\n");
        }
        text.push_str(MESSAGE_END);
        text.push_str("\n\n");
    }
    text
}

fn parse(text: &str) -> Vec<StoredChatMessage> {
    let mut messages = Vec::new();
    let mut rest = text;
    while let Some(start) = rest.find(MESSAGE_START) {
        let block = &rest[start..];
        let Some(header_end) = block.find("-->") else {
            break;
        };
        let header = &block[..header_end];
        let role = attr(header, "role").unwrap_or_else(|| "assistant".to_owned());
        let body_start = &block[header_end + 3..];
        let Some(end) = body_start.find(MESSAGE_END) else {
            break;
        };
        let body = &body_start[..end];
        rest = &body_start[end + MESSAGE_END.len()..];
        let (content, reasoning) = parse_body(body);
        if content.trim().is_empty() && reasoning.trim().is_empty() {
            continue;
        }
        messages.push(StoredChatMessage {
            role: if role == "user" {
                "user".to_owned()
            } else {
                "assistant".to_owned()
            },
            content,
            reasoning,
        });
    }
    messages
}

fn parse_body(body: &str) -> (String, String) {
    let body = body.trim_start_matches(['\r', '\n']);
    let body = body
        .strip_prefix("## 用户")
        .or_else(|| body.strip_prefix("## AI"))
        .unwrap_or(body);
    let body = body.trim_start_matches(['\r', '\n']);
    if let Some(start) = body.find(REASONING_START) {
        let after = &body[start + REASONING_START.len()..];
        if let Some(end) = after.find(REASONING_END) {
            let reasoning = after[..end].trim().to_owned();
            let mut content = body[..start].to_owned();
            if let Some(details) = content.rfind("<details>") {
                content.truncate(details);
            }
            return (content.trim_end().to_owned(), reasoning);
        }
    }
    (body.trim_end().to_owned(), String::new())
}

fn attr(header: &str, key: &str) -> Option<String> {
    let needle = format!("{key}=\"");
    let start = header.find(&needle)? + needle.len();
    let end = header[start..].find('"')? + start;
    Some(header[start..end].to_owned())
}

fn truncate_chars(text: &str, limit: usize) -> String {
    if text.chars().count() <= limit {
        return text.to_owned();
    }
    let mut result = text.chars().take(limit).collect::<String>();
    result.push_str("\n\n[内容过长，已截断]");
    result
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn chat_path_uses_project_name_and_suffix() {
        let root = Path::new("D:/work/blink");
        assert_eq!(chat_path(root, "blink"), root.join("blink.ai-chat.md"));
        assert_eq!(
            chat_path(root, "my/project"),
            root.join("my_project.ai-chat.md")
        );
    }

    #[test]
    fn roundtrips_content_and_reasoning() {
        let messages = vec![
            StoredChatMessage {
                role: "user".to_owned(),
                content: "给 main.c 加串口回显".to_owned(),
                reasoning: String::new(),
            },
            StoredChatMessage {
                role: "assistant".to_owned(),
                content: "## 方案\n\n```c\nint main(void) {}\n```".to_owned(),
                reasoning: "先确认串口和波特率。".to_owned(),
            },
        ];
        let text = render("blink", &messages);
        assert!(text.contains("# Simple embedded AI 对话：blink"));
        assert!(text.contains("## 用户"));
        assert!(text.contains("思考过程"));
        assert_eq!(parse(&text), messages);
    }

    #[test]
    fn ignores_header_without_messages() {
        let text = render("blink", &[]);
        assert!(parse(&text).is_empty());
    }

    #[test]
    fn load_accepts_new_header_only_file() {
        let dir = tempfile::tempdir().unwrap();
        let path = chat_path(dir.path(), "blink");
        write(&path, "blink", &[]).unwrap();
        assert!(load(&path).unwrap().is_empty());
    }

    #[test]
    fn parser_keeps_inner_markdown_headings() {
        let messages = vec![StoredChatMessage {
            role: "assistant".to_owned(),
            content: "## 1. 分层架构\n\n## 2. 软件架构\n\n内容".to_owned(),
            reasoning: String::new(),
        }];
        let text = render("blink", &messages);
        assert_eq!(parse(&text), messages);
    }

    #[test]
    fn write_and_load_file() {
        let dir = tempfile::tempdir().unwrap();
        let path = chat_path(dir.path(), "demo");
        let messages = vec![StoredChatMessage {
            role: "user".to_owned(),
            content: "你好".to_owned(),
            reasoning: String::new(),
        }];
        write(&path, "demo", &messages).unwrap();
        assert_eq!(load(&path).unwrap(), messages);
        assert!(path.is_file());
    }
}
