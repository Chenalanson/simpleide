//! Identifier helpers for lightweight go-to-definition and completion.

pub fn word_at(text: &str, char_index: usize) -> Option<(String, usize, usize)> {
    let chars = text.chars().collect::<Vec<_>>();
    if chars.is_empty() {
        return None;
    }
    let mut index = char_index.min(chars.len());
    if index == chars.len() || !is_identifier(chars[index]) {
        if index > 0 && is_identifier(chars[index - 1]) {
            index -= 1;
        } else {
            return None;
        }
    }
    let mut start = index;
    while start > 0 && is_identifier(chars[start - 1]) {
        start -= 1;
    }
    let mut end = index + 1;
    while end < chars.len() && is_identifier(chars[end]) {
        end += 1;
    }
    Some((chars[start..end].iter().collect(), start, end))
}

pub fn word_prefix(text: &str, char_index: usize) -> (usize, String) {
    let chars = text.chars().collect::<Vec<_>>();
    let mut start = char_index.min(chars.len());
    while start > 0 && is_identifier(chars[start - 1]) {
        start -= 1;
    }
    (
        start,
        chars[start..char_index.min(chars.len())].iter().collect(),
    )
}

pub fn replace_range_chars(
    text: &str,
    start: usize,
    end: usize,
    replacement: &str,
) -> (String, usize) {
    let chars = text.chars().collect::<Vec<_>>();
    let start = start.min(chars.len());
    let end = end.min(chars.len()).max(start);
    let mut result = String::with_capacity(text.len() + replacement.len());
    result.extend(chars[..start].iter());
    result.push_str(replacement);
    result.extend(chars[end..].iter());
    (result, start + replacement.chars().count())
}

fn is_identifier(ch: char) -> bool {
    ch.is_ascii_alphanumeric() || ch == '_'
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn finds_identifier_at_or_before_cursor() {
        let text = "int value_1 = other;";
        assert_eq!(word_at(text, 8), Some(("value_1".to_owned(), 4, 11)));
        assert_eq!(word_at(text, 11), Some(("value_1".to_owned(), 4, 11)));
        assert_eq!(
            word_at(text, 3).map(|value| value.0),
            Some("int".to_owned())
        );
        assert_eq!(word_at(" value", 0), None);
    }

    #[test]
    fn extracts_prefix_for_completion() {
        assert_eq!(word_prefix("int val", 7), (4, "val".to_owned()));
        assert_eq!(word_prefix("int ", 4), (4, String::new()));
    }

    #[test]
    fn replaces_character_ranges() {
        let (text, cursor) = replace_range_chars("int va = 1;", 4, 6, "value");
        assert_eq!(text, "int value = 1;");
        assert_eq!(cursor, 9);
    }
}
