//! Lightweight completion context and member index.
//!
//! This is intentionally not a C/C++ parser. It collects struct/union members
//! and global/local variable declarations with conservative regexes so the
//! editor can offer member completion without an LSP.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::OnceLock;

use regex::Regex;

use crate::ignore::IgnoreRules;
use crate::util::read_text_file;

const MAX_FILE_BYTES: u64 = 2 * 1024 * 1024;
const MAX_TYPES: usize = 4000;
const MAX_MEMBERS_PER_TYPE: usize = 500;
const MAX_VARIABLES: usize = 8000;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CompletionKind {
    Function,
    Type,
    Macro,
    Field,
    Label,
}

impl CompletionKind {
    pub fn label(self) -> &'static str {
        match self {
            Self::Function => "函数",
            Self::Type => "类型",
            Self::Macro => "宏",
            Self::Field => "字段",
            Self::Label => "标签",
        }
    }

    pub fn color(self) -> eframe::egui::Color32 {
        match self {
            Self::Function => crate::theme::info(),
            Self::Type => crate::theme::accent(),
            Self::Macro => crate::theme::warning(),
            Self::Field => crate::theme::text(),
            Self::Label => crate::theme::dir(),
        }
    }
}

#[derive(Clone, Debug)]
pub struct CompletionItem {
    pub name: String,
    pub kind: CompletionKind,
    pub detail: Option<String>,
}

#[derive(Clone, Debug)]
struct MemberField {
    name: String,
    type_name: String,
}

#[derive(Default)]
pub struct MemberIndex {
    members: BTreeMap<String, Vec<MemberField>>,
    variables: BTreeMap<String, String>,
}

impl MemberIndex {
    pub fn resolve_members(&self, chain: &str) -> Vec<CompletionItem> {
        let parts = chain
            .split('.')
            .map(str::trim)
            .filter(|part| !part.is_empty())
            .collect::<Vec<_>>();
        let Some(first) = parts.first() else {
            return Vec::new();
        };
        let mut type_name = self.variables.get(*first).cloned().or_else(|| {
            self.members
                .contains_key(*first)
                .then(|| (*first).to_owned())
        });
        let Some(mut type_name) = type_name.take() else {
            return Vec::new();
        };
        for part in parts.iter().skip(1) {
            let Some(field) = self
                .members
                .get(&type_name)
                .and_then(|fields| fields.iter().find(|field| field.name == *part))
            else {
                return Vec::new();
            };
            type_name = field.type_name.clone();
        }
        self.members
            .get(&type_name)
            .map(|fields| {
                fields
                    .iter()
                    .map(|field| CompletionItem {
                        name: field.name.clone(),
                        kind: CompletionKind::Field,
                        detail: Some(field.type_name.clone()),
                    })
                    .collect()
            })
            .unwrap_or_default()
    }
}

pub struct Context {
    pub prefix_start: usize,
    pub prefix: String,
    pub member_chain: Option<String>,
}

pub fn context_at(text: &str, cursor: usize) -> Context {
    let chars = text.chars().collect::<Vec<_>>();
    let cursor = cursor.min(chars.len());
    let (prefix_start, prefix) = crate::navigation::word_prefix(text, cursor);
    let arrow =
        prefix_start >= 2 && chars[prefix_start - 1] == '>' && chars[prefix_start - 2] == '-';
    let dot = prefix_start >= 1 && chars[prefix_start - 1] == '.';
    if !arrow && !dot {
        return Context {
            prefix_start,
            prefix,
            member_chain: None,
        };
    }
    let chain_end = if arrow {
        prefix_start - 2
    } else {
        prefix_start - 1
    };
    let from = chain_end.saturating_sub(256);
    let segment = chars[from..chain_end].iter().collect::<String>();
    let member_chain = chain_regex()
        .captures(&segment)
        .and_then(|captures| captures.get(1))
        .map(|value| value.as_str().replace("->", ".").replace(' ', ""));
    Context {
        prefix_start,
        prefix,
        member_chain,
    }
}

pub fn build(root: &Path, ignore: &IgnoreRules) -> MemberIndex {
    let mut index = MemberIndex::default();
    for path in source_files(root, ignore) {
        let Ok(metadata) = std::fs::metadata(&path) else {
            continue;
        };
        if metadata.len() > MAX_FILE_BYTES {
            continue;
        }
        let Ok(loaded) = read_text_file(&path) else {
            continue;
        };
        parse_text(&loaded.text, &mut index);
        if index.members.len() >= MAX_TYPES {
            break;
        }
    }
    index
}

fn source_files(root: &Path, ignore: &IgnoreRules) -> Vec<PathBuf> {
    let mut files = Vec::new();
    let mut stack = vec![root.to_path_buf()];
    while let Some(dir) = stack.pop() {
        let Ok(read_dir) = std::fs::read_dir(&dir) else {
            continue;
        };
        let mut entries = read_dir.flatten().collect::<Vec<_>>();
        entries.sort_by_key(|entry| entry.file_name());
        for entry in entries {
            let path = entry.path();
            let Ok(file_type) = entry.file_type() else {
                continue;
            };
            if file_type.is_symlink() {
                continue;
            }
            let relative = path.strip_prefix(root).unwrap_or(path.as_path());
            if file_type.is_dir() {
                if !ignore.is_ignored(relative, true) {
                    stack.push(path);
                }
                continue;
            }
            let extension = path
                .extension()
                .map(|value| value.to_string_lossy().to_lowercase())
                .unwrap_or_default();
            if file_type.is_file()
                && !ignore.is_ignored(relative, false)
                && matches!(
                    extension.as_str(),
                    "c" | "h" | "cc" | "cpp" | "cxx" | "hh" | "hpp" | "hxx" | "ino"
                )
            {
                files.push(path);
            }
        }
    }
    files
}

fn parse_text(text: &str, index: &mut MemberIndex) {
    let text = strip_comments(text);

    // Struct / union / enum definitions. The optional tag allows both
    // `struct Foo {` and the common `typedef struct\n{` shape.
    let mut last_end = 0usize;
    for captures in definition_regex().captures_iter(&text) {
        let Some(whole) = captures.get(0) else {
            continue;
        };
        if whole.start() < last_end {
            continue;
        }
        let Some(kind) = captures.get(1).map(|value| value.as_str()) else {
            continue;
        };
        let tag = captures
            .get(2)
            .map(|value| value.as_str().to_owned())
            .filter(|value| !value.is_empty());
        let open = whole.end().saturating_sub(1);
        let Some(close) = matching_brace(&text, open) else {
            continue;
        };
        last_end = close + 1;
        let body = &text[open + 1..close];
        let alias = alias_after(&text[close + 1..]);
        let mut type_names = Vec::new();
        if let Some(tag) = &tag {
            type_names.push(tag.clone());
        }
        if let Some(alias) = alias {
            if !type_names.iter().any(|name| name == &alias) {
                type_names.push(alias);
            }
        }
        if type_names.is_empty() || kind == "enum" {
            continue;
        }
        let fields = parse_members(body);
        if fields.is_empty() {
            continue;
        }
        if index.members.len() >= MAX_TYPES {
            break;
        }
        for type_name in type_names {
            index.members.entry(type_name).or_insert_with(|| {
                fields
                    .iter()
                    .map(|(name, type_name)| MemberField {
                        name: name.clone(),
                        type_name: type_name.clone(),
                    })
                    .collect()
            });
        }
    }

    // Variable declarations. Member declarations also match this regex, but
    // that is harmless: the compiler only uses the first segment of a member
    // chain, which is a real variable or type name.
    for captures in variable_regex().captures_iter(&text) {
        let Some(whole) = captures.get(0) else {
            continue;
        };
        let whole_text = whole.as_str();
        if whole_text.contains('(')
            || whole_text.trim_start().starts_with('#')
            || whole_text.trim_start().starts_with("typedef ")
        {
            continue;
        }
        let Some(type_name) = captures.get(1) else {
            continue;
        };
        let Some(name) = captures.get(2) else {
            continue;
        };
        let type_name = normalize_type(type_name.as_str());
        if type_name.is_empty()
            || matches!(
                type_name.as_str(),
                "return"
                    | "if"
                    | "for"
                    | "while"
                    | "switch"
                    | "case"
                    | "break"
                    | "continue"
                    | "goto"
                    | "sizeof"
                    | "else"
                    | "do"
                    | "typedef"
            )
        {
            continue;
        }
        if index.variables.len() >= MAX_VARIABLES {
            break;
        }
        index
            .variables
            .entry(name.as_str().to_owned())
            .or_insert(type_name);
    }
}

fn parse_members(body: &str) -> Vec<(String, String)> {
    let mut fields = Vec::new();
    for line in body.lines() {
        let line = line.trim();
        if line.is_empty()
            || line.starts_with('#')
            || line.starts_with('{')
            || line.starts_with('}')
            || line.contains('(')
            || line.ends_with('{')
        {
            continue;
        }
        let Some(captures) = member_regex().captures(line) else {
            continue;
        };
        let Some(type_name) = captures.get(1) else {
            continue;
        };
        let Some(name) = captures.get(2) else {
            continue;
        };
        let type_name = normalize_type(type_name.as_str());
        if type_name.is_empty() {
            continue;
        }
        fields.push((name.as_str().to_owned(), type_name));
        if fields.len() >= MAX_MEMBERS_PER_TYPE {
            break;
        }
    }
    fields
}

fn matching_brace(text: &str, open: usize) -> Option<usize> {
    let bytes = text.as_bytes();
    if bytes.get(open) != Some(&b'{') {
        return None;
    }
    let mut depth = 0usize;
    for (index, byte) in bytes.iter().enumerate().skip(open) {
        match byte {
            b'{' => depth += 1,
            b'}' => {
                depth = depth.saturating_sub(1);
                if depth == 0 {
                    return Some(index);
                }
            }
            _ => {}
        }
    }
    None
}

fn alias_after(text: &str) -> Option<String> {
    let semi = text.find(';')?;
    let segment = &text[..semi];
    let mut flattened = String::with_capacity(segment.len());
    let mut depth = 0usize;
    for character in segment.chars() {
        match character {
            '(' => {
                depth += 1;
                flattened.push(' ');
            }
            ')' => {
                depth = depth.saturating_sub(1);
                flattened.push(' ');
            }
            _ if depth == 0 => flattened.push(character),
            _ => flattened.push(' '),
        }
    }
    for name in alias_regex().find_iter(&flattened) {
        let name = name.as_str();
        if name.starts_with("__")
            || matches!(
                name,
                "const"
                    | "volatile"
                    | "static"
                    | "struct"
                    | "union"
                    | "enum"
                    | "typedef"
                    | "attribute"
            )
        {
            continue;
        }
        return Some(name.to_owned());
    }
    None
}

fn normalize_type(value: &str) -> String {
    let mut value = value.trim();
    loop {
        let stripped = [
            "const ",
            "volatile ",
            "static ",
            "mutable ",
            "extern ",
            "register ",
            "struct ",
            "union ",
            "enum ",
        ]
        .iter()
        .find_map(|prefix| value.strip_prefix(prefix));
        match stripped {
            Some(rest) => value = rest.trim_start(),
            None => break,
        }
    }
    value
        .chars()
        .filter(|character| !matches!(character, '*' | '&' | ' ' | '\t' | '\r' | '\n'))
        .collect()
}

fn strip_comments(text: &str) -> String {
    let mut output = String::with_capacity(text.len());
    let mut chars = text.chars().peekable();
    let mut block = false;
    let mut line = false;
    let mut string = false;
    let mut escape = false;
    while let Some(character) = chars.next() {
        if block {
            if character == '*' && chars.peek() == Some(&'/') {
                chars.next();
                block = false;
                output.push(' ');
            } else if character == '\n' {
                output.push('\n');
            } else {
                output.push(' ');
            }
            continue;
        }
        if line {
            if character == '\n' {
                output.push('\n');
                line = false;
            } else {
                output.push(' ');
            }
            continue;
        }
        if string {
            if escape {
                escape = false;
                output.push(' ');
            } else if character == '\\' {
                escape = true;
                output.push(' ');
            } else if character == '"' {
                string = false;
                output.push(' ');
            } else {
                output.push(' ');
            }
            continue;
        }
        if character == '/' && chars.peek() == Some(&'/') {
            chars.next();
            line = true;
            output.push(' ');
            continue;
        }
        if character == '/' && chars.peek() == Some(&'*') {
            chars.next();
            block = true;
            output.push(' ');
            continue;
        }
        if character == '"' {
            string = true;
            output.push(' ');
            continue;
        }
        output.push(character);
    }
    output
}

fn definition_regex() -> &'static Regex {
    static REGEX: OnceLock<Regex> = OnceLock::new();
    REGEX.get_or_init(|| {
        Regex::new(r"(?s)(?:typedef\s+)?(struct|union|enum)\b\s*([A-Za-z_]\w*)?\s*\{")
            .expect("valid definition regex")
    })
}

fn member_regex() -> &'static Regex {
    static REGEX: OnceLock<Regex> = OnceLock::new();
    REGEX.get_or_init(|| {
        Regex::new(
            r"^[ \t]*(?:(?:const|volatile|static|mutable|unsigned|signed|long|short|struct|union|enum)[ \t]+)*([A-Za-z_]\w*)[ \t]*(?:\*+[ \t]*)?([A-Za-z_]\w*)[ \t]*(?:\[[^\]]*\][ \t]*)?(?::[^;]+)?;[ \t]*$",
        )
        .expect("valid member regex")
    })
}

fn variable_regex() -> &'static Regex {
    static REGEX: OnceLock<Regex> = OnceLock::new();
    REGEX.get_or_init(|| {
        Regex::new(
            r"(?m)^[ \t]*(?:(?:static|extern|volatile|const|register|unsigned|signed|long|short|struct|union|enum)[ \t]+)*([A-Za-z_]\w*)[ \t]*(?:\*+[ \t]*)?([A-Za-z_]\w*)[ \t]*(?:\[[^\]]*\][ \t]*)?(?:=[^;]*)?;[ \t]*$",
        )
        .expect("valid variable regex")
    })
}

fn chain_regex() -> &'static Regex {
    static REGEX: OnceLock<Regex> = OnceLock::new();
    REGEX.get_or_init(|| {
        Regex::new(r"([A-Za-z_]\w*(?:[ \t]*(?:\.|->)[ \t]*[A-Za-z_]\w*)*)[ \t]*$")
            .expect("valid chain regex")
    })
}

fn alias_regex() -> &'static Regex {
    static REGEX: OnceLock<Regex> = OnceLock::new();
    REGEX.get_or_init(|| Regex::new(r"[A-Za-z_]\w*").expect("valid alias regex"))
}

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE: &str = r#"
typedef struct
{
  uint32_t PLLState;
  uint32_t PLLSource;
  RCC_PLLInitTypeDef PLL;
} RCC_OscInitTypeDef;

typedef struct
{
  uint32_t PLLM;
  uint32_t PLLN;
  uint32_t PLLP;
} RCC_PLLInitTypeDef;

RCC_OscInitTypeDef RCC_OscInitStruct = {0};

typedef enum
{
  RCC_PLLSOURCE_HSE,
  RCC_PLLSOURCE_HSI
} RCC_PLLSourceTypeDef;
"#;

    #[test]
    fn indexes_struct_members_and_variables() {
        let mut index = MemberIndex::default();
        parse_text(SAMPLE, &mut index);
        let fields = index.resolve_members("RCC_OscInitStruct.PLL");
        let names = fields
            .iter()
            .map(|field| field.name.as_str())
            .collect::<Vec<_>>();
        assert_eq!(names, vec!["PLLM", "PLLN", "PLLP"]);
        assert_eq!(
            index.variables.get("RCC_OscInitStruct").map(String::as_str),
            Some("RCC_OscInitTypeDef")
        );
    }

    #[test]
    fn builds_index_from_project_files_with_tagged_structs() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(
            dir.path().join("rcc.h"),
            r#"
typedef struct {
  uint32_t PLLState;
  struct RCC_PLLInitTypeDef *PLL;
} RCC_OscInitTypeDef;

typedef struct RCC_PLLInitTypeDef {
  uint32_t PLLM;
  uint32_t PLLN;
  uint32_t PLLP;
} RCC_PLLInitTypeDef;

RCC_OscInitTypeDef RCC_OscInitStruct;

typedef struct { uint32_t value; } __attribute__((packed)) PackedType;
"#,
        )
        .unwrap();
        let index = build(dir.path(), &IgnoreRules::default());
        let fields = index.resolve_members("RCC_OscInitStruct.PLL");
        assert_eq!(
            fields
                .iter()
                .map(|field| field.name.as_str())
                .collect::<Vec<_>>(),
            vec!["PLLM", "PLLN", "PLLP"]
        );
        let packed = index.resolve_members("PackedType");
        assert_eq!(packed[0].name, "value");
    }

    #[test]
    fn finds_member_context_after_dot_and_arrow() {
        let text = "RCC_OscInitStruct.PLL.PL";
        let context = context_at(text, text.chars().count());
        assert_eq!(context.prefix, "PL");
        assert_eq!(context.prefix_start, text.chars().count() - 2);
        assert_eq!(
            context.member_chain.as_deref(),
            Some("RCC_OscInitStruct.PLL")
        );

        let text = "config->PLL";
        let context = context_at(text, text.chars().count());
        assert_eq!(context.prefix, "PLL");
        assert_eq!(context.member_chain.as_deref(), Some("config"));
    }

    #[test]
    fn no_member_context_for_plain_identifier() {
        let text = "HAL_RCC_";
        let context = context_at(text, text.len());
        assert_eq!(context.prefix, "HAL_RCC_");
        assert!(context.member_chain.is_none());
    }
}
