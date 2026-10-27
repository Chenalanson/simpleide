//! Lightweight C/C++/assembly symbol index used by symbol search.

use std::path::{Path, PathBuf};

use regex::Regex;

use crate::ignore::IgnoreRules;
use crate::theme;
use crate::util::read_text_file;

const MAX_SYMBOLS: usize = 2000;
const MAX_SYMBOL_FILE_BYTES: u64 = 2 * 1024 * 1024;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SymbolKind {
    Function,
    Struct,
    Macro,
    Label,
}

impl SymbolKind {
    pub fn label(self) -> &'static str {
        match self {
            Self::Function => "fn",
            Self::Struct => "struct",
            Self::Macro => "macro",
            Self::Label => "label",
        }
    }

    pub fn color(self) -> eframe::egui::Color32 {
        match self {
            Self::Function => theme::info(),
            Self::Struct => theme::accent(),
            Self::Macro => theme::warning(),
            Self::Label => theme::dir(),
        }
    }
}

#[derive(Clone, Debug)]
pub struct Symbol {
    pub name: String,
    pub kind: SymbolKind,
    pub path: PathBuf,
    pub line: usize,
    pub column: usize,
}

pub struct SymbolOutcome {
    pub symbols: Vec<Symbol>,
    pub truncated: bool,
}

pub fn scan_project(root: &Path, ignore: &IgnoreRules) -> Result<SymbolOutcome, String> {
    let function = Regex::new(
        r"^\s*(?:[A-Za-z_][\w\s\*&:<>,]*?)\b([A-Za-z_]\w*)\s*\([^;{}]*\)\s*(?:const\s*)?\{",
    )
    .map_err(|err| err.to_string())?;
    let function_next_line = Regex::new(
        r"^\s*(?:[A-Za-z_][\w\s\*&:<>,]*?)\b([A-Za-z_]\w*)\s*\([^;{}]*\)\s*(?:const\s*)?$",
    )
    .map_err(|err| err.to_string())?;
    let record =
        Regex::new(r"^\s*(?:typedef\s+)?(?:struct|union|enum|class)\s+([A-Za-z_]\w*)\s*(?:\{|$)")
            .map_err(|err| err.to_string())?;
    let macro_re = Regex::new(r"^\s*#\s*define\s+([A-Za-z_]\w*)").map_err(|err| err.to_string())?;
    let label = Regex::new(r"^\s*([A-Za-z_.$][\w.$]*):").map_err(|err| err.to_string())?;

    let mut symbols = Vec::new();
    let mut truncated = false;
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
            if !file_type.is_file() || ignore.is_ignored(relative, false) {
                continue;
            }
            let extension = path
                .extension()
                .map(|value| value.to_string_lossy().to_lowercase())
                .unwrap_or_default();
            let is_source = matches!(
                extension.as_str(),
                "c" | "h" | "cc" | "cpp" | "cxx" | "hh" | "hpp" | "hxx" | "ino"
            );
            let is_asm = matches!(extension.as_str(), "s" | "asm");
            if !is_source && !is_asm {
                continue;
            }
            let Ok(metadata) = entry.metadata() else {
                continue;
            };
            if metadata.len() > MAX_SYMBOL_FILE_BYTES {
                continue;
            }
            let Ok(loaded) = read_text_file(&path) else {
                continue;
            };
            let lines = loaded.text.lines().collect::<Vec<_>>();
            for (index, line) in lines.iter().enumerate() {
                let trimmed = line.trim_start();
                if trimmed.starts_with("//") || trimmed.starts_with('*') {
                    continue;
                }
                let mut found = None;
                if is_asm {
                    if let Some(captures) = label.captures(line) {
                        if let Some(name) = captures.get(1) {
                            found = Some((name.as_str().to_owned(), SymbolKind::Label));
                        }
                    }
                } else if let Some(captures) = macro_re.captures(line) {
                    if let Some(name) = captures.get(1) {
                        found = Some((name.as_str().to_owned(), SymbolKind::Macro));
                    }
                } else if let Some(captures) = record.captures(line) {
                    if let Some(name) = captures.get(1) {
                        found = Some((name.as_str().to_owned(), SymbolKind::Struct));
                    }
                } else if let Some(name) = function_name(&function, line) {
                    found = Some((name, SymbolKind::Function));
                } else if let Some(name) = function_name(&function_next_line, line) {
                    let next = lines[index + 1..]
                        .iter()
                        .find(|next| !next.trim().is_empty());
                    if next.is_some_and(|next| next.trim_start().starts_with('{')) {
                        found = Some((name, SymbolKind::Function));
                    }
                }
                if let Some((name, kind)) = found {
                    let column = line.find(&name).map(|index| index + 1).unwrap_or(1);
                    symbols.push(Symbol {
                        name,
                        kind,
                        path: path.clone(),
                        line: index + 1,
                        column,
                    });
                    if symbols.len() >= MAX_SYMBOLS {
                        truncated = true;
                        break;
                    }
                }
            }
            if symbols.len() >= MAX_SYMBOLS {
                break;
            }
        }
        if symbols.len() >= MAX_SYMBOLS {
            break;
        }
    }
    Ok(SymbolOutcome { symbols, truncated })
}

fn function_name(regex: &Regex, line: &str) -> Option<String> {
    if line.trim_start().starts_with('#') {
        return None;
    }
    let capture = regex.captures(line)?;
    let name = capture.get(1)?.as_str();
    if matches!(
        name,
        "if" | "for"
            | "while"
            | "switch"
            | "return"
            | "sizeof"
            | "do"
            | "else"
            | "case"
            | "defined"
            | "static_assert"
    ) {
        return None;
    }
    Some(name.to_owned())
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
    fn finds_functions_records_macros_and_labels() {
        let dir = tempfile::tempdir().unwrap();
        write(
            &dir.path().join("src/main.c"),
            "#define BUFFER_SIZE 32\nstruct Point {\n    int x;\n};\nstatic int helper(void) {\n    return 1;\n}\nint main(void)\n{\n    if (1) {\n        return helper();\n    }\n}\n",
        );
        write(
            &dir.path().join("src/startup.s"),
            "Reset_Handler:\n    b main\n",
        );
        let ignore = IgnoreRules::load(dir.path(), true, "");
        let outcome = scan_project(dir.path(), &ignore).unwrap();
        let names = outcome
            .symbols
            .iter()
            .map(|symbol| symbol.name.as_str())
            .collect::<Vec<_>>();
        assert!(names.contains(&"BUFFER_SIZE"));
        assert!(names.contains(&"Point"));
        assert!(names.contains(&"helper"));
        assert!(names.contains(&"main"));
        assert!(names.contains(&"Reset_Handler"));
        assert!(!names.contains(&"if"));
    }

    #[test]
    fn respects_ignore_rules() {
        let dir = tempfile::tempdir().unwrap();
        write(
            &dir.path().join("build/generated.c"),
            "void generated(void) {}\n",
        );
        write(&dir.path().join("src/real.c"), "void real(void) {}\n");
        let ignore = IgnoreRules::load(dir.path(), true, "build/");
        let outcome = scan_project(dir.path(), &ignore).unwrap();
        assert!(outcome.symbols.iter().any(|symbol| symbol.name == "real"));
        assert!(!outcome
            .symbols
            .iter()
            .any(|symbol| symbol.name == "generated"));
    }
}
