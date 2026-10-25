//! Keil/CMSIS device support packages stored in the project root.
//!
//! A project can contain `.pack` (a zip with a `.pdsc`) and standalone
//! `.pdsc` files. We keep the files in the project root and parse enough
//! package metadata for the project-info panel.

use std::fs::File;
use std::io::Read;
use std::path::{Path, PathBuf};

use regex::Regex;
use zip::ZipArchive;

const MAX_PDSC_BYTES: u64 = 4 * 1024 * 1024;
const MAX_SVD_BYTES: u64 = 16 * 1024 * 1024;

#[derive(Clone, Debug)]
pub struct SupportPack {
    pub path: PathBuf,
    pub file_name: String,
    pub name: String,
    pub vendor: Option<String>,
    pub version: Option<String>,
    pub devices: Vec<String>,
    pub svd_count: usize,
    pub error: Option<String>,
}

impl SupportPack {
    pub fn device_summary(&self) -> String {
        if self.devices.is_empty() {
            "未识别设备".to_owned()
        } else if self.devices.len() == 1 {
            self.devices[0].clone()
        } else {
            format!("{} 等 {} 个设备", self.devices[0], self.devices.len())
        }
    }
}

pub fn scan(root: &Path) -> Vec<SupportPack> {
    let Ok(entries) = std::fs::read_dir(root) else {
        return Vec::new();
    };
    let mut packs = entries
        .flatten()
        .map(|entry| entry.path())
        .filter(|path| path.is_file() && is_supported(path))
        .map(|path| parse(&path))
        .collect::<Vec<_>>();
    packs.sort_by_key(|pack| pack.file_name.to_lowercase());
    packs
}

/// Find a matching device family pack in common Keil/CMSIS pack roots.
///
/// This is a fallback for projects that do not keep a `.pack` / `.pdsc` in
/// their own folder. Nothing is copied into the project; the UI offers an
/// explicit import action for external results.
pub fn scan_keil_installed(device: Option<&str>) -> Vec<SupportPack> {
    scan_keil_roots(&keil_pack_roots(), device)
}

fn scan_keil_roots(roots: &[PathBuf], device: Option<&str>) -> Vec<SupportPack> {
    let Some(device) = device else {
        return Vec::new();
    };
    let token = pack_name_token(device);
    if token.is_empty() {
        return Vec::new();
    }
    let mut candidates = Vec::new();
    for root in roots {
        collect_pack_candidates(root, &token, 0, &mut 0, &mut candidates);
        if candidates.len() >= 40 {
            break;
        }
    }
    let mut seen = std::collections::BTreeSet::new();
    let mut packs = Vec::new();
    for path in candidates {
        if !seen.insert(path.to_string_lossy().to_ascii_lowercase()) {
            continue;
        }
        let pack = parse(&path);
        if pack.error.is_some() || !device_matches(device, &pack.devices) {
            continue;
        }
        packs.push(pack);
        if packs.len() >= 8 {
            break;
        }
    }
    packs.sort_by_key(|pack| pack.file_name.to_lowercase());
    packs
}

fn keil_pack_roots() -> Vec<PathBuf> {
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
            roots.push(base.join("Keil").join("ARM").join("PACK"));
        }
    }
    roots.push(PathBuf::from(r"D:\Keil5\Core\ARM\PACK"));
    roots.push(PathBuf::from(r"D:\Keil5\ARM\PACK"));
    roots.push(PathBuf::from(r"C:\Keil_v5\ARM\PACK"));
    roots.push(PathBuf::from(r"C:\Keil\ARM\PACK"));
    if let Some(uv4) = find_in_path("UV4.exe") {
        if let Some(keil) = uv4.parent().and_then(|parent| parent.parent()) {
            roots.push(keil.join("ARM").join("PACK"));
            roots.push(keil.join("Core").join("ARM").join("PACK"));
        }
    }
    let mut seen = std::collections::BTreeSet::new();
    roots.retain(|root| seen.insert(root.to_string_lossy().to_ascii_lowercase()));
    roots
}

fn find_in_path(name: &str) -> Option<PathBuf> {
    let path = std::env::var_os("PATH")?;
    for dir in std::env::split_paths(&path) {
        let candidate = dir.join(name);
        if candidate.is_file() {
            return Some(candidate);
        }
    }
    None
}

fn collect_pack_candidates(
    dir: &Path,
    token: &str,
    depth: usize,
    visited: &mut usize,
    out: &mut Vec<PathBuf>,
) {
    if depth > 5 || *visited > 20_000 || out.len() >= 40 {
        return;
    }
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        *visited += 1;
        if *visited > 20_000 || out.len() >= 40 {
            return;
        }
        let path = entry.path();
        let Ok(file_type) = entry.file_type() else {
            continue;
        };
        if file_type.is_dir() {
            collect_pack_candidates(&path, token, depth + 1, visited, out);
            continue;
        }
        if !file_type.is_file() || !is_supported(&path) {
            continue;
        }
        let name = entry.file_name().to_string_lossy().to_ascii_lowercase();
        if name.contains(token) {
            out.push(path);
        }
    }
}

fn device_family(device: &str) -> String {
    let upper = device.trim().to_ascii_uppercase();
    if upper.starts_with("STM32") && upper.len() >= 9 {
        upper[..9].to_owned()
    } else {
        upper
    }
}

fn pack_name_token(device: &str) -> String {
    let family = device_family(device);
    if family.starts_with("STM32") && family.len() >= 7 {
        family[..7].to_ascii_lowercase()
    } else {
        family.to_ascii_lowercase()
    }
}

fn device_matches(device: &str, devices: &[String]) -> bool {
    let family = device_family(device);
    devices
        .iter()
        .any(|candidate| candidate.to_ascii_uppercase().starts_with(&family))
}

pub fn import(root: &Path, sources: &[PathBuf]) -> Result<Vec<PathBuf>, String> {
    if sources.is_empty() {
        return Ok(Vec::new());
    }

    // Validate everything before copying so a bad selection cannot leave a
    // partially imported package list behind.
    let mut planned: Vec<(PathBuf, PathBuf)> = Vec::with_capacity(sources.len());
    for source in sources {
        if !is_supported(source) {
            return Err(format!("不是 Keil/CMSIS 支持包：{}", source.display()));
        }
        let Some(name) = source.file_name() else {
            return Err(format!("无法确定文件名：{}", source.display()));
        };
        let destination = root.join(name);
        if planned
            .iter()
            .any(|(_, existing)| same_path(existing, &destination))
        {
            return Err(format!("重复选择了同名支持包：{}", name.to_string_lossy()));
        }
        if !same_path(&destination, source) && destination.exists() {
            return Err(format!(
                "工程根目录已存在同名文件：{}",
                destination.display()
            ));
        }
        planned.push((source.clone(), destination));
    }

    let mut copied = Vec::with_capacity(planned.len());
    for (source, destination) in planned {
        if !same_path(&destination, &source) {
            std::fs::copy(&source, &destination).map_err(|err| {
                format!(
                    "复制支持包 {} 到 {} 失败：{err}",
                    source.display(),
                    destination.display()
                )
            })?;
        }
        copied.push(destination);
    }
    Ok(copied)
}

/// Read the first SVD entry from a Keil/CMSIS `.pack` so the register browser
/// can use the package without extracting it into the project.
/// Read the package description from a `.pack` archive or a standalone
/// `.pdsc`. Used for device memory information and SVds.
pub fn read_pdsc(path: &Path) -> Option<String> {
    let extension = path
        .extension()
        .and_then(|ext| ext.to_str())
        .unwrap_or_default()
        .to_ascii_lowercase();
    if extension == "pack" {
        let file = File::open(path).ok()?;
        let mut archive = ZipArchive::new(file).ok()?;
        let index = archive.file_names().enumerate().find_map(|(index, name)| {
            name.ok()
                .filter(|name| name.to_ascii_lowercase().ends_with(".pdsc"))
                .map(|_| index)
        })?;
        let mut text = String::new();
        archive
            .by_index(index)
            .ok()?
            .take(MAX_PDSC_BYTES)
            .read_to_string(&mut text)
            .ok()?;
        Some(text)
    } else {
        std::fs::read_to_string(path).ok()
    }
}

pub fn read_first_svd(path: &Path) -> Option<(String, String)> {
    let file = File::open(path).ok()?;
    let mut archive = ZipArchive::new(file).ok()?;
    let index = archive.file_names().enumerate().find_map(|(index, name)| {
        name.ok()
            .filter(|name| name.to_ascii_lowercase().ends_with(".svd"))
            .map(|name| (index, name.into_owned()))
    })?;
    let (index, name) = index;
    let mut text = String::new();
    archive
        .by_index(index)
        .ok()?
        .take(MAX_SVD_BYTES)
        .read_to_string(&mut text)
        .ok()?;
    Some((name, text))
}

pub fn is_supported(path: &Path) -> bool {
    path.extension()
        .and_then(|ext| ext.to_str())
        .is_some_and(|ext| ext.eq_ignore_ascii_case("pack") || ext.eq_ignore_ascii_case("pdsc"))
}

fn parse(path: &Path) -> SupportPack {
    let file_name = path
        .file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .unwrap_or_else(|| path.display().to_string());
    let extension = path
        .extension()
        .and_then(|ext| ext.to_str())
        .unwrap_or_default()
        .to_ascii_lowercase();
    let result = if extension == "pack" {
        parse_pack(path)
    } else {
        std::fs::read_to_string(path)
            .map(|text| parse_pdsc(&text))
            .map_err(|err| format!("读取 {file_name} 失败：{err}"))
    };
    match result {
        Ok((name, vendor, version, devices, svd_count)) => SupportPack {
            path: path.to_path_buf(),
            file_name,
            name: name.unwrap_or_else(|| {
                path.file_stem()
                    .map(|name| name.to_string_lossy().into_owned())
                    .unwrap_or_else(|| "支持包".to_owned())
            }),
            vendor,
            version,
            devices,
            svd_count,
            error: None,
        },
        Err(error) => SupportPack {
            path: path.to_path_buf(),
            file_name,
            name: path
                .file_stem()
                .map(|name| name.to_string_lossy().into_owned())
                .unwrap_or_else(|| "支持包".to_owned()),
            vendor: None,
            version: None,
            devices: Vec::new(),
            svd_count: 0,
            error: Some(error),
        },
    }
}

type ParsedPack = (
    Option<String>,
    Option<String>,
    Option<String>,
    Vec<String>,
    usize,
);

fn parse_pack(path: &Path) -> Result<ParsedPack, String> {
    let file = File::open(path).map_err(|err| format!("打开 {} 失败：{err}", path.display()))?;
    let mut archive =
        ZipArchive::new(file).map_err(|err| format!("解析 {} 失败：{err}", path.display()))?;
    let names = archive
        .file_names()
        .filter_map(|name| name.ok().map(|name| name.into_owned()))
        .collect::<Vec<_>>();
    let pdsc_index = names
        .iter()
        .position(|name| name.to_ascii_lowercase().ends_with(".pdsc"))
        .ok_or_else(|| format!("{} 中没有 .pdsc 文件", path.display()))?;
    let mut pdsc = String::new();
    archive
        .by_index(pdsc_index)
        .map_err(|err| format!("读取 {} 的 .pdsc 失败：{err}", path.display()))?
        .take(MAX_PDSC_BYTES)
        .read_to_string(&mut pdsc)
        .map_err(|err| format!("解码 {} 的 .pdsc 失败：{err}", path.display()))?;
    let mut parsed = parse_pdsc(&pdsc);
    let svd_count = names
        .iter()
        .filter(|name| name.to_ascii_lowercase().ends_with(".svd"))
        .count();
    if parsed.4 == 0 {
        parsed.4 = svd_count;
    }
    Ok(parsed)
}

fn parse_pdsc(text: &str) -> ParsedPack {
    let name = first_capture(text, r"(?is)<name>\s*([^<]+?)\s*</name>");
    let vendor = first_capture(text, r"(?is)<vendor>\s*([^<]+?)\s*</vendor>");
    let version = first_capture(text, r"(?is)<version>\s*([^<]+?)\s*</version>");
    let devices = all_captures(text, r#"(?i)Dname\s*=\s*"([^"]+)""#);
    let devices = if devices.is_empty() {
        all_captures(text, r#"(?i)Dfamily\s*=\s*"([^"]+)""#)
    } else {
        devices
    };
    let svd_count = all_captures(text, r"(?is)<svd>\s*([^<]+?)\s*</svd>").len()
        + all_captures(text, r#"(?is)<debug\s+[^>]*svd\s*=\s*"([^"]+)""#).len();
    (name, vendor, version, devices, svd_count)
}

fn first_capture(text: &str, pattern: &str) -> Option<String> {
    let regex = Regex::new(pattern).ok()?;
    regex
        .captures(text)
        .and_then(|captures| captures.get(1))
        .map(|value| value.as_str().trim().to_owned())
        .filter(|value| !value.is_empty())
}

fn all_captures(text: &str, pattern: &str) -> Vec<String> {
    let Ok(regex) = Regex::new(pattern) else {
        return Vec::new();
    };
    let mut values = Vec::new();
    for captures in regex.captures_iter(text) {
        let Some(value) = captures.get(1) else {
            continue;
        };
        let value = value.as_str().trim();
        if !value.is_empty()
            && !values
                .iter()
                .any(|existing: &String| existing.eq_ignore_ascii_case(value))
        {
            values.push(value.to_owned());
        }
        if values.len() >= 2000 {
            break;
        }
    }
    values
}

fn same_path(a: &Path, b: &Path) -> bool {
    match (std::fs::canonicalize(a), std::fs::canonicalize(b)) {
        (Ok(a), Ok(b)) => a == b,
        _ => a
            .to_string_lossy()
            .eq_ignore_ascii_case(&b.to_string_lossy()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE: &str = r#"<?xml version="1.0"?>
<package schemaVersion="1.4" xmlns:xs="http://www.w3.org/2001/XMLSchema-instance">
  <name>STM32F1xx_DFP</name>
  <vendor>Keil</vendor>
  <version>2.4.1</version>
  <devices>
    <family Dfamily="STM32F1 Series" Dvendor="STMicroelectronics:13">
      <device Dname="STM32F103C8">
        <memory name="IROM1" access="read" start="0x08000000" size="0x10000"/>
      </device>
      <device Dname="STM32F103RC">
        <memory name="IROM1" access="read" start="0x08000000" size="0x40000"/>
      </device>
    </family>
    <svd>CMSIS/SVD/STM32F103xx.svd</svd>
  </devices>
</package>"#;

    #[test]
    fn parses_pdsc_metadata() {
        let (name, vendor, version, devices, svd_count) = parse_pdsc(SAMPLE);
        assert_eq!(name.as_deref(), Some("STM32F1xx_DFP"));
        assert_eq!(vendor.as_deref(), Some("Keil"));
        assert_eq!(version.as_deref(), Some("2.4.1"));
        assert_eq!(devices.len(), 2);
        assert_eq!(devices[1], "STM32F103RC");
        assert_eq!(svd_count, 1);
    }

    #[test]
    fn scans_pdsc_in_project_root() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("sample.pdsc"), SAMPLE).unwrap();
        let packs = scan(dir.path());
        assert_eq!(packs.len(), 1);
        assert_eq!(packs[0].name, "STM32F1xx_DFP");
        assert!(packs[0].error.is_none());
        assert_eq!(packs[0].svd_count, 1);
    }

    #[test]
    fn parses_pdsc_inside_pack_zip() {
        use std::io::Write;

        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("sample.pack");
        let file = File::create(&path).unwrap();
        let mut zip = zip::ZipWriter::new(file);
        let options = zip::write::SimpleFileOptions::default();
        zip.start_file("sample.pdsc", options).unwrap();
        zip.write_all(SAMPLE.as_bytes()).unwrap();
        zip.start_file("CMSIS/SVD/STM32F103xx.svd", options)
            .unwrap();
        zip.write_all(b"<device/>").unwrap();
        zip.finish().unwrap();

        let pack = parse(&path);
        assert!(pack.error.is_none());
        assert_eq!(pack.name, "STM32F1xx_DFP");
        assert_eq!(pack.version.as_deref(), Some("2.4.1"));
        assert_eq!(pack.devices.len(), 2);
        assert_eq!(pack.svd_count, 1);

        let (svd_name, svd) = read_first_svd(&path).unwrap();
        assert!(svd_name.ends_with("STM32F103xx.svd"));
        assert_eq!(svd, "<device/>");
    }

    #[test]
    fn imports_pack_into_project_root() {
        let dir = tempfile::tempdir().unwrap();
        let source_dir = tempfile::tempdir().unwrap();
        let source = source_dir.path().join("sample.pdsc");
        std::fs::write(&source, SAMPLE).unwrap();
        let copied = import(dir.path(), &[source]).unwrap();
        assert_eq!(copied.len(), 1);
        assert!(dir.path().join("sample.pdsc").is_file());
    }

    #[test]
    fn validates_all_sources_before_copying() {
        let dir = tempfile::tempdir().unwrap();
        let source_dir = tempfile::tempdir().unwrap();
        let good = source_dir.path().join("sample.pdsc");
        let bad = source_dir.path().join("notes.txt");
        std::fs::write(&good, SAMPLE).unwrap();
        std::fs::write(&bad, "not a pack").unwrap();

        let error = import(dir.path(), &[good, bad]).unwrap_err();
        assert!(error.contains("不是 Keil/CMSIS 支持包"));
        assert!(!dir.path().join("sample.pdsc").exists());
    }
}
