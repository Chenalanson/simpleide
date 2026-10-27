//! Memory regions and firmware footprint.
//!
//! Totals come from the project linker script first, then the matching
//! Keil/CMSIS device support package. Usage comes from the `text/data/bss`
//! table printed by `arm-none-eabi-size` after a build.

use std::path::Path;

use regex::Regex;

use crate::config::{MemoryConfig, MemoryKind, MemoryRegionConfig};
use crate::project_meta::ProjectMeta;
use crate::support_pack::SupportPack;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Region {
    pub start: u64,
    pub size: u64,
}

impl Region {
    pub fn is_valid(self) -> bool {
        self.size > 0
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct MemoryLayout {
    pub flash: Region,
    pub ram: Region,
}

impl MemoryLayout {
    pub fn is_valid(self) -> bool {
        self.flash.is_valid() || self.ram.is_valid()
    }
}

#[derive(Clone, Debug)]
pub struct ResolvedMemory {
    pub layout: MemoryLayout,
    pub source: String,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct MemoryUsage {
    pub flash_used: u64,
    pub ram_used: u64,
}

/// Parse `0x20000000`, `0x2000`, `64K`, `512K`, `1M` or plain decimal values.
pub fn parse_number(text: &str) -> Option<u64> {
    let text = text.trim().trim_matches(';').replace('_', "");
    if text.is_empty() {
        return None;
    }
    let (number, multiplier) = match text.chars().last()?.to_ascii_uppercase() {
        'K' => (&text[..text.len() - 1], 1024),
        'M' => (&text[..text.len() - 1], 1024 * 1024),
        'G' => (&text[..text.len() - 1], 1024 * 1024 * 1024),
        _ => (text.as_str(), 1),
    };
    let number = number.trim();
    let value = if let Some(hex) = number
        .strip_prefix("0x")
        .or_else(|| number.strip_prefix("0X"))
    {
        u64::from_str_radix(hex, 16).ok()?
    } else {
        number.parse::<u64>().ok()?
    };
    value.checked_mul(multiplier)
}

pub fn format_hex(value: u64) -> String {
    format!("0x{value:08X}")
}

pub fn format_size(bytes: u64) -> String {
    if bytes >= 1024 * 1024 {
        format!("{:.1} MB", bytes as f64 / (1024.0 * 1024.0))
    } else if bytes >= 1024 {
        format!("{:.1} KB", bytes as f64 / 1024.0)
    } else {
        format!("{bytes} B")
    }
}

/// Compact editable size, e.g. `64K`, `512K`, `1M`.
pub fn format_capacity(bytes: u64) -> String {
    if bytes >= 1024 * 1024 && bytes.is_multiple_of(1024 * 1024) {
        format!("{}M", bytes / (1024 * 1024))
    } else if bytes >= 1024 && bytes.is_multiple_of(1024) {
        format!("{}K", bytes / 1024)
    } else {
        format_hex(bytes)
    }
}

/// Extract the last `text data bss` row from the build log.
pub fn parse_size_output(text: &str) -> Option<MemoryUsage> {
    let lines = text.lines().collect::<Vec<_>>();
    let mut usage = None;
    for (index, line) in lines.iter().enumerate() {
        let header = line.to_ascii_lowercase();
        if !(header.contains("text") && header.contains("data") && header.contains("bss")) {
            continue;
        }
        for candidate in lines.iter().skip(index + 1).take(8) {
            let columns = candidate.split_whitespace().collect::<Vec<_>>();
            if columns.len() < 4 {
                continue;
            }
            let Some(numbers) = columns[..4]
                .iter()
                .map(|value| value.parse::<u64>().ok())
                .collect::<Option<Vec<_>>>()
            else {
                continue;
            };
            let (text, data, bss) = (numbers[0], numbers[1], numbers[2]);
            if text + data + bss > 0 {
                usage = Some(MemoryUsage {
                    flash_used: text.saturating_add(data),
                    ram_used: data.saturating_add(bss),
                });
            }
        }
    }
    usage
}

/// Read allocated section sizes from an ELF file. This works even when the
/// build log does not contain an `arm-none-eabi-size` table.
pub fn usage_from_elf(path: &Path) -> Option<MemoryUsage> {
    let data = std::fs::read(path).ok()?;
    if data.len() < 52 || &data[..4] != b"\x7fELF" {
        return None;
    }
    let class = data[4];
    let little_endian = data[5] != 2;
    let (shoff, shentsize, shnum) = match class {
        1 => (
            read_u32(&data, 32, little_endian)? as u64,
            read_u16(&data, 46, little_endian)? as u64,
            read_u16(&data, 48, little_endian)? as u64,
        ),
        2 => (
            read_u64(&data, 40, little_endian)?,
            read_u16(&data, 58, little_endian)? as u64,
            read_u16(&data, 60, little_endian)? as u64,
        ),
        _ => return None,
    };
    if shoff == 0 || shentsize < 40 || shnum == 0 {
        return None;
    }
    let section = |index: usize| -> Option<ElfSection> {
        let offset = shoff.checked_add((index as u64).checked_mul(shentsize)?)?;
        let offset = usize::try_from(offset).ok()?;
        match class {
            1 => Some(ElfSection {
                kind: read_u32(&data, offset + 4, little_endian)?,
                flags: read_u32(&data, offset + 8, little_endian)? as u64,
                size: read_u32(&data, offset + 20, little_endian)? as u64,
            }),
            2 => Some(ElfSection {
                kind: read_u32(&data, offset + 4, little_endian)?,
                flags: read_u64(&data, offset + 8, little_endian)?,
                size: read_u64(&data, offset + 32, little_endian)?,
            }),
            _ => None,
        }
    };
    let mut flash_used = 0u64;
    let mut ram_used = 0u64;
    for index in 0..shnum as usize {
        let Some(section) = section(index) else {
            continue;
        };
        if section.flags & 0x2 == 0 {
            continue;
        }
        if section.kind != 8 {
            flash_used = flash_used.saturating_add(section.size);
        }
        if section.flags & 0x1 != 0 {
            ram_used = ram_used.saturating_add(section.size);
        }
    }
    (flash_used > 0 || ram_used > 0).then_some(MemoryUsage {
        flash_used,
        ram_used,
    })
}

struct ElfSection {
    kind: u32,
    flags: u64,
    size: u64,
}

/// One writable global/static object found in the ELF symbol table.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FirmwareSymbol {
    pub name: String,
    pub address: u64,
    pub size: u64,
    pub section: String,
}

/// Lightweight firmware picture used by the build-output memory view.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct FirmwareAnalysis {
    /// Global/static objects in writable, allocated sections, largest first.
    pub globals: Vec<FirmwareSymbol>,
    pub data_size: u64,
    pub bss_size: u64,
    pub stack_top: Option<u64>,
    pub stack_size: Option<u64>,
    pub stack_limit: Option<u64>,
    pub heap_size: Option<u64>,
    pub heap_kind: Option<String>,
    pub has_symbols: bool,
}

impl FirmwareAnalysis {
    /// Stack bytes currently in use, assuming `sp` is the active stack pointer.
    pub fn stack_used(&self, sp: u64) -> Option<u64> {
        let top = self.stack_top?;
        let limit = self.stack_limit?;
        (sp >= limit && sp <= top).then(|| top.saturating_sub(sp))
    }
}

#[derive(Clone, Debug, Default)]
struct ElfSectionDetail {
    name: String,
    kind: u32,
    flags: u64,
    offset: u64,
    size: u64,
    link: u32,
    entsize: u64,
}

#[derive(Clone, Debug)]
struct ElfRawSymbol {
    name: String,
    value: u64,
    size: u64,
    info: u8,
    section_name: String,
    section_flags: u64,
}

/// Parse an ELF/AXF file and extract globals, the configured stack region and
/// the FreeRTOS heap symbols. Returns `None` for non-ELF files or invalid data.
pub fn analyze_elf(path: &Path) -> Option<FirmwareAnalysis> {
    let data = std::fs::read(path).ok()?;
    if data.len() < 52 || &data[..4] != b"\x7fELF" {
        return None;
    }
    let class = data[4];
    let little_endian = data[5] != 2;
    let (shoff, shentsize, shnum, shstrndx) = match class {
        1 => (
            read_u32(&data, 32, little_endian)? as u64,
            read_u16(&data, 46, little_endian)? as u64,
            read_u16(&data, 48, little_endian)? as u64,
            read_u16(&data, 50, little_endian)? as u64,
        ),
        2 => (
            read_u64(&data, 40, little_endian)?,
            read_u16(&data, 58, little_endian)? as u64,
            read_u16(&data, 60, little_endian)? as u64,
            read_u16(&data, 62, little_endian)? as u64,
        ),
        _ => return None,
    };
    if shoff == 0 || shentsize < if class == 1 { 40 } else { 64 } || shnum == 0 {
        return None;
    }
    let count = (shnum as usize).min(8192);
    let mut sections = Vec::with_capacity(count);
    for index in 0..count {
        let offset = shoff.checked_add((index as u64).checked_mul(shentsize)?)?;
        let offset = usize::try_from(offset).ok()?;
        let section = match class {
            1 => ElfSectionDetail {
                kind: read_u32(&data, offset + 4, little_endian)?,
                flags: read_u32(&data, offset + 8, little_endian)? as u64,
                offset: read_u32(&data, offset + 16, little_endian)? as u64,
                size: read_u32(&data, offset + 20, little_endian)? as u64,
                link: read_u32(&data, offset + 24, little_endian)?,
                entsize: read_u32(&data, offset + 36, little_endian)? as u64,
                ..Default::default()
            },
            2 => ElfSectionDetail {
                kind: read_u32(&data, offset + 4, little_endian)?,
                flags: read_u64(&data, offset + 8, little_endian)?,
                offset: read_u64(&data, offset + 24, little_endian)?,
                size: read_u64(&data, offset + 32, little_endian)?,
                link: read_u32(&data, offset + 40, little_endian)?,
                entsize: read_u64(&data, offset + 56, little_endian)?,
                ..Default::default()
            },
            _ => return None,
        };
        sections.push(section);
    }

    if let Some(names_section) = sections.get(shstrndx as usize) {
        let start = usize::try_from(names_section.offset).ok()?;
        let end = usize::try_from(names_section.offset.checked_add(names_section.size)?).ok()?;
        if let Some(names) = data.get(start..end) {
            for (index, section) in sections.iter_mut().enumerate() {
                let offset = shoff.checked_add((index as u64).checked_mul(shentsize)?)?;
                let offset = usize::try_from(offset).ok()?;
                let name_offset = read_u32(&data, offset, little_endian)? as usize;
                section.name = read_cstr(names, name_offset);
            }
        }
    }

    let data_size = sections
        .iter()
        .filter(|section| section.name == ".data" || section.name.starts_with(".data."))
        .map(|section| section.size)
        .sum();
    let bss_size = sections
        .iter()
        .filter(|section| section.name == ".bss" || section.name.starts_with(".bss."))
        .map(|section| section.size)
        .sum();

    let symbol_section = sections
        .iter()
        .enumerate()
        .find(|(_, section)| section.kind == 2)
        .or_else(|| {
            sections
                .iter()
                .enumerate()
                .find(|(_, section)| section.kind == 11)
        });
    let mut raw_symbols = Vec::new();
    if let Some((_, symbol_section)) = symbol_section {
        let string_section = sections.get(symbol_section.link as usize);
        if let Some(string_section) = string_section {
            let symbols_start = usize::try_from(symbol_section.offset).ok()?;
            let symbols_end =
                usize::try_from(symbol_section.offset.checked_add(symbol_section.size)?).ok()?;
            let strings_start = usize::try_from(string_section.offset).ok()?;
            let strings_end =
                usize::try_from(string_section.offset.checked_add(string_section.size)?).ok()?;
            if let (Some(symbol_data), Some(strings)) = (
                data.get(symbols_start..symbols_end),
                data.get(strings_start..strings_end),
            ) {
                let entry_size = if symbol_section.entsize > 0 {
                    symbol_section.entsize as usize
                } else if class == 1 {
                    16
                } else {
                    24
                };
                if entry_size >= if class == 1 { 16 } else { 24 } {
                    for offset in (0..symbol_data.len()).step_by(entry_size) {
                        let name_offset = read_u32(symbol_data, offset, little_endian)? as usize;
                        let name = read_cstr(strings, name_offset);
                        let (value, size, info, shndx) = match class {
                            1 => (
                                read_u32(symbol_data, offset + 4, little_endian)? as u64,
                                read_u32(symbol_data, offset + 8, little_endian)? as u64,
                                *symbol_data.get(offset + 12)?,
                                read_u16(symbol_data, offset + 14, little_endian)?,
                            ),
                            2 => (
                                read_u64(symbol_data, offset + 8, little_endian)?,
                                read_u64(symbol_data, offset + 16, little_endian)?,
                                *symbol_data.get(offset + 4)?,
                                read_u16(symbol_data, offset + 6, little_endian)?,
                            ),
                            _ => return None,
                        };
                        let (section_name, section_flags) = sections
                            .get(shndx as usize)
                            .map(|section| (section.name.clone(), section.flags))
                            .unwrap_or_default();
                        raw_symbols.push(ElfRawSymbol {
                            name,
                            value,
                            size,
                            info,
                            section_name,
                            section_flags,
                        });
                    }
                }
            }
        }
    }

    let mut globals = raw_symbols
        .iter()
        .filter(|symbol| {
            let symbol_type = symbol.info & 0xf;
            let writable = symbol.section_flags & 0x1 != 0;
            let allocated = symbol.section_flags & 0x2 != 0;
            let executable = symbol.section_flags & 0x4 != 0;
            let lower_name = symbol.name.to_ascii_lowercase();
            !symbol.name.is_empty()
                && !symbol.name.starts_with('.')
                && !symbol.name.starts_with('$')
                && !matches!(
                    lower_name.as_str(),
                    "ucheap"
                        | "uc_heap"
                        | "xfreebytesremaining"
                        | "xminimumeverfreebytesremaining"
                        | "xnextfreebyte"
                        | "pxend"
                )
                && symbol.size > 0
                && (symbol_type == 1 || symbol_type == 5 || symbol_type == 6)
                && writable
                && allocated
                && !executable
                && !symbol.section_name.to_ascii_lowercase().contains("heap")
                && !symbol.section_name.to_ascii_lowercase().contains("stack")
        })
        .map(|symbol| FirmwareSymbol {
            name: symbol.name.clone(),
            address: symbol.value,
            size: symbol.size,
            section: symbol.section_name.clone(),
        })
        .collect::<Vec<_>>();
    globals.sort_by(|left, right| {
        right
            .size
            .cmp(&left.size)
            .then_with(|| left.name.cmp(&right.name))
    });
    globals.truncate(200);

    let stack_top = find_symbol(
        &raw_symbols,
        &[
            "_estack",
            "__stacktop",
            "__stack_top",
            "_stack_top",
            "__initial_sp",
            "estack",
        ],
    )
    .map(|symbol| symbol.value)
    .filter(|value| *value > 0);
    let stack_size = find_symbol(
        &raw_symbols,
        &[
            "_min_stack_size",
            "__stack_size",
            "_stack_size",
            "stack_size",
            "__stack_size__",
        ],
    )
    .and_then(symbol_size_or_value);
    let stack_limit = find_symbol(
        &raw_symbols,
        &[
            "__stacklimit",
            "_stack_limit",
            "__stack_limit",
            "__stack_limit__",
        ],
    )
    .map(|symbol| symbol.value)
    .filter(|value| *value > 0);
    let (stack_top, stack_size, stack_limit) = match (stack_top, stack_size, stack_limit) {
        (Some(top), Some(size), _) => (Some(top), Some(size), Some(top.saturating_sub(size))),
        (Some(top), None, Some(limit)) => (Some(top), Some(top.saturating_sub(limit)), Some(limit)),
        (None, Some(size), Some(limit)) => {
            (Some(limit.saturating_add(size)), Some(size), Some(limit))
        }
        (top, size, limit) => (top, size, limit),
    };

    let heap_symbol = find_symbol(&raw_symbols, &["ucheap", "uc_heap"]);
    let heap_size = heap_symbol
        .map(|symbol| symbol.size)
        .filter(|size| *size > 0)
        .or_else(|| {
            sections
                .iter()
                .find(|section| {
                    section.name.eq_ignore_ascii_case(".heap") || section.name.starts_with(".heap.")
                })
                .map(|section| section.size)
                .filter(|size| *size > 0)
        });
    let heap_kind = if heap_symbol.is_some() || heap_size.is_some() {
        if find_symbol(
            &raw_symbols,
            &["xfreebytesremaining", "xminimumeverfreebytesremaining"],
        )
        .is_some()
        {
            Some("heap_4".to_owned())
        } else if find_symbol(&raw_symbols, &["pxend"]).is_some() {
            Some("heap_5".to_owned())
        } else if find_symbol(&raw_symbols, &["xnextfreebyte"]).is_some() {
            Some("heap_1/2".to_owned())
        } else {
            Some("FreeRTOS".to_owned())
        }
    } else {
        None
    };

    Some(FirmwareAnalysis {
        globals,
        data_size,
        bss_size,
        stack_top,
        stack_size,
        stack_limit,
        heap_size,
        heap_kind,
        has_symbols: !raw_symbols.is_empty(),
    })
}

fn find_symbol<'a>(symbols: &'a [ElfRawSymbol], names: &[&str]) -> Option<&'a ElfRawSymbol> {
    symbols.iter().find(|symbol| {
        names
            .iter()
            .any(|name| symbol.name.eq_ignore_ascii_case(name))
    })
}

fn symbol_size_or_value(symbol: &ElfRawSymbol) -> Option<u64> {
    if symbol.size > 0 {
        Some(symbol.size)
    } else if symbol.value > 0 {
        Some(symbol.value)
    } else {
        None
    }
}

fn read_cstr(data: &[u8], offset: usize) -> String {
    data.get(offset..)
        .and_then(|rest| rest.split(|byte| *byte == 0).next())
        .map(|bytes| String::from_utf8_lossy(bytes).into_owned())
        .unwrap_or_default()
}

fn read_u16(data: &[u8], offset: usize, little_endian: bool) -> Option<u16> {
    let bytes: [u8; 2] = data.get(offset..offset + 2)?.try_into().ok()?;
    Some(if little_endian {
        u16::from_le_bytes(bytes)
    } else {
        u16::from_be_bytes(bytes)
    })
}

fn read_u32(data: &[u8], offset: usize, little_endian: bool) -> Option<u32> {
    let bytes: [u8; 4] = data.get(offset..offset + 4)?.try_into().ok()?;
    Some(if little_endian {
        u32::from_le_bytes(bytes)
    } else {
        u32::from_be_bytes(bytes)
    })
}

fn read_u64(data: &[u8], offset: usize, little_endian: bool) -> Option<u64> {
    let bytes: [u8; 8] = data.get(offset..offset + 8)?.try_into().ok()?;
    Some(if little_endian {
        u64::from_le_bytes(bytes)
    } else {
        u64::from_be_bytes(bytes)
    })
}

pub fn parse_linker_script(path: &Path) -> Option<MemoryLayout> {
    let text = std::fs::read_to_string(path).ok()?;
    parse_linker_text(&text)
}

pub fn parse_linker_text(text: &str) -> Option<MemoryLayout> {
    let block = Regex::new(r"(?is)\bMEMORY\s*\{(.*?)\}")
        .ok()?
        .captures(text)?
        .get(1)?
        .as_str();
    let line_regex = Regex::new(
        r"(?i)([A-Za-z0-9_]+)\s*(?:\([^)]*\))?\s*:\s*ORIGIN\s*=\s*([^,]+),\s*LENGTH\s*=\s*([^\s,]+)",
    )
    .ok()?;
    let mut layout = MemoryLayout::default();
    for captures in line_regex.captures_iter(block) {
        let name = captures.get(1)?.as_str().to_ascii_uppercase();
        let Some(start) = parse_number(captures.get(2)?.as_str()) else {
            continue;
        };
        let Some(size) = parse_number(captures.get(3)?.as_str()) else {
            continue;
        };
        let region = Region { start, size };
        if (name.contains("FLASH") || name.contains("ROM") || name.contains("IROM"))
            && !layout.flash.is_valid()
        {
            layout.flash = region;
        } else if (name.contains("RAM") || name.contains("IRAM")) && !layout.ram.is_valid() {
            layout.ram = region;
        }
    }
    layout.is_valid().then_some(layout)
}

/// Select the device block matching `device` and read its `<memory>` tags.
pub fn parse_pdsc_memory(text: &str, device: &str) -> Option<MemoryLayout> {
    let regex = Regex::new(r#"(?is)<device\b([^>]*)>(.*?)</device>"#).ok()?;
    let family = device_family(device);
    let generic_suffix = device
        .trim()
        .chars()
        .last()
        .map(|value| value.to_ascii_uppercase());
    let mut fallback = None;
    let mut chosen = None;
    for captures in regex.captures_iter(text) {
        let attrs = captures.get(1)?.as_str();
        let Some(name) = attr(attrs, "Dname") else {
            continue;
        };
        if name.eq_ignore_ascii_case(device) {
            chosen = Some(captures.get(2)?.as_str());
            break;
        }
        let upper = name.to_ascii_uppercase();
        if upper.starts_with(&family) {
            let same_flash = generic_suffix.is_some_and(|suffix| upper.ends_with(suffix));
            if same_flash {
                chosen = Some(captures.get(2)?.as_str());
                break;
            }
            if fallback.is_none() {
                fallback = Some(captures.get(2)?.as_str());
            }
        }
    }
    let block = chosen.or(fallback)?;
    parse_memory_tags(block)
}

fn parse_memory_tags(block: &str) -> Option<MemoryLayout> {
    let regex = Regex::new(r"(?is)<memory\b([^>]*)/?>").ok()?;
    let mut layout = MemoryLayout::default();
    for captures in regex.captures_iter(block) {
        let attrs = captures.get(1)?.as_str();
        let Some(id) = attr(attrs, "id") else {
            continue;
        };
        let Some(start) = attr(attrs, "start").and_then(|value| parse_number(&value)) else {
            continue;
        };
        let Some(size) = attr(attrs, "size").and_then(|value| parse_number(&value)) else {
            continue;
        };
        let id = id.to_ascii_uppercase();
        let region = Region { start, size };
        if (id.contains("IROM") || id.contains("ROM") || id.contains("FLASH"))
            && !layout.flash.is_valid()
        {
            layout.flash = region;
        } else if (id.contains("IRAM") || id.contains("RAM")) && !layout.ram.is_valid() {
            layout.ram = region;
        }
    }
    layout.is_valid().then_some(layout)
}

fn attr(attrs: &str, name: &str) -> Option<String> {
    let pattern = format!(r#"(?i)\b{name}\s*=\s*"([^"]+)""#);
    Regex::new(&pattern)
        .ok()?
        .captures(attrs)?
        .get(1)
        .map(|value| value.as_str().to_owned())
}

fn device_family(device: &str) -> String {
    let upper = device.trim().to_ascii_uppercase();
    if upper.starts_with("STM32") && upper.len() >= 9 {
        upper[..9].to_owned()
    } else {
        upper
    }
}

/// Resolve the effective memory layout: manual overrides, linker script,
/// matching support pack, then Keil CPU ranges.
pub fn resolve(
    root: &Path,
    meta: &ProjectMeta,
    packs: &[SupportPack],
    overrides: &MemoryConfig,
) -> Option<ResolvedMemory> {
    let mut layout = MemoryLayout::default();
    let mut source = String::new();

    if let Some(linker) = meta.linker_script.as_deref() {
        let path = root.join(linker);
        if let Some(found) = parse_linker_script(&path) {
            layout = found;
            source = "链接脚本".to_owned();
        }
    }

    if !layout.is_valid() {
        if let Some(device) = meta.device.as_deref() {
            for pack in packs {
                let Some(text) = crate::support_pack::read_pdsc(&pack.path) else {
                    continue;
                };
                if let Some(found) = parse_pdsc_memory(&text, device) {
                    layout = found;
                    source = "芯片支持包".to_owned();
                    break;
                }
            }
        }
    }

    if !layout.is_valid() {
        if let Some(text) = meta.memory_layout.as_deref() {
            if let Some(found) = parse_meta_layout(text) {
                layout = found;
                source = "工程信息".to_owned();
            }
        }
    }

    let mut overridden = false;
    if let Some(value) = parse_number(&overrides.flash_start) {
        layout.flash.start = value;
        overridden = true;
    }
    if let Some(value) = parse_number(&overrides.flash_size) {
        layout.flash.size = value;
        overridden = true;
    }
    if let Some(value) = parse_number(&overrides.ram_start) {
        layout.ram.start = value;
        overridden = true;
    }
    if let Some(value) = parse_number(&overrides.ram_size) {
        layout.ram.size = value;
        overridden = true;
    }
    if overridden {
        source = "手动设置".to_owned();
    }

    layout
        .is_valid()
        .then_some(ResolvedMemory { layout, source })
}

fn parse_meta_layout(text: &str) -> Option<MemoryLayout> {
    let mut layout = MemoryLayout::default();
    let flash = Regex::new(r"(?i)\bflash\s+([0-9][^\s·]*)")
        .ok()?
        .captures(text)
        .and_then(|captures| captures.get(1))
        .and_then(|value| parse_number(value.as_str()));
    let ram = Regex::new(r"(?i)\bram\s+([0-9][^\s·]*)")
        .ok()?
        .captures(text)
        .and_then(|captures| captures.get(1))
        .and_then(|value| parse_number(value.as_str()));
    if let Some(size) = flash {
        layout.flash = Region {
            start: 0x0800_0000,
            size,
        };
    }
    if let Some(size) = ram {
        layout.ram = Region {
            start: 0x2000_0000,
            size,
        };
    }
    layout.is_valid().then_some(layout)
}

pub fn effective_download_address(overrides: &MemoryConfig) -> Option<String> {
    let value = overrides.download_address.trim();
    if !value.is_empty() {
        return Some(value.to_owned());
    }
    let value = overrides.flash_start.trim();
    (!value.is_empty()).then(|| value.to_owned())
}

/// Build the fixed Cortex-Debug-style region grid used by the layout editor.
pub fn default_regions(layout: Option<MemoryLayout>) -> Vec<MemoryRegionConfig> {
    let flash = layout.map(|layout| layout.flash).unwrap_or_default();
    let ram = layout.map(|layout| layout.ram).unwrap_or_default();
    let region = |name: &str,
                  kind: MemoryKind,
                  on_chip: bool,
                  enabled: bool,
                  start: u64,
                  size: u64,
                  startup: bool,
                  no_init: bool| {
        MemoryRegionConfig {
            name: name.to_owned(),
            kind,
            on_chip,
            enabled,
            start: if enabled || start > 0 {
                format_hex(start)
            } else {
                "0x0".to_owned()
            },
            size: if enabled || size > 0 {
                format_capacity(size)
            } else {
                "0x0".to_owned()
            },
            no_init,
            startup,
        }
    };
    vec![
        region(
            "IRAM1",
            MemoryKind::Ram,
            true,
            ram.is_valid(),
            ram.start,
            ram.size,
            false,
            false,
        ),
        region("IRAM2", MemoryKind::Ram, true, false, 0, 0, false, false),
        region("RAM1", MemoryKind::Ram, false, false, 0, 0, false, false),
        region("RAM2", MemoryKind::Ram, false, false, 0, 0, false, false),
        region("RAM3", MemoryKind::Ram, false, false, 0, 0, false, false),
        region(
            "IROM1",
            MemoryKind::Rom,
            true,
            flash.is_valid(),
            flash.start,
            flash.size,
            true,
            false,
        ),
        region("IROM2", MemoryKind::Rom, true, false, 0, 0, false, false),
        region("ROM1", MemoryKind::Rom, false, false, 0, 0, false, false),
        region("ROM2", MemoryKind::Rom, false, false, 0, 0, false, false),
        region("ROM3", MemoryKind::Rom, false, false, 0, 0, false, false),
    ]
}

/// Ensure the fixed IRAM/IROM/RAM/ROM row order expected by the editor.
pub fn normalize_regions(regions: Vec<MemoryRegionConfig>) -> Vec<MemoryRegionConfig> {
    let mut templates = default_regions(None);
    for template in &mut templates {
        if let Some(saved) = regions
            .iter()
            .find(|region| region.name == template.name && region.kind == template.kind)
        {
            *template = saved.clone();
        }
    }
    templates
}

pub fn regions_total(regions: &[MemoryRegionConfig], kind: MemoryKind) -> u64 {
    regions
        .iter()
        .filter(|region| region.enabled && region.kind == kind)
        .filter_map(|region| parse_number(&region.size))
        .sum()
}

pub fn primary_start(regions: &[MemoryRegionConfig], kind: MemoryKind) -> Option<u64> {
    regions
        .iter()
        .filter(|region| region.enabled && region.kind == kind)
        .find(|region| kind == MemoryKind::Rom && region.startup)
        .or_else(|| {
            regions
                .iter()
                .find(|region| region.enabled && region.kind == kind)
        })
        .and_then(|region| parse_number(&region.start))
}

pub fn format_usage(usage: MemoryUsage, layout: Option<MemoryLayout>) -> String {
    let mut parts = Vec::new();
    match layout.filter(|layout| layout.flash.is_valid()) {
        Some(layout) => parts.push(format!(
            "Flash {} / {} ({:.0}%)",
            format_size(usage.flash_used),
            format_size(layout.flash.size),
            percentage(usage.flash_used, layout.flash.size)
        )),
        None => parts.push(format!("Flash {}", format_size(usage.flash_used))),
    }
    match layout.filter(|layout| layout.ram.is_valid()) {
        Some(layout) => parts.push(format!(
            "RAM {} / {} ({:.0}%)",
            format_size(usage.ram_used),
            format_size(layout.ram.size),
            percentage(usage.ram_used, layout.ram.size)
        )),
        None => parts.push(format!("RAM {}", format_size(usage.ram_used))),
    }
    parts.join(" · ")
}

fn percentage(used: u64, total: u64) -> f64 {
    if total == 0 {
        return 0.0;
    }
    used as f64 / total as f64 * 100.0
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_size_table() {
        let log = "\
   text\t   data\t    bss\t    dec\t    hex\tfilename
   2816\t     12\t   1572\t   4400\t   1130\tbuild/test32.elf
";
        let usage = parse_size_output(log).unwrap();
        assert_eq!(usage.flash_used, 2828);
        assert_eq!(usage.ram_used, 1584);
        assert!(parse_size_output("no table").is_none());
    }

    #[test]
    fn parses_linker_memory_block() {
        let text = r"
MEMORY
{
  RAM (xrw)      : ORIGIN = 0x20000000, LENGTH = 20K
  FLASH (rx)     : ORIGIN = 0x8000000, LENGTH = 64K
}";
        let layout = parse_linker_text(text).unwrap();
        assert_eq!(layout.flash.start, 0x0800_0000);
        assert_eq!(layout.flash.size, 64 * 1024);
        assert_eq!(layout.ram.start, 0x2000_0000);
        assert_eq!(layout.ram.size, 20 * 1024);
    }

    #[test]
    fn parses_pdsc_device_memory() {
        let text = r#"
<package>
  <device Dname="STM32F103C8">
    <memory id="IROM1" start="0x08000000" size="0x00010000" default="1"/>
    <memory id="IRAM1" start="0x20000000" size="0x00005000"/>
  </device>
</package>"#;
        let layout = parse_pdsc_memory(text, "STM32F103xB").unwrap();
        assert_eq!(layout.flash.size, 64 * 1024);
        assert_eq!(layout.ram.size, 20 * 1024);
    }

    #[test]
    fn parses_allocated_elf_sections() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("demo.elf");
        let mut data = vec![0u8; 52 + 4 * 40];
        data[..4].copy_from_slice(b"\x7fELF");
        data[4] = 1;
        data[5] = 1;
        data[6] = 1;
        put_u16(&mut data, 16, 2);
        put_u16(&mut data, 18, 40);
        put_u32(&mut data, 20, 1);
        put_u32(&mut data, 32, 52);
        put_u16(&mut data, 40, 52);
        put_u16(&mut data, 46, 40);
        put_u16(&mut data, 48, 4);
        // .text
        let text = 52 + 40;
        put_u32(&mut data, text + 4, 1);
        put_u32(&mut data, text + 8, 0x6);
        put_u32(&mut data, text + 20, 16);
        // .data
        let data_section = 52 + 80;
        put_u32(&mut data, data_section + 4, 1);
        put_u32(&mut data, data_section + 8, 0x3);
        put_u32(&mut data, data_section + 20, 8);
        // .bss
        let bss = 52 + 120;
        put_u32(&mut data, bss + 4, 8);
        put_u32(&mut data, bss + 8, 0x3);
        put_u32(&mut data, bss + 20, 24);
        std::fs::write(&path, data).unwrap();
        let usage = usage_from_elf(&path).unwrap();
        assert_eq!(usage.flash_used, 24);
        assert_eq!(usage.ram_used, 32);
    }

    fn put_u16(data: &mut [u8], offset: usize, value: u16) {
        data[offset..offset + 2].copy_from_slice(&value.to_le_bytes());
    }

    fn put_u32(data: &mut [u8], offset: usize, value: u32) {
        data[offset..offset + 4].copy_from_slice(&value.to_le_bytes());
    }

    #[test]
    fn analyzes_elf_globals_stack_and_heap() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("analysis.elf");
        let section_names = b"\0.shstrtab\0.symtab\0.strtab\0.bss\0.data\0";
        let symbol_names =
            b"\0counter\0buffer\0ucHeap\0_estack\0_Min_Stack_Size\0xFreeBytesRemaining\0";
        let shoff = 52usize;
        let section_count = 6usize;
        let shstrtab_offset = shoff + section_count * 40;
        let symtab_offset = shstrtab_offset + section_names.len();
        let symtab_size = 7 * 16;
        let strtab_offset = symtab_offset + symtab_size;
        let file_size = strtab_offset + symbol_names.len();
        let mut data = vec![0u8; file_size];
        data[..4].copy_from_slice(b"\x7fELF");
        data[4] = 1;
        data[5] = 1;
        data[6] = 1;
        put_u16(&mut data, 16, 2);
        put_u16(&mut data, 18, 40);
        put_u32(&mut data, 20, 1);
        put_u32(&mut data, 32, shoff as u32);
        put_u16(&mut data, 40, 52);
        put_u16(&mut data, 46, 40);
        put_u16(&mut data, 48, section_count as u16);
        put_u16(&mut data, 50, 1);
        data[shstrtab_offset..shstrtab_offset + section_names.len()].copy_from_slice(section_names);
        data[strtab_offset..strtab_offset + symbol_names.len()].copy_from_slice(symbol_names);

        write_elf_section(
            &mut data,
            shoff,
            1,
            1,
            3,
            0,
            0,
            shstrtab_offset as u32,
            section_names.len() as u32,
            0,
            0,
        );
        write_elf_section(
            &mut data,
            shoff,
            2,
            11,
            2,
            0,
            0,
            symtab_offset as u32,
            symtab_size as u32,
            3,
            16,
        );
        write_elf_section(
            &mut data,
            shoff,
            3,
            19,
            3,
            0,
            0,
            strtab_offset as u32,
            symbol_names.len() as u32,
            0,
            0,
        );
        write_elf_section(&mut data, shoff, 4, 27, 8, 3, 0x2000_0000, 0, 0x40, 0, 0);
        write_elf_section(&mut data, shoff, 5, 32, 1, 3, 0x2000_0040, 0, 8, 0, 0);
        write_elf_symbol(&mut data, symtab_offset, 1, 1, 0x2000_0000, 4, 0x11, 4);
        write_elf_symbol(&mut data, symtab_offset, 2, 9, 0x2000_0004, 0x20, 0x11, 4);
        write_elf_symbol(&mut data, symtab_offset, 3, 16, 0x2000_0024, 0x100, 0x11, 4);
        write_elf_symbol(
            &mut data,
            symtab_offset,
            4,
            23,
            0x2000_4000,
            0,
            0x10,
            0xfff1,
        );
        write_elf_symbol(&mut data, symtab_offset, 5, 31, 0x400, 0, 0x10, 0xfff1);
        write_elf_symbol(&mut data, symtab_offset, 6, 47, 0x2000_0124, 4, 0x11, 4);
        std::fs::write(&path, data).unwrap();

        let analysis = analyze_elf(&path).expect("analysis should parse");
        assert!(analysis.has_symbols);
        assert_eq!(analysis.data_size, 8);
        assert_eq!(analysis.bss_size, 0x40);
        assert_eq!(analysis.globals.len(), 2, "{:?}", analysis.globals);
        assert_eq!(analysis.globals[0].name, "buffer");
        assert_eq!(analysis.globals[0].size, 0x20);
        assert_eq!(analysis.globals[1].name, "counter");
        assert_eq!(analysis.stack_top, Some(0x2000_4000));
        assert_eq!(analysis.stack_size, Some(0x400));
        assert_eq!(analysis.stack_limit, Some(0x2000_3c00));
        assert_eq!(analysis.stack_used(0x2000_3e00), Some(0x200));
        assert_eq!(analysis.stack_used(0x2000_3000), None);
        assert_eq!(analysis.heap_size, Some(0x100));
        assert_eq!(analysis.heap_kind.as_deref(), Some("heap_4"));
    }

    #[allow(clippy::too_many_arguments)]
    fn write_elf_section(
        data: &mut [u8],
        shoff: usize,
        index: usize,
        name: u32,
        kind: u32,
        flags: u32,
        address: u32,
        offset: u32,
        size: u32,
        link: u32,
        entsize: u32,
    ) {
        let base = shoff + index * 40;
        put_u32(data, base, name);
        put_u32(data, base + 4, kind);
        put_u32(data, base + 8, flags);
        put_u32(data, base + 12, address);
        put_u32(data, base + 16, offset);
        put_u32(data, base + 20, size);
        put_u32(data, base + 24, link);
        put_u32(data, base + 36, entsize);
    }

    #[allow(clippy::too_many_arguments)]
    fn write_elf_symbol(
        data: &mut [u8],
        symtab_offset: usize,
        index: usize,
        name: u32,
        value: u32,
        size: u32,
        info: u8,
        shndx: u16,
    ) {
        let base = symtab_offset + index * 16;
        put_u32(data, base, name);
        put_u32(data, base + 4, value);
        put_u32(data, base + 8, size);
        data[base + 12] = info;
        put_u16(data, base + 14, shndx);
    }

    #[test]
    fn formats_numbers_and_usage() {
        assert_eq!(parse_number("0x2000"), Some(0x2000));
        assert_eq!(parse_number("64K"), Some(64 * 1024));
        assert_eq!(
            format_usage(
                MemoryUsage {
                    flash_used: 2828,
                    ram_used: 1584
                },
                Some(MemoryLayout {
                    flash: Region {
                        start: 0x0800_0000,
                        size: 64 * 1024
                    },
                    ram: Region {
                        start: 0x2000_0000,
                        size: 20 * 1024
                    }
                })
            ),
            "Flash 2.8 KB / 64.0 KB (4%) · RAM 1.5 KB / 20.0 KB (8%)"
        );
    }
}
