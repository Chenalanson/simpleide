//! Indentation helpers shared by the editor.

use crate::util::line_column_at_char;

pub fn indent_unit(width: usize, use_spaces: bool) -> String {
    if use_spaces {
        " ".repeat(width)
    } else {
        "\t".to_owned()
    }
}

pub fn insert_at_cursor(
    text: &str,
    char_index: usize,
    width: usize,
    use_spaces: bool,
) -> (String, usize) {
    let unit = indent_unit(width, use_spaces);
    let byte = char_to_byte(text, char_index);
    let mut new_text = String::with_capacity(text.len() + unit.len());
    new_text.push_str(&text[..byte]);
    new_text.push_str(&unit);
    new_text.push_str(&text[byte..]);
    (new_text, char_index + unit.chars().count())
}

/// Insert a newline that keeps the current line's indentation and adds one
/// extra level after an opening bracket. `start..end` is replaced.
pub fn auto_indent(
    text: &str,
    start: usize,
    end: usize,
    width: usize,
    use_spaces: bool,
) -> (String, usize) {
    let start_byte = char_to_byte(text, start);
    let end_byte = char_to_byte(text, end.max(start));
    let line_start = text[..start_byte]
        .rfind('\n')
        .map(|index| index + 1)
        .unwrap_or(0);
    let prefix = &text[line_start..start_byte];
    let indentation: String = prefix
        .chars()
        .take_while(|ch| *ch == ' ' || *ch == '\t')
        .collect();
    let mut insertion = String::from("\n");
    insertion.push_str(&indentation);
    if matches!(
        text[..start_byte].chars().next_back(),
        Some('{') | Some('[') | Some('(')
    ) {
        insertion.push_str(&indent_unit(width, use_spaces));
    }
    let cursor = start + insertion.chars().count();
    let mut new_text = String::with_capacity(text.len() + insertion.len());
    new_text.push_str(&text[..start_byte]);
    new_text.push_str(&insertion);
    new_text.push_str(&text[end_byte..]);
    (new_text, cursor)
}

/// Indent or unindent all lines touched by `start..=end`.
pub fn change_lines(
    text: &str,
    start: usize,
    end: usize,
    width: usize,
    use_spaces: bool,
    unindent: bool,
) -> (String, usize, usize) {
    let end_for_line = if end > start {
        end.saturating_sub(1)
    } else {
        end
    };
    let (first_line, _) = line_column_at_char(text, start);
    let (last_line, _) = line_column_at_char(text, end_for_line);
    let lines = text.split_inclusive('\n').collect::<Vec<_>>();
    let first_index = first_line.saturating_sub(1);
    let last_index = (last_line.saturating_sub(1)).min(lines.len().saturating_sub(1));
    if lines.is_empty() || first_index >= lines.len() {
        return (text.to_owned(), start, end);
    }

    let unit = indent_unit(width, use_spaces);
    let mut result =
        String::with_capacity(text.len() + unit.len() * (last_index - first_index + 1));
    let mut new_start = 0usize;
    let mut new_end = 0usize;

    for (index, line) in lines.iter().enumerate() {
        if index == first_index {
            new_start = result.chars().count();
        }
        if index >= first_index && index <= last_index {
            if unindent {
                let mut remove_bytes = 0usize;
                for ch in line.chars().take(width) {
                    if ch == ' ' {
                        remove_bytes += 1;
                    } else if ch == '\t' {
                        remove_bytes += 1;
                        break;
                    } else {
                        break;
                    }
                }
                result.push_str(&line[remove_bytes..]);
            } else {
                result.push_str(&unit);
                result.push_str(line);
            }
        } else {
            result.push_str(line);
        }
        if index == last_index {
            new_end = result.chars().count();
        }
    }

    (result, new_start, new_end.max(new_start))
}

fn char_to_byte(text: &str, char_index: usize) -> usize {
    if char_index == 0 {
        return 0;
    }
    text.char_indices()
        .nth(char_index)
        .map(|(index, _)| index)
        .unwrap_or(text.len())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn inserts_spaces_or_tab_at_cursor() {
        assert_eq!(
            insert_at_cursor("abc", 1, 4, true),
            ("a    bc".to_owned(), 5)
        );
        assert_eq!(
            insert_at_cursor("abc", 1, 4, false),
            ("a\tbc".to_owned(), 2)
        );
    }

    #[test]
    fn auto_indent_keeps_existing_prefix() {
        let text = "    if (ready) {\n        run();\n    }\n";
        let cursor = text.chars().count() - 7;
        let (new_text, new_cursor) = auto_indent(text, cursor, cursor, 4, true);
        assert_eq!(new_text.matches('\n').count(), 4);
        assert!(new_text.contains("\n        \n"));
        assert_eq!(new_text.chars().nth(new_cursor).unwrap_or('\0'), '\n');
    }

    #[test]
    fn auto_indent_adds_level_after_open_brace() {
        let text = "if (ready) {";
        let (new_text, cursor) =
            auto_indent(text, text.chars().count(), text.chars().count(), 4, true);
        assert_eq!(new_text, "if (ready) {\n    ");
        assert_eq!(cursor, new_text.chars().count());
    }

    #[test]
    fn auto_indent_replaces_selection() {
        let text = "abcXYZdef";
        let (new_text, cursor) = auto_indent(text, 3, 6, 4, true);
        assert_eq!(new_text, "abc\ndef");
        assert_eq!(cursor, 4);
    }

    #[test]
    fn indents_selected_lines() {
        let text = "int a;\nint b;\nint c;\n";
        let (new_text, start, end) = change_lines(text, 0, 13, 4, true, false);
        assert_eq!(new_text, "    int a;\n    int b;\nint c;\n");
        assert_eq!(&new_text[start..start], "");
        assert!(end > start);
    }

    #[test]
    fn unindents_mixed_indentation() {
        let text = "    int a;\n  int b;\n\tint c;\n";
        let (new_text, _, _) = change_lines(text, 0, 30, 4, true, true);
        assert_eq!(new_text, "int a;\nint b;\nint c;\n");
    }

    #[test]
    fn unindent_does_not_eat_code() {
        let (new_text, _, _) = change_lines("abc\n", 0, 1, 4, true, true);
        assert_eq!(new_text, "abc\n");
    }
}
