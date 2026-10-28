//! Lightweight C-style brace folding.
//!
//! The editor keeps the source buffer untouched and renders a read-only view
//! when at least one block is folded. Typing automatically clears the fold set.

use std::collections::BTreeSet;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct FoldRange {
    /// 0-based line containing the opening `{`.
    pub start_line: usize,
    /// 0-based line containing the matching `}`.
    pub end_line: usize,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum State {
    Normal,
    LineComment,
    BlockComment,
    String,
    Char,
}

/// Find multiline brace blocks while ignoring comments and strings.
pub fn ranges(text: &str) -> Vec<FoldRange> {
    let mut ranges = Vec::new();
    let mut stack = Vec::new();
    let mut line = 0usize;
    let mut state = State::Normal;
    let mut escape = false;
    let mut chars = text.chars().peekable();
    while let Some(ch) = chars.next() {
        match state {
            State::Normal => match ch {
                '\n' => line += 1,
                '/' if chars.peek() == Some(&'/') => {
                    chars.next();
                    state = State::LineComment;
                }
                '/' if chars.peek() == Some(&'*') => {
                    chars.next();
                    state = State::BlockComment;
                }
                '"' => state = State::String,
                '\'' => state = State::Char,
                '{' => stack.push(line),
                '}' => {
                    if let Some(start_line) = stack.pop() {
                        if line > start_line {
                            ranges.push(FoldRange {
                                start_line,
                                end_line: line,
                            });
                        }
                    }
                }
                _ => {}
            },
            State::LineComment => {
                if ch == '\n' {
                    line += 1;
                    state = State::Normal;
                }
            }
            State::BlockComment => {
                if ch == '\n' {
                    line += 1;
                } else if ch == '*' && chars.peek() == Some(&'/') {
                    chars.next();
                    state = State::Normal;
                }
            }
            State::String => {
                if escape {
                    escape = false;
                } else if ch == '\\' {
                    escape = true;
                } else if ch == '"' {
                    state = State::Normal;
                } else if ch == '\n' {
                    line += 1;
                }
            }
            State::Char => {
                if escape {
                    escape = false;
                } else if ch == '\\' {
                    escape = true;
                } else if ch == '\'' {
                    state = State::Normal;
                } else if ch == '\n' {
                    line += 1;
                }
            }
        }
    }
    ranges.sort_by_key(|range| (range.start_line, range.end_line));
    ranges
}

/// Return the innermost fold start for a 0-based cursor line.
pub fn fold_start_for_line(line: usize, ranges: &[FoldRange]) -> Option<usize> {
    ranges
        .iter()
        .filter(|range| range.start_line <= line && line <= range.end_line)
        .min_by_key(|range| range.end_line - range.start_line)
        .map(|range| range.start_line)
}

/// Build the folded display text and the view-line -> fold-start map.
pub fn view(
    text: &str,
    folds: &BTreeSet<usize>,
    ranges: &[FoldRange],
) -> (String, Vec<(usize, usize)>) {
    let lines = text.split_inclusive('\n').collect::<Vec<_>>();
    let mut selected = ranges
        .iter()
        .copied()
        .filter(|range| folds.contains(&range.start_line) && range.end_line > range.start_line)
        .collect::<Vec<_>>();
    selected.sort_by_key(|range| (range.start_line, range.end_line));

    let mut output = String::new();
    let mut markers = Vec::new();
    let mut view_line = 0usize;
    let mut skip_until = 0usize;
    for (line_index, line) in lines.iter().enumerate() {
        if line_index < skip_until {
            continue;
        }
        if let Some(range) = selected.iter().find(|range| range.start_line == line_index) {
            output.push_str(line);
            if !line.ends_with('\n') {
                output.push('\n');
            }
            markers.push((view_line + 1, range.start_line));
            output.push_str(&format!(
                "    ...   ({} 行)\n",
                range.end_line - range.start_line
            ));
            view_line += 2;
            skip_until = range.end_line + 1;
        } else {
            output.push_str(line);
            view_line += line.chars().filter(|ch| *ch == '\n').count().max(1);
        }
    }
    (output, markers)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn finds_multiline_blocks_and_ignores_comments() {
        let text = "// {\nint a;\nvoid f() {\n    if (x) {\n    }\n}\n";
        let ranges = ranges(text);
        assert!(ranges.contains(&FoldRange {
            start_line: 2,
            end_line: 5
        }));
        assert!(ranges.contains(&FoldRange {
            start_line: 3,
            end_line: 4
        }));
        assert_eq!(fold_start_for_line(4, &ranges), Some(3));
        assert_eq!(fold_start_for_line(5, &ranges), Some(2));
    }

    #[test]
    fn folded_view_hides_the_body_and_keeps_markers() {
        let text = "void f() {\n    a();\n    b();\n}\nint x;\n";
        let ranges = ranges(text);
        let folds = BTreeSet::from([0usize]);
        let (view, markers) = view(text, &folds, &ranges);
        assert!(view.contains("void f() {"));
        assert!(view.contains("..."));
        assert!(!view.contains("a();"));
        assert!(view.contains("int x;"));
        assert_eq!(markers, vec![(1, 0)]);
    }

    #[test]
    fn nested_selected_blocks_do_not_duplicate_hidden_lines() {
        let text = "void f() {\n    if (x) {\n        y();\n    }\n}\n";
        let ranges = ranges(text);
        let folds = BTreeSet::from([0usize, 1usize]);
        let (view, markers) = view(text, &folds, &ranges);
        assert_eq!(markers, vec![(1, 0)]);
        assert!(!view.contains("y();"));
    }
}
