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
    /// `None` for registers read from a support `.pack`; there is no file to
    /// jump to without extracting the package.
    pub path: Option<PathBuf>,
    pub line: usize,
}

pub struct RegisterIndex {
    pub items: Vec<RegisterItem>,
    pub source: String,
    pub truncated: bool,
}

pub fn scan_project(
    root: &Path,
    ignore: &IgnoreRules,
    device: Option<&str>,
) -> Result<RegisterIndex, String> {
    let files = collect_files(root, ignore);
    if let Some(svd) = files.iter().find(|path| {
        path.extension()
            .is_some_and(|extension| extension.eq_ignore_ascii_case("svd"))
    }) {
        if let Ok(text) = read_text_file(svd) {
            let (items, truncated) = parse_svd(&text.text, Some(svd));
            if !items.is_empty() {
                return Ok(RegisterIndex {
                    items,
                    source: format!("SVD · {}", crate::util::file_name(svd)),
                    truncated,
                });
            }
        }
    }

    if let Some(pack) = files.iter().find(|path| {
        path.extension()
            .is_some_and(|extension| extension.eq_ignore_ascii_case("pack"))
    }) {
        if let Some((name, text)) = crate::support_pack::read_first_svd(pack) {
            let (items, truncated) = parse_svd(&text, None);
            if !items.is_empty() {
                return Ok(RegisterIndex {
                    items,
                    source: format!("支持包 · {name}"),
                    truncated,
                });
            }
        }
    }

    if let Some(index) = scan_keil_svd(device) {
        return Ok(index);
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
                path: Some(path.clone()),
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

/// Find a matching SVD in installed Keil/CMSIS pack directories when the
/// project itself does not contain one.
pub fn scan_keil_svd(device: Option<&str>) -> Option<RegisterIndex> {
    let token = device_family_token(device?);
    if token.is_empty() {
        return None;
    }
    let mut visited = 0usize;
    for root in keil_svd_roots() {
        if let Some(svd) = find_svd(&root, &token, 0, &mut visited) {
            let Ok(loaded) = read_text_file(&svd) else {
                continue;
            };
            let (items, truncated) = parse_svd(&loaded.text, Some(&svd));
            if !items.is_empty() {
                return Some(RegisterIndex {
                    items,
                    source: format!("Keil SVD · {}", crate::util::file_name(&svd)),
                    truncated,
                });
            }
        }
    }
    None
}

fn find_svd(dir: &Path, token: &str, depth: usize, visited: &mut usize) -> Option<PathBuf> {
    if depth > 7 || *visited > 20_000 {
        return None;
    }
    let Ok(entries) = std::fs::read_dir(dir) else {
        return None;
    };
    for entry in entries.flatten() {
        *visited += 1;
        if *visited > 20_000 {
            return None;
        }
        let path = entry.path();
        let Ok(file_type) = entry.file_type() else {
            continue;
        };
        if file_type.is_dir() {
            if let Some(found) = find_svd(&path, token, depth + 1, visited) {
                return Some(found);
            }
            continue;
        }
        let name = entry.file_name().to_string_lossy().to_ascii_lowercase();
        if file_type.is_file() && name.ends_with(".svd") && name.contains(token) {
            return Some(path);
        }
    }
    None
}

fn keil_svd_roots() -> Vec<PathBuf> {
    let mut roots = Vec::new();
    if let Some(root) = std::env::var_os("KEIL_PACK_ROOT") {
        roots.push(PathBuf::from(root));
    }
    if let Some(local) = std::env::var_os("LOCALAPPDATA") {
        roots.push(PathBuf::from(local).join("Arm").join("Packs"));
    }
    if let Some(roaming) = std::env::var_os("APPDATA") {
        roots.push(PathBuf::from(roaming).join("Arm").join("Packs"));
    }
    for key in ["ProgramFiles", "ProgramFiles(x86)"] {
        if let Some(base) = std::env::var_os(key) {
            let base = PathBuf::from(base);
            roots.push(base.join("Keil_v5").join("ARM").join("PACK"));
            roots.push(base.join("Keil_v5").join("Pack"));
            roots.push(base.join("Keil").join("ARM").join("PACK"));
        }
    }
    roots.push(PathBuf::from(r"D:\Keil5\Pack"));
    roots.push(PathBuf::from(r"D:\Keil5\Core\ARM\PACK"));
    roots.push(PathBuf::from(r"C:\Keil_v5\ARM\PACK"));
    roots.push(PathBuf::from(r"C:\Keil_v5\Pack"));
    let mut seen = HashSet::new();
    roots.retain(|root| seen.insert(root.to_string_lossy().to_ascii_lowercase()));
    roots
}

fn device_family_token(device: &str) -> String {
    let upper = device.trim().to_ascii_uppercase();
    if upper.starts_with("STM32") && upper.len() >= 7 {
        upper[..7].to_ascii_lowercase()
    } else {
        upper.to_ascii_lowercase()
    }
}

fn parse_svd(text: &str, path: Option<&Path>) -> (Vec<RegisterItem>, bool) {
    let Ok(peripheral_re) = Regex::new(r"(?is)<peripheral\b([^>]*)>(.*?)</peripheral>") else {
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
    let derived_re = Regex::new(r#"(?i)derivedFrom\s*=\s*"([^"]+)""#).ok();

    let mut items = Vec::new();
    for peripheral in peripheral_re.captures_iter(text) {
        let Some(attrs) = peripheral.get(1) else {
            continue;
        };
        let Some(block) = peripheral.get(2) else {
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

        let mut sources = vec![(block_text, block_line)];
        if let Some(derived) = derived_re
            .as_ref()
            .and_then(|regex| regex.captures(attrs.as_str()))
            .and_then(|captures| captures.get(1))
            .map(|value| value.as_str().to_owned())
        {
            if let Some(base_match) = find_peripheral(text, &derived) {
                if let Some(base_block) = base_match.get(1) {
                    let line = text[..base_match.get(0).map(|m| m.start()).unwrap_or(0)]
                        .lines()
                        .count()
                        + 1;
                    sources.push((base_block.as_str(), line));
                }
            }
        }

        for (source_text, source_line) in sources {
            let mut found = false;
            for register in register_re.captures_iter(source_text) {
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
                    .map(|value| clean_description(value.as_str()))
                    .unwrap_or_default();
                let register_start = register.get(0).map(|matched| matched.start()).unwrap_or(0);
                let line = source_line + source_text[..register_start].lines().count();
                items.push(RegisterItem {
                    name: format!("{peripheral_name}.{name}"),
                    address: base.wrapping_add(offset),
                    description,
                    path: path.map(Path::to_path_buf),
                    line,
                });
                found = true;
                if items.len() >= MAX_REGISTERS {
                    return (items, true);
                }
            }
            if found {
                break;
            }
        }
    }
    (items, false)
}

fn find_peripheral<'a>(text: &'a str, name: &str) -> Option<regex::Captures<'a>> {
    let pattern = format!(
        r#"(?is)<peripheral\b[^>]*>\s*<name>\s*{}\s*</name>(.*?)</peripheral>"#,
        regex::escape(name)
    );
    Regex::new(&pattern).ok()?.captures(text)
}

fn clean_description(value: &str) -> String {
    let tag_re = Regex::new(r"(?is)<[^>]+>").ok();
    tag_re
        .map(|tag| tag.replace_all(value, " "))
        .unwrap_or_else(|| value.into())
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
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

    const SAMPLE_SVD: &str = r#"<device>
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

    #[test]
    fn parses_svd_peripherals_and_registers() {
        let (items, truncated) = parse_svd(SAMPLE_SVD, Some(Path::new("chip.svd")));
        assert!(!truncated);
        assert_eq!(items.len(), 2);
        assert_eq!(items[0].name, "GPIOA.CRL");
        assert_eq!(items[0].address, 0x4001_0800);
        assert_eq!(items[1].address, 0x4001_080C);
        assert!(items[0].description.contains("Port configuration"));
    }

    #[test]
    fn derived_peripherals_reuse_register_layout() {
        let svd = r#"<device><peripherals>
  <peripheral>
    <name>GPIOA</name>
    <baseAddress>0x40010800</baseAddress>
    <registers>
      <register><name>CRL</name><addressOffset>0x00</addressOffset></register>
    </registers>
  </peripheral>
  <peripheral derivedFrom="GPIOA">
    <name>GPIOB</name>
    <baseAddress>0x40010C00</baseAddress>
  </peripheral>
</peripherals></device>"#;
        let (items, _) = parse_svd(svd, None);
        assert!(items.iter().any(|item| item.name == "GPIOA.CRL"));
        let gpio_b = items
            .iter()
            .find(|item| item.name == "GPIOB.CRL")
            .expect("derived register should exist");
        assert_eq!(gpio_b.address, 0x4001_0C00);
    }

    #[test]
    fn scans_svd_inside_support_pack() {
        use std::io::Write;

        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("chip.pack");
        let file = std::fs::File::create(&path).unwrap();
        let mut zip = zip::ZipWriter::new(file);
        let options = zip::write::SimpleFileOptions::default();
        zip.start_file("CMSIS/SVD/chip.svd", options).unwrap();
        zip.write_all(SAMPLE_SVD.as_bytes()).unwrap();
        zip.finish().unwrap();

        let index = scan_project(dir.path(), &IgnoreRules::default(), None).unwrap();
        assert!(index.source.starts_with("支持包 · "));
        assert_eq!(index.items.len(), 2);
        assert_eq!(index.items[0].name, "GPIOA.CRL");
        assert!(index.items[0].path.is_none());
    }

    #[test]
    fn parses_macro_addresses() {
        assert_eq!(parse_hex("0x40021000"), Some(0x4002_1000));
        assert_eq!(parse_hex("1024"), Some(1024));
        assert_eq!(parse_hex("nope"), None);
    }
}
