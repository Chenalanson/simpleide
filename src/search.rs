//! Project-wide literal / regex search with gitignore-aware traversal.

use std::path::{Path, PathBuf};

use crate::ignore::IgnoreRules;
use crate::util::read_text_file;

pub const MAX_SEARCH_RESULTS: usize = 500;
const MAX_SEARCH_FILE_BYTES: u64 = 2 * 1024 * 1024;
const MAX_SEARCH_FILES: usize = 20_000;

#[derive(Clone, Debug)]
pub struct SearchHit {
    pub path: PathBuf,
    pub line: usize,
    pub column: usize,
    pub preview: String,
}

#[derive(Clone, Debug)]
pub struct SearchOptions {
    pub query: String,
    pub case_sensitive: bool,
    pub use_regex: bool,
}

pub struct SearchOutcome {
    pub hits: Vec<SearchHit>,
    pub scanned_files: usize,
    pub truncated: bool,
}

pub fn search_project(
    root: &Path,
    ignore: &IgnoreRules,
    options: &SearchOptions,
) -> Result<SearchOutcome, String> {
    if options.query.is_empty() {
        return Ok(SearchOutcome {
            hits: Vec::new(),
            scanned_files: 0,
            truncated: false,
        });
    }
    let pattern = if options.use_regex {
        options.query.clone()
    } else {
        regex::escape(&options.query)
    };
    let regex = regex::RegexBuilder::new(&pattern)
        .case_insensitive(!options.case_sensitive)
        .build()
        .map_err(|err| err.to_string())?;

    let mut hits = Vec::new();
    let mut scanned_files = 0usize;
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
            if metadata.len() > MAX_SEARCH_FILE_BYTES {
                continue;
            }
            scanned_files += 1;
            if scanned_files > MAX_SEARCH_FILES {
                truncated = true;
                break;
            }
            let Ok(loaded) = read_text_file(&path) else {
                continue;
            };
            for (line_index, line) in loaded.text.lines().enumerate() {
                for matched in regex.find_iter(line) {
                    if matched.start() == matched.end() {
                        continue;
                    }
                    let column = line[..matched.start()].chars().count() + 1;
                    hits.push(SearchHit {
                        path: path.clone(),
                        line: line_index + 1,
                        column,
                        preview: line.trim_end().to_owned(),
                    });
                    if hits.len() >= MAX_SEARCH_RESULTS {
                        truncated = true;
                        break;
                    }
                }
                if hits.len() >= MAX_SEARCH_RESULTS {
                    break;
                }
            }
            if hits.len() >= MAX_SEARCH_RESULTS {
                break;
            }
        }
        if hits.len() >= MAX_SEARCH_RESULTS || scanned_files > MAX_SEARCH_FILES {
            break;
        }
    }
    Ok(SearchOutcome {
        hits,
        scanned_files,
        truncated,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::util::write_text_file;
    use crate::util::{LineEnding, TextEncoding};

    fn write(path: &Path, text: &str) {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).unwrap();
        }
        write_text_file(path, text, LineEnding::Lf, false, TextEncoding::Utf8).unwrap();
    }

    #[test]
    fn searches_text_and_reports_positions() {
        let dir = tempfile::tempdir().unwrap();
        write(&dir.path().join("src/main.c"), "int foo;\nint bar = foo;\n");
        write(&dir.path().join("README.md"), "nothing here\n");
        let ignore = IgnoreRules::load(dir.path(), true, "");
        let outcome = search_project(
            dir.path(),
            &ignore,
            &SearchOptions {
                query: "foo".to_owned(),
                case_sensitive: false,
                use_regex: false,
            },
        )
        .unwrap();
        assert_eq!(outcome.hits.len(), 2);
        assert_eq!(outcome.hits[0].line, 1);
        assert_eq!(outcome.hits[0].column, 5);
        assert_eq!(outcome.hits[1].line, 2);
        assert_eq!(outcome.scanned_files, 2);
    }

    #[test]
    fn respects_gitignore_and_extra_patterns() {
        let dir = tempfile::tempdir().unwrap();
        write(&dir.path().join(".gitignore"), "build/\n");
        write(&dir.path().join("build/out.c"), "foo\n");
        write(&dir.path().join("src/main.c"), "foo\n");
        write(&dir.path().join("src/main.tmp"), "foo\n");
        let ignore = IgnoreRules::load(dir.path(), true, "*.tmp");
        let outcome = search_project(
            dir.path(),
            &ignore,
            &SearchOptions {
                query: "foo".to_owned(),
                case_sensitive: false,
                use_regex: false,
            },
        )
        .unwrap();
        assert_eq!(outcome.hits.len(), 1);
        assert!(outcome.hits[0].path.ends_with("main.c"));
    }

    #[test]
    fn supports_regex_and_case_sensitivity() {
        let dir = tempfile::tempdir().unwrap();
        write(&dir.path().join("a.c"), "Foo\nfoo\nfao\n");
        let ignore = IgnoreRules::load(dir.path(), false, "");
        let outcome = search_project(
            dir.path(),
            &ignore,
            &SearchOptions {
                query: "f.o".to_owned(),
                case_sensitive: true,
                use_regex: true,
            },
        )
        .unwrap();
        assert_eq!(outcome.hits.len(), 2);
        assert_eq!(outcome.hits[0].line, 2);
        assert_eq!(outcome.hits[1].line, 3);
    }
}
