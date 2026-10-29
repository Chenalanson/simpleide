//! Lightweight project context index for AI mode.
//!
//! This is a Rust port of the parts of `context-mode` that are useful inside
//! the editor: chunk the project, build an inverted index, retrieve only the
//! chunks relevant to the user's request, and compress noisy tool output before
//! it is sent to the model. It deliberately avoids SQLite/FTS5 so the editor
//! remains a single executable with no extra runtime.

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::OnceLock;

use regex::Regex;

const CHUNK_LINES: usize = 48;
const CHUNK_OVERLAP: usize = 8;
const MAX_FILES: usize = 1200;
const MAX_FILE_BYTES: usize = 1_200_000;
const MAX_CHUNKS: usize = 6000;
const MAX_QUERY_TERMS: usize = 24;

const STOP_WORDS: &[&str] = &[
    "the", "and", "for", "with", "this", "that", "from", "are", "was", "were", "will", "can",
    "not", "you", "your", "our", "use", "using", "into", "then", "than", "there", "here",
];

#[derive(Clone, Debug)]
pub struct IndexedChunk {
    pub path: String,
    pub start_line: usize,
    pub end_line: usize,
    pub text: String,
    pub tokens: u32,
}

#[derive(Clone, Copy, Debug, Default)]
pub struct IndexStats {
    pub files: usize,
    pub chunks: usize,
    pub bytes: usize,
}

#[derive(Clone, Debug)]
pub struct SearchHit {
    pub path: String,
    pub start_line: usize,
    pub end_line: usize,
    pub text: String,
    pub score: f32,
}

#[derive(Debug, Default)]
pub struct ContextIndex {
    chunks: Vec<IndexedChunk>,
    postings: HashMap<String, Vec<(u32, u16)>>,
    files: usize,
    bytes: usize,
}

impl ContextIndex {
    pub fn build(root: &Path) -> Self {
        let mut files = Vec::new();
        collect_files(root, 0, &mut files);
        files.sort();
        let mut index = Self::default();
        for path in files {
            if index.chunks.len() >= MAX_CHUNKS {
                break;
            }
            if index_file(&path, root, &mut index) {
                index.files += 1;
            }
        }
        index
    }

    pub fn stats(&self) -> IndexStats {
        IndexStats {
            files: self.files,
            chunks: self.chunks.len(),
            bytes: self.bytes,
        }
    }

    /// Retrieve chunks matching the request. Scoring is BM25-like with a path
    /// boost so a query mentioning a file name prefers that file's chunks.
    pub fn search(&self, query: &str, limit: usize) -> Vec<SearchHit> {
        let mut terms = Vec::new();
        let mut seen = HashSet::new();
        for term in tokenize(query) {
            if seen.insert(term.clone()) {
                terms.push(term);
            }
            if terms.len() >= MAX_QUERY_TERMS {
                break;
            }
        }
        if terms.is_empty() || self.chunks.is_empty() {
            return Vec::new();
        }
        let count = self.chunks.len() as f32;
        let average = self
            .chunks
            .iter()
            .map(|chunk| chunk.tokens as f32)
            .sum::<f32>()
            / count.max(1.0);
        let mut scores = HashMap::<usize, f32>::new();
        for term in &terms {
            let Some(postings) = self.postings.get(term) else {
                continue;
            };
            let idf = ((count + 1.0) / (postings.len() as f32 + 1.0)).ln() + 1.0;
            for (index, frequency) in postings {
                let chunk = &self.chunks[*index as usize];
                let denominator = *frequency as f32
                    + 1.2 * (0.25 + 0.75 * chunk.tokens as f32 / average.max(1.0));
                let score = idf * (*frequency as f32 * 2.2) / denominator.max(0.001);
                *scores.entry(*index as usize).or_default() += score;
            }
        }
        if scores.is_empty() {
            return Vec::new();
        }
        let lower_terms = terms
            .iter()
            .map(|term| term.to_ascii_lowercase())
            .collect::<Vec<_>>();
        for (index, score) in &mut scores {
            let path = self.chunks[*index].path.to_ascii_lowercase();
            for term in &lower_terms {
                if term.len() >= 2 && path.contains(term) {
                    *score += 1.5;
                }
            }
        }
        let mut hits = scores
            .into_iter()
            .map(|(index, score)| {
                let chunk = &self.chunks[index];
                SearchHit {
                    path: chunk.path.clone(),
                    start_line: chunk.start_line,
                    end_line: chunk.end_line,
                    text: chunk.text.clone(),
                    score,
                }
            })
            .collect::<Vec<_>>();
        hits.sort_by(|a, b| {
            b.score
                .partial_cmp(&a.score)
                .unwrap_or(std::cmp::Ordering::Equal)
                .then_with(|| a.path.cmp(&b.path))
                .then_with(|| a.start_line.cmp(&b.start_line))
        });
        hits.truncate(limit);
        hits
    }
}

fn index_file(path: &Path, root: &Path, index: &mut ContextIndex) -> bool {
    let relative = path
        .strip_prefix(root)
        .unwrap_or(path)
        .to_string_lossy()
        .replace('\\', "/");
    if relative.ends_with("framework.md")
        || relative.ends_with(".ai-chat.md")
        || relative.ends_with(".ai-issues.md")
    {
        return false;
    }
    let Ok(loaded) = crate::util::read_text_file(path) else {
        return false;
    };
    if loaded.text.len() > MAX_FILE_BYTES || loaded.text.contains('\0') {
        return false;
    }
    let lines = loaded.text.lines().collect::<Vec<_>>();
    if lines.is_empty() {
        return false;
    }
    let step = CHUNK_LINES.saturating_sub(CHUNK_OVERLAP).max(1);
    let mut start = 0usize;
    let mut indexed = false;
    while start < lines.len() {
        if index.chunks.len() >= MAX_CHUNKS {
            break;
        }
        let end = (start + CHUNK_LINES).min(lines.len());
        let text = lines[start..end].join("\n");
        if !text.trim().is_empty() {
            let tokens = tokenize(&text);
            if !tokens.is_empty() {
                let mut counts = HashMap::<String, u16>::new();
                for token in &tokens {
                    let entry = counts.entry(token.clone()).or_default();
                    *entry = entry.saturating_add(1);
                }
                let chunk_index = index.chunks.len() as u32;
                for (term, frequency) in counts {
                    index
                        .postings
                        .entry(term)
                        .or_default()
                        .push((chunk_index, frequency));
                }
                index.bytes += text.len();
                index.chunks.push(IndexedChunk {
                    path: relative.clone(),
                    start_line: start + 1,
                    end_line: end,
                    text,
                    tokens: tokens.len() as u32,
                });
                indexed = true;
            }
        }
        if end >= lines.len() {
            break;
        }
        start += step;
    }
    indexed
}

fn collect_files(directory: &Path, depth: usize, files: &mut Vec<PathBuf>) {
    if depth > 8 || files.len() >= MAX_FILES {
        return;
    }
    let Ok(entries) = std::fs::read_dir(directory) else {
        return;
    };
    let mut entries = entries.flatten().collect::<Vec<_>>();
    entries.sort_by_key(|entry| entry.file_name());
    for entry in entries {
        if files.len() >= MAX_FILES {
            break;
        }
        let path = entry.path();
        let Ok(file_type) = entry.file_type() else {
            continue;
        };
        if file_type.is_dir() {
            let name = entry.file_name().to_string_lossy().to_ascii_lowercase();
            if skip_dir(&name) {
                continue;
            }
            collect_files(&path, depth + 1, files);
        } else if file_type.is_file() && is_indexable(&path) {
            files.push(path);
        }
    }
}

fn skip_dir(name: &str) -> bool {
    matches!(
        name,
        "build"
            | "debug"
            | "release"
            | "out"
            | "target"
            | "objects"
            | "listings"
            | "drivers"
            | "middlewares"
            | "cmsis"
            | "third_party"
            | "third-party"
            | "vendor"
            | "node_modules"
            | "tools"
            | "dist"
            | "code_style_samples"
            | "style-samples"
            | "style_samples"
            | ".git"
            | ".simple-embedded"
            | ".opencode"
            | ".vscode"
            | ".idea"
    )
}

fn is_indexable(path: &Path) -> bool {
    let name = path
        .file_name()
        .map(|name| name.to_string_lossy().to_ascii_lowercase())
        .unwrap_or_default();
    if matches!(
        name.as_str(),
        "makefile" | "cmakelists.txt" | "kconfig" | "dockerfile"
    ) {
        return true;
    }
    path.extension()
        .map(|extension| extension.to_string_lossy().to_ascii_lowercase())
        .is_some_and(|extension| {
            matches!(
                extension.as_str(),
                "c" | "h"
                    | "cc"
                    | "cpp"
                    | "cxx"
                    | "hpp"
                    | "hh"
                    | "hxx"
                    | "s"
                    | "asm"
                    | "py"
                    | "md"
                    | "json"
                    | "dts"
                    | "dtsi"
                    | "ld"
                    | "ioc"
                    | "toml"
                    | "yaml"
                    | "yml"
                    | "txt"
                    | "cmake"
            )
        })
}

fn is_cjk(ch: char) -> bool {
    matches!(
        ch as u32,
        0x3400..=0x4DBF | 0x4E00..=0x9FFF | 0xF900..=0xFAFF
    )
}

pub fn tokenize(text: &str) -> Vec<String> {
    let mut tokens = Vec::new();
    let mut word = String::new();
    let mut previous_cjk = None;
    for ch in text.chars() {
        if is_cjk(ch) {
            flush_word(&mut word, &mut tokens);
            tokens.push(ch.to_string());
            if let Some(previous) = previous_cjk {
                tokens.push(format!("{previous}{ch}"));
            }
            previous_cjk = Some(ch);
        } else if ch.is_ascii_alphanumeric() || ch == '_' {
            previous_cjk = None;
            word.push(ch);
        } else {
            previous_cjk = None;
            flush_word(&mut word, &mut tokens);
        }
    }
    flush_word(&mut word, &mut tokens);
    tokens
}

fn flush_word(word: &mut String, output: &mut Vec<String>) {
    if word.is_empty() {
        return;
    }
    let raw = std::mem::take(word);
    push_token(&raw.to_ascii_lowercase(), output);
    for part in raw.split('_') {
        if part.is_empty() {
            continue;
        }
        let mut current = String::new();
        let mut previous_lower = false;
        for ch in part.chars() {
            if ch.is_uppercase() && previous_lower && !current.is_empty() {
                push_token(&current, output);
                current.clear();
            }
            current.push(ch.to_ascii_lowercase());
            previous_lower = ch.is_lowercase() || ch.is_ascii_digit();
        }
        push_token(&current, output);
    }
}

fn push_token(token: &str, output: &mut Vec<String>) {
    let token = token.trim();
    if token.chars().count() >= 2 && !STOP_WORDS.contains(&token) {
        output.push(token.to_owned());
    }
}

fn ansi_regex() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| Regex::new(r"\x1b\[[0-9;?]*[ -/]*[@-~]").expect("ansi regex"))
}

/// Compress command/build/serial output before it is sent to the model:
/// strip ANSI, collapse repeated lines and thin the middle while keeping a
/// head, a tail and error/warning lines. Returns the compressed text and the
/// number of characters saved.
pub fn compress_output(text: &str, max_chars: usize) -> (String, usize) {
    let original_chars = text.chars().count();
    let mut lines = Vec::<String>::new();
    let mut previous: Option<String> = None;
    let mut repeats = 0usize;
    for raw in text.lines() {
        let line = ansi_regex().replace_all(raw, "").trim_end().to_owned();
        if previous.as_deref() == Some(line.as_str()) {
            repeats += 1;
            continue;
        }
        if repeats > 0 {
            lines.push(format!("…（重复 {repeats} 次）"));
            repeats = 0;
        }
        if line.trim().is_empty() && lines.last().is_some_and(|last| last.trim().is_empty()) {
            continue;
        }
        previous = Some(line.clone());
        lines.push(line);
    }
    if repeats > 0 {
        lines.push(format!("…（重复 {repeats} 次）"));
    }
    let mut result = lines.join("\n");
    if result.chars().count() > max_chars {
        let important = lines
            .iter()
            .filter(|line| is_important_line(line))
            .take(120)
            .cloned()
            .collect::<Vec<_>>()
            .join("\n");
        let head = result.chars().take(max_chars / 3).collect::<String>();
        let tail_chars = max_chars / 3;
        let total_chars = result.chars().count();
        let tail = result
            .chars()
            .skip(total_chars.saturating_sub(tail_chars))
            .collect::<String>();
        let mut compressed = String::new();
        compressed.push_str(&head);
        compressed.push_str("\n…（中间省略）\n");
        if !important.is_empty() {
            compressed.push_str("…（错误/警告行）\n");
            compressed.push_str(&important);
            compressed.push('\n');
        }
        compressed.push_str("…（尾部）\n");
        compressed.push_str(&tail);
        result = compressed.chars().take(max_chars).collect();
    }
    let saved = original_chars.saturating_sub(result.chars().count());
    (result, saved)
}

fn is_important_line(line: &str) -> bool {
    let lower = line.to_ascii_lowercase();
    lower.contains("error")
        || lower.contains("warning")
        || lower.contains("undefined")
        || lower.contains("fatal")
        || lower.contains("failed")
        || lower.contains("failure")
        || lower.contains("assert")
        || lower.contains("hardfault")
        || line.contains("错误")
        || line.contains("警告")
        || line.contains("失败")
        || line.contains("无法")
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
    fn tokenizes_snake_case_camel_case_and_cjk() {
        let tokens = tokenize("UART_HandleTypeDef huart1; // 串口接收中断");
        assert!(tokens.contains(&"uart".to_owned()));
        assert!(tokens.contains(&"handle".to_owned()));
        assert!(tokens.contains(&"type".to_owned()));
        assert!(tokens.contains(&"串口".to_owned()));
        assert!(tokens.contains(&"接收".to_owned()));
    }

    #[test]
    fn search_prefers_matching_chunk_and_path() {
        let dir = tempfile::tempdir().unwrap();
        write(
            &dir.path().join("BSP/PERIPHERAL/bsp_uart.c"),
            "void bsp_uart_echo_init(void) {\n  /* uart echo init */\n}\n",
        );
        write(
            &dir.path().join("BSP/PERIPHERAL/bsp_led.c"),
            "void bsp_led_on(void) {\n  /* led on */\n}\n",
        );
        let index = ContextIndex::build(dir.path());
        let hits = index.search("uart echo", 4);
        assert!(!hits.is_empty());
        assert!(hits[0].path.contains("bsp_uart.c"));
        assert!(hits[0].text.contains("bsp_uart_echo_init"));
    }

    #[test]
    fn build_skips_generated_and_chat_files() {
        let dir = tempfile::tempdir().unwrap();
        write(
            &dir.path().join("src/main.c"),
            "int main(void) { return 0; }\n",
        );
        write(
            &dir.path().join("build/generated.c"),
            "void generated_marker(void) {}\n",
        );
        write(
            &dir.path().join("demo.ai-chat.md"),
            "chat marker should not be indexed\n",
        );
        write(
            &dir.path().join("code_style_samples/style_marker.c"),
            "void style_marker(void) {}\n",
        );
        let index = ContextIndex::build(dir.path());
        let hits = index.search("generated marker chat style", 8);
        assert!(hits.iter().all(|hit| !hit.path.contains("build/")));
        assert!(hits.iter().all(|hit| !hit.path.contains("ai-chat")));
        assert!(hits
            .iter()
            .all(|hit| !hit.path.contains("code_style_samples")));
    }

    #[test]
    fn compresses_repeated_and_noisy_output() {
        let input = "start\nwarning: uart timeout\nerror: undefined reference\n".to_owned()
            + &"same progress line\n".repeat(30)
            + "end\n";
        let (compressed, saved) = compress_output(&input, 500);
        assert!(saved > 0);
        assert!(compressed.contains("重复 29 次") || compressed.contains("重复"));
        assert!(compressed.contains("undefined reference"));
        assert!(compressed.len() < input.len());
    }
}
