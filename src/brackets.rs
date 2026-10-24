//! Lightweight bracket matching for the editor.
//!
//! This is intentionally syntax-aware only for C-like comments and strings.
//! It runs on demand (when the cursor is next to a bracket), so it does not
//! add work to every frame.

const MAX_MATCH_CHARS: usize = 1_000_000;

#[derive(Clone, Copy, PartialEq, Eq)]
enum ScanState {
    Code,
    LineComment,
    BlockComment,
    String,
    Char,
}

/// Returns `(bracket_at_or_before_cursor, matching_bracket)` as char indices.
pub fn matching_bracket(text: &str, cursor: usize) -> Option<(usize, usize)> {
    let chars = text.chars().take(MAX_MATCH_CHARS + 1).collect::<Vec<_>>();
    if chars.len() > MAX_MATCH_CHARS || chars.is_empty() {
        return None;
    }
    let mask = code_mask(&chars);
    let selected = if cursor < chars.len() && is_bracket(chars[cursor]) && mask[cursor] {
        cursor
    } else if cursor > 0 && is_bracket(chars[cursor - 1]) && mask[cursor - 1] {
        cursor - 1
    } else {
        return None;
    };

    let (open, close) = match chars[selected] {
        '(' => ('(', ')'),
        '[' => ('[', ']'),
        '{' => ('{', '}'),
        ')' => (')', '('),
        ']' => (']', '['),
        '}' => ('}', '{'),
        _ => return None,
    };
    let closing = matches!(chars[selected], ')' | ']' | '}');
    let mut depth = 0i32;
    if !closing {
        for (index, ch) in chars.iter().enumerate().skip(selected) {
            if !mask[index] {
                continue;
            }
            if *ch == open {
                depth += 1;
            } else if *ch == close {
                depth -= 1;
                if depth == 0 {
                    return Some((selected, index));
                }
            }
        }
    } else {
        for index in (0..=selected).rev() {
            if !mask[index] {
                continue;
            }
            let ch = chars[index];
            if ch == open {
                depth += 1;
            } else if ch == close {
                depth -= 1;
                if depth == 0 {
                    return Some((selected, index));
                }
            }
        }
    }
    None
}

fn is_bracket(ch: char) -> bool {
    matches!(ch, '(' | ')' | '[' | ']' | '{' | '}')
}

fn code_mask(chars: &[char]) -> Vec<bool> {
    let mut mask = vec![true; chars.len()];
    let mut state = ScanState::Code;
    let mut index = 0usize;
    while index < chars.len() {
        let ch = chars[index];
        let next = chars.get(index + 1).copied();
        match state {
            ScanState::Code => {
                if ch == '/' && next == Some('/') {
                    mask[index] = false;
                    if index + 1 < chars.len() {
                        mask[index + 1] = false;
                    }
                    state = ScanState::LineComment;
                    index += 2;
                } else if ch == '/' && next == Some('*') {
                    mask[index] = false;
                    if index + 1 < chars.len() {
                        mask[index + 1] = false;
                    }
                    state = ScanState::BlockComment;
                    index += 2;
                } else if ch == '"' {
                    mask[index] = false;
                    state = ScanState::String;
                    index += 1;
                } else if ch == '\'' {
                    mask[index] = false;
                    state = ScanState::Char;
                    index += 1;
                } else {
                    index += 1;
                }
            }
            ScanState::LineComment => {
                mask[index] = false;
                if ch == '\n' {
                    state = ScanState::Code;
                }
                index += 1;
            }
            ScanState::BlockComment => {
                mask[index] = false;
                if ch == '*' && next == Some('/') {
                    if index + 1 < chars.len() {
                        mask[index + 1] = false;
                    }
                    state = ScanState::Code;
                    index += 2;
                } else {
                    index += 1;
                }
            }
            ScanState::String | ScanState::Char => {
                mask[index] = false;
                if ch == '\\' {
                    if index + 1 < chars.len() {
                        mask[index + 1] = false;
                    }
                    index += 2;
                } else if (state == ScanState::String && ch == '"')
                    || (state == ScanState::Char && ch == '\'')
                {
                    state = ScanState::Code;
                    index += 1;
                } else {
                    index += 1;
                }
            }
        }
    }
    mask
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn matches_nested_brackets() {
        let text = "int f(int a[2])";
        assert_eq!(matching_bracket(text, 5), Some((5, 14)));
        assert_eq!(matching_bracket(text, 14), Some((14, 5)));
        assert_eq!(matching_bracket(text, 11), Some((11, 13)));

        let text = "x = (a[0]);";
        assert_eq!(matching_bracket(text, 4), Some((4, 9)));
        assert_eq!(matching_bracket(text, 6), Some((6, 8)));
    }

    #[test]
    fn ignores_strings_and_comments() {
        assert_eq!(matching_bracket("\"(\"", 1), None);
        assert_eq!(matching_bracket("// (", 3), None);
        assert_eq!(matching_bracket("/* ( */", 4), None);
    }

    #[test]
    fn handles_cursor_after_bracket() {
        let text = "func(a)";
        assert_eq!(matching_bracket(text, 5), Some((4, 6)));
        assert_eq!(matching_bracket(text, 7), Some((6, 4)));
    }
}
