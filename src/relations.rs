//! Lightweight project-wide call and reference index for hover previews.
//!
//! This is deliberately a text index rather than a full C/C++ front end. It
//! scans saved source files, removes comments and string literals, extracts
//! function bodies, and records function calls plus identifier references.
//! Same-name functions (for example two `static` helpers in different files)
//! are merged, and calls through function pointers are not resolved.

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

use regex::Regex;

use crate::ignore::IgnoreRules;
use crate::util::read_text_file;

const MAX_FILE_BYTES: u64 = 2 * 1024 * 1024;
const MAX_FILES: usize = 20_000;
const MAX_FUNCTIONS: usize = 20_000;
const MAX_CALL_EDGES: usize = 40_000;
const MAX_REFERENCES: usize = 60_000;
const MAX_REFERENCES_PER_NAME: usize = 24;
const MAX_PREVIEW_CHARS: usize = 120;
const MAX_TREE_DEPTH: usize = 2;
const DIRECT_NODES: usize = 3;
const CHILD_NODES: usize = 1;
const TREE_BUDGET: usize = 4;

#[derive(Clone, Debug, Default)]
pub struct FunctionInfo {
    pub path: PathBuf,
    pub line: usize,
}

#[derive(Clone, Debug, Default)]
pub struct Reference {
    pub path: PathBuf,
    pub line: usize,
    pub column: usize,
    pub preview: String,
    pub is_definition: bool,
}

#[derive(Clone, Debug, Default)]
pub struct ReferenceList {
    pub items: Vec<Reference>,
    pub truncated: bool,
}

#[derive(Clone, Debug, Default)]
pub struct RelationIndex {
    pub functions: BTreeMap<String, FunctionInfo>,
    pub calls: BTreeMap<String, BTreeSet<String>>,
    pub callers: BTreeMap<String, BTreeSet<String>>,
    pub references: BTreeMap<String, ReferenceList>,
    pub truncated: bool,
}

#[derive(Clone, Debug, Default)]
pub struct CallNode {
    pub name: String,
    pub path: Option<PathBuf>,
    pub line: Option<usize>,
    pub children: Vec<CallNode>,
}

#[derive(Clone, Debug, Default)]
pub struct ReferenceGroup {
    pub path: PathBuf,
    pub references: Vec<Reference>,
    pub total: usize,
}

#[derive(Clone, Debug, Default)]
pub struct HoverRelations {
    pub word: String,
    pub function: bool,
    pub callers: Vec<CallNode>,
    pub callees: Vec<CallNode>,
    pub caller_total: usize,
    pub callee_total: usize,
    pub reference_groups: Vec<ReferenceGroup>,
    pub reference_total: usize,
    pub file_total: usize,
    pub references_truncated: bool,
    pub index_truncated: bool,
}

impl RelationIndex {
    /// Build the compact view shown under the existing hover card. Returns
    /// `None` when the identifier is neither a project function nor indexed.
    pub fn hover(&self, word: &str) -> Option<HoverRelations> {
        let function = self.functions.contains_key(word);
        let references = self.references.get(word);
        if !function && references.is_none_or(|list| list.items.is_empty()) {
            return None;
        }

        let mut view = HoverRelations {
            word: word.to_owned(),
            function,
            index_truncated: self.truncated,
            ..HoverRelations::default()
        };

        if let Some(list) = references {
            view.reference_total = list.items.len();
            view.references_truncated = list.truncated;
            let mut grouped: BTreeMap<PathBuf, Vec<Reference>> = BTreeMap::new();
            for reference in &list.items {
                grouped
                    .entry(reference.path.clone())
                    .or_default()
                    .push(reference.clone());
            }
            view.file_total = grouped.len();
            for (path, items) in grouped.into_iter().take(3) {
                let total = items.len();
                view.reference_groups.push(ReferenceGroup {
                    path,
                    references: items.into_iter().take(2).collect(),
                    total,
                });
            }
        }

        if function {
            view.caller_total = self.callers.get(word).map_or(0, |names| names.len());
            view.callee_total = self.calls.get(word).map_or(0, |names| names.len());
            let mut visited = BTreeSet::from([word.to_owned()]);
            let mut budget = TREE_BUDGET;
            view.callers = self.call_nodes(word, false, 0, &mut visited, &mut budget);
            let mut visited = BTreeSet::from([word.to_owned()]);
            let mut budget = TREE_BUDGET;
            view.callees = self.call_nodes(word, true, 0, &mut visited, &mut budget);
        }

        Some(view)
    }

    fn call_nodes(
        &self,
        start: &str,
        outgoing: bool,
        depth: usize,
        visited: &mut BTreeSet<String>,
        budget: &mut usize,
    ) -> Vec<CallNode> {
        let names = if outgoing {
            self.calls.get(start)
        } else {
            self.callers.get(start)
        };
        let Some(names) = names else {
            return Vec::new();
        };
        let limit = if depth == 0 {
            DIRECT_NODES
        } else {
            CHILD_NODES
        };
        let mut nodes = Vec::new();
        for name in names.iter().take(limit) {
            if *budget == 0 {
                break;
            }
            if !visited.insert(name.clone()) {
                continue;
            }
            *budget -= 1;
            let info = self.functions.get(name);
            let mut node = CallNode {
                name: name.clone(),
                path: info.map(|info| info.path.clone()),
                line: info.map(|info| info.line),
                children: Vec::new(),
            };
            if depth + 1 < MAX_TREE_DEPTH {
                node.children = self.call_nodes(name, outgoing, depth + 1, visited, budget);
            }
            nodes.push(node);
        }
        nodes
    }
}

/// Scan a project. The caller normally runs this on a background thread.
pub fn scan_project(root: &Path, ignore: &IgnoreRules) -> Result<RelationIndex, String> {
    let macro_re = Regex::new(r"^\s*#\s*define\s+([A-Za-z_]\w*)").map_err(|err| err.to_string())?;
    let record_re =
        Regex::new(r"^\s*(?:typedef\s+)?(?:struct|union|enum|class)\s+([A-Za-z_]\w*)\s*(?:\{|$)")
            .map_err(|err| err.to_string())?;

    let mut files = Vec::new();
    collect_files(root, ignore, &mut files);
    files.sort_by(|left, right| {
        file_priority(root, left)
            .cmp(&file_priority(root, right))
            .then_with(|| left.cmp(right))
    });
    let mut index = RelationIndex {
        truncated: files.len() >= MAX_FILES,
        ..RelationIndex::default()
    };
    let mut raw_calls: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();
    let mut reference_total = 0usize;

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
        let cleaned = sanitize_lines(&loaded.text);
        let original = loaded.text.lines().collect::<Vec<_>>();
        let mut definitions: BTreeMap<usize, Vec<String>> = BTreeMap::new();

        for (line_index, line) in cleaned.iter().enumerate() {
            if let Some(captures) = macro_re.captures(line) {
                if let Some(name) = captures.get(1) {
                    add_definition(&mut definitions, line_index + 1, name.as_str());
                }
            }
            if let Some(captures) = record_re.captures(line) {
                if let Some(name) = captures.get(1) {
                    add_definition(&mut definitions, line_index + 1, name.as_str());
                }
            }
        }

        let mut line_index = 0usize;
        while line_index < cleaned.len() {
            let Some(function) = parse_function_at(&cleaned, line_index) else {
                line_index += 1;
                continue;
            };
            if index.functions.len() < MAX_FUNCTIONS {
                index
                    .functions
                    .entry(function.name.clone())
                    .or_insert_with(|| FunctionInfo {
                        path: path.clone(),
                        line: function.start_line + 1,
                    });
            } else {
                index.truncated = true;
            }
            add_definition(
                &mut definitions,
                function.start_line + 1,
                function.name.as_str(),
            );
            collect_calls(&cleaned, &function, &mut raw_calls);
            line_index = function.body_end_line.saturating_add(1);
        }

        collect_references(
            &cleaned,
            &original,
            &path,
            &definitions,
            &mut index.references,
            &mut reference_total,
            &mut index.truncated,
        );
    }

    let mut calls: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();
    let mut callers: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();
    let mut edge_count = 0usize;
    for (caller, callees) in raw_calls {
        if !index.functions.contains_key(&caller) {
            continue;
        }
        let mut kept = BTreeSet::new();
        for callee in callees {
            if !index.functions.contains_key(&callee) {
                continue;
            }
            if kept.insert(callee.clone()) {
                edge_count += 1;
                if edge_count > MAX_CALL_EDGES {
                    index.truncated = true;
                    break;
                }
            }
        }
        if kept.is_empty() {
            continue;
        }
        for callee in &kept {
            callers
                .entry(callee.clone())
                .or_default()
                .insert(caller.clone());
        }
        calls.insert(caller, kept);
    }
    index.calls = calls;
    index.callers = callers;
    Ok(index)
}

fn add_definition(definitions: &mut BTreeMap<usize, Vec<String>>, line: usize, name: &str) {
    let names = definitions.entry(line).or_default();
    if !names.iter().any(|existing| existing == name) {
        names.push(name.to_owned());
    }
}

fn collect_files(root: &Path, ignore: &IgnoreRules, files: &mut Vec<PathBuf>) {
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
                if !ignore.is_ignored(relative, true) && !is_generated_dir(relative) {
                    stack.push(path);
                }
                continue;
            }
            if !file_type.is_file() || ignore.is_ignored(relative, false) || !is_source(&path) {
                continue;
            }
            if files.len() >= MAX_FILES {
                return;
            }
            files.push(path);
        }
    }
}

fn is_source(path: &Path) -> bool {
    path.extension()
        .map(|extension| extension.to_string_lossy().to_lowercase())
        .is_some_and(|extension| {
            matches!(
                extension.as_str(),
                "c" | "h" | "cc" | "cpp" | "cxx" | "hh" | "hpp" | "hxx" | "ino"
            )
        })
}

/// Index user code before vendor SDK trees so the global reference cap keeps
/// application symbols even in projects with a large HAL/CMSIS tree.
fn file_priority(root: &Path, path: &Path) -> u8 {
    let relative = path.strip_prefix(root).unwrap_or(path);
    if relative.components().any(|component| {
        matches!(
            component
                .as_os_str()
                .to_string_lossy()
                .to_ascii_lowercase()
                .as_str(),
            "drivers"
                | "cmsis"
                | "middlewares"
                | "third_party"
                | "third-party"
                | "vendor"
                | "hal_driver"
                | "hal_drivers"
        )
    }) {
        1
    } else {
        0
    }
}

fn is_generated_dir(relative: &Path) -> bool {
    relative.components().any(|component| {
        matches!(
            component
                .as_os_str()
                .to_string_lossy()
                .to_ascii_lowercase()
                .as_str(),
            ".git"
                | ".svn"
                | ".simple-embedded"
                | "build"
                | "debug"
                | "release"
                | "out"
                | "target"
                | "objects"
                | "listings"
                | "node_modules"
        )
    })
}

/// Whole-file variant of [`sanitize_lines`], used by the architecture view.
/// Comments and string literals become spaces while line numbers stay stable.
pub(crate) fn sanitize_source(text: &str) -> String {
    sanitize_lines(text).join("\n")
}

/// Replace comments and string/char literals with spaces while preserving
/// line breaks and character positions. Braces and identifiers in comments
/// therefore do not affect the lightweight function parser.
fn sanitize_lines(text: &str) -> Vec<String> {
    let mut result = Vec::new();
    let mut in_block = false;
    let mut quote: Option<char> = None;
    for line in text.lines() {
        let chars = line.chars().collect::<Vec<_>>();
        let mut out = String::with_capacity(line.len());
        let mut index = 0usize;
        while index < chars.len() {
            let character = chars[index];
            if in_block {
                if character == '*' && chars.get(index + 1) == Some(&'/') {
                    out.push_str("  ");
                    in_block = false;
                    index += 2;
                } else {
                    out.push(' ');
                    index += 1;
                }
                continue;
            }
            if let Some(active_quote) = quote {
                if character == '\\' {
                    out.push(' ');
                    if index + 1 < chars.len() {
                        out.push(' ');
                        index += 2;
                    } else {
                        index += 1;
                    }
                } else if character == active_quote {
                    out.push(' ');
                    quote = None;
                    index += 1;
                } else {
                    out.push(' ');
                    index += 1;
                }
                continue;
            }
            if character == '/' && chars.get(index + 1) == Some(&'/') {
                while index < chars.len() {
                    out.push(' ');
                    index += 1;
                }
                break;
            }
            if character == '/' && chars.get(index + 1) == Some(&'*') {
                out.push_str("  ");
                in_block = true;
                index += 2;
                continue;
            }
            if character == '"' || character == '\'' {
                quote = Some(character);
                out.push(' ');
                index += 1;
                continue;
            }
            out.push(character);
            index += 1;
        }
        result.push(out);
    }
    result
}

fn parse_function_at(lines: &[String], start_line: usize) -> Option<FunctionStart> {
    let line = lines.get(start_line)?;
    let trimmed = line.trim_start();
    if trimmed.is_empty()
        || trimmed.starts_with('#')
        || trimmed.starts_with('}')
        || trimmed.starts_with('{')
        || trimmed.starts_with(';')
    {
        return None;
    }
    let bytes = line.as_bytes();
    let mut index = 0usize;
    while index < bytes.len() {
        if !is_identifier_start(bytes[index]) {
            index += 1;
            continue;
        }
        let start = index;
        index += 1;
        while index < bytes.len() && is_identifier_continue(bytes[index]) {
            index += 1;
        }
        let name = &line[start..index];
        if is_keyword(name) || !prefix_looks_like_return_type(&line[..start]) {
            continue;
        }
        let mut probe = index;
        while probe < bytes.len() && matches!(bytes[probe], b' ' | b'\t') {
            probe += 1;
        }
        if bytes.get(probe) != Some(&b'(') {
            continue;
        }
        let open_column = line[..probe].chars().count();
        if let Some(function) = build_function(lines, start_line, name, open_column) {
            return Some(function);
        }
    }
    None
}

fn prefix_looks_like_return_type(prefix: &str) -> bool {
    let trimmed = prefix.trim();
    if trimmed.is_empty() {
        return false;
    }
    if trimmed.chars().any(|character| {
        matches!(
            character,
            '=' | ';'
                | '{'
                | '}'
                | '('
                | ')'
                | '['
                | ']'
                | '!'
                | '+'
                | '-'
                | '/'
                | '%'
                | '|'
                | '^'
                | '?'
                | '.'
                | '#'
        )
    }) {
        return false;
    }
    let first = trimmed
        .split(|character: char| !is_identifier_char(character))
        .find(|part| !part.is_empty())
        .unwrap_or("");
    if first.is_empty() || is_control_keyword(first) {
        return false;
    }
    trimmed
        .chars()
        .any(|character| character.is_ascii_alphabetic())
}

fn build_function(
    lines: &[String],
    start_line: usize,
    name: &str,
    open_column: usize,
) -> Option<FunctionStart> {
    let (close_line, close_column) = find_matching_paren(lines, start_line, open_column)?;
    let (body_start_line, body_start_column) = find_body_open(lines, close_line, close_column + 1)?;
    let (body_end_line, _) = find_matching_brace(lines, body_start_line, body_start_column)?;
    Some(FunctionStart {
        name: name.to_owned(),
        start_line,
        body_start_line,
        body_start_column,
        body_end_line,
    })
}

fn find_matching_paren(
    lines: &[String],
    start_line: usize,
    start_column: usize,
) -> Option<(usize, usize)> {
    let mut depth = 0usize;
    for (line_index, line) in lines.iter().enumerate().skip(start_line) {
        let chars = line.chars().collect::<Vec<_>>();
        let begin = if line_index == start_line {
            start_column
        } else {
            0
        };
        for (offset, character) in chars.iter().enumerate().skip(begin) {
            if *character == '(' {
                depth += 1;
            } else if *character == ')' {
                depth = depth.saturating_sub(1);
                if depth == 0 {
                    return Some((line_index, offset));
                }
            }
        }
    }
    None
}

fn find_body_open(
    lines: &[String],
    start_line: usize,
    start_column: usize,
) -> Option<(usize, usize)> {
    for line_index in (start_line..).take(40) {
        let line = lines.get(line_index)?;
        let chars = line.chars().collect::<Vec<_>>();
        let begin = if line_index == start_line {
            start_column
        } else {
            0
        };
        for (offset, character) in chars.iter().enumerate().skip(begin) {
            match character {
                ';' | '}' | '=' => return None,
                '{' => return Some((line_index, offset)),
                _ => {}
            }
        }
    }
    None
}

fn find_matching_brace(
    lines: &[String],
    start_line: usize,
    start_column: usize,
) -> Option<(usize, usize)> {
    let mut depth = 0usize;
    for (line_index, line) in lines.iter().enumerate().skip(start_line) {
        let chars = line.chars().collect::<Vec<_>>();
        let begin = if line_index == start_line {
            start_column
        } else {
            0
        };
        for (offset, character) in chars.iter().enumerate().skip(begin) {
            if *character == '{' {
                depth += 1;
            } else if *character == '}' {
                depth = depth.saturating_sub(1);
                if depth == 0 {
                    return Some((line_index, offset));
                }
            }
        }
    }
    None
}

fn collect_calls(
    lines: &[String],
    function: &FunctionStart,
    calls: &mut BTreeMap<String, BTreeSet<String>>,
) {
    for line_index in function.body_start_line..=function.body_end_line {
        let Some(line) = lines.get(line_index) else {
            break;
        };
        if line.trim_start().starts_with('#') {
            continue;
        }
        let text = if line_index == function.body_start_line {
            line.chars()
                .skip(function.body_start_column + 1)
                .collect::<String>()
        } else {
            line.clone()
        };
        scan_calls_in_line(&text, &function.name, calls);
    }
}

fn scan_calls_in_line(line: &str, caller: &str, calls: &mut BTreeMap<String, BTreeSet<String>>) {
    let bytes = line.as_bytes();
    let mut index = 0usize;
    while index < bytes.len() {
        if !is_identifier_start(bytes[index]) {
            index += 1;
            continue;
        }
        let start = index;
        index += 1;
        while index < bytes.len() && is_identifier_continue(bytes[index]) {
            index += 1;
        }
        let name = &line[start..index];
        if is_keyword(name) {
            continue;
        }
        let previous = line[..start].chars().next_back();
        if matches!(previous, Some('.') | Some('>')) {
            continue;
        }
        let mut probe = index;
        while probe < bytes.len() && matches!(bytes[probe], b' ' | b'\t') {
            probe += 1;
        }
        if bytes.get(probe) == Some(&b'(') {
            calls
                .entry(caller.to_owned())
                .or_default()
                .insert(name.to_owned());
        }
    }
}

fn collect_references(
    cleaned: &[String],
    original: &[&str],
    path: &Path,
    definitions: &BTreeMap<usize, Vec<String>>,
    references: &mut BTreeMap<String, ReferenceList>,
    total: &mut usize,
    truncated: &mut bool,
) {
    for (line_index, line) in cleaned.iter().enumerate() {
        let trimmed = line.trim_start();
        if trimmed.starts_with("#include") || trimmed.starts_with("# include") {
            continue;
        }
        let directive_start = line.find('#').and_then(|hash| {
            line[hash + 1..]
                .find(|character: char| character.is_ascii_alphabetic())
                .map(|offset| hash + 1 + offset)
        });
        let original_line = original.get(line_index).copied().unwrap_or("");
        let bytes = line.as_bytes();
        let mut index = 0usize;
        while index < bytes.len() {
            if !is_identifier_start(bytes[index]) {
                index += 1;
                continue;
            }
            let start = index;
            index += 1;
            while index < bytes.len() && is_identifier_continue(bytes[index]) {
                index += 1;
            }
            let name = &line[start..index];
            if directive_start == Some(start) || is_keyword(name) || name.len() > 64 {
                continue;
            }
            if *total >= MAX_REFERENCES {
                *truncated = true;
                return;
            }
            let is_definition = definitions
                .get(&(line_index + 1))
                .is_some_and(|names| names.iter().any(|defined| defined == name));
            let list = references.entry(name.to_owned()).or_default();
            if list.items.len() >= MAX_REFERENCES_PER_NAME {
                list.truncated = true;
                continue;
            }
            list.items.push(Reference {
                path: path.to_path_buf(),
                line: line_index + 1,
                column: line[..start].chars().count() + 1,
                preview: truncate_text(original_line.trim(), MAX_PREVIEW_CHARS),
                is_definition,
            });
            *total += 1;
        }
    }
}

fn truncate_text(text: &str, max_chars: usize) -> String {
    if text.chars().count() <= max_chars {
        return text.to_owned();
    }
    let mut result = text.chars().take(max_chars).collect::<String>();
    result.push_str("...");
    result
}

fn is_identifier_start(byte: u8) -> bool {
    byte.is_ascii_alphabetic() || byte == b'_'
}

fn is_identifier_continue(byte: u8) -> bool {
    byte.is_ascii_alphanumeric() || byte == b'_'
}

fn is_identifier_char(character: char) -> bool {
    character.is_ascii_alphanumeric() || character == '_'
}

fn is_control_keyword(name: &str) -> bool {
    matches!(
        name,
        "if" | "else"
            | "for"
            | "while"
            | "do"
            | "switch"
            | "case"
            | "default"
            | "return"
            | "goto"
            | "break"
            | "continue"
            | "sizeof"
            | "defined"
            | "static_assert"
            | "catch"
            | "throw"
            | "new"
            | "delete"
            | "try"
    )
}

fn is_keyword(name: &str) -> bool {
    is_control_keyword(name)
        || matches!(
            name,
            "typedef"
                | "struct"
                | "union"
                | "enum"
                | "class"
                | "static"
                | "extern"
                | "const"
                | "volatile"
                | "inline"
                | "register"
                | "auto"
                | "void"
                | "char"
                | "short"
                | "int"
                | "long"
                | "float"
                | "double"
                | "signed"
                | "unsigned"
                | "bool"
                | "true"
                | "false"
                | "using"
                | "namespace"
                | "template"
                | "typename"
                | "public"
                | "private"
                | "protected"
                | "virtual"
                | "override"
                | "final"
                | "noexcept"
                | "constexpr"
                | "consteval"
                | "operator"
        )
}

#[derive(Clone, Debug)]
struct FunctionStart {
    name: String,
    start_line: usize,
    body_start_line: usize,
    body_start_column: usize,
    body_end_line: usize,
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

    fn scan(dir: &tempfile::TempDir) -> RelationIndex {
        let ignore = IgnoreRules::load(dir.path(), true, "");
        scan_project(dir.path(), &ignore).unwrap()
    }

    #[test]
    fn indexes_calls_and_references_without_comments_or_strings() {
        let dir = tempfile::tempdir().unwrap();
        write(
            &dir.path().join("src/main.c"),
            "#define LIMIT 10\n\
             struct Point { int x; };\n\
             static void helper(void) { }\n\
             int main(void) {\n\
                 // helper();\n\
                 char *text = \"helper()\";\n\
                 helper();\n\
                 return LIMIT;\n\
             }\n",
        );
        let index = scan(&dir);

        assert!(index.functions.contains_key("main"));
        assert!(index.functions.contains_key("helper"));
        assert!(index.calls["main"].contains("helper"));
        assert!(index.callers["helper"].contains("main"));

        let helper = &index.references["helper"];
        assert_eq!(helper.items.len(), 2);
        assert!(helper.items.iter().any(|item| item.is_definition));
        let limit = &index.references["LIMIT"];
        assert_eq!(limit.items.len(), 2);
        assert!(limit.items.iter().any(|item| item.is_definition));
        assert!(index.references.contains_key("Point"));
        assert_eq!(index.references["text"].items.len(), 1);
        assert!(!index.references.contains_key("define"));
    }

    #[test]
    fn parses_multiline_function_signatures() {
        let dir = tempfile::tempdir().unwrap();
        write(
            &dir.path().join("src/math.c"),
            "static int add(\n\
                 int a,\n\
                 int b)\n\
             {\n\
                 return a + b;\n\
             }\n\
             int main(void) { return add(1, 2); }\n",
        );
        let index = scan(&dir);

        assert!(index.functions.contains_key("add"));
        assert!(index.functions.contains_key("main"));
        assert!(index.calls["main"].contains("add"));
        assert!(!index.calls.contains_key("add"));
    }

    #[test]
    fn call_tree_stops_at_cycles() {
        let dir = tempfile::tempdir().unwrap();
        write(
            &dir.path().join("src/cycle.c"),
            "void a(void) { b(); }\nvoid b(void) { a(); }\n",
        );
        let index = scan(&dir);
        let view = index.hover("a").expect("function hover");

        assert!(view.function);
        assert_eq!(view.caller_total, 1);
        assert_eq!(view.callee_total, 1);
        assert_eq!(view.callers[0].name, "b");
        assert_eq!(view.callees[0].name, "b");
        assert!(view.callers[0].children.is_empty());
        assert!(view.callees[0].children.is_empty());
    }

    #[test]
    fn builds_one_extra_call_tree_level() {
        let dir = tempfile::tempdir().unwrap();
        write(
            &dir.path().join("src/tree.c"),
            "void c(void) { }\nvoid b(void) { c(); }\nvoid a(void) { b(); }\n",
        );
        let index = scan(&dir);
        let from_a = index.hover("a").expect("a");
        assert_eq!(from_a.callees[0].name, "b");
        assert_eq!(from_a.callees[0].children[0].name, "c");

        let from_c = index.hover("c").expect("c");
        assert_eq!(from_c.callers[0].name, "b");
        assert_eq!(from_c.callers[0].children[0].name, "a");
    }

    #[test]
    fn skips_generated_directories_and_ignored_files() {
        let dir = tempfile::tempdir().unwrap();
        write(&dir.path().join("src/real.c"), "void real(void) {}\n");
        write(
            &dir.path().join("build/generated.c"),
            "void generated(void) {}\n",
        );
        write(&dir.path().join("src/ignored.c"), "void ignored(void) {}\n");
        let ignore = IgnoreRules::load(dir.path(), true, "src/ignored.c");
        let index = scan_project(dir.path(), &ignore).unwrap();

        assert!(index.functions.contains_key("real"));
        assert!(!index.functions.contains_key("generated"));
        assert!(!index.functions.contains_key("ignored"));
    }

    #[test]
    fn groups_references_by_file_and_marks_definitions() {
        let dir = tempfile::tempdir().unwrap();
        write(
            &dir.path().join("src/a.c"),
            "int shared;\nvoid use_a(void) { shared = 1; }\n",
        );
        write(
            &dir.path().join("src/b.c"),
            "extern int shared;\nvoid use_b(void) { shared = 2; }\n",
        );
        let index = scan(&dir);
        let view = index.hover("shared").expect("variable hover");

        assert!(!view.function);
        assert_eq!(view.reference_total, 4);
        assert_eq!(view.file_total, 2);
        assert_eq!(view.reference_groups.len(), 2);
    }
}
