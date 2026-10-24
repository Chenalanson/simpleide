//! Column editing across a selected line range.

use crate::util::line_column_at_char;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ColumnOperation {
    Insert,
    Delete,
    Replace,
}

impl ColumnOperation {
    pub fn label(self) -> &'static str {
        match self {
            Self::Insert => "插入",
            Self::Delete => "删除",
            Self::Replace => "替换",
        }
    }
}

/// Apply a column operation to every line touched by `start..end`.
/// `column` is 1-based; lines shorter than the column are padded with spaces.
pub fn apply(
    text: &str,
    start: usize,
    end: usize,
    column: usize,
    count: usize,
    operation: ColumnOperation,
    insert: &str,
) -> (String, usize) {
    let end_for_line = if end > start {
        end.saturating_sub(1)
    } else {
        end
    };
    let (first_line, _) = line_column_at_char(text, start);
    let (last_line, _) = line_column_at_char(text, end_for_line);
    let line_start = first_line.saturating_sub(1);
    let line_end = last_line.saturating_sub(1);
    let column = column.max(1) - 1;
    let count = if operation == ColumnOperation::Insert {
        0
    } else {
        count.max(1)
    };

    let mut result = String::with_capacity(text.len() + insert.len() * 8);
    for (index, line) in text.split_inclusive('\n').enumerate() {
        if index < line_start || index > line_end {
            result.push_str(line);
            continue;
        }
        let (body, ending) = match line.strip_suffix('\n') {
            Some(body) => (body, "\n"),
            None => (line, ""),
        };
        let chars = body.chars().collect::<Vec<_>>();
        let mut edited = String::with_capacity(body.len() + insert.len() + 8);
        let column_chars = column.min(chars.len());
        edited.extend(chars[..column_chars].iter());
        for _ in chars.len()..column {
            edited.push(' ');
        }
        let suffix_start = match operation {
            ColumnOperation::Insert => column_chars,
            ColumnOperation::Delete => (column + count).min(chars.len()),
            ColumnOperation::Replace => {
                edited.push_str(insert);
                (column + count).min(chars.len())
            }
        };
        if operation == ColumnOperation::Insert {
            edited.push_str(insert);
        }
        edited.extend(chars[suffix_start..].iter());
        result.push_str(&edited);
        result.push_str(ending);
    }
    let cursor = (start + insert.chars().count()).min(result.chars().count());
    (result, cursor)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn inserts_at_column_across_lines() {
        let text = "a\nlonger\nxy\n";
        let (result, _) = apply(text, 0, text.len(), 3, 1, ColumnOperation::Insert, "!");
        assert_eq!(result, "a !\nlo!nger\nxy!\n");
    }

    #[test]
    fn deletes_and_replaces_columns() {
        let text = "abcdef\nabcdef\n";
        let (result, _) = apply(text, 0, text.len(), 2, 2, ColumnOperation::Delete, "");
        assert_eq!(result, "adef\nadef\n");
        let (result, _) = apply(text, 0, text.len(), 2, 2, ColumnOperation::Replace, "XY");
        assert_eq!(result, "aXYdef\naXYdef\n");
    }

    #[test]
    fn pads_short_lines() {
        let text = "a\n";
        let (result, _) = apply(text, 0, text.len(), 4, 1, ColumnOperation::Insert, "x");
        assert_eq!(result, "a  x\n");
    }
}
