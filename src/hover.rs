//! Hover information for identifiers: signature, doc comment and type.

use std::path::Path;

use regex::Regex;

use crate::symbols::{Symbol, SymbolKind};

#[derive(Clone, Debug)]
pub struct HoverInfo {
    pub signature: String,
    pub doc: String,
    pub type_label: &'static str,
    pub type_text: String,
    pub kind: &'static str,
}

pub fn build(
    current_path: &Path,
    text: &str,
    word: &str,
    line: usize,
    symbols: &[Symbol],
) -> Option<HoverInfo> {
    if word.is_empty()
        || !word
            .chars()
            .all(|ch| ch.is_ascii_alphanumeric() || ch == '_')
    {
        return None;
    }
    if let Some(info) = local_declaration(current_path, text, word, line) {
        return Some(info);
    }

    let symbol = symbols.iter().find(|symbol| symbol.name == word)?;
    let loaded = crate::util::read_text_file(&symbol.path).ok()?;
    let lines = loaded.text.lines().collect::<Vec<_>>();
    let index = symbol
        .line
        .saturating_sub(1)
        .min(lines.len().saturating_sub(1));
    let declaration = lines.get(index)?.trim().to_owned();
    let doc = doc_comment(&lines, index);
    let (type_label, type_text) = match symbol.kind {
        SymbolKind::Function => ("返回值", return_type(&declaration, word)),
        SymbolKind::Macro => ("", String::new()),
        _ => ("类型", declaration.clone()),
    };
    Some(HoverInfo {
        signature: clean_signature(&declaration),
        doc,
        type_label,
        type_text,
        kind: match symbol.kind {
            SymbolKind::Function => "函数",
            SymbolKind::Struct => "类型",
            SymbolKind::Macro => "宏",
            SymbolKind::Label => "标签",
        },
    })
}

fn local_declaration(path: &Path, text: &str, word: &str, line: usize) -> Option<HoverInfo> {
    let lines = text.lines().collect::<Vec<_>>();
    let word_re = regex::escape(word);
    let function = Regex::new(&format!(
        r"^\s*(?:static\s+|inline\s+|extern\s+|const\s+|volatile\s+)*[A-Za-z_][\w\s\*]*?\b{word_re}\s*\("
    ))
    .ok()?;
    let variable = Regex::new(&format!(
        r"^\s*(?:static\s+|extern\s+|volatile\s+|const\s+)*[A-Za-z_][\w\s\*]*?\b{word_re}\b\s*(?:=|;|\[|,)"
    ))
    .ok()?;
    let end = line.saturating_sub(1).min(lines.len().saturating_sub(1));
    for index in (0..=end).rev() {
        let line_text = lines[index].trim();
        if line_text.is_empty()
            || line_text.starts_with("//")
            || line_text.starts_with('*')
            || line_text.starts_with("/*")
        {
            continue;
        }
        if first_word_is_keyword(line_text) {
            continue;
        }
        let is_function = function.is_match(line_text);
        let is_variable = !is_function && variable.is_match(line_text);
        if !is_function && !is_variable {
            continue;
        }
        let signature = clean_signature(line_text);
        let doc = doc_comment(&lines, index);
        if is_function {
            return Some(HoverInfo {
                type_label: "返回值",
                type_text: return_type(&signature, word),
                doc,
                signature,
                kind: "函数",
            });
        }
        let type_text = declaration_type(&signature, word);
        return Some(HoverInfo {
            signature,
            doc,
            type_label: "类型",
            type_text,
            kind: "变量",
        });
    }
    let _ = path;
    None
}

fn first_word_is_keyword(line: &str) -> bool {
    let first = line
        .split(|character: char| !character.is_ascii_alphanumeric() && character != '_')
        .find(|part| !part.is_empty())
        .unwrap_or("");
    matches!(
        first,
        "if" | "for" | "while" | "switch" | "return" | "else" | "goto" | "case" | "sizeof" | "do"
    )
}

fn clean_signature(line: &str) -> String {
    let line = line.split('{').next().unwrap_or(line).trim();
    let line = line
        .split("//")
        .next()
        .unwrap_or(line)
        .trim()
        .trim_end_matches(';')
        .trim();
    line.to_owned()
}

fn declaration_type(signature: &str, word: &str) -> String {
    let prefix = word_prefix(signature, word).trim();
    let mut tokens = prefix.split_whitespace().collect::<Vec<_>>();
    while tokens.first().is_some_and(|token| {
        matches!(
            token.trim_matches('*'),
            "static" | "extern" | "volatile" | "const" | "register" | "struct"
        )
    }) {
        tokens.remove(0);
    }
    tokens.join(" ")
}

fn return_type(signature: &str, word: &str) -> String {
    let mut text = declaration_type(signature, word);
    if text.is_empty() {
        text = "void".to_owned();
    }
    text
}

/// Returns the portion of `text` before the first standalone occurrence of
/// `word`. Using `split(word)` directly would cut inside identifiers such as
/// `uint8_t` when the hovered variable is named `i`.
fn word_prefix<'a>(text: &'a str, word: &str) -> &'a str {
    if word.is_empty() {
        return "";
    }
    let mut search_from = 0;
    while let Some(found) = text[search_from..].find(word) {
        let start = search_from + found;
        let end = start + word.len();
        let before_ok = text[..start]
            .chars()
            .next_back()
            .is_none_or(|character| !is_identifier_char(character));
        let after_ok = text[end..]
            .chars()
            .next()
            .is_none_or(|character| !is_identifier_char(character));
        if before_ok && after_ok {
            return &text[..start];
        }
        search_from = end;
    }
    ""
}

fn is_identifier_char(character: char) -> bool {
    character.is_ascii_alphanumeric() || character == '_'
}

fn doc_comment(lines: &[&str], declaration: usize) -> String {
    if declaration == 0 {
        return String::new();
    }
    let previous = lines[declaration - 1].trim();
    let mut collected = Vec::new();
    if previous.ends_with("*/") {
        let mut start = declaration - 1;
        while start > 0 && !lines[start].contains("/*") {
            start -= 1;
        }
        for line in &lines[start..declaration] {
            collected.push(strip_comment_line(line));
        }
    } else if previous.starts_with("//") {
        let mut start = declaration - 1;
        while start > 0 && lines[start - 1].trim().starts_with("//") {
            start -= 1;
        }
        for line in &lines[start..declaration] {
            collected.push(line.trim().trim_start_matches("//").trim().to_owned());
        }
    } else if previous.starts_with('*') {
        let mut start = declaration - 1;
        while start > 0 && !lines[start].contains("/*") {
            start -= 1;
        }
        for line in &lines[start..declaration] {
            collected.push(strip_comment_line(line));
        }
    }
    let text = collected
        .into_iter()
        .filter(|line| !line.is_empty())
        .collect::<Vec<_>>()
        .join(" ");
    let text = text.replace("@brief", " ");
    let text = text.split("@retval").next().unwrap_or(&text);
    let text = text.split_whitespace().collect::<Vec<_>>().join(" ");
    if text.is_empty()
        || text.starts_with("USER CODE BEGIN")
        || text.starts_with("USER CODE END")
        || text
            .chars()
            .all(|character| matches!(character, '-' | '*' | '/' | ' ' | '='))
    {
        return String::new();
    }
    text
}

fn strip_comment_line(line: &str) -> String {
    line.trim()
        .trim_start_matches("/**")
        .trim_start_matches("/*")
        .trim_end_matches("*/")
        .trim_start_matches('*')
        .trim()
        .to_owned()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn builds_function_hover_with_doc_and_return_type() {
        let text = "\
/**
 * @brief The application entry point.
 * @retval int
 */
int main(void)
{
  return 0;
}";
        let info = build(Path::new("main.c"), text, "main", 6, &[]).unwrap();
        assert_eq!(info.signature, "int main(void)");
        assert!(info.doc.contains("The application entry point"));
        assert_eq!(info.type_label, "返回值");
        assert_eq!(info.type_text, "int");
    }

    #[test]
    fn builds_variable_hover() {
        let text = "int counter = 0;\n\nvoid f(void) { counter++; }";
        let info = build(Path::new("main.c"), text, "counter", 3, &[]).unwrap();
        assert_eq!(info.signature, "int counter = 0");
        assert_eq!(info.type_text, "int");
        assert_eq!(info.type_label, "类型");
    }

    #[test]
    fn extracts_type_without_cutting_inside_uint8_t() {
        let text = "/* USER CODE BEGIN 2 */\nuint8_t i = 0;\nuint8_t qi = 0;";
        let info = build(Path::new("main.c"), text, "i", 2, &[]).unwrap();
        assert_eq!(info.signature, "uint8_t i = 0");
        assert_eq!(info.type_text, "uint8_t");
        assert!(info.doc.is_empty());
    }

    #[test]
    fn uses_symbol_index_for_other_files() {
        let dir = tempfile::tempdir().unwrap();
        let header = dir.path().join("demo.h");
        std::fs::write(
            &header,
            "/** Adds two numbers. */\nint add(int a, int b);\n",
        )
        .unwrap();
        let symbols = vec![Symbol {
            name: "add".to_owned(),
            kind: SymbolKind::Function,
            path: header,
            line: 2,
            column: 5,
        }];
        let info = build(
            Path::new("main.c"),
            "int x = add(1, 2);",
            "add",
            1,
            &symbols,
        )
        .unwrap();
        assert_eq!(info.signature, "int add(int a, int b)");
        assert!(info.doc.contains("Adds two numbers"));
        assert_eq!(info.type_text, "int");
    }
}
