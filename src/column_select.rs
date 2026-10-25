//! Rectangular (column) selection text operations.

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ColumnRange {
    /// 0-based, inclusive.
    pub start_line: usize,
    pub end_line: usize,
    /// 0-based, exclusive.
    pub start_col: usize,
    pub end_col: usize,
}

impl ColumnRange {
    pub fn normalized(
        anchor_line: usize,
        anchor_col: usize,
        cursor_line: usize,
        cursor_col: usize,
    ) -> Self {
        Self {
            start_line: anchor_line.min(cursor_line),
            end_line: anchor_line.max(cursor_line),
            start_col: anchor_col.min(cursor_col),
            end_col: anchor_col.max(cursor_col),
        }
    }
}

pub fn selected_text(text: &str, range: ColumnRange) -> String {
    let lines = text.split('\n').collect::<Vec<_>>();
    let width = range.end_col.saturating_sub(range.start_col);
    let mut selected = Vec::new();
    for line_index in range.start_line..=range.end_line {
        let Some(line) = lines.get(line_index) else {
            continue;
        };
        let chars = line.chars().collect::<Vec<_>>();
        let mut value = chars
            .iter()
            .skip(range.start_col)
            .take(width)
            .collect::<String>();
        let actual = value.chars().count();
        if actual < width {
            value.push_str(&" ".repeat(width - actual));
        }
        selected.push(value);
    }
    selected.join("\n")
}

pub fn replace(text: &str, range: ColumnRange, insert: &str) -> String {
    let lines = text.split_inclusive('\n').collect::<Vec<_>>();
    let mut result = String::with_capacity(text.len() + insert.len() * 8);
    for (index, line) in lines.iter().enumerate() {
        if index < range.start_line || index > range.end_line {
            result.push_str(line);
            continue;
        }
        let (body, ending) = match line.strip_suffix('\n') {
            Some(body) => (body, "\n"),
            None => (*line, ""),
        };
        let chars = body.chars().collect::<Vec<_>>();
        let start = range.start_col.min(chars.len());
        let end = range.end_col.min(chars.len()).max(start);
        result.extend(chars[..start].iter());
        if !insert.is_empty() && start > chars.len() {
            result.push_str(&" ".repeat(start - chars.len()));
        }
        result.push_str(insert);
        if end < chars.len() || start < chars.len() {
            result.extend(chars[end..].iter());
        }
        result.push_str(ending);
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn copies_rectangular_text_with_padding() {
        let text = "abcdef\nxy\nabcdef\n";
        let range = ColumnRange::normalized(0, 1, 2, 4);
        assert_eq!(selected_text(text, range), "bcd\ny  \nbcd");
    }

    #[test]
    fn deletes_columns_across_lines() {
        let text = "abcdef\nabcdef\n";
        let range = ColumnRange::normalized(0, 2, 1, 4);
        assert_eq!(replace(text, range, ""), "abef\nabef\n");
    }

    #[test]
    fn replaces_columns_with_text() {
        let text = "abcdef\nxy\n";
        let range = ColumnRange::normalized(0, 2, 1, 4);
        assert_eq!(replace(text, range, "Z"), "abZef\nxyZ\n");
    }
}
