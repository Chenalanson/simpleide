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

#[derive(Clone, Debug, Default)]
pub struct SvdField {
    pub name: String,
    pub offset: u32,
    pub width: u32,
    pub description: String,
    pub access: String,
}

#[derive(Clone, Debug, Default)]
pub struct SvdRegister {
    pub name: String,
    pub offset: u64,
    pub description: String,
    pub size: u32,
    pub access: String,
    pub fields: Vec<SvdField>,
    pub line: usize,
}

#[derive(Clone, Debug, Default)]
pub struct SvdPeripheral {
    pub name: String,
    pub base: u64,
    pub description: String,
    pub registers: Vec<SvdRegister>,
}

pub struct RegisterIndex {
    pub items: Vec<RegisterItem>,
    /// Structured SVD data for the debug peripheral tree. Empty when the
    /// fallback (header macros) is used.
    pub peripherals: Vec<SvdPeripheral>,
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
            let (peripherals, items, truncated) = parse_svd_full(&text.text, Some(svd));
            if !items.is_empty() {
                return Ok(RegisterIndex {
                    items,
                    peripherals,
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
            let (peripherals, items, truncated) = parse_svd_full(&text, None);
            if !items.is_empty() {
                return Ok(RegisterIndex {
                    items,
                    peripherals,
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
        peripherals: Vec::new(),
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
    let tokens = device_svd_tokens(device?);
    if tokens.is_empty() {
        return None;
    }
    for root in keil_svd_roots() {
        for token in &tokens {
            let mut visited = 0usize;
            let Some(svd) = find_svd(&root, token, 0, &mut visited) else {
                continue;
            };
            let Ok(loaded) = read_text_file(&svd) else {
                continue;
            };
            let (peripherals, items, truncated) = parse_svd_full(&loaded.text, Some(&svd));
            if !items.is_empty() {
                return Some(RegisterIndex {
                    items,
                    peripherals,
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

fn device_svd_tokens(device: &str) -> Vec<String> {
    let upper = device.trim().to_ascii_uppercase();
    if !upper.starts_with("STM32") {
        return if upper.is_empty() {
            Vec::new()
        } else {
            vec![upper.to_ascii_lowercase()]
        };
    }
    let mut tokens = Vec::new();
    if upper.len() >= 9 {
        tokens.push(upper[..9].to_ascii_lowercase());
    }
    if upper.len() >= 7 {
        let family = upper[..7].to_ascii_lowercase();
        if !tokens.contains(&family) {
            tokens.push(family);
        }
    }
    tokens
}

#[derive(Clone)]
struct RawRegister {
    name: String,
    offset: u64,
    description: String,
    size: u32,
    access: String,
    fields: Vec<SvdField>,
    derived: Option<String>,
    line: usize,
}

#[derive(Clone)]
struct RawPeripheral {
    name: String,
    base: u64,
    description: String,
    registers: Vec<RawRegister>,
    derived: Option<String>,
}

fn parse_svd_full(
    text: &str,
    path: Option<&Path>,
) -> (Vec<SvdPeripheral>, Vec<RegisterItem>, bool) {
    let Ok(peripheral_re) = Regex::new(r"(?is)<peripheral\b([^>]*)>(.*?)</peripheral>") else {
        return (Vec::new(), Vec::new(), false);
    };
    let Ok(name_re) = Regex::new(r"(?is)<name>\s*([^<]+?)\s*</name>") else {
        return (Vec::new(), Vec::new(), false);
    };
    let Ok(base_re) = Regex::new(r"(?is)<baseAddress>\s*([^<]+?)\s*</baseAddress>") else {
        return (Vec::new(), Vec::new(), false);
    };
    let Ok(register_re) = Regex::new(r"(?is)<register\b([^>]*)>(.*?)</register>") else {
        return (Vec::new(), Vec::new(), false);
    };
    let Ok(field_re) = Regex::new(r"(?is)<field\b[^>]*>(.*?)</field>") else {
        return (Vec::new(), Vec::new(), false);
    };
    let Ok(offset_re) = Regex::new(r"(?is)<addressOffset>\s*([^<]+?)\s*</addressOffset>") else {
        return (Vec::new(), Vec::new(), false);
    };
    let Ok(description_re) = Regex::new(r"(?is)<description>\s*(.*?)\s*</description>") else {
        return (Vec::new(), Vec::new(), false);
    };
    let Ok(size_re) = Regex::new(r"(?is)<size>\s*([^<]+?)\s*</size>") else {
        return (Vec::new(), Vec::new(), false);
    };
    let Ok(access_re) = Regex::new(r"(?is)<access>\s*([^<]+?)\s*</access>") else {
        return (Vec::new(), Vec::new(), false);
    };
    let Ok(bit_offset_re) = Regex::new(r"(?is)<bitOffset>\s*([^<]+?)\s*</bitOffset>") else {
        return (Vec::new(), Vec::new(), false);
    };
    let Ok(bit_width_re) = Regex::new(r"(?is)<bitWidth>\s*([^<]+?)\s*</bitWidth>") else {
        return (Vec::new(), Vec::new(), false);
    };
    let Ok(lsb_re) = Regex::new(r"(?is)<lsb>\s*([^<]+?)\s*</lsb>") else {
        return (Vec::new(), Vec::new(), false);
    };
    let Ok(msb_re) = Regex::new(r"(?is)<msb>\s*([^<]+?)\s*</msb>") else {
        return (Vec::new(), Vec::new(), false);
    };
    let Ok(bit_range_re) =
        Regex::new(r"(?is)<bitRange>\s*\[\s*(\d+)\s*:\s*(\d+)\s*\]\s*</bitRange>")
    else {
        return (Vec::new(), Vec::new(), false);
    };
    let derived_re = Regex::new(r#"(?i)derivedFrom\s*=\s*"([^"]+)""#).ok();

    let mut raw = Vec::new();
    for peripheral in peripheral_re.captures_iter(text) {
        let Some(attrs) = peripheral.get(1) else {
            continue;
        };
        let Some(block) = peripheral.get(2) else {
            continue;
        };
        let block_text = block.as_str();
        let block_line = text[..block.start()].lines().count() + 1;
        let Some(peripheral_name) = first_capture(&name_re, block_text) else {
            continue;
        };
        let Some(base) = first_capture(&base_re, block_text).and_then(|value| parse_hex(&value))
        else {
            continue;
        };
        let description = description_re
            .captures(block_text)
            .and_then(|captures| captures.get(1))
            .map(|value| clean_description(value.as_str()))
            .unwrap_or_default();
        let derived = derived_re
            .as_ref()
            .and_then(|regex| regex.captures(attrs.as_str()))
            .and_then(|captures| captures.get(1))
            .map(|value| value.as_str().trim().to_owned());

        let mut registers = Vec::new();
        for register in register_re.captures_iter(block_text) {
            let register_attrs = register.get(1).map(|value| value.as_str()).unwrap_or("");
            let Some(register_block) = register.get(2) else {
                continue;
            };
            let register_text = register_block.as_str();
            let Some(name) = first_capture(&name_re, register_text) else {
                continue;
            };
            let Some(offset) =
                first_capture(&offset_re, register_text).and_then(|value| parse_hex(&value))
            else {
                continue;
            };
            let size = first_capture(&size_re, register_text)
                .and_then(|value| value.trim().parse::<u32>().ok())
                .unwrap_or(32)
                .clamp(1, 64);
            let access = first_capture(&access_re, register_text).unwrap_or_default();
            let description = description_re
                .captures(register_text)
                .and_then(|captures| captures.get(1))
                .map(|value| clean_description(value.as_str()))
                .unwrap_or_default();
            let register_derived = derived_re
                .as_ref()
                .and_then(|regex| regex.captures(register_attrs))
                .and_then(|captures| captures.get(1))
                .map(|value| value.as_str().trim().to_owned());

            let mut fields = Vec::new();
            for field in field_re.captures_iter(register_text) {
                let Some(field_block) = field.get(1) else {
                    continue;
                };
                let field_text = field_block.as_str();
                let Some(field_name) = first_capture(&name_re, field_text) else {
                    continue;
                };
                let mut field_offset = first_capture(&bit_offset_re, field_text)
                    .and_then(|value| value.trim().parse::<u32>().ok());
                let mut field_width = first_capture(&bit_width_re, field_text)
                    .and_then(|value| value.trim().parse::<u32>().ok());
                if let Some(lsb) = first_capture(&lsb_re, field_text)
                    .and_then(|value| value.trim().parse::<u32>().ok())
                {
                    field_offset = Some(lsb);
                    if let Some(msb) = first_capture(&msb_re, field_text)
                        .and_then(|value| value.trim().parse::<u32>().ok())
                    {
                        field_width = Some(msb.saturating_sub(lsb).saturating_add(1));
                    }
                }
                if let Some(captures) = bit_range_re.captures(field_text) {
                    let msb = captures
                        .get(1)
                        .and_then(|value| value.as_str().parse::<u32>().ok());
                    let lsb = captures
                        .get(2)
                        .and_then(|value| value.as_str().parse::<u32>().ok());
                    if let (Some(msb), Some(lsb)) = (msb, lsb) {
                        field_offset = Some(lsb);
                        field_width = Some(msb.saturating_sub(lsb).saturating_add(1));
                    }
                }
                let (Some(offset), Some(width)) = (field_offset, field_width) else {
                    continue;
                };
                if width == 0 {
                    continue;
                }
                fields.push(SvdField {
                    name: field_name,
                    offset,
                    width: width.min(64),
                    description: description_re
                        .captures(field_text)
                        .and_then(|captures| captures.get(1))
                        .map(|value| clean_description(value.as_str()))
                        .unwrap_or_default(),
                    access: first_capture(&access_re, field_text).unwrap_or_default(),
                });
            }
            fields.sort_by_key(|field| std::cmp::Reverse(field.offset));

            let register_start = register.get(0).map(|matched| matched.start()).unwrap_or(0);
            registers.push(RawRegister {
                name,
                offset,
                description,
                size,
                access,
                fields,
                derived: register_derived,
                line: block_line + block_text[..register_start].lines().count(),
            });
        }
        raw.push(RawPeripheral {
            name: peripheral_name,
            base,
            description,
            registers,
            derived,
        });
    }

    // Resolve peripheral-level derivedFrom (chains up to 8 levels).
    for _ in 0..8 {
        let mut changed = false;
        for index in 0..raw.len() {
            let Some(base_name) = raw[index].derived.clone() else {
                continue;
            };
            let Some(base_index) = raw
                .iter()
                .position(|peripheral| peripheral.name == base_name)
            else {
                continue;
            };
            if base_index == index {
                continue;
            }
            let base_registers = raw[base_index].registers.clone();
            let base_description = raw[base_index].description.clone();
            let target = &mut raw[index];
            if target.description.is_empty() {
                target.description = base_description;
            }
            if target.registers.is_empty() {
                target.registers = base_registers;
                changed = true;
            } else {
                for base_register in base_registers {
                    if !target
                        .registers
                        .iter()
                        .any(|register| register.name == base_register.name)
                    {
                        target.registers.push(base_register);
                        changed = true;
                    }
                }
            }
        }
        if !changed {
            break;
        }
    }

    // Resolve register-level derivedFrom inside each peripheral.
    for peripheral in &mut raw {
        for _ in 0..8 {
            let mut changed = false;
            for index in 0..peripheral.registers.len() {
                let Some(base_name) = peripheral.registers[index].derived.clone() else {
                    continue;
                };
                let Some(base_index) = peripheral
                    .registers
                    .iter()
                    .position(|register| register.name == base_name)
                else {
                    continue;
                };
                if base_index == index {
                    continue;
                }
                let base = peripheral.registers[base_index].clone();
                let target = &mut peripheral.registers[index];
                if target.description.is_empty() {
                    target.description = base.description;
                }
                if target.access.is_empty() {
                    target.access = base.access;
                }
                if target.fields.is_empty() {
                    target.fields = base.fields;
                    changed = true;
                }
            }
            if !changed {
                break;
            }
        }
        peripheral
            .registers
            .sort_by_key(|register| (register.offset, register.name.clone()));
    }

    let mut peripherals = raw
        .into_iter()
        .map(|peripheral| SvdPeripheral {
            name: peripheral.name,
            base: peripheral.base,
            description: peripheral.description,
            registers: peripheral
                .registers
                .into_iter()
                .map(|register| SvdRegister {
                    name: register.name,
                    offset: register.offset,
                    description: register.description,
                    size: register.size,
                    access: register.access,
                    fields: register.fields,
                    line: register.line,
                })
                .collect(),
        })
        .collect::<Vec<_>>();
    peripherals.sort_by_key(|peripheral| peripheral.name.to_ascii_lowercase());

    let mut items = Vec::new();
    let mut truncated = false;
    'outer: for peripheral in &peripherals {
        for register in &peripheral.registers {
            items.push(RegisterItem {
                name: format!("{}.{}", peripheral.name, register.name),
                address: peripheral.base.wrapping_add(register.offset),
                description: register.description.clone(),
                path: path.map(Path::to_path_buf),
                line: register.line,
            });
            if items.len() >= MAX_REGISTERS {
                truncated = true;
                break 'outer;
            }
        }
    }
    (peripherals, items, truncated)
}

fn first_capture(regex: &Regex, text: &str) -> Option<String> {
    regex
        .captures(text)
        .and_then(|captures| captures.get(1))
        .map(|value| value.as_str().trim().to_owned())
        .filter(|value| !value.is_empty())
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
          <fields>
            <field><name>MODE0</name><bitOffset>0</bitOffset><bitWidth>2</bitWidth><description>Mode 0</description><access>read-write</access></field>
            <field><name>CNF0</name><lsb>2</lsb><msb>3</msb></field>
          </fields>
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
    fn prefers_device_specific_svd_tokens() {
        assert_eq!(
            device_svd_tokens("STM32F103C8Tx"),
            vec!["stm32f103", "stm32f1"]
        );
        assert_eq!(
            device_svd_tokens("STM32G431CBTx"),
            vec!["stm32g431", "stm32g4"]
        );
    }

    #[test]
    fn parses_svd_peripherals_and_registers() {
        let (peripherals, items, truncated) =
            parse_svd_full(SAMPLE_SVD, Some(Path::new("chip.svd")));
        assert!(!truncated);
        assert_eq!(items.len(), 2);
        assert_eq!(items[0].name, "GPIOA.CRL");
        assert_eq!(items[0].address, 0x4001_0800);
        assert_eq!(items[1].address, 0x4001_080C);
        assert!(items[0].description.contains("Port configuration"));
        assert_eq!(peripherals.len(), 1);
        assert_eq!(peripherals[0].name, "GPIOA");
        assert_eq!(peripherals[0].registers.len(), 2);
        let crl = &peripherals[0].registers[0];
        assert_eq!(crl.name, "CRL");
        let mode = crl
            .fields
            .iter()
            .find(|field| field.name == "MODE0")
            .expect("MODE0 field");
        assert_eq!((mode.offset, mode.width), (0, 2));
        let cnf = crl
            .fields
            .iter()
            .find(|field| field.name == "CNF0")
            .expect("CNF0 field");
        assert_eq!((cnf.offset, cnf.width), (2, 2));
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
        let (peripherals, items, _) = parse_svd_full(svd, None);
        assert!(items.iter().any(|item| item.name == "GPIOA.CRL"));
        let gpio_b = items
            .iter()
            .find(|item| item.name == "GPIOB.CRL")
            .expect("derived register should exist");
        assert_eq!(gpio_b.address, 0x4001_0C00);
        assert_eq!(peripherals.len(), 2);
        assert_eq!(peripherals[1].registers.len(), 1);
    }

    #[test]
    fn derived_registers_inherit_fields() {
        let svd = r#"<device><peripherals>
  <peripheral>
    <name>TIM1</name>
    <baseAddress>0x40010000</baseAddress>
    <registers>
      <register>
        <name>CR1</name><addressOffset>0x00</addressOffset>
        <fields><field><name>EN</name><bitOffset>0</bitOffset><bitWidth>1</bitWidth></field></fields>
      </register>
      <register derivedFrom="CR1"><name>CR2</name><addressOffset>0x04</addressOffset></register>
    </registers>
  </peripheral>
</peripherals></device>"#;
        let (peripherals, _, _) = parse_svd_full(svd, None);
        let registers = &peripherals[0].registers;
        assert_eq!(registers.len(), 2);
        let cr2 = registers
            .iter()
            .find(|register| register.name == "CR2")
            .expect("derived register");
        assert_eq!(cr2.offset, 0x04);
        assert_eq!(cr2.fields.len(), 1);
        assert_eq!(cr2.fields[0].name, "EN");
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
