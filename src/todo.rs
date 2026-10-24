//! Project TODO / FIXME annotation scan.

use std::path::{Path, PathBuf};

use regex::Regex;

use crate::ignore::IgnoreRules;
use crate::theme;
use crate::util::read_text_file;

const MAX_TODO_RESULTS: usize = 500;
const MAX_TODO_FILE_BYTES: u64 = 2 * 1024 * 1024;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TodoKind {
    Todo,
    Fixme,
    Hack,
    Note,
}

impl TodoKind {
    pub fn label(self) -> &'static str {
        match self {
            Self::Todo => "TODO",
            Self::Fixme => "FIXME",
            Self::Hack => "HACK",
            Self::Note => "NOTE",
        }
    }

    pub fn color(self) -> eframe::egui::Color32 {
        match self {
            Self::Todo => theme::INFO,
            Self::Fixme => theme::ERROR,
            Self::Hack => theme::WARNING,
            Self::Note => theme::TEXT_DIM,
        }
    }
}

#[derive(Clone, Debug)]
pub struct TodoItem {
    pub path: PathBuf,
    pub line: usize,
    pub kind: TodoKind,
    pub text: String,
}

pub struct TodoOutcome {
    pub items: Vec<TodoItem>,
    pub truncated: bool,
}

pub fn scan_project(root: &Path, ignore: &IgnoreRules) -> Result<TodoOutcome, String> {
    let regex = Regex::new(r"\b(TODO|FIXME|HACK|NOTE)\b").map_err(|err| err.to_string())?;
    let mut items = Vec::new();
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
            let Ok(metadata) = entry.metadata() else {
                continue;
            };
            if metadata.len() > MAX_TODO_FILE_BYTES {
                continue;
            }
            let Ok(loaded) = read_text_file(&path) else {
                continue;
            };
            for (line_index, line) in loaded.text.lines().enumerate() {
                let Some(matched) = regex.captures(line) else {
                    continue;
                };
                let Some(keyword) = matched.get(1) else {
                    continue;
                };
                let kind = match keyword.as_str() {
                    "TODO" => TodoKind::Todo,
                    "FIXME" => TodoKind::Fixme,
                    "HACK" => TodoKind::Hack,
                    _ => TodoKind::Note,
                };
                items.push(TodoItem {
                    path: path.clone(),
                    line: line_index + 1,
                    kind,
                    text: line.trim().to_owned(),
                });
                if items.len() >= MAX_TODO_RESULTS {
                    truncated = true;
                    break;
                }
            }
            if items.len() >= MAX_TODO_RESULTS {
                break;
            }
        }
        if items.len() >= MAX_TODO_RESULTS {
            break;
        }
    }
    Ok(TodoOutcome { items, truncated })
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
    fn finds_and_classifies_annotations() {
        let dir = tempfile::tempdir().unwrap();
        write(
            &dir.path().join("src/main.c"),
            "// TODO: implement\nint x;\n// FIXME later\n// NOTE ok\n",
        );
        write(&dir.path().join("build/out.c"), "// TODO ignored\n");
        let ignore = IgnoreRules::load(dir.path(), true, "build/");
        let outcome = scan_project(dir.path(), &ignore).unwrap();
        assert_eq!(outcome.items.len(), 3);
        assert_eq!(outcome.items[0].kind, TodoKind::Todo);
        assert_eq!(outcome.items[0].line, 1);
        assert_eq!(outcome.items[1].kind, TodoKind::Fixme);
        assert_eq!(outcome.items[2].kind, TodoKind::Note);
    }

    #[test]
    fn ignores_lowercase_words_and_non_markers() {
        let dir = tempfile::tempdir().unwrap();
        write(
            &dir.path().join("a.c"),
            "int todo;\n// todolist\n// TODO list\n",
        );
        let ignore = IgnoreRules::load(dir.path(), true, "");
        let outcome = scan_project(dir.path(), &ignore).unwrap();
        assert_eq!(outcome.items.len(), 1);
        assert_eq!(outcome.items[0].line, 3);
    }
}
