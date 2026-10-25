//! Lightweight macro diagnostics for C/C++/assembly files.
//!
//! Two checks are intentionally file-local:
//!
//! * The same macro name is defined twice without an intervening `#undef`.
//! * A numbered macro family (for example `PWM1PIN`, `PWM2PIN`, `PWM3PIN`)
//!   uses the exact same replacement value. This usually means a copied block
//!   was not updated and is the kind of mistake shown by repeated GPIO pin
//!   macros.
//!
//! The second check is a heuristic and therefore only a warning. Vendor
//! headers (CMSIS/HAL/Middlewares) are excluded because they legitimately
//! define many register aliases with the same value.

use std::collections::{HashMap, HashSet};
use std::path::Path;

use crate::util::strip_c_comments;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MacroIssueLevel {
    Error,
    Warning,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MacroIssue {
    /// Macro name without any function-like argument list.
    pub name: String,
    /// 1-based line of this definition.
    pub line: usize,
    /// 0-based character offset of the name inside the line.
    pub column: usize,
    /// Name length in characters.
    pub length: usize,
    pub level: MacroIssueLevel,
    pub message: String,
}

impl MacroIssue {
    pub fn description(&self) -> &str {
        &self.message
    }
}

#[derive(Clone, Debug)]
struct Definition {
    name: String,
    line: usize,
    column: usize,
    length: usize,
    value: String,
    epoch: u64,
}

/// Returns diagnostics for duplicate macro names and, when
/// `check_duplicate_values` is enabled, suspicious duplicate replacement
/// values. `path` is used only to skip vendor headers for the optional value
/// heuristic.
pub fn analyze(path: &Path, text: &str, check_duplicate_values: bool) -> Vec<MacroIssue> {
    let cleaned = strip_c_comments(text);
    let definitions = collect_definitions(&cleaned);
    let mut issues = duplicate_name_issues(&definitions);
    if check_duplicate_values && !is_vendor_path(path) {
        issues.extend(duplicate_value_issues(&definitions, &issues));
    }
    issues.sort_by_key(|issue| (issue.line, issue.column));
    issues.dedup_by(|a, b| a.line == b.line && a.name == b.name);
    issues
}

/// Compatibility helper used by tests and callers that only want the
/// same-name check.
#[cfg(test)]
pub fn duplicate_macro_definitions(text: &str) -> Vec<MacroIssue> {
    let cleaned = strip_c_comments(text);
    let definitions = collect_definitions(&cleaned);
    let mut issues = duplicate_name_issues(&definitions);
    issues.sort_by_key(|issue| (issue.line, issue.column));
    issues
}

pub fn supports_macros(path: &Path) -> bool {
    let extension = path
        .extension()
        .and_then(|value| value.to_str())
        .unwrap_or_default()
        .to_ascii_lowercase();
    matches!(
        extension.as_str(),
        "c" | "h" | "cc" | "cpp" | "cxx" | "hpp" | "hh" | "hxx" | "ino" | "s"
    )
}

fn collect_definitions(cleaned: &str) -> Vec<Definition> {
    let mut definitions = Vec::new();
    let mut epoch = 0u64;
    for (index, line) in cleaned.lines().enumerate() {
        let line_number = index + 1;
        let Some((directive, rest)) = directive(line) else {
            continue;
        };
        match directive {
            "undef" => {
                epoch = epoch.wrapping_add(1);
            }
            "define" => {
                let Some(name) = identifier(rest) else {
                    continue;
                };
                let column = line
                    .find(name)
                    .map(|offset| line[..offset].chars().count())
                    .unwrap_or(0);
                let value = normalize_value(rest.trim_start()[name.len()..].trim());
                definitions.push(Definition {
                    name: name.to_owned(),
                    line: line_number,
                    column,
                    length: name.chars().count(),
                    value,
                    epoch,
                });
            }
            _ => {}
        }
    }
    definitions
}

fn duplicate_name_issues(definitions: &[Definition]) -> Vec<MacroIssue> {
    let mut groups: HashMap<(String, u64), Vec<usize>> = HashMap::new();
    for (index, definition) in definitions.iter().enumerate() {
        groups
            .entry((definition.name.clone(), definition.epoch))
            .or_default()
            .push(index);
    }
    let mut issues = Vec::new();
    for indices in groups.values() {
        if indices.len() < 2 {
            continue;
        }
        let first = &definitions[indices[0]];
        let lines = indices
            .iter()
            .map(|index| definitions[*index].line.to_string())
            .collect::<Vec<_>>()
            .join("、");
        let message = format!(
            "宏 {} 在本文件重复定义（第 {} 行）；同一文件内只需定义一次",
            first.name, lines
        );
        for index in indices {
            let definition = &definitions[*index];
            issues.push(MacroIssue {
                name: definition.name.clone(),
                line: definition.line,
                column: definition.column,
                length: definition.length,
                level: MacroIssueLevel::Error,
                message: message.clone(),
            });
        }
    }
    issues
}

fn duplicate_value_issues(definitions: &[Definition], existing: &[MacroIssue]) -> Vec<MacroIssue> {
    let already_reported = existing
        .iter()
        .map(|issue| (issue.line, issue.name.clone()))
        .collect::<HashSet<_>>();
    let mut groups: HashMap<(String, String, u64), Vec<usize>> = HashMap::new();
    for (index, definition) in definitions.iter().enumerate() {
        if !is_numbered_family(&definition.name) {
            continue;
        }
        if definition.value.is_empty() {
            continue;
        }
        let first = definition.value.chars().next().unwrap_or_default();
        if !(first.is_ascii_alphabetic() || first == '_') {
            continue;
        }
        groups
            .entry((
                normalize_name(&definition.name),
                definition.value.clone(),
                definition.epoch,
            ))
            .or_default()
            .push(index);
    }

    let mut issues = Vec::new();
    for indices in groups.values() {
        let names = indices
            .iter()
            .map(|index| definitions[*index].name.clone())
            .collect::<HashSet<_>>();
        if names.len() < 2 {
            continue;
        }
        let description = indices
            .iter()
            .map(|index| {
                let definition = &definitions[*index];
                format!("{}（第 {} 行）", definition.name, definition.line)
            })
            .collect::<Vec<_>>()
            .join("、");
        let value = &definitions[indices[0]].value;
        let message = format!(
            "宏 {} 使用相同的替换值 {}；请确认是否复制后忘记修改",
            description, value
        );
        for index in indices {
            let definition = &definitions[*index];
            if already_reported.contains(&(definition.line, definition.name.clone())) {
                continue;
            }
            issues.push(MacroIssue {
                name: definition.name.clone(),
                line: definition.line,
                column: definition.column,
                length: definition.length,
                level: MacroIssueLevel::Warning,
                message: message.clone(),
            });
        }
    }
    issues
}

fn is_numbered_family(name: &str) -> bool {
    let normalized = normalize_name(name);
    normalized != name && normalized.chars().count() >= 3
}

fn normalize_name(name: &str) -> String {
    name.chars()
        .filter(|character| !character.is_ascii_digit())
        .collect()
}

fn normalize_value(value: &str) -> String {
    value
        .trim_end_matches('\\')
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
}

fn is_vendor_path(path: &Path) -> bool {
    let lower = path
        .to_string_lossy()
        .to_ascii_lowercase()
        .replace('\\', "/");
    [
        "/drivers/",
        "/cmsis/",
        "/middlewares/",
        "/third_party/",
        "/thirdparty/",
        "/vendor/",
        "/external/",
        "/pack/",
        "/keil/",
    ]
    .iter()
    .any(|needle| lower.contains(needle))
}

fn directive(line: &str) -> Option<(&str, &str)> {
    let trimmed = line.trim_start();
    let rest = trimmed.strip_prefix('#')?.trim_start();
    let end = rest
        .find(|character: char| !character.is_ascii_alphanumeric() && character != '_')
        .unwrap_or(rest.len());
    let keyword = &rest[..end];
    if !matches!(keyword, "define" | "undef") {
        return None;
    }
    let after = &rest[end..];
    if !after.starts_with(|character: char| character.is_whitespace()) {
        return None;
    }
    Some((keyword, after))
}

fn identifier(text: &str) -> Option<&str> {
    let text = text.trim_start();
    let end = text
        .find(|character: char| !character.is_ascii_alphanumeric() && character != '_')
        .unwrap_or(text.len());
    if end == 0 {
        return None;
    }
    let name = &text[..end];
    let mut characters = name.chars();
    let first = characters.next()?;
    if first.is_ascii_alphabetic() || first == '_' {
        Some(name)
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn finds_repeated_object_like_macros() {
        let text = "\
#define PWM1PIN GPIO_PIN0
#define PWM1PORT GPIO_D
#define PWM1PIN GPIO_PIN0
#define PWM1PORT GPIO_D
";
        let issues = duplicate_macro_definitions(text);
        assert_eq!(issues.len(), 4);
        assert_eq!(issues[0].name, "PWM1PIN");
        assert_eq!(issues[0].line, 1);
        assert_eq!(issues[0].level, MacroIssueLevel::Error);
        assert_eq!(issues[0].column, 8);
        assert_eq!(issues[0].length, 7);
    }

    #[test]
    fn reports_function_like_macros_once_per_definition() {
        let text = "\
#define CLAMP(x) ((x) < 0 ? 0 : (x))
#define CLAMP(x) ((x) > 0 ? 0 : (x))
";
        let issues = duplicate_macro_definitions(text);
        assert_eq!(issues.len(), 2);
        assert!(issues.iter().all(|issue| issue.name == "CLAMP"));
    }

    #[test]
    fn undef_closes_the_previous_definition() {
        let text = "\
#define LIMIT 10
#undef LIMIT
#define LIMIT 20
";
        assert!(duplicate_macro_definitions(text).is_empty());
    }

    #[test]
    fn warns_when_numbered_family_reuses_the_same_value() {
        let text = "\
#define PWM1PIN    GPIO_PIN0
#define PWM1PORT   GPIO_D
#define PWM2PIN    GPIO_PIN0
#define PWM2PORT   GPIO_D
#define PWM3PIN    GPIO_PIN0
#define PWM3PORT   GPIO_D
";
        let issues = analyze(Path::new("Core/Src/main.c"), text, true);
        let warnings = issues
            .iter()
            .filter(|issue| issue.level == MacroIssueLevel::Warning)
            .collect::<Vec<_>>();
        assert_eq!(warnings.len(), 6);
        assert!(warnings
            .iter()
            .all(|issue| issue.message.contains("使用相同的替换值")));

        assert!(analyze(Path::new("Core/Src/main.c"), text, false).is_empty());
    }

    #[test]
    fn vendor_headers_skip_the_value_heuristic() {
        let text = "\
#define USB_EP0R_EA_Pos (0U)
#define USB_EP1R_EA_Pos (0U)
";
        let issues = analyze(Path::new("Drivers/CMSIS/Device/stm32f103xb.h"), text, true);
        assert!(issues.is_empty());
    }

    #[test]
    fn ignores_comments_and_unrelated_directives() {
        let text = "\
/* #define VALUE1 1
#define VALUE2 2 */
// #define VALUE3 3
#define OTHER 4
#define OTHER 5
";
        let issues = duplicate_macro_definitions(text);
        assert_eq!(issues.len(), 2);
        assert!(issues.iter().all(|issue| issue.name == "OTHER"));
    }

    #[test]
    fn only_scans_c_like_files() {
        assert!(supports_macros(Path::new("main.c")));
        assert!(supports_macros(Path::new("board.hpp")));
        assert!(!supports_macros(Path::new("README.md")));
    }
}
