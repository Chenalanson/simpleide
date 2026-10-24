//! Small gitignore-style matcher used by the explorer and project search.

use std::path::Path;

use regex::Regex;

#[derive(Clone, Debug)]
struct Rule {
    regex: Regex,
    basename: bool,
    negated: bool,
    dir_only: bool,
}

#[derive(Clone, Debug, Default)]
pub struct IgnoreRules {
    rules: Vec<Rule>,
}

impl IgnoreRules {
    pub fn load(root: &Path, respect_gitignore: bool, extra: &str) -> Self {
        let mut patterns = Vec::new();
        if respect_gitignore {
            if let Ok(text) = std::fs::read_to_string(root.join(".gitignore")) {
                patterns.extend(text.lines().map(str::to_owned));
            }
        }
        patterns.extend(extra.lines().map(str::to_owned));
        patterns.extend(extra.split(';').map(str::to_owned));
        let mut rules = Vec::new();
        for pattern in patterns {
            if let Some(rule) = Rule::parse(&pattern) {
                rules.push(rule);
            }
        }
        Self { rules }
    }

    /// `relative` is the path relative to the scanned project root.
    pub fn is_ignored(&self, relative: &Path, is_dir: bool) -> bool {
        if relative
            .components()
            .any(|component| component.as_os_str() == ".git")
        {
            return true;
        }
        let full = relative.to_string_lossy().replace('\\', "/");
        let name = relative
            .file_name()
            .map(|name| name.to_string_lossy())
            .unwrap_or_default();
        let mut ignored = false;
        for rule in &self.rules {
            if rule.dir_only && !is_dir {
                continue;
            }
            let target = if rule.basename {
                name.as_ref()
            } else {
                full.as_str()
            };
            if rule.regex.is_match(target) {
                ignored = !rule.negated;
            }
        }
        ignored
    }
}

impl Rule {
    fn parse(raw: &str) -> Option<Self> {
        let mut pattern = raw.trim();
        if pattern.is_empty() || pattern.starts_with('#') {
            return None;
        }
        let mut negated = false;
        if let Some(rest) = pattern.strip_prefix('!') {
            negated = true;
            pattern = rest.trim();
        }
        let mut dir_only = false;
        if let Some(rest) = pattern.strip_suffix('/') {
            dir_only = true;
            pattern = rest;
        }
        let anchored = pattern.starts_with('/');
        if anchored {
            pattern = &pattern[1..];
        }
        if pattern.is_empty() {
            return None;
        }
        let basename = !anchored && !pattern.contains('/');
        let source = glob_to_regex(pattern);
        let regex = Regex::new(&format!("^{source}$")).ok()?;
        Some(Self {
            regex,
            basename,
            negated,
            dir_only,
        })
    }
}

fn glob_to_regex(pattern: &str) -> String {
    let mut result = String::with_capacity(pattern.len() + 8);
    let mut chars = pattern.chars().peekable();
    while let Some(ch) = chars.next() {
        match ch {
            '*' => {
                if chars.peek() == Some(&'*') {
                    chars.next();
                    if chars.peek() == Some(&'/') {
                        chars.next();
                        result.push_str("(?:.*/)?");
                    } else {
                        result.push_str(".*");
                    }
                } else {
                    result.push_str("[^/]*");
                }
            }
            '?' => result.push_str("[^/]"),
            '/' => result.push('/'),
            _ => result.push_str(&regex::escape(&ch.to_string())),
        }
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rules(text: &str) -> IgnoreRules {
        let mut rules = Vec::new();
        for pattern in text.lines() {
            if let Some(rule) = Rule::parse(pattern) {
                rules.push(rule);
            }
        }
        IgnoreRules { rules }
    }

    #[test]
    fn ignores_build_directories_at_any_level() {
        let rules = rules("build/\n");
        assert!(rules.is_ignored(Path::new("build"), true));
        assert!(rules.is_ignored(Path::new("Core/build"), true));
        assert!(!rules.is_ignored(Path::new("build"), false));
        assert!(!rules.is_ignored(Path::new("Core/build/out.o"), false));
    }

    #[test]
    fn matches_basenames_and_full_paths() {
        let rules = rules("*.log\nsrc/generated/*.c\n");
        assert!(rules.is_ignored(Path::new("logs/app.log"), false));
        assert!(rules.is_ignored(Path::new("src/generated/table.c"), false));
        assert!(!rules.is_ignored(Path::new("src/generated/table.h"), false));
    }

    #[test]
    fn negation_restores_a_file() {
        let rules = rules("build/\n!build/keep.txt\n");
        assert!(rules.is_ignored(Path::new("build"), true));
        assert!(!rules.is_ignored(Path::new("build/keep.txt"), false));
    }

    #[test]
    fn double_star_matches_nested_paths() {
        let rules = rules("**/generated/*.c\n");
        assert!(rules.is_ignored(Path::new("generated/a.c"), false));
        assert!(rules.is_ignored(Path::new("Core/generated/a.c"), false));
        assert!(!rules.is_ignored(Path::new("Core/generated/a.h"), false));
    }

    #[test]
    fn comments_and_blank_lines_are_skipped() {
        let rules = rules("# comment\n\n");
        assert!(rules.rules.is_empty());
    }

    #[test]
    fn git_directory_is_always_ignored() {
        let rules = rules("");
        assert!(rules.is_ignored(Path::new(".git"), true));
        assert!(rules.is_ignored(Path::new(".git/HEAD"), false));
    }
}
