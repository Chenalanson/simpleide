//! Flash tool configuration and OpenOCD command construction.
//!
//! The UI keeps a small, serializable description of the selected flasher.
//! OpenOCD is launched directly (no shell) so paths with spaces do not need
//! platform-specific quoting.

use std::ffi::OsString;
use std::path::{Path, PathBuf};
use std::time::UNIX_EPOCH;

use serde::{Deserialize, Serialize};

use crate::build::RunSpec;
use crate::project_meta::ProjectMeta;
use crate::util;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FlashTool {
    #[default]
    Custom,
    #[serde(rename = "openocd")]
    OpenOcd,
}

impl FlashTool {
    pub fn label(self) -> &'static str {
        match self {
            Self::Custom => "自定义命令",
            Self::OpenOcd => "OpenOCD",
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(default)]
pub struct OpenOcdConfig {
    /// `openocd`, an absolute path, or a name on PATH.
    pub executable: String,
    /// Supports `${ExecutableName}`, `${ProjectName}`, `${workspaceFolder}`
    /// and `${workspaceRoot}`.
    pub program_file: String,
    /// Target script, e.g. `stm32f1x.cfg` or `target/stm32f1x.cfg`.
    pub target_config: String,
    /// Interface script, e.g. `stlink.cfg` or `interface/stlink.cfg`.
    pub interface_config: String,
}

impl Default for OpenOcdConfig {
    fn default() -> Self {
        Self {
            executable: "openocd".to_owned(),
            program_file: "${ExecutableName}.hex".to_owned(),
            target_config: String::new(),
            interface_config: "stlink.cfg".to_owned(),
        }
    }
}

impl OpenOcdConfig {
    pub fn normalized(&self) -> Self {
        Self {
            executable: self.executable.trim().to_owned(),
            program_file: self.program_file.trim().to_owned(),
            target_config: self.target_config.trim().to_owned(),
            interface_config: self.interface_config.trim().to_owned(),
        }
    }
}

pub fn openocd_ready(config: &OpenOcdConfig) -> bool {
    !config.program_file.trim().is_empty()
        && !config.target_config.trim().is_empty()
        && !config.interface_config.trim().is_empty()
}

pub fn infer_target_config(device: Option<&str>) -> Option<String> {
    let device = device?.trim().to_ascii_uppercase();
    let family = [
        ("STM32F0", "stm32f0x"),
        ("STM32F1", "stm32f1x"),
        ("STM32F2", "stm32f2x"),
        ("STM32F3", "stm32f3x"),
        ("STM32F4", "stm32f4x"),
        ("STM32F7", "stm32f7x"),
        ("STM32G0", "stm32g0x"),
        ("STM32G4", "stm32g4x"),
        ("STM32H5", "stm32h5x"),
        ("STM32H7", "stm32h7x"),
        ("STM32L0", "stm32l0x"),
        ("STM32L1", "stm32l1x"),
        ("STM32L4", "stm32l4x"),
        ("STM32L5", "stm32l5x"),
        ("STM32U5", "stm32u5x"),
        ("STM32WB", "stm32wbx"),
        ("STM32WL", "stm32wlx"),
    ]
    .iter()
    .find_map(|(prefix, script)| device.starts_with(prefix).then_some(*script))?;
    Some(format!("{family}.cfg"))
}

pub fn infer_interface_config(driver: Option<&str>) -> Option<String> {
    let driver = driver?.to_ascii_uppercase();
    if driver.contains("JLINK") || driver.contains("J-LINK") {
        Some("jlink.cfg".to_owned())
    } else if driver.contains("STLINK") || driver.contains("ST-LINK") {
        Some("stlink.cfg".to_owned())
    } else if driver.contains("CMSIS-DAP") || driver.contains("CMSIS_DAP") {
        Some("cmsis-dap.cfg".to_owned())
    } else {
        None
    }
}

/// Build a direct `openocd` invocation. The program is passed as one Tcl
/// command through `-c`, so it must exist before the flash starts.
pub fn make_spec(config: &OpenOcdConfig, root: &Path) -> Result<RunSpec, String> {
    let program = resolve_program(config, root, true)?;
    let interface = resolve_script(root, &config.interface_config, "interface")?;
    let target = resolve_script(root, &config.target_config, "target")?;
    let program_command = format!("program {} verify reset exit", tcl_word(&program.command));
    let executable = resolve_executable(effective_executable(config), root);
    let args: Vec<OsString> = vec![
        "-f".into(),
        interface.clone().into(),
        "-f".into(),
        target.clone().into(),
        "-c".into(),
        program_command.clone().into(),
    ];
    let display = format!(
        "{} -f {} -f {} -c \"{}\"",
        util::relative_display(root, &executable),
        interface,
        target,
        program_command
    );
    Ok(RunSpec {
        program: executable.into_os_string(),
        args,
        cwd: root.to_path_buf(),
        display,
        env_paths: Vec::new(),
    })
}

/// Same composition as [`make_spec`] but does not require the build output to
/// exist yet. Used by the configuration modal for a live preview.
pub fn preview(config: &OpenOcdConfig, root: &Path) -> Result<String, String> {
    let program = resolve_program(config, root, false)?;
    let interface = resolve_script(root, &config.interface_config, "interface")?;
    let target = resolve_script(root, &config.target_config, "target")?;
    let program_command = format!("program {} verify reset exit", tcl_word(&program.command));
    let executable = resolve_executable(effective_executable(config), root);
    Ok(format!(
        "{} -f {} -f {} -c \"{}\"",
        util::relative_display(root, &executable),
        interface,
        target,
        program_command
    ))
}

pub fn effective_executable(config: &OpenOcdConfig) -> &str {
    let value = config.executable.trim();
    if value.is_empty() {
        "openocd"
    } else {
        value
    }
}

fn resolve_executable(value: &str, root: &Path) -> PathBuf {
    let path = Path::new(value);
    if path.is_absolute() || (!value.contains('/') && !value.contains('\\')) {
        path.to_path_buf()
    } else {
        root.join(path)
    }
}

pub fn openocd_available(executable: &str, root: &Path) -> bool {
    let value = executable.trim();
    if value.is_empty() {
        return find_in_path("openocd").is_some();
    }
    if value.contains('/') || value.contains('\\') || Path::new(value).is_absolute() {
        return resolve_executable(value, root).is_file();
    }
    find_in_path(value).is_some()
}

/// Candidate executable paths for the tool-path dropdown. The first entry is
/// the currently configured value, followed by PATH and common installs.
pub fn openocd_candidates(configured: &str) -> Vec<String> {
    let mut values = Vec::new();
    let configured = configured.trim();
    if !configured.is_empty() {
        push_unique(&mut values, configured.to_owned());
    }
    if find_in_path("openocd").is_some() {
        push_unique(&mut values, "openocd".to_owned());
    }

    #[cfg(target_os = "windows")]
    {
        if let Some(base) = std::env::var_os("ProgramFiles") {
            scan_openocd_dirs(Path::new(&base), &mut values);
        }
        if let Some(base) = std::env::var_os("ProgramFiles(x86)") {
            scan_openocd_dirs(Path::new(&base), &mut values);
        }
        if let Some(base) = std::env::var_os("LOCALAPPDATA") {
            let programs = Path::new(&base).join("Programs");
            if programs.is_dir() {
                scan_openocd_dirs(&programs, &mut values);
            } else {
                push_if_file(&mut values, programs.join("OpenOCD/bin/openocd.exe"));
            }
        }
        if let Some(base) = std::env::var_os("USERPROFILE") {
            let packages = Path::new(&base).join(".platformio/packages");
            if let Ok(entries) = std::fs::read_dir(&packages) {
                for entry in entries.flatten() {
                    let name = entry.file_name().to_string_lossy().to_ascii_lowercase();
                    if name.starts_with("tool-openocd") {
                        push_if_file(&mut values, entry.path().join("bin/openocd.exe"));
                    }
                }
            }
        }
    }
    #[cfg(not(target_os = "windows"))]
    {
        push_if_file(&mut values, PathBuf::from("/usr/bin/openocd"));
        push_if_file(&mut values, PathBuf::from("/usr/local/bin/openocd"));
    }

    if values.is_empty() {
        values.push("openocd".to_owned());
    }
    values
}

fn scan_openocd_dirs(base: &Path, values: &mut Vec<String>) {
    push_if_file(values, base.join("OpenOCD/bin/openocd.exe"));
    let Ok(entries) = std::fs::read_dir(base) else {
        return;
    };
    for entry in entries.flatten() {
        let name = entry.file_name().to_string_lossy().to_ascii_lowercase();
        if name.starts_with("openocd") {
            push_if_file(values, entry.path().join("bin/openocd.exe"));
        }
    }
}

fn push_if_file(values: &mut Vec<String>, path: PathBuf) {
    if path.is_file() {
        push_unique(values, util::display_path(&path));
    }
}

fn push_unique(values: &mut Vec<String>, value: String) {
    if value.trim().is_empty() {
        return;
    }
    if values
        .iter()
        .any(|existing| existing.eq_ignore_ascii_case(&value))
    {
        return;
    }
    values.push(value);
}

fn find_in_path(name: &str) -> Option<PathBuf> {
    let paths = std::env::var_os("PATH")?;
    let mut names = vec![name.to_owned()];
    if cfg!(target_os = "windows") && Path::new(name).extension().is_none() {
        names.push(format!("{name}.exe"));
    }
    std::env::split_paths(&paths).find_map(|dir| {
        names
            .iter()
            .map(|name| dir.join(name))
            .find(|candidate| candidate.is_file())
    })
}

struct ResolvedProgram {
    command: String,
}

fn resolve_program(
    config: &OpenOcdConfig,
    root: &Path,
    require_exists: bool,
) -> Result<ResolvedProgram, String> {
    let template = config.program_file.trim();
    if template.is_empty() {
        return Err("程序文件不能为空".to_owned());
    }

    let project_name = root
        .file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .filter(|name| !name.is_empty())
        .unwrap_or_else(|| "firmware".to_owned());
    let uses_executable_name = template.contains("${ExecutableName}");
    // Avoid parsing a potentially large .uvprojx on every preview frame when
    // the build output is already in one of the common directories.
    let mut meta = None;
    let mut artifact = uses_executable_name
        .then(|| find_artifact(root, template, None))
        .flatten();
    if uses_executable_name && artifact.is_none() {
        let detected = ProjectMeta::detect(root);
        artifact = find_artifact(root, template, detected.output_dir.as_deref());
        meta = Some(detected);
    }
    let executable_name = artifact
        .as_deref()
        .map(|path| util::relative_display(root, &path.with_extension("")))
        .unwrap_or_else(|| {
            let output_dir = if uses_executable_name {
                meta.get_or_insert_with(|| ProjectMeta::detect(root))
                    .output_dir
                    .as_deref()
                    .unwrap_or("")
                    .to_owned()
            } else {
                String::new()
            };
            let output_dir = output_dir.trim().trim_matches(['/', '\\']);
            if output_dir.is_empty() {
                project_name.clone()
            } else {
                format!(
                    "{}/{}",
                    output_dir.replace('\\', "/").trim_end_matches('/'),
                    project_name
                )
            }
        });
    let root_text = root.to_string_lossy().replace('\\', "/");
    let expanded = template
        .replace("${ExecutableName}", &executable_name)
        .replace("${ProjectName}", &project_name)
        .replace("${workspaceFolder}", &root_text)
        .replace("${workspaceRoot}", &root_text)
        .replace('\\', "/");
    let path = PathBuf::from(&expanded);
    let absolute = if path.is_absolute() {
        path
    } else {
        root.join(&path)
    };
    if absolute
        .extension()
        .and_then(|extension| extension.to_str())
        .is_some_and(|extension| extension.eq_ignore_ascii_case("bin"))
    {
        return Err("OpenOCD 烧录 .bin 需要额外指定起始地址，请改用 .hex 或 .elf".to_owned());
    }
    if require_exists && !absolute.is_file() {
        let hint = if template.contains("${ExecutableName}") {
            "，请先构建或修改程序文件"
        } else {
            ""
        };
        return Err(format!(
            "找不到程序文件 {}{hint}",
            util::relative_display(root, &absolute)
        ));
    }
    Ok(ResolvedProgram {
        command: util::relative_display(root, &absolute),
    })
}

fn resolve_script(root: &Path, value: &str, folder: &str) -> Result<String, String> {
    let value = value.trim();
    if value.is_empty() {
        return Err(match folder {
            "interface" => "接口配置不能为空".to_owned(),
            _ => "芯片配置不能为空".to_owned(),
        });
    }
    let normalized = value.replace('\\', "/");
    if root.join(value).is_file() || normalized.contains('/') || Path::new(value).is_absolute() {
        Ok(normalized)
    } else {
        Ok(format!("{folder}/{normalized}"))
    }
}

fn find_artifact(root: &Path, template: &str, output_dir: Option<&str>) -> Option<PathBuf> {
    let preferred = Path::new(template)
        .extension()
        .and_then(|extension| extension.to_str())
        .map(|extension| extension.to_ascii_lowercase());
    let mut extensions = Vec::new();
    if let Some(extension) = preferred.filter(|extension| {
        matches!(
            extension.as_str(),
            "hex" | "elf" | "axf" | "bin" | "s19" | "srec"
        )
    }) {
        extensions.push(extension);
    }
    for extension in ["hex", "elf", "axf", "bin"] {
        if !extensions.iter().any(|value| value == extension) {
            extensions.push(extension.to_owned());
        }
    }

    let mut dirs = Vec::new();
    if let Some(output_dir) = output_dir {
        let dir = root.join(output_dir.trim());
        if dir.is_dir() {
            dirs.push(dir);
        }
    }
    for name in [
        "build", "Build", "Debug", "Release", "out", "Objects", "target",
    ] {
        let dir = root.join(name);
        if dir.is_dir() {
            dirs.push(dir);
        }
    }
    dirs.push(root.to_path_buf());
    let mut seen = std::collections::BTreeSet::new();
    dirs.retain(|dir| seen.insert(dir.to_string_lossy().to_ascii_lowercase()));

    for extension in extensions {
        let mut best: Option<(std::time::SystemTime, PathBuf)> = None;
        for dir in &dirs {
            let Ok(entries) = std::fs::read_dir(dir) else {
                continue;
            };
            for entry in entries.flatten() {
                let path = entry.path();
                if !path.is_file()
                    || !path
                        .extension()
                        .and_then(|value| value.to_str())
                        .is_some_and(|value| value.eq_ignore_ascii_case(&extension))
                {
                    continue;
                }
                let modified = entry
                    .metadata()
                    .and_then(|metadata| metadata.modified())
                    .unwrap_or(UNIX_EPOCH);
                if best.as_ref().is_none_or(|(current, _)| modified > *current) {
                    best = Some((modified, path));
                }
            }
        }
        if let Some((_, path)) = best {
            return Some(path);
        }
    }
    None
}

fn tcl_word(value: &str) -> String {
    if value.chars().any(|character| {
        character.is_whitespace() || matches!(character, '{' | '}' | '$' | '[' | ']' | '"' | ';')
    }) {
        format!("{{{}}}", value.replace('}', "\\}"))
    } else {
        value.to_owned()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_config() -> OpenOcdConfig {
        OpenOcdConfig {
            executable: "openocd".to_owned(),
            program_file: "${ExecutableName}.hex".to_owned(),
            target_config: "stm32f1x.cfg".to_owned(),
            interface_config: "stlink.cfg".to_owned(),
        }
    }

    #[test]
    fn infers_stm32_openocd_scripts() {
        assert_eq!(
            infer_target_config(Some("STM32F103C8T6")).as_deref(),
            Some("stm32f1x.cfg")
        );
        assert_eq!(
            infer_target_config(Some("STM32G474CBTx")).as_deref(),
            Some("stm32g4x.cfg")
        );
        assert_eq!(infer_target_config(Some("ESP32")), None);
        assert_eq!(
            infer_interface_config(Some("ST-Link")).as_deref(),
            Some("stlink.cfg")
        );
        assert_eq!(
            infer_interface_config(Some("J-Link")).as_deref(),
            Some("jlink.cfg")
        );
    }

    #[test]
    fn resolves_artifact_and_builds_openocd_spec() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(dir.path().join("build")).unwrap();
        std::fs::write(dir.path().join("build/demo.hex"), b":00000001FF").unwrap();

        let spec = make_spec(&sample_config(), dir.path()).unwrap();
        assert_eq!(spec.program, OsString::from("openocd"));
        let args = spec
            .args
            .iter()
            .map(|value| value.to_string_lossy().into_owned())
            .collect::<Vec<_>>();
        assert_eq!(
            args,
            vec![
                "-f",
                "interface/stlink.cfg",
                "-f",
                "target/stm32f1x.cfg",
                "-c",
                "program build/demo.hex verify reset exit",
            ]
        );
        assert_eq!(spec.cwd, dir.path());
        assert!(spec
            .display
            .contains("program build/demo.hex verify reset exit"));
    }

    #[test]
    fn keeps_local_scripts_and_quotes_program_with_spaces() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(
            dir.path().join("board.cfg"),
            b"source [find target/stm32f1x.cfg]",
        )
        .unwrap();
        std::fs::create_dir_all(dir.path().join("build")).unwrap();
        std::fs::write(dir.path().join("build/my demo.hex"), b":00000001FF").unwrap();

        let config = OpenOcdConfig {
            target_config: "board.cfg".to_owned(),
            ..sample_config()
        };
        let preview = preview(&config, dir.path()).unwrap();
        assert!(preview.contains("-f interface/stlink.cfg"));
        assert!(preview.contains("-f board.cfg"));
        assert!(preview.contains("program {build/my demo.hex} verify reset exit"));
    }

    #[test]
    fn reports_missing_program_file_on_flash() {
        let dir = tempfile::tempdir().unwrap();
        let error = make_spec(&sample_config(), dir.path())
            .err()
            .expect("missing program file should fail");
        assert!(error.contains("找不到程序文件"));
        assert!(preview(&sample_config(), dir.path()).is_ok());
    }

    #[test]
    fn ready_requires_the_three_openocd_fields() {
        let mut config = sample_config();
        assert!(openocd_ready(&config));
        config.target_config.clear();
        assert!(!openocd_ready(&config));
    }

    #[test]
    fn checks_absolute_and_project_relative_openocd_paths() {
        let dir = tempfile::tempdir().unwrap();
        let executable = dir.path().join("openocd.exe");
        std::fs::write(&executable, b"fake").unwrap();
        assert!(openocd_available(
            &util::display_path(&executable),
            dir.path()
        ));
        assert!(!openocd_available(
            &util::display_path(&dir.path().join("missing.exe")),
            dir.path()
        ));

        std::fs::create_dir_all(dir.path().join("tools")).unwrap();
        std::fs::write(dir.path().join("tools/openocd.exe"), b"fake").unwrap();
        assert!(openocd_available("tools/openocd.exe", dir.path()));
    }
}
