//! Git integration through the system `git` executable.

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};
use std::process::Command;

use eframe::egui::Color32;

use crate::theme;

#[derive(Clone, Debug)]
pub struct GitFile {
    pub path: PathBuf,
    pub key: String,
    pub code: String,
}

#[derive(Clone, Debug, Default)]
pub struct GitStatus {
    pub is_repo: bool,
    pub branch: String,
    pub files: Vec<GitFile>,
    pub error: Option<String>,
}

impl GitStatus {
    pub fn load(root: &Path) -> Self {
        let mut command = Command::new("git");
        command
            .arg("-C")
            .arg(root)
            .args([
                "status",
                "--porcelain=v1",
                "-z",
                "--branch",
                "--untracked-files=all",
            ])
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped());
        crate::util::hide_console(&mut command);
        let output = match command.output() {
            Ok(output) => output,
            Err(err) => {
                return Self {
                    is_repo: false,
                    error: Some(format!("无法执行 git：{err}")),
                    ..Self::default()
                };
            }
        };
        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr).trim().to_owned();
            return Self {
                is_repo: false,
                error: Some(if stderr.is_empty() {
                    "当前工程不是 Git 仓库".to_owned()
                } else {
                    stderr
                }),
                ..Self::default()
            };
        }
        let text = String::from_utf8_lossy(&output.stdout);
        Self::parse_with_prefix(&text, root, &repo_prefix(root))
    }

    fn parse_with_prefix(text: &str, root: &Path, prefix: &str) -> Self {
        let mut status = Self {
            is_repo: true,
            ..Self::default()
        };
        let mut items = text.split('\0').filter(|item| !item.is_empty()).peekable();
        while let Some(item) = items.next() {
            if let Some(branch) = item.strip_prefix("## ") {
                status.branch = branch
                    .split("...")
                    .next()
                    .unwrap_or(branch)
                    .trim()
                    .to_owned();
                continue;
            }
            if item.len() < 4 {
                continue;
            }
            let code = item[..2].to_owned();
            let mut path = item[3..].to_owned();
            if code.starts_with('R') || code.starts_with('C') {
                let _ = items.next();
            }
            if !prefix.is_empty() {
                let Some(relative) = path.strip_prefix(prefix) else {
                    continue;
                };
                path = relative.to_owned();
            }
            let path = root.join(path);
            status.files.push(GitFile {
                key: crate::util::project_key(&path),
                path,
                code,
            });
        }
        status
    }
}

fn repo_prefix(root: &Path) -> String {
    let mut command = Command::new("git");
    command
        .arg("-C")
        .arg(root)
        .args(["rev-parse", "--show-prefix"])
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::null());
    crate::util::hide_console(&mut command);
    let Ok(output) = command.output() else {
        return String::new();
    };
    if !output.status.success() {
        return String::new();
    }
    String::from_utf8_lossy(&output.stdout).trim().to_owned()
}

pub fn badge_for_code(code: &str) -> Option<char> {
    let mut chars = code.chars();
    let index = chars.next().unwrap_or(' ');
    let worktree = chars.next().unwrap_or(' ');
    if code == "??" {
        return Some('U');
    }
    if code.contains('U') || code == "AA" || code == "DD" {
        return Some('!');
    }
    let ch = if worktree != ' ' { worktree } else { index };
    match ch {
        'M' | 'T' => Some('M'),
        'A' => Some('A'),
        'D' => Some('D'),
        'R' => Some('R'),
        'C' => Some('C'),
        ' ' => None,
        other => Some(other),
    }
}

pub fn badge_color(badge: char) -> Color32 {
    match badge {
        'A' | 'U' => theme::success(),
        'M' => theme::warning(),
        'D' => theme::error(),
        'R' | 'C' => theme::info(),
        '!' => theme::accent_dark(),
        _ => theme::text_dim(),
    }
}

/// Line numbers added or modified in `path` relative to `HEAD`.
pub fn changed_lines(root: &Path, path: &Path) -> BTreeSet<usize> {
    let relative = path.strip_prefix(root).unwrap_or(path);
    let mut command = Command::new("git");
    command
        .arg("-C")
        .arg(root)
        .args(["diff", "HEAD", "--unified=0", "--no-color", "--"])
        .arg(relative)
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::null());
    crate::util::hide_console(&mut command);
    let Ok(output) = command.output() else {
        return BTreeSet::new();
    };
    if !output.status.success() {
        return BTreeSet::new();
    }
    parse_diff_lines(&String::from_utf8_lossy(&output.stdout))
}

fn parse_diff_lines(diff: &str) -> BTreeSet<usize> {
    let mut lines = BTreeSet::new();
    let mut new_line = 0usize;
    let mut in_hunk = false;
    for line in diff.lines() {
        if line.starts_with("@@") {
            in_hunk = true;
            new_line = line
                .split('+')
                .nth(1)
                .and_then(|part| part.split([',', ' ']).next())
                .and_then(|value| value.parse::<usize>().ok())
                .unwrap_or(0);
            continue;
        }
        if !in_hunk {
            continue;
        }
        if line.starts_with('+') {
            if new_line > 0 {
                lines.insert(new_line);
            }
            new_line += 1;
        } else if line.starts_with('-') {
            // Deleted line: no new-file line number.
        } else if line.starts_with(' ') {
            new_line += 1;
        } else if line.starts_with("@@") {
            continue;
        }
    }
    lines
}

/// Group file statuses for quick tree lookups.
pub fn status_badges(status: &GitStatus) -> BTreeMap<String, char> {
    status
        .files
        .iter()
        .filter_map(|file| badge_for_code(&file.code).map(|badge| (file.key.clone(), badge)))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_porcelain_status_and_branch() {
        let root = Path::new(r"D:\project");
        let text = "## main...origin/main\0 M src/main.c\0?? new.c\0R  src/moved.c\0old.c\0";
        let status = GitStatus::parse_with_prefix(text, root, "");
        assert!(status.is_repo);
        assert_eq!(status.branch, "main");
        assert_eq!(status.files.len(), 3);
        let badges = status_badges(&status);
        assert_eq!(
            badges.get(&crate::util::project_key(Path::new(
                r"D:\project\src\main.c"
            ))),
            Some(&'M')
        );
        assert_eq!(
            badges.get(&crate::util::project_key(Path::new(r"D:\project\new.c"))),
            Some(&'U')
        );
        assert_eq!(
            badges.get(&crate::util::project_key(Path::new(
                r"D:\project\src\moved.c"
            ))),
            Some(&'R')
        );
    }

    #[test]
    fn filters_subdirectory_repository_paths() {
        let text = "## main\0 M sub/src/a.c\0 M other.c\0?? sub/new.c\0";
        let status = GitStatus::parse_with_prefix(text, Path::new(r"D:\project\sub"), "sub/");
        assert_eq!(status.files.len(), 2);
        assert!(status.files[0].path.ends_with(Path::new("sub/src/a.c")));
        assert!(status.files[1].path.ends_with(Path::new("sub/new.c")));
    }

    #[test]
    fn parses_changed_line_numbers() {
        let diff = "diff --git a/main.c b/main.c\nindex 1..2 100644\n--- a/main.c\n+++ b/main.c\n@@ -1,0 +2,2 @@\n+int a;\n+int b;\n@@ -8 +10 @@\n-old\n+new\n";
        let lines = parse_diff_lines(diff);
        assert_eq!(lines.into_iter().collect::<Vec<_>>(), vec![2, 3, 10]);
    }

    #[test]
    fn error_output_is_reported() {
        let dir = tempfile::tempdir().unwrap();
        let status = GitStatus::load(dir.path());
        assert!(!status.is_repo);
        assert!(status.error.is_some());
    }
}
