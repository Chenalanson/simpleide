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

fn char_to_byte(text: &str, char_index: usize) -> usize {
    if char_index == 0 {
        return 0;
    }
    text.char_indices()
        .nth(char_index)
        .map(|(byte, _)| byte)
        .unwrap_or(text.len())
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

struct ViewSegment {
    view_start: usize,
    view_end: usize,
    source_start: usize,
    source_end: usize,
}

/// Editable folded view over the real source text.
///
/// Visible lines are copied into `view`; hidden body lines stay in `source`.
/// Edits are mapped back to source character ranges, so typing in the folded
/// view edits the real file rather than a read-only snapshot.
pub struct FoldBuffer {
    source: String,
    view: String,
    view_lines: Vec<usize>,
    markers: Vec<(usize, usize, usize)>,
    segments: Vec<ViewSegment>,
    folds: BTreeSet<usize>,
    changed: bool,
    lines_changed: bool,
    cursor_hint: Option<usize>,
}

impl FoldBuffer {
    pub fn new(source: String, folds: BTreeSet<usize>) -> Self {
        let mut buffer = Self {
            source,
            view: String::new(),
            view_lines: Vec::new(),
            markers: Vec::new(),
            segments: Vec::new(),
            folds,
            changed: false,
            lines_changed: false,
            cursor_hint: None,
        };
        buffer.rebuild();
        buffer
    }

    pub fn view(&self) -> &str {
        &self.view
    }

    pub fn view_lines(&self) -> &[usize] {
        &self.view_lines
    }

    pub fn markers(&self) -> &[(usize, usize, usize)] {
        &self.markers
    }

    pub fn folds(&self) -> &BTreeSet<usize> {
        &self.folds
    }

    pub fn source(&self) -> &str {
        &self.source
    }

    pub fn changed(&self) -> bool {
        self.changed
    }

    /// True when an edit added or removed a line break. Fold anchors are line
    /// based, so the caller should drop them and return to the full editor.
    pub fn lines_changed(&self) -> bool {
        self.lines_changed
    }

    /// Source caret position after the last edit, if that edit changed lines.
    pub fn cursor_hint(&self) -> Option<usize> {
        self.cursor_hint
    }

    pub fn view_to_source(&self, index: usize) -> usize {
        let index = index.min(self.view.chars().count());
        let position = self
            .segments
            .partition_point(|segment| segment.view_start <= index);
        let Some(segment) = position.checked_sub(1).and_then(|i| self.segments.get(i)) else {
            return 0;
        };
        if index < segment.view_end {
            segment.source_start
                + (index - segment.view_start).min(segment.source_end - segment.source_start)
        } else {
            segment.source_end
        }
    }

    pub fn source_to_view(&self, index: usize) -> usize {
        let index = index.min(self.source.chars().count());
        let position = self
            .segments
            .partition_point(|segment| segment.source_start <= index);
        let Some(segment) = position.checked_sub(1).and_then(|i| self.segments.get(i)) else {
            return 0;
        };
        if index < segment.source_end {
            segment.view_start + (index - segment.source_start)
        } else if index == segment.source_end {
            segment.view_end
        } else {
            // The source position is inside a hidden block: keep the caret on
            // the visible start line, just after the fold marker.
            segment.view_start
        }
    }

    fn rebuild(&mut self) {
        let ranges = ranges(&self.source);
        let mut selected = ranges
            .into_iter()
            .filter(|range| {
                self.folds.contains(&range.start_line) && range.end_line > range.start_line
            })
            .collect::<Vec<_>>();
        selected.sort_by_key(|range| (range.start_line, range.end_line));

        self.view.clear();
        self.view_lines.clear();
        self.markers.clear();
        self.segments.clear();
        let mut view_pos = 0usize;
        let mut source_pos = 0usize;
        let mut skip_until = 0usize;
        for (line_index, line) in self.source.split_inclusive('\n').enumerate() {
            let line_len = line.chars().count();
            if line_index < skip_until {
                source_pos += line_len;
                continue;
            }
            let view_line = self.view_lines.len();
            self.view_lines.push(line_index + 1);
            let (body, ending) = line
                .strip_suffix('\n')
                .map(|body| (body, "\n"))
                .unwrap_or((line, ""));
            let body_len = body.chars().count();
            let body_start = view_pos;
            self.view.push_str(body);
            view_pos += body_len;
            self.segments.push(ViewSegment {
                view_start: body_start,
                view_end: view_pos,
                source_start: source_pos,
                source_end: source_pos + body_len,
            });
            if let Some(range) = selected.iter().find(|range| range.start_line == line_index) {
                self.markers
                    .push((view_line, range.start_line, range.end_line));
                // Keep the closing brace line visible; only hide the body
                // between the start line and the matching brace.
                skip_until = range.end_line;
            }
            if !ending.is_empty() {
                let newline_start = view_pos;
                self.view.push('\n');
                view_pos += 1;
                self.segments.push(ViewSegment {
                    view_start: newline_start,
                    view_end: view_pos,
                    source_start: source_pos + body_len,
                    source_end: source_pos + body_len + 1,
                });
            }
            source_pos += line_len;
        }
    }
}

impl egui::TextBuffer for FoldBuffer {
    fn is_mutable(&self) -> bool {
        true
    }

    fn as_str(&self) -> &str {
        &self.view
    }

    fn insert_text(&mut self, text: &str, char_index: usize) -> usize {
        if text.is_empty() {
            return 0;
        }
        let at = self.view_to_source(char_index);
        let byte = char_to_byte(&self.source, at);
        if text.contains('\n') {
            self.lines_changed = true;
            self.cursor_hint = Some(at + text.chars().count());
        }
        self.source.insert_str(byte, text);
        self.changed = true;
        self.rebuild();
        text.chars().count()
    }

    fn delete_char_range(&mut self, char_range: std::ops::Range<usize>) {
        let view_len = self.view.chars().count();
        let start = char_range.start.min(view_len);
        let end = char_range.end.min(view_len);
        if end <= start {
            return;
        }
        // Delete only the characters that are actually visible. A selection
        // that crosses a folded block must not silently remove hidden lines.
        let mut sources = Vec::new();
        for segment in &self.segments {
            if segment.view_end <= start {
                continue;
            }
            if segment.view_start >= end {
                break;
            }
            let from = start.max(segment.view_start);
            let to = end.min(segment.view_end);
            sources.extend(
                (from..to).map(|index| segment.source_start + (index - segment.view_start)),
            );
        }
        if sources.is_empty() {
            return;
        }
        let cursor_hint = sources.first().copied();
        let source_len = self.source.chars().count();
        let byte_ranges = sources
            .into_iter()
            .filter(|index| *index < source_len)
            .map(|index| {
                let from = char_to_byte(&self.source, index);
                let to = char_to_byte(&self.source, index + 1);
                from..to
            })
            .collect::<Vec<_>>();
        let mut line_change = false;
        for range in byte_ranges.into_iter().rev() {
            if self.source[range.clone()].contains('\n') {
                line_change = true;
            }
            self.source.replace_range(range, "");
        }
        if line_change {
            self.lines_changed = true;
            self.cursor_hint = cursor_hint;
        }
        self.changed = true;
        self.rebuild();
    }

    fn type_id(&self) -> std::any::TypeId {
        std::any::TypeId::of::<Self>()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use egui::TextBuffer;

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
    fn fold_buffer_hides_the_block_and_keeps_source_line_numbers() {
        let text = "void f() {\n    a();\n    b();\n}\nint x;\n";
        let buffer = FoldBuffer::new(text.to_owned(), BTreeSet::from([0usize]));
        assert_eq!(buffer.view(), "void f() {\n}\nint x;\n");
        assert_eq!(buffer.view_lines(), &[1, 4, 5]);
        assert_eq!(buffer.markers(), &[(0, 0, 3)]);
        assert!(!buffer.view().contains("a();"));
        assert!(buffer.view().contains('}'));
    }

    #[test]
    fn fold_buffer_maps_edits_back_to_the_source() {
        let text = "void f() {\n    a();\n}\nint x;\n";
        let mut buffer = FoldBuffer::new(text.to_owned(), BTreeSet::from([0usize]));
        let visible = buffer.view().find("int x;").expect("visible line");
        let visible_char = buffer.view()[..visible].chars().count();
        assert_eq!(
            buffer.view_to_source(visible_char),
            text[..text.find("int x;").unwrap()].chars().count()
        );
        buffer.insert_text("X", 0);
        assert!(buffer.changed());
        assert!(buffer.source().starts_with("Xvoid f()"));
        assert!(buffer.view().starts_with("Xvoid f()"));
    }

    #[test]
    fn fold_buffer_keeps_hidden_lines_when_deleting_the_visible_newline() {
        let text = "void f() {\n    a();\n}\nint x;\n";
        let mut buffer = FoldBuffer::new(text.to_owned(), BTreeSet::from([0usize]));
        let next_line = buffer.view().find("int x;").expect("visible line");
        let next_char = buffer.view()[..next_line].chars().count();
        buffer.delete_char_range(next_char - 1..next_char);
        assert!(buffer.changed());
        assert!(buffer.lines_changed());
        assert_eq!(
            buffer.cursor_hint(),
            Some(
                text[..text.find("}\n").expect("closing newline") + 1]
                    .chars()
                    .count()
            )
        );
        assert!(buffer.source().contains("    a();"));
        assert!(buffer.source().contains("}int x;"));
    }

    #[test]
    fn fold_buffer_tracks_the_caret_when_inserting_a_line_break() {
        let text = "void f() {\n    a();\n}\nint x;\n";
        let mut buffer = FoldBuffer::new(text.to_owned(), BTreeSet::from([0usize]));
        let at = "void f() {".chars().count();
        buffer.insert_text("\n", at);
        assert!(buffer.lines_changed());
        assert_eq!(buffer.cursor_hint(), Some(at + 1));
        assert!(buffer.source().starts_with("void f() {\n\n"));
        assert!(buffer.source().contains("    a();"));
    }

    #[test]
    fn fold_buffer_maps_source_positions_into_the_view() {
        let text = "void f() {\n    a();\n    b();\n}\nint x;\n";
        let buffer = FoldBuffer::new(text.to_owned(), BTreeSet::from([0usize]));
        assert_eq!(buffer.source_to_view(0), 0);
        // A position inside the hidden block clamps to the folded start line.
        assert_eq!(
            buffer.source_to_view(text.find("a();").expect("hidden body")),
            "void f() {".chars().count()
        );
        let line = text.find("int x;").expect("visible line");
        let view_line = buffer.view().find("int x;").expect("visible line in view");
        assert_eq!(
            buffer.source_to_view(text[..line].chars().count()),
            buffer.view()[..view_line].chars().count()
        );
    }
}
