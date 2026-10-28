//! Pure text edits shared by the multi-cursor editor paths.
//!
//! The editor keeps the primary selection in egui's `TextEditState`; secondary
//! selections are only character ranges. Applying edits from the highest range
//! downwards keeps the lower indexes valid, and the mapping helpers rebuild
//! every caret after insertions or deletions.

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct EditResult {
    pub text: String,
    pub primary: usize,
    pub secondary: Vec<usize>,
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

fn normalize_range((start, end): (usize, usize), len: usize) -> (usize, usize) {
    let start = start.min(len);
    let end = end.min(len);
    (start.min(end), start.max(end))
}

fn collect_ranges(
    len: usize,
    primary: (usize, usize),
    secondary: &[(usize, usize)],
) -> Vec<(usize, usize)> {
    let mut ranges = Vec::with_capacity(secondary.len() + 1);
    ranges.push(normalize_range(primary, len));
    ranges.extend(secondary.iter().map(|range| normalize_range(*range, len)));
    ranges.sort_unstable();
    ranges.dedup();
    ranges
}

fn map_cursor(cursor: usize, edits: &[(usize, usize, String)]) -> usize {
    let mut delta = 0isize;
    for (start, end, replacement) in edits {
        if cursor < *start {
            break;
        }
        if cursor <= *end {
            return (*start as isize + delta + replacement.chars().count() as isize) as usize;
        }
        delta += replacement.chars().count() as isize - (*end as isize - *start as isize);
    }
    (cursor as isize + delta).max(0) as usize
}

fn apply_edits(
    text: &str,
    edits: &[(usize, usize, String)],
    primary: (usize, usize),
    secondary: &[(usize, usize)],
    len: usize,
) -> EditResult {
    let mut result = text.to_owned();
    for (start, end, replacement) in edits.iter().rev() {
        let start_byte = char_to_byte(&result, *start);
        let end_byte = char_to_byte(&result, *end);
        result.replace_range(start_byte..end_byte, replacement);
    }

    let primary_end = normalize_range(primary, len).1;
    let primary = map_cursor(primary_end, edits);
    let mut mapped = secondary
        .iter()
        .map(|range| map_cursor(normalize_range(*range, len).1, edits))
        .filter(|cursor| *cursor != primary)
        .collect::<Vec<_>>();
    mapped.sort_unstable();
    mapped.dedup();
    EditResult {
        text: result,
        primary,
        secondary: mapped,
    }
}

/// Insert the same text at the primary selection and every secondary selection.
pub fn insert(
    text: &str,
    primary: (usize, usize),
    secondary: &[(usize, usize)],
    inserted: &str,
) -> EditResult {
    let len = text.chars().count();
    let edits = collect_ranges(len, primary, secondary)
        .into_iter()
        .map(|(start, end)| (start, end, inserted.to_owned()))
        .collect::<Vec<_>>();
    apply_edits(text, &edits, primary, secondary, len)
}

/// Delete one character before every caret, or the selection when one exists.
pub fn backspace(text: &str, primary: (usize, usize), secondary: &[(usize, usize)]) -> EditResult {
    let len = text.chars().count();
    let mut edits = collect_ranges(len, primary, secondary)
        .into_iter()
        .filter_map(|(start, end)| {
            if end > start {
                Some((start, end, String::new()))
            } else {
                (start > 0).then(|| (start - 1, start, String::new()))
            }
        })
        .collect::<Vec<_>>();
    edits.dedup();
    apply_edits(text, &edits, primary, secondary, len)
}

/// Delete the character after every caret, or the selection when one exists.
pub fn delete(text: &str, primary: (usize, usize), secondary: &[(usize, usize)]) -> EditResult {
    let len = text.chars().count();
    let edits = collect_ranges(len, primary, secondary)
        .into_iter()
        .filter_map(|(start, end)| {
            if end > start {
                Some((start, end, String::new()))
            } else {
                (start < len).then(|| (start, start + 1, String::new()))
            }
        })
        .collect::<Vec<_>>();
    apply_edits(text, &edits, primary, secondary, len)
}

/// Insert an indented newline at every selection, reusing the single-cursor
/// `indent::auto_indent` rules.
pub fn enter(
    text: &str,
    primary: (usize, usize),
    secondary: &[(usize, usize)],
    width: usize,
    use_spaces: bool,
) -> EditResult {
    let len = text.chars().count();
    let mut edits = Vec::new();
    for (start, end) in collect_ranges(len, primary, secondary) {
        let (new_text, new_cursor) =
            crate::indent::auto_indent(text, start, end, width, use_spaces);
        let inserted = new_text
            .chars()
            .skip(start)
            .take(new_cursor.saturating_sub(start))
            .collect::<String>();
        edits.push((start, end, inserted));
    }
    apply_edits(text, &edits, primary, secondary, len)
}

/// Collect selected text from the primary and secondary selections.
pub fn selected_text(text: &str, primary: (usize, usize), secondary: &[(usize, usize)]) -> String {
    let chars = text.chars().collect::<Vec<_>>();
    let len = chars.len();
    let mut values = Vec::new();
    for (start, end) in collect_ranges(len, primary, secondary) {
        if end > start {
            values.push(chars[start..end].iter().collect::<String>());
        }
    }
    values.join("\n")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn inserts_at_every_caret_without_shifting_earlier_positions() {
        let result = insert("abcd", (1, 1), &[(4, 4)], "X");
        assert_eq!(result.text, "aXbcdX");
        assert_eq!(result.primary, 2);
        assert_eq!(result.secondary, vec![6]);
    }

    #[test]
    fn replaces_primary_and_secondary_selections() {
        let result = insert("one two one", (0, 3), &[(8, 11)], "X");
        assert_eq!(result.text, "X two X");
        assert_eq!(result.primary, 1);
        assert_eq!(result.secondary, vec![7]);
    }

    #[test]
    fn backspace_deletes_one_character_per_caret() {
        let result = backspace("abcdef", (2, 2), &[(4, 4)]);
        assert_eq!(result.text, "acef");
        assert_eq!(result.primary, 1);
        assert_eq!(result.secondary, vec![2]);
    }

    #[test]
    fn backspace_deletes_selected_ranges() {
        let result = backspace("one two one", (0, 3), &[(8, 11)]);
        assert_eq!(result.text, " two ");
        assert_eq!(result.primary, 0);
        assert_eq!(result.secondary, vec![5]);
    }

    #[test]
    fn delete_removes_the_character_after_every_caret() {
        let result = delete("abcdef", (1, 1), &[(3, 3)]);
        assert_eq!(result.text, "acef");
        assert_eq!(result.primary, 1);
        assert_eq!(result.secondary, vec![2]);
    }

    #[test]
    fn enter_keeps_indentation_for_each_caret() {
        let text = "if (ready) {\nrun();\n}";
        let end_of_if = "if (ready) {".chars().count();
        let result = enter(
            text,
            (end_of_if, end_of_if),
            &[(end_of_if + 10, end_of_if + 10)],
            4,
            true,
        );
        assert!(result.text.contains("if (ready) {\n    "));
        assert_eq!(result.text.matches('\n').count(), 4);
    }

    #[test]
    fn duplicate_carets_are_merged() {
        let result = insert("ab", (1, 1), &[(1, 1)], "Z");
        assert_eq!(result.text, "aZb");
        assert_eq!(result.primary, 2);
        assert!(result.secondary.is_empty());
    }

    #[test]
    fn copies_all_selected_ranges() {
        assert_eq!(selected_text("one two one", (0, 3), &[(8, 11)]), "one\none");
    }
}
