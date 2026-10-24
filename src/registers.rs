//! Peripheral register browser: SVD files first, CMSIS-style macro fallback.

use std::collections::HashSet;
use std::path::{Path, PathBuf};

use regex::Regex;

use crate::ignore::IgnoreRules;
use crate::util::read_text_file;

const MAX_REGISTERS: usize = 2000;
const MAX_REGISTER_FILE_BYTES: u64 = 4 * 1024 * 1024;

#[derive(Clone, Debug)]
pub struct RegisterItem {
    pub name: String,
    pub address: u64,
    pub description: String,
    pub path: PathBuf,
    pub line: usize,
}

pub struct RegisterIndex {
    pub items: Vec<RegisterItem>,
    pub source: String,
    pub truncated: bool,
}

pub fn scan_project(root: &Path, ignore: &IgnoreRules) -> Result<RegisterIndex, String> {
    let files = collect_files(root, ignore);
    if let Some(svd) = files.iter().find(|path| {
        path.extension()
            .is_some_and(|extension| extension.eq_ignore_ascii_case("svd"))
    }) {
        if let Ok(text) = read_text_file(svd) {
            let (items, truncated) = parse_svd(&text.text, svd);
            if !items.is_empty() {
                return Ok(RegisterIndex {
                    items,
                    source: format!("SVD · {}", crate::util::file_name(svd)),
                    truncated,
                });
            }
        }
    }

    let mut items = Vec::new();
    let mut seen = HashSet::new();
    let mut truncated = false;
    let macro_re = Regex::new(
        r"(?m)^\s*#\s*define\s+([A-Za-z_]\w*(?:_BASE|_BASEADDR|_ADDR|_ADDRESS))\s+([^/\r\n]+)",
    )
    .map_err(|err| err.to_string())?;
    let hex_re = Regex::new(r"0[xX][0-9A-Fa-f]+").map_err(|err| err.to_string())?;
    for path in files.iter().filter(|path| {
        path.extension().is_some_and(|extension| {
            matches!(
                extension.to_string_lossy().to_lowercase().as_str(),
                "h" | "hpp" | "c" | "cpp"
            )
        })
    }) {
        let Ok(metadata) = std::fs::metadata(path) else {
            continue;
        };
        if metadata.len() > MAX_REGISTER_FILE_BYTES {
            continue;
        }
        let Ok(loaded) = read_text_file(path) else {
            continue;
        };
        for (line_index, line) in loaded.text.lines().enumerate() {
            let Some(captures) = macro_re.captures(line) else {
                continue;
            };
            let Some(name) = captures.get(1) else {
                continue;
            };
            let value = captures.get(2).map(|value| value.as_str()).unwrap_or("");
            let Some(address) = hex_re
                .find(value)
                .and_then(|matched| parse_hex(matched.as_str()))
            else {
                continue;
            };
            if !seen.insert((name.as_str().to_owned(), address)) {
                continue;
            }
            items.push(RegisterItem {
                name: name.as_str().to_owned(),
                address,
                description: "头文件宏定义".to_owned(),
                path: path.clone(),
                line: line_index + 1,
            });
            if items.len() >= MAX_REGISTERS {
                truncated = true;
                break;
            }
        }
        if items.len() >= MAX_REGISTERS {
            break;
        }
    }
    Ok(RegisterIndex {
        items,
        source: "头文件寄存器宏".to_owned(),
        truncated,
    })
}

fn collect_files(root: &Path, ignore: &IgnoreRules) -> Vec<PathBuf> {
    let mut files = Vec::new();
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
                if !ignore.is_ignored(relative, true) {
                    stack.push(path);
                }
                continue;
            }
            if file_type.is_file() && !ignore.is_ignored(relative, false) {
                files.push(path);
            }
        }
    }
    files
}

fn parse_svd(text: &str, path: &Path) -> (Vec<RegisterItem>, bool) {
    let Ok(peripheral_re) = Regex::new(r"(?is)<peripheral\b[^>]*>(.*?)</peripheral>") else {
        return (Vec::new(), false);
    };
    let Ok(name_re) = Regex::new(r"(?is)<name>\s*([^<]+?)\s*</name>") else {
        return (Vec::new(), false);
    };
    let Ok(base_re) = Regex::new(r"(?is)<baseAddress>\s*([^<]+?)\s*</baseAddress>") else {
        return (Vec::new(), false);
    };
    let Ok(register_re) = Regex::new(r"(?is)<register\b[^>]*>(.*?)</register>") else {
        return (Vec::new(), false);
    };
    let Ok(offset_re) = Regex::new(r"(?is)<addressOffset>\s*([^<]+?)\s*</addressOffset>") else {
        return (Vec::new(), false);
    };
    let Ok(description_re) = Regex::new(r"(?is)<description>\s*(.*?)\s*</description>") else {
        return (Vec::new(), false);
    };
    let tag_re = Regex::new(r"(?is)<[^>]+>").ok();

    let mut items = Vec::new();
    for peripheral in peripheral_re.captures_iter(text) {
        let Some(block) = peripheral.get(1) else {
            continue;
        };
        let block_text = block.as_str();
        let block_line = text[..block.start()].lines().count() + 1;
        let Some(peripheral_name) = name_re
            .captures(block_text)
            .and_then(|captures| captures.get(1))
            .map(|value| value.as_str().trim().to_owned())
        else {
            continue;
        };
        let Some(base) = base_re
            .captures(block_text)
            .and_then(|captures| captures.get(1))
            .and_then(|value| parse_hex(value.as_str().trim()))
        else {
            continue;
        };
        for register in register_re.captures_iter(block_text) {
            let Some(register_block) = register.get(1) else {
                continue;
            };
            let register_text = register_block.as_str();
            let Some(name) = name_re
                .captures(register_text)
                .and_then(|captures| captures.get(1))
                .map(|value| value.as_str().trim().to_owned())
            else {
                continue;
            };
            let Some(offset) = offset_re
                .captures(register_text)
                .and_then(|captures| captures.get(1))
                .and_then(|value| parse_hex(value.as_str().trim()))
            else {
                continue;
            };
            let description = description_re
                .captures(register_text)
                .and_then(|captures| captures.get(1))
                .map(|value| {
                    tag_re
                        .as_ref()
                        .map(|tag| tag.replace_all(value.as_str(), " "))
                        .unwrap_or_else(|| value.as_str().into())
                        .split_whitespace()
                        .collect::<Vec<_>>()
                        .join(" ")
                })
                .unwrap_or_default();
            let register_start = register.get(0).map(|matched| matched.start()).unwrap_or(0);
            let line = block_line + block_text[..register_start].lines().count();
            items.push(RegisterItem {
                name: format!("{peripheral_name}.{name}"),
                address: base.wrapping_add(offset),
                description,
                path: path.to_path_buf(),
                line,
            });
            if items.len() >= MAX_REGISTERS {
                return (items, true);
            }
        }
    }
    (items, false)
}

fn parse_hex(value: &str) -> Option<u64> {
    let value = value.trim();
    if let Some(hex) = value
        .strip_prefix("0x")
        .or_else(|| value.strip_prefix("0X"))
    {
        u64::from_str_radix(hex, 16).ok()
    } else {
        value.parse().ok()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_svd_peripherals_and_registers() {
        let svd = r#"<device>
  <peripherals>
    <peripheral>
      <name>GPIOA</name>
      <baseAddress>0x40010800</baseAddress>
      <registers>
        <register>
          <name>CRL</name>
          <addressOffset>0x00</addressOffset>
          <description>Port configuration low</description>
        </register>
        <register>
          <name>ODR</name>
          <addressOffset>0x0C</addressOffset>
          <description>Output data register</description>
        </register>
      </registers>
    </peripheral>
  </peripherals>
</device>"#;
        let (items, truncated) = parse_svd(svd, Path::new("chip.svd"));
        assert!(!truncated);
        assert_eq!(items.len(), 2);
        assert_eq!(items[0].name, "GPIOA.CRL");
        assert_eq!(items[0].address, 0x4001_0800);
        assert_eq!(items[1].address, 0x4001_080C);
        assert!(items[0].description.contains("Port configuration"));
    }

    #[test]
    fn parses_macro_addresses() {
        assert_eq!(parse_hex("0x40021000"), Some(0x4002_1000));
        assert_eq!(parse_hex("1024"), Some(1024));
        assert_eq!(parse_hex("nope"), None);
    }
}
