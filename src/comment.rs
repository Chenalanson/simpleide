//! Line-comment toggling for the code editor.
//!
//! The editor stores text as UTF-8 but cursor positions are character indices,
//! so all offsets in this module are character counts, not bytes.

use std::path::Path;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ToggleResult {
    pub text: String,
    pub start: usize,
    pub end: usize,
    pub changed: bool,
}

/// Pick a line-comment marker for the file. Unknown text formats default to
/// `//` because most code files in the editor are C-like.
pub fn line_comment_token(path: &Path, language: &str) -> Option<&'static str> {
    let name = path
        .file_name()
        .map(|name| name.to_string_lossy().to_ascii_lowercase())
        .unwrap_or_default();
    let extension = path
        .extension()
        .map(|extension| extension.to_string_lossy().to_ascii_lowercase())
        .unwrap_or_default();

    if matches!(
        name.as_str(),
        "makefile"
            | "gnumakefile"
            | "cmakelists.txt"
            | "dockerfile"
            | "containerfile"
            | ".gitignore"
            | ".dockerignore"
            | ".env"
    ) || matches!(extension.as_str(), "mk" | "cmake")
    {
        return Some("#");
    }

    match extension.as_str() {
        "py" | "pyw" | "sh" | "bash" | "zsh" | "fish" | "ksh" | "yaml" | "yml" | "toml" | "rb"
        | "pl" | "pm" | "r" | "ps1" | "psm1" | "tcl" | "awk" | "nim" | "cr" | "ex" | "exs" => {
            Some("#")
        }
        "sql" | "lua" | "hs" | "lhs" | "vhd" | "vhdl" | "vho" | "ada" | "adb" | "ads" => Some("--"),
        "asm" | "s" => Some(";"),
        "f" | "for" | "f90" | "f95" | "f03" | "f08" => Some("!"),
        "bat" | "cmd" => Some("REM"),
        "html" | "htm" | "xml" | "svg" | "md" | "markdown" | "mdx" | "css" | "scss" | "less"
        | "json" | "ld" | "txt" => None,
        "c" | "h" | "cc" | "cpp" | "cxx" | "hpp" | "hh" | "hxx" | "m" | "mm" | "cs" | "java"
        | "kt" | "kts" | "rs" | "go" | "js" | "jsx" | "ts" | "tsx" | "swift" | "php" | "dart"
        | "scala" | "groovy" | "gradle" | "proto" | "glsl" | "vert" | "frag" | "cu" | "cuh"
        | "ino" | "pde" | "zig" | "v" | "sv" | "svh" | "jsonc" => Some("//"),
        _ => match language.to_ascii_lowercase().as_str() {
            language
                if language.contains("python")
                    || language.contains("ruby")
                    || language.contains("perl")
                    || language.contains("shell")
                    || language.contains("bash")
                    || language.contains("powershell")
                    || language.contains("makefile")
                    || language.contains("cmake")
                    || language.contains("yaml")
                    || language.contains("toml") =>
            {
                Some("#")
            }
            language
                if language.contains("sql")
                    || language.contains("lua")
                    || language.contains("haskell")
                    || language.contains("vhdl")
                    || language.contains("ada") =>
            {
                Some("--")
            }
            language if language.contains("assembly") || language.contains("assembler") => {
                Some(";")
            }
            language if language.contains("fortran") => Some("!"),
            language
                if language.contains("html")
                    || language.contains("xml")
                    || language.contains("css")
                    || language.contains("markdown")
                    || language.contains("json") =>
            {
                None
            }
            _ => Some("//"),
        },
    }
}

/// Toggle line comments over `[start, end)`. When every non-empty line is
/// already commented the markers are removed; otherwise markers are added to
/// the uncommented lines. Blank lines and lines that already have a marker are
/// left untouched in add mode.
pub fn toggle(text: &str, start: usize, end: usize, token: &str) -> ToggleResult {
    let total = text.chars().count();
    let mut start = start.min(total);
    let mut end = end.min(total);
    if start > end {
        std::mem::swap(&mut start, &mut end);
    }
    if token.is_empty() {
        return unchanged(text, start, end);
    }

    let lines = text.split('\n').collect::<Vec<_>>();
    let mut starts = Vec::with_capacity(lines.len());
    let mut offset = 0usize;
    for line in &lines {
        starts.push(offset);
        offset += line.chars().count() + 1;
    }

    let start_line = line_index(&starts, start);
    let mut end_line = line_index(&starts, end);
    // A selection that ends exactly at the next line's start does not include
    // that line, matching the usual editor behavior.
    if end > start && end_line > start_line && starts[end_line] == end {
        end_line -= 1;
    }

    let mut any_content = false;
    let mut all_commented = true;
    for line in &lines[start_line..=end_line] {
        let indent = leading_whitespace_byte(line);
        let rest = &line[indent..];
        if rest.is_empty() {
            continue;
        }
        any_content = true;
        if !starts_with_token(rest, token) {
            all_commented = false;
        }
    }
    if !any_content {
        return unchanged(text, start, end);
    }

    let remove = all_commented;
    let mut output = Vec::with_capacity(lines.len());
    let mut edits = Vec::new();
    for (index, line) in lines.iter().enumerate() {
        if index < start_line || index > end_line {
            output.push((*line).to_owned());
            continue;
        }
        let indent_byte = leading_whitespace_byte(line);
        let rest = &line[indent_byte..];
        if rest.is_empty() {
            output.push((*line).to_owned());
            continue;
        }
        let indent_chars = line[..indent_byte].chars().count();
        if remove {
            let mut remove_bytes = token.len();
            if rest
                .get(remove_bytes..)
                .is_some_and(|after| after.starts_with(' ') || after.starts_with('\t'))
            {
                remove_bytes += 1;
            }
            let removed_chars = rest[..remove_bytes].chars().count();
            let mut new_line = String::with_capacity(line.len());
            new_line.push_str(&line[..indent_byte]);
            new_line.push_str(&rest[remove_bytes..]);
            output.push(new_line);
            edits.push((starts[index], indent_chars, -(removed_chars as isize)));
        } else if starts_with_token(rest, token) {
            output.push((*line).to_owned());
        } else {
            let insert = format!("{token} ");
            let delta = insert.chars().count() as isize;
            let mut new_line = String::with_capacity(line.len() + insert.len());
            new_line.push_str(&line[..indent_byte]);
            new_line.push_str(&insert);
            new_line.push_str(rest);
            output.push(new_line);
            edits.push((starts[index], indent_chars, delta));
        }
    }

    if edits.is_empty() {
        return unchanged(text, start, end);
    }

    let new_text = output.join("\n");
    let new_len = new_text.chars().count();
    let mut new_start = start as isize;
    let mut new_end = end as isize;
    for (line_start, column, delta) in &edits {
        let position = (*line_start + *column) as isize;
        if position <= start as isize {
            new_start += delta;
        }
        if position <= end as isize {
            new_end += delta;
        }
    }
    ToggleResult {
        text: new_text,
        start: new_start.clamp(0, new_len as isize) as usize,
        end: new_end.clamp(0, new_len as isize) as usize,
        changed: true,
    }
}

fn unchanged(text: &str, start: usize, end: usize) -> ToggleResult {
    ToggleResult {
        text: text.to_owned(),
        start,
        end,
        changed: false,
    }
}

fn line_index(starts: &[usize], index: usize) -> usize {
    match starts.binary_search(&index) {
        Ok(line) => line,
        Err(0) => 0,
        Err(line) => line - 1,
    }
    .min(starts.len().saturating_sub(1))
}

fn leading_whitespace_byte(line: &str) -> usize {
    line.char_indices()
        .find(|(_, ch)| !matches!(ch, ' ' | '\t'))
        .map(|(index, _)| index)
        .unwrap_or(line.len())
}

fn starts_with_token(rest: &str, token: &str) -> bool {
    if token == "REM" {
        rest.get(..3)
            .is_some_and(|prefix| prefix.eq_ignore_ascii_case("REM"))
            && rest[3..]
                .chars()
                .next()
                .is_none_or(|ch| ch.is_ascii_whitespace())
    } else {
        rest.starts_with(token)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn adds_line_comments_at_indentation() {
        let source = "    int value = 1;\n    return value;\n";
        let result = toggle(source, 0, source.chars().count(), "//");
        assert!(result.changed);
        assert_eq!(result.text, "    // int value = 1;\n    // return value;\n");
        assert_eq!(result.start, 0);
        assert_eq!(result.end, result.text.chars().count());
    }

    #[test]
    fn removes_comments_when_all_selected_lines_are_commented() {
        let result = toggle("    // int value = 1;\n    // return value;\n", 0, 54, "//");
        assert!(result.changed);
        assert_eq!(result.text, "    int value = 1;\n    return value;\n");
    }

    #[test]
    fn mixed_selection_only_comments_uncommented_lines() {
        let result = toggle("// one\nint two;\n", 0, 17, "//");
        assert_eq!(result.text, "// one\n// int two;\n");
    }

    #[test]
    fn blank_lines_are_skipped() {
        let result = toggle("int one;\n\nint two;\n", 0, 19, "//");
        assert_eq!(result.text, "// int one;\n\n// int two;\n");
    }

    #[test]
    fn selection_ending_at_next_line_start_excludes_that_line() {
        let result = toggle("int one;\nint two;\n", 0, 9, "//");
        assert_eq!(result.text, "// int one;\nint two;\n");
    }

    #[test]
    fn cursor_in_middle_of_line_keeps_logical_position() {
        let result = toggle("    int value;\n", 9, 9, "//");
        assert_eq!(result.text, "    // int value;\n");
        assert_eq!(result.start, 12);
        assert_eq!(result.end, 12);
    }

    #[test]
    fn unicode_text_uses_character_indices() {
        let result = toggle("    // 中文变量\n", 6, 6, "//");
        assert_eq!(result.text, "    中文变量\n");
    }

    #[test]
    fn hash_comments_work_for_makefiles_and_python() {
        assert_eq!(
            line_comment_token(Path::new("Makefile"), "Makefile"),
            Some("#")
        );
        assert_eq!(
            line_comment_token(Path::new("script.py"), "Python"),
            Some("#")
        );
        assert_eq!(line_comment_token(Path::new("main.c"), "C"), Some("//"));
        assert_eq!(line_comment_token(Path::new("page.html"), "HTML"), None);
    }

    #[test]
    fn batch_rem_comments_are_case_insensitive() {
        let result = toggle("rem hello\nREM world\n", 0, 20, "REM");
        assert_eq!(result.text, "hello\nworld\n");
    }
}
