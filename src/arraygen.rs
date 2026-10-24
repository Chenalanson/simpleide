//! Generate C array literals from text selections or file bytes.

pub fn generate(bytes: &[u8], name: &str, hex: bool, per_line: usize) -> String {
    let name = sanitize_name(name);
    if bytes.is_empty() {
        return format!("/* {name}: 空数据 */\n");
    }
    let per_line = per_line.clamp(4, 32);
    let mut output = String::with_capacity(bytes.len() * 6 + 64);
    output.push_str(&format!(
        "/* {} 字节 */\nconst uint8_t {name}[{}] = {{\n",
        bytes.len(),
        bytes.len()
    ));
    for chunk in bytes.chunks(per_line) {
        output.push_str("    ");
        for (index, byte) in chunk.iter().enumerate() {
            if index > 0 {
                output.push_str(", ");
            }
            if hex {
                output.push_str(&format!("0x{byte:02X}"));
            } else {
                output.push_str(&byte.to_string());
            }
        }
        output.push_str(",\n");
    }
    output.push_str("};\n");
    output
}

pub fn sanitize_name(name: &str) -> String {
    let mut result = String::with_capacity(name.len());
    for ch in name.chars() {
        if ch.is_ascii_alphanumeric() || ch == '_' {
            result.push(ch);
        } else if !result.ends_with('_') {
            result.push('_');
        }
    }
    let result = result.trim_matches('_').to_owned();
    if result.is_empty() {
        return "data".to_owned();
    }
    if result.chars().next().is_some_and(|ch| ch.is_ascii_digit()) {
        format!("data_{result}")
    } else {
        result
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn generates_hex_and_decimal_arrays() {
        let hex = generate(&[0, 1, 0xFF], "buffer", true, 2);
        assert!(hex.contains("const uint8_t buffer[3] = {"));
        assert!(hex.contains("0x00, 0x01,"));
        assert!(hex.contains("0xFF,"));
        let dec = generate(&[10, 20], "values", false, 8);
        assert!(dec.contains("10, 20,"));
    }

    #[test]
    fn sanitizes_identifier() {
        assert_eq!(sanitize_name("main-data"), "main_data");
        assert_eq!(sanitize_name("123"), "data_123");
        assert_eq!(sanitize_name("  "), "data");
        assert_eq!(sanitize_name("already_ok"), "already_ok");
    }

    #[test]
    fn empty_input_is_a_comment() {
        let output = generate(&[], "empty", true, 16);
        assert!(output.contains("空数据"));
    }
}
