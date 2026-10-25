//! Lightweight C memory/array safety scanner.
//!
//! This is intentionally heuristic: it looks for definite constant-index array
//! overflows, unsafe string copies, double free, suspicious malloc sizes and
//! returning local arrays. It complements (and never replaces) address
//! sanitizers or runtime watchpoints.

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};

use regex::Regex;

use crate::ignore::IgnoreRules;
use crate::util::read_text_file;

const MAX_FILE_BYTES: u64 = 2 * 1024 * 1024;
const MAX_FILES: usize = 2000;

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum Severity {
    Error,
    Warning,
    Info,
}

#[derive(Clone, Debug)]
pub struct Finding {
    pub path: PathBuf,
    pub line: usize,
    pub severity: Severity,
    pub category: &'static str,
    pub message: String,
    pub snippet: String,
}

#[derive(Clone, Debug, Default)]
pub struct Report {
    pub findings: Vec<Finding>,
    pub files: usize,
    pub arrays: usize,
}

pub fn scan_project(root: &Path, ignore: &IgnoreRules) -> Result<Report, String> {
    let files = collect_files(root, ignore);
    let mut report = Report::default();
    for path in files {
        let Ok(metadata) = std::fs::metadata(&path) else {
            continue;
        };
        if metadata.len() > MAX_FILE_BYTES {
            continue;
        }
        let Ok(loaded) = read_text_file(&path) else {
            continue;
        };
        report.files += 1;
        scan_file(&path, &loaded.text, &mut report);
    }
    report.findings.sort_by(|a, b| {
        a.path
            .cmp(&b.path)
            .then_with(|| a.line.cmp(&b.line))
            .then_with(|| a.message.cmp(&b.message))
    });
    report
        .findings
        .dedup_by(|a, b| a.path == b.path && a.line == b.line && a.message == b.message);
    Ok(report)
}

fn collect_files(root: &Path, ignore: &IgnoreRules) -> Vec<PathBuf> {
    let mut files = Vec::new();
    let mut stack = vec![root.to_path_buf()];
    while let Some(dir) = stack.pop() {
        let Ok(read_dir) = std::fs::read_dir(&dir) else {
            continue;
        };
        let mut entries = read_dir.flatten().collect::<Vec<_>>();
        entries.sort_by_key(|entry| entry.file_name());
        for entry in entries {
            if files.len() >= MAX_FILES {
                return files;
            }
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
                .and_then(|value| value.to_str())
                .unwrap_or_default()
                .to_ascii_lowercase();
            if file_type.is_file()
                && matches!(extension.as_str(), "c" | "h" | "cpp" | "hpp")
                && !ignore.is_ignored(relative, false)
            {
                files.push(path);
            }
        }
    }
    files
}

fn scan_file(path: &Path, text: &str, report: &mut Report) {
    let clean = sanitize(text);
    let no_comments = strip_comments(text);
    let macros = collect_macros(&clean);
    let mut declarations = HashSet::new();
    let arrays = collect_arrays(&clean, &macros, &mut declarations);
    report.arrays += arrays.len();

    let mut used_arrays = HashSet::new();
    for (name, array) in &arrays {
        if array.size == 0 {
            report.findings.push(Finding {
                path: path.to_path_buf(),
                line: array.line,
                severity: Severity::Error,
                category: "数组声明",
                message: format!("数组 {name} 的大小为 0"),
                snippet: line_text(text, array.line),
            });
        }
        let use_regex = Regex::new(&format!(
            r"\b{}\s*\[\s*([^\]\n]+?)\s*\]",
            regex::escape(name)
        ))
        .ok();
        if let Some(regex) = &use_regex {
            for captures in regex.captures_iter(&clean) {
                let Some(whole) = captures.get(0) else {
                    continue;
                };
                if declarations.contains(&whole.start()) {
                    continue;
                }
                let Some(expr) = captures.get(1) else {
                    continue;
                };
                let Some(index) = eval_int(expr.as_str(), &macros) else {
                    continue;
                };
                if index < 0 || index as u64 >= array.size {
                    used_arrays.insert(name.clone());
                    report.findings.push(Finding {
                        path: path.to_path_buf(),
                        line: line_of(&clean, whole.start()),
                        severity: Severity::Error,
                        category: "数组越界",
                        message: format!(
                            "{name}[{}] 越界：合法下标为 0..{}",
                            expr.as_str().trim(),
                            array.size.saturating_sub(1)
                        ),
                        snippet: line_text(text, line_of(&clean, whole.start())),
                    });
                }
            }
        }

        if let Ok(regex) = Regex::new(&format!(r"\breturn\s+{}\s*;", regex::escape(name))) {
            if let Some(matched) = regex.find(&clean) {
                report.findings.push(Finding {
                    path: path.to_path_buf(),
                    line: line_of(&clean, matched.start()),
                    severity: Severity::Warning,
                    category: "悬垂指针",
                    message: format!("返回数组 {name}；若它是局部数组，函数返回后会悬垂"),
                    snippet: line_text(text, line_of(&clean, matched.start())),
                });
            }
        }
    }

    scan_unsafe_calls(path, text, &no_comments, &arrays, report);
    scan_double_free(path, &clean, text, report);
    scan_malloc(path, &clean, text, report);
    let _ = used_arrays;
}

struct ArrayInfo {
    size: u64,
    line: usize,
}

fn collect_arrays(
    clean: &str,
    macros: &HashMap<String, i64>,
    declarations: &mut HashSet<usize>,
) -> HashMap<String, ArrayInfo> {
    let Ok(regex) = Regex::new(
        r"(?m)\b(?:char|signed\s+char|unsigned\s+char|short|unsigned\s+short|int|unsigned\s+int|long|unsigned\s+long|float|double|u?int(?:8|16|32|64)_t|size_t|[A-Za-z_]\w*_t)\s+([A-Za-z_]\w*)\s*\[\s*([A-Za-z_0-9xX\s()+\-*/]+?)\s*\]",
    ) else {
        return HashMap::new();
    };
    let mut arrays = HashMap::new();
    for captures in regex.captures_iter(clean) {
        let Some(name) = captures.get(1) else {
            continue;
        };
        let Some(size) = captures
            .get(2)
            .and_then(|value| eval_int(value.as_str(), macros))
        else {
            continue;
        };
        if size < 0 {
            continue;
        }
        let name_pos = captures.get(1).map(|value| value.start()).unwrap_or(0);
        declarations.insert(name_pos);
        arrays.insert(
            name.as_str().to_owned(),
            ArrayInfo {
                size: size as u64,
                line: line_of(clean, name_pos),
            },
        );
    }
    arrays
}

fn scan_unsafe_calls(
    path: &Path,
    text: &str,
    no_comments: &str,
    arrays: &HashMap<String, ArrayInfo>,
    report: &mut Report,
) {
    let Ok(call_regex) = Regex::new(
        r"(?s)\b(strcpy|strcat|strncpy|memcpy|memset|sprintf|gets)\s*\(\s*([A-Za-z_]\w*)\s*,",
    ) else {
        return;
    };
    for captures in call_regex.captures_iter(no_comments) {
        let Some(call) = captures.get(1).map(|value| value.as_str()) else {
            continue;
        };
        let Some(dest) = captures.get(2).map(|value| value.as_str()) else {
            continue;
        };
        let Some(array) = arrays.get(dest) else {
            continue;
        };
        let Some(whole) = captures.get(0) else {
            continue;
        };
        let line = line_of(no_comments, whole.start());
        let rest = &no_comments[whole.end()..];
        let args = rest.split(';').next().unwrap_or("");
        let message = match call {
            "gets" => Some((Severity::Error, "gets 不检查目标长度，必须改用 fgets")),
            "sprintf" => Some((
                Severity::Warning,
                "sprintf 不检查目标长度，建议使用 snprintf",
            )),
            "strcpy" => literal_len(args).and_then(|length| {
                (length + 1 > array.size).then_some((
                    Severity::Error,
                    "strcpy 的字符串长度超过目标数组（含结尾 \\0）",
                ))
            }),
            "strcat" => literal_len(args).and_then(|length| {
                (length + 1 > array.size).then_some((
                    Severity::Warning,
                    "strcat 追加后可能超过目标数组，建议使用 strncat",
                ))
            }),
            "strncpy" | "memcpy" | "memset" => third_arg_len(args).and_then(|length| {
                (length > array.size).then_some((Severity::Error, "常量复制长度大于目标数组大小"))
            }),
            _ => None,
        };
        if let Some((severity, message)) = message {
            report.findings.push(Finding {
                path: path.to_path_buf(),
                line,
                severity,
                category: "危险函数",
                message: format!("{call}({dest}, ...)：{message}"),
                snippet: line_text(text, line),
            });
        }
    }
}

fn scan_double_free(path: &Path, clean: &str, text: &str, report: &mut Report) {
    let Ok(regex) = Regex::new(r"\bfree\s*\(\s*([A-Za-z_]\w*)\s*\)") else {
        return;
    };
    let mut seen = HashSet::new();
    for captures in regex.captures_iter(clean) {
        let Some(name) = captures.get(1) else {
            continue;
        };
        let key = name.as_str().to_owned();
        if !seen.insert(key.clone()) {
            let line = line_of(
                clean,
                captures.get(0).map(|value| value.start()).unwrap_or(0),
            );
            report.findings.push(Finding {
                path: path.to_path_buf(),
                line,
                severity: Severity::Warning,
                category: "重复释放",
                message: format!("{key} 被 free 多次；第二次 free 是未定义行为"),
                snippet: line_text(text, line),
            });
        }
    }
}

fn scan_malloc(path: &Path, clean: &str, text: &str, report: &mut Report) {
    let Ok(regex) =
        Regex::new(r"\b([A-Za-z_]\w*)\s*\*\s*([A-Za-z_]\w*)\s*=\s*malloc\s*\(\s*([0-9]+)\s*\)")
    else {
        return;
    };
    for captures in regex.captures_iter(clean) {
        let Some(size) = captures
            .get(3)
            .and_then(|value| value.as_str().parse::<u64>().ok())
        else {
            continue;
        };
        let line = line_of(
            clean,
            captures.get(0).map(|value| value.start()).unwrap_or(0),
        );
        let severity = if size == 0 {
            Severity::Warning
        } else {
            Severity::Info
        };
        report.findings.push(Finding {
            path: path.to_path_buf(),
            line,
            severity,
            category: "堆分配",
            message: format!(
                "malloc({size}) 只分配字节数；若要存放多个元素请乘以 sizeof(元素类型)"
            ),
            snippet: line_text(text, line),
        });
    }
}

fn literal_len(args: &str) -> Option<u64> {
    let start = args.find('"')?;
    let rest = &args[start + 1..];
    let mut escaped = false;
    let mut length = 0u64;
    for character in rest.chars() {
        if escaped {
            escaped = false;
            length += 1;
            continue;
        }
        match character {
            '\\' => escaped = true,
            '"' => return Some(length),
            _ => length += 1,
        }
    }
    None
}

fn third_arg_len(args: &str) -> Option<u64> {
    let parts = args.split(',').collect::<Vec<_>>();
    parts.get(2)?.trim().parse::<u64>().ok()
}

fn collect_macros(clean: &str) -> HashMap<String, i64> {
    let Ok(regex) =
        Regex::new(r"(?m)^\s*#\s*define\s+([A-Za-z_]\w*)\s+([0-9]+|0[xX][0-9A-Fa-f]+)\s*$")
    else {
        return HashMap::new();
    };
    regex
        .captures_iter(clean)
        .filter_map(|captures| {
            let name = captures.get(1)?.as_str().to_owned();
            let value = eval_int(captures.get(2)?.as_str(), &HashMap::new())?;
            Some((name, value))
        })
        .collect()
}

fn eval_int(value: &str, macros: &HashMap<String, i64>) -> Option<i64> {
    let value = value.trim().trim_matches(|character: char| {
        character.is_whitespace() || character == '(' || character == ')'
    });
    if value.is_empty() {
        return None;
    }
    if let Some(rest) = value.strip_prefix('-') {
        return Some(-eval_int(rest, macros)?);
    }
    if let Some(rest) = value.strip_prefix('+') {
        return eval_int(rest, macros);
    }
    if let Some((left, right)) = value.split_once('*') {
        return Some(eval_int(left, macros)? * eval_int(right, macros)?);
    }
    if let Some((left, right)) = value.split_once('+') {
        return Some(eval_int(left, macros)? + eval_int(right, macros)?);
    }
    if let Some((left, right)) = value.split_once('-') {
        return Some(eval_int(left, macros)? - eval_int(right, macros)?);
    }
    if let Some(hex) = value
        .strip_prefix("0x")
        .or_else(|| value.strip_prefix("0X"))
    {
        return i64::from_str_radix(hex, 16).ok();
    }
    if let Ok(number) = value.parse::<i64>() {
        return Some(number);
    }
    macros.get(value).copied()
}

fn line_of(text: &str, index: usize) -> usize {
    text[..index.min(text.len())]
        .bytes()
        .filter(|byte| *byte == b'\n')
        .count()
        + 1
}

fn line_text(text: &str, line: usize) -> String {
    text.lines()
        .nth(line.saturating_sub(1))
        .map(|value| value.trim().to_owned())
        .unwrap_or_default()
}

/// Remove comments and string/char literal contents while preserving line
/// numbers. This prevents `arr[999]` inside a comment or string from being
/// reported as code.
fn sanitize(text: &str) -> String {
    let chars = text.chars().collect::<Vec<_>>();
    let mut output = String::with_capacity(text.len());
    let mut index = 0;
    while index < chars.len() {
        let character = chars[index];
        if character == '/' && chars.get(index + 1) == Some(&'/') {
            while index < chars.len() && chars[index] != '\n' {
                index += 1;
            }
            continue;
        }
        if character == '/' && chars.get(index + 1) == Some(&'*') {
            index += 2;
            while index < chars.len() {
                if chars[index] == '*' && chars.get(index + 1) == Some(&'/') {
                    index += 2;
                    break;
                }
                if chars[index] == '\n' {
                    output.push('\n');
                }
                index += 1;
            }
            continue;
        }
        if character == '"' || character == '\'' {
            output.push(' ');
            index += 1;
            while index < chars.len() {
                if chars[index] == '\\' {
                    index += 2;
                    continue;
                }
                if chars[index] == character {
                    index += 1;
                    break;
                }
                if chars[index] == '\n' {
                    output.push('\n');
                }
                index += 1;
            }
            output.push(' ');
            continue;
        }
        output.push(character);
        index += 1;
    }
    output
}

fn strip_comments(text: &str) -> String {
    let chars = text.chars().collect::<Vec<_>>();
    let mut output = String::with_capacity(text.len());
    let mut index = 0;
    let mut in_string = false;
    let mut in_char = false;
    while index < chars.len() {
        let character = chars[index];
        if in_string || in_char {
            output.push(character);
            if character == '\\' {
                if let Some(next) = chars.get(index + 1) {
                    output.push(*next);
                    index += 2;
                    continue;
                }
            }
            if (in_string && character == '"') || (in_char && character == '\'') {
                in_string = false;
                in_char = false;
            }
            index += 1;
            continue;
        }
        if character == '"' {
            in_string = true;
            output.push(character);
            index += 1;
            continue;
        }
        if character == '\'' {
            in_char = true;
            output.push(character);
            index += 1;
            continue;
        }
        if character == '/' && chars.get(index + 1) == Some(&'/') {
            while index < chars.len() && chars[index] != '\n' {
                index += 1;
            }
            continue;
        }
        if character == '/' && chars.get(index + 1) == Some(&'*') {
            index += 2;
            while index < chars.len() {
                if chars[index] == '*' && chars.get(index + 1) == Some(&'/') {
                    index += 2;
                    break;
                }
                if chars[index] == '\n' {
                    output.push('\n');
                }
                index += 1;
            }
            continue;
        }
        output.push(character);
        index += 1;
    }
    output
}

#[cfg(test)]
mod tests {
    use super::*;

    fn scan(source: &str) -> Report {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("main.c"), source).unwrap();
        scan_project(dir.path(), &IgnoreRules::default()).unwrap()
    }

    #[test]
    fn finds_constant_array_overflow_and_safe_use() {
        let report =
            scan("int main(void) { int values[4]; values[3] = 1; values[4] = 2; return 0; }");
        assert!(report
            .findings
            .iter()
            .any(|finding| finding.category == "数组越界" && finding.line == 1));
        assert_eq!(
            report
                .findings
                .iter()
                .filter(|finding| finding.category == "数组越界")
                .count(),
            1
        );
    }

    #[test]
    fn finds_strcpy_overflow_and_sprintf() {
        let report = scan(
            r#"
void f(void) {
  char buffer[4];
  strcpy(buffer, "hello");
  sprintf(buffer, "%d", 1);
}
"#,
        );
        assert!(report.findings.iter().any(|finding| {
            finding.category == "危险函数" && finding.message.contains("strcpy")
        }));
        assert!(report
            .findings
            .iter()
            .any(|finding| finding.message.contains("sprintf")));
    }

    #[test]
    fn ignores_comments_and_strings() {
        let report = scan(
            r#"
/* int bad[2]; bad[9] = 1; */
void f(void) {
  const char *message = "values[99]";
  (void)message;
}
"#,
        );
        assert!(report.findings.is_empty());
    }

    #[test]
    fn finds_double_free() {
        let report = scan("void f(void *p) { free(p); free(p); }");
        assert!(report
            .findings
            .iter()
            .any(|finding| finding.category == "重复释放"));
    }
}
