//! Lightweight project metadata extracted from Makefiles and Keil uVision projects.

use std::path::{Path, PathBuf};

#[derive(Clone, Debug, Default)]
pub struct ProjectMeta {
    pub toolchain: Option<String>,
    pub target: Option<String>,
    pub device: Option<String>,
    pub cpu: Option<String>,
    pub fpu: Option<String>,
    pub memory_layout: Option<String>,
    pub linker_script: Option<String>,
    pub output_dir: Option<String>,
    pub include_dirs: Vec<String>,
    pub library_dirs: Vec<String>,
    pub defines: Vec<String>,
    pub compiler: Option<String>,
    pub optimization: Option<String>,
}

impl ProjectMeta {
    pub fn detect(root: &Path) -> Self {
        if let Some(path) = find_uvprojx(root) {
            return Self::from_keil(&path);
        }
        if let Some(path) = find_makefile(root) {
            return Self::from_makefile(&path);
        }
        Self::default()
    }

    fn from_makefile(path: &Path) -> Self {
        let Ok(text) = std::fs::read_to_string(path) else {
            return Self::default();
        };

        let target = make_var(&text, "TARGET");
        let output_dir =
            make_var(&text, "BUILD_DIR").or_else(|| target.as_deref().and_then(target_output_dir));
        let linker_script = make_var(&text, "LDSCRIPT");
        let optimization = make_var(&text, "OPT");
        let prefix = make_var(&text, "PREFIX");
        let cc = make_var(&text, "CC");
        let cpu_raw = make_var(&text, "CPU")
            .or_else(|| make_var(&text, "MCU"))
            .unwrap_or_default();
        let fpu_raw = make_var(&text, "FPU").unwrap_or_default();

        let cflags = make_var(&text, "CFLAGS").unwrap_or_default();
        let defines = make_var(&text, "C_DEFS")
            .unwrap_or_else(|| cflags.clone())
            .split_whitespace()
            .filter_map(|token| token.strip_prefix("-D"))
            .map(ToOwned::to_owned)
            .collect::<Vec<_>>();
        let include_dirs = make_var(&text, "C_INCLUDES")
            .unwrap_or(cflags)
            .split_whitespace()
            .filter_map(|token| token.strip_prefix("-I"))
            .map(ToOwned::to_owned)
            .collect::<Vec<_>>();
        let library_dirs = make_var(&text, "LIBDIR")
            .or_else(|| make_var(&text, "LDFLAGS"))
            .unwrap_or_default()
            .split_whitespace()
            .filter_map(|token| token.strip_prefix("-L"))
            .map(ToOwned::to_owned)
            .collect::<Vec<_>>();

        let device = defines
            .iter()
            .find(|define| define.starts_with("STM32"))
            .cloned();
        let cpu = cpu_raw
            .split_whitespace()
            .find_map(|token| token.strip_prefix("-mcpu="))
            .map(display_cpu)
            .or_else(|| (!cpu_raw.is_empty()).then(|| display_cpu(&cpu_raw)));
        let fpu = if fpu_raw.trim().is_empty() {
            Some("无".to_owned())
        } else {
            Some(display_fpu(&fpu_raw))
        };
        let toolchain =
            if cc.as_deref().is_some_and(|value| value.contains("gcc")) || prefix.is_some() {
                Some("Make · GCC".to_owned())
            } else {
                Some("Make".to_owned())
            };
        let compiler = cc.map(|value| {
            let value = value.replace("$(PREFIX)", prefix.as_deref().unwrap_or(""));
            value.replace("$(GCC_PATH)", "GCC_PATH")
        });

        Self {
            toolchain,
            target,
            device,
            cpu,
            fpu,
            memory_layout: None,
            linker_script,
            output_dir,
            include_dirs,
            library_dirs,
            defines,
            compiler,
            optimization,
        }
    }

    fn from_keil(path: &Path) -> Self {
        let Ok(xml) = std::fs::read_to_string(path) else {
            return Self::default();
        };

        let target = first_nonempty(&xml, "TargetName");
        let device = first_nonempty(&xml, "Device");
        let cpu_raw = first_nonempty(&xml, "Cpu").unwrap_or_default();
        let cpu = quoted_value(&cpu_raw, "CPUTYPE").map(|value| value.to_owned());
        let fpu = if cpu_raw.contains("FPU3") {
            Some("双精度".to_owned())
        } else if cpu_raw.contains("FPU2") {
            Some("单精度".to_owned())
        } else {
            Some("无".to_owned())
        };
        let memory_layout = memory_layout(&cpu_raw);
        let linker_script =
            first_nonempty(&xml, "ScatterFile").or_else(|| Some("自动生成".to_owned()));
        let output_dir = first_nonempty(&xml, "OutputDirectory");
        let include_dirs = first_nonempty(&xml, "IncludePath")
            .map(|value| split_list(&value, ';'))
            .unwrap_or_default();
        let library_dirs = first_nonempty(&xml, "LibPath")
            .map(|value| split_list(&value, ';'))
            .unwrap_or_default();
        let defines = first_nonempty(&xml, "Define")
            .map(|value| split_list(&value, ','))
            .unwrap_or_default();
        let ac6 = first_nonempty(&xml, "uAC6").as_deref() == Some("1");
        let compiler = first_nonempty(&xml, "pCCUsed").map(|value| {
            value
                .split("::")
                .nth(1)
                .unwrap_or(value.as_str())
                .to_owned()
        });
        let optimization = first_nonempty(&xml, "Optim").map(|value| {
            match value.as_str() {
                "1" => "O0",
                "2" => "O1",
                "3" => "O2",
                "4" => "O3",
                _ => value.as_str(),
            }
            .to_owned()
        });

        Self {
            toolchain: Some(if ac6 {
                "Keil MDK · AC6".to_owned()
            } else {
                "Keil MDK · AC5".to_owned()
            }),
            target,
            device,
            cpu,
            fpu,
            memory_layout,
            linker_script,
            output_dir,
            include_dirs,
            library_dirs,
            defines,
            compiler,
            optimization,
        }
    }
}

fn find_makefile(root: &Path) -> Option<PathBuf> {
    for name in ["Makefile", "makefile", "GNUmakefile"] {
        let path = root.join(name);
        if path.is_file() {
            return Some(path);
        }
    }
    None
}

fn find_uvprojx(root: &Path) -> Option<PathBuf> {
    for name in ["MDK-ARM", "MDK", "Project", "Projects", "Keil", "keil"] {
        let dir = root.join(name);
        if let Some(path) = find_extension(&dir, "uvprojx", 1) {
            return Some(path);
        }
    }
    find_extension(root, "uvprojx", 2)
}

fn find_extension(root: &Path, extension: &str, max_depth: usize) -> Option<PathBuf> {
    let mut pending = vec![(root.to_path_buf(), 0usize)];
    while let Some((dir, depth)) = pending.pop() {
        let Ok(entries) = std::fs::read_dir(&dir) else {
            continue;
        };
        let mut paths: Vec<PathBuf> = entries.flatten().map(|entry| entry.path()).collect();
        paths.sort();
        for path in &paths {
            if path.is_file()
                && path
                    .extension()
                    .is_some_and(|value| value.eq_ignore_ascii_case(extension))
            {
                return Some(path.clone());
            }
        }
        if depth >= max_depth {
            continue;
        }
        for path in paths {
            if !path.is_dir() {
                continue;
            }
            let name = path
                .file_name()
                .map(|value| value.to_string_lossy().to_ascii_lowercase())
                .unwrap_or_default();
            if name.starts_with('.')
                || matches!(
                    name.as_str(),
                    "build" | "target" | "drivers" | "core" | "components" | "rte" | "objects"
                )
            {
                continue;
            }
            pending.push((path, depth + 1));
        }
    }
    None
}

fn make_var(text: &str, key: &str) -> Option<String> {
    let lines: Vec<&str> = text.lines().collect();
    let mut index = 0;
    while index < lines.len() {
        let trimmed = lines[index].trim_start();
        let Some(rest) = trimmed.strip_prefix(key) else {
            index += 1;
            continue;
        };
        let rest = rest.trim_start();
        let Some(value) = rest
            .strip_prefix(":=")
            .or_else(|| rest.strip_prefix("?="))
            .or_else(|| rest.strip_prefix("+="))
            .or_else(|| rest.strip_prefix('='))
        else {
            index += 1;
            continue;
        };

        let mut value = value.trim().to_owned();
        while value.ends_with('\\') && index + 1 < lines.len() {
            value.pop();
            index += 1;
            value.push(' ');
            value.push_str(lines[index].trim());
        }
        if !value.is_empty() {
            return Some(value.trim().to_owned());
        }
        index += 1;
    }
    None
}

fn first_nonempty(xml: &str, name: &str) -> Option<String> {
    tag_values(xml, name)
        .into_iter()
        .find(|value| !value.is_empty())
}

fn tag_values(xml: &str, name: &str) -> Vec<String> {
    let open = format!("<{name}>");
    let close = format!("</{name}>");
    let mut values = Vec::new();
    let mut rest = xml;
    while let Some(start) = rest.find(&open) {
        let after = &rest[start + open.len()..];
        let Some(end) = after.find(&close) else {
            break;
        };
        values.push(xml_unescape(after[..end].trim()));
        rest = &after[end + close.len()..];
    }
    values
}

fn xml_unescape(value: &str) -> String {
    value
        .replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&quot;", "\"")
        .replace("&apos;", "'")
        .replace("&amp;", "&")
}

fn quoted_value<'a>(text: &'a str, key: &str) -> Option<&'a str> {
    let marker = format!("{key}(\"");
    let start = text.find(&marker)? + marker.len();
    let end = text[start..].find("\")")? + start;
    Some(&text[start..end])
}

fn memory_layout(cpu: &str) -> Option<String> {
    let flash = range_value(cpu, "IROM");
    let ram = range_value(cpu, "IRAM");
    match (flash, ram) {
        (Some(flash), Some(ram)) => Some(format!("Flash {flash} · RAM {ram}")),
        (Some(flash), None) => Some(format!("Flash {flash}")),
        (None, Some(ram)) => Some(format!("RAM {ram}")),
        (None, None) => None,
    }
}

fn range_value(cpu: &str, key: &str) -> Option<String> {
    let marker = format!("{key}(");
    let start = cpu.find(&marker)? + marker.len();
    let end = cpu[start..].find(')')? + start;
    Some(cpu[start..end].to_owned())
}

fn split_list(value: &str, separator: char) -> Vec<String> {
    value
        .split(separator)
        .flat_map(|item| item.split(';'))
        .map(str::trim)
        .filter(|item| !item.is_empty())
        .map(ToOwned::to_owned)
        .collect()
}

fn display_cpu(value: &str) -> String {
    let value = value.trim();
    let upper = value.to_ascii_lowercase();
    if upper.contains("cortex-m0") {
        "Cortex-M0".to_owned()
    } else if upper.contains("cortex-m3") {
        "Cortex-M3".to_owned()
    } else if upper.contains("cortex-m4") {
        "Cortex-M4".to_owned()
    } else if upper.contains("cortex-m7") {
        "Cortex-M7".to_owned()
    } else {
        value.to_owned()
    }
}

fn display_fpu(value: &str) -> String {
    let lower = value.to_ascii_lowercase();
    if lower.contains("fpv5-d") || lower.contains("double") {
        "双精度".to_owned()
    } else if lower.contains("fpv") || lower.contains("single") {
        "单精度".to_owned()
    } else if lower.contains("none") || lower.is_empty() {
        "无".to_owned()
    } else {
        value.to_owned()
    }
}

fn target_output_dir(target: &str) -> Option<String> {
    let path = Path::new(target);
    let parent = path.parent()?;
    if parent.as_os_str().is_empty() {
        None
    } else {
        Some(parent.display().to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_cube_makefile_metadata() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(
            dir.path().join("Makefile"),
            "TARGET = demo\nBUILD_DIR = build\nCPU = -mcpu=cortex-m3\nFPU =\nLDSCRIPT = flash.ld\nOPT = -Og\nPREFIX = arm-none-eabi-\nCC = $(PREFIX)gcc\nC_DEFS = \\\n-DUSE_HAL_DRIVER \\\n-DSTM32F103xB\nC_INCLUDES = \\\n-ICore/Inc \\\n-IDrivers/HAL/Inc\n",
        )
        .unwrap();

        let meta = ProjectMeta::detect(dir.path());
        assert_eq!(meta.toolchain.as_deref(), Some("Make · GCC"));
        assert_eq!(meta.target.as_deref(), Some("demo"));
        assert_eq!(meta.cpu.as_deref(), Some("Cortex-M3"));
        assert_eq!(meta.device.as_deref(), Some("STM32F103xB"));
        assert_eq!(meta.linker_script.as_deref(), Some("flash.ld"));
        assert_eq!(meta.output_dir.as_deref(), Some("build"));
        assert!(meta.include_dirs.iter().any(|value| value == "Core/Inc"));
        assert!(meta.defines.iter().any(|value| value == "USE_HAL_DRIVER"));
    }

    #[test]
    fn parses_keil_metadata() {
        let dir = tempfile::tempdir().unwrap();
        let mdk = dir.path().join("MDK-ARM");
        std::fs::create_dir_all(&mdk).unwrap();
        std::fs::write(
            mdk.join("demo.uvprojx"),
            r#"<Project>
<Targets><Target>
<TargetName>demo</TargetName>
<uAC6>1</uAC6>
<pCCUsed>5060960::V6.19::.\ARMCLANG</pCCUsed>
<TargetOption><TargetCommonOption>
<Device>STM32G474CBTx</Device>
<Cpu>IRAM(0x20000000-0x2001FFFF) IROM(0x8000000-0x801FFFF) FPU2 CPUTYPE("Cortex-M4")</Cpu>
<OutputDirectory>build\</OutputDirectory>
<OutputName>demo</OutputName>
<CreateHexFile>1</CreateHexFile>
<ScatterFile>flash.sct</ScatterFile>
<Flash2>BIN\UL2V8M.DLL</Flash2>
</TargetCommonOption></TargetOption>
<Groups><Group><GroupOption><Cads><VariousControls>
<Define>USE_HAL_DRIVER,STM32G474xx</Define>
<IncludePath>../Core/Inc;../Drivers/CMSIS/Include</IncludePath>
</VariousControls></Cads></GroupOption></Group></Groups>
<Cads><Optim>4</Optim></Cads>
</Target></Targets>
</Project>"#,
        )
        .unwrap();

        let meta = ProjectMeta::detect(dir.path());
        assert_eq!(meta.toolchain.as_deref(), Some("Keil MDK · AC6"));
        assert_eq!(meta.target.as_deref(), Some("demo"));
        assert_eq!(meta.device.as_deref(), Some("STM32G474CBTx"));
        assert_eq!(meta.cpu.as_deref(), Some("Cortex-M4"));
        assert_eq!(meta.fpu.as_deref(), Some("单精度"));
        assert_eq!(meta.output_dir.as_deref(), Some("build\\"));
        assert!(meta.include_dirs.iter().any(|value| value == "../Core/Inc"));
        assert!(meta.defines.iter().any(|value| value == "STM32G474xx"));
    }
}
