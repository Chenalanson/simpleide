//! Flash tool configuration and OpenOCD / pyOCD command construction.
//!
//! The UI keeps a small, serializable description of the selected flasher.
//! OpenOCD and pyOCD are launched directly (no shell) so paths with spaces do
//! not need platform-specific quoting.

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
    #[serde(rename = "pyocd")]
    PyOcd,
}

impl FlashTool {
    pub fn label(self) -> &'static str {
        match self {
            Self::Custom => "自定义命令",
            Self::OpenOcd => "OpenOCD",
            Self::PyOcd => "pyOCD",
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
            // Do not silently assume ST-Link. DAPLink/J-Link users must pick
            // (or auto-detect) their probe; the flash dialog warns when empty.
            interface_config: String::new(),
        }
    }
}

/// Common OpenOCD interface scripts shown in the flash configuration dialog.
pub const OPENOCD_INTERFACES: &[(&str, &str)] = &[
    ("stlink.cfg", "ST-Link"),
    ("cmsis-dap.cfg", "CMSIS-DAP / DAPLink"),
    ("jlink.cfg", "J-Link"),
    ("xds110.cfg", "XDS110"),
    ("ulink.cfg", "ULINK"),
    ("stlink-dap.cfg", "ST-Link DAP"),
    ("ftdi/olimex-arm-usb-ocd-h.cfg", "FTDI Olimex"),
    ("raspberrypi-swd.cfg", "Raspberry Pi SWD"),
];

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DetectedInterface {
    pub config: String,
    pub label: String,
}

pub fn interface_label(config: &str) -> Option<&'static str> {
    let normalized = config
        .trim()
        .trim_start_matches("interface/")
        .replace('\\', "/");
    OPENOCD_INTERFACES
        .iter()
        .find(|(value, _)| *value == normalized.as_str())
        .map(|(_, label)| *label)
}

/// Best-effort detection of a connected debug probe. It looks at USB VID/PID
/// data exposed by Windows PnP (or `lsusb` elsewhere); OpenOCD itself does not
/// provide a portable "which probe is plugged in" query.
pub fn detect_interface_config() -> Result<Option<DetectedInterface>, String> {
    #[cfg(target_os = "windows")]
    {
        let mut command = std::process::Command::new("powershell");
        command.args([
            "-NoProfile",
            "-NonInteractive",
            "-ExecutionPolicy",
            "Bypass",
            "-Command",
            "Get-PnpDevice -PresentOnly -ErrorAction SilentlyContinue | Where-Object { $_.Status -eq 'OK' } | Select-Object -ExpandProperty InstanceId",
        ]);
        util::hide_console(&mut command);
        let output = command
            .output()
            .map_err(|err| format!("无法运行 USB 设备检测：{err}"))?;
        let text = String::from_utf8_lossy(&output.stdout);
        Ok(detected_from_usb_text(&text))
    }
    #[cfg(not(target_os = "windows"))]
    {
        let output = std::process::Command::new("lsusb").output();
        let Ok(output) = output else {
            return Ok(None);
        };
        let text = String::from_utf8_lossy(&output.stdout);
        Ok(detected_from_usb_text(&text))
    }
}

fn detected_from_usb_text(text: &str) -> Option<DetectedInterface> {
    let upper = text.to_ascii_uppercase();
    let has_vid =
        |vid: &str| upper.contains(&format!("VID_{vid}")) || upper.contains(&format!("ID {vid}:"));
    let probe = if has_vid("0483") {
        Some(("stlink.cfg", "ST-Link"))
    } else if has_vid("0D28") || has_vid("1209") || has_vid("C251") {
        Some(("cmsis-dap.cfg", "CMSIS-DAP / DAPLink"))
    } else if has_vid("1366") {
        Some(("jlink.cfg", "J-Link"))
    } else {
        None
    }?;
    Some(DetectedInterface {
        config: probe.0.to_owned(),
        label: probe.1.to_owned(),
    })
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

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(default)]
pub struct PyOcdConfig {
    /// `pyocd`, an absolute path, or a name on PATH.
    pub executable: String,
    /// pyOCD target name, e.g. `stm32f103xb`.
    pub target: String,
    /// Optional CMSIS pack path. Empty means use pyOCD's installed packs.
    pub pack: String,
    /// Supports `${ExecutableName}`, `${ProjectName}`, `${workspaceFolder}`
    /// and `${workspaceRoot}`.
    pub program_file: String,
}

impl Default for PyOcdConfig {
    fn default() -> Self {
        Self {
            executable: "pyocd".to_owned(),
            target: String::new(),
            pack: String::new(),
            program_file: "${ExecutableName}.hex".to_owned(),
        }
    }
}

impl PyOcdConfig {
    pub fn normalized(&self) -> Self {
        Self {
            executable: self.executable.trim().to_owned(),
            target: self.target.trim().to_owned(),
            pack: self.pack.trim().to_owned(),
            program_file: self.program_file.trim().to_owned(),
        }
    }
}

pub fn pyocd_ready(config: &PyOcdConfig) -> bool {
    !config.target.trim().is_empty() && !config.program_file.trim().is_empty()
}

pub fn infer_pyocd_target(device: Option<&str>) -> Option<String> {
    let device = device?.trim().to_ascii_lowercase();
    (device.starts_with("stm32") && device.len() > 5).then_some(device)
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

#[cfg(test)]
pub fn infer_interface_config(driver: Option<&str>) -> Option<String> {
    let driver = driver?.to_ascii_uppercase();
    if driver.contains("JLINK") || driver.contains("J-LINK") {
        Some("jlink.cfg".to_owned())
    } else if driver.contains("STLINK") || driver.contains("ST-LINK") {
        Some("stlink.cfg".to_owned())
    } else if driver.contains("CMSIS-DAP")
        || driver.contains("CMSIS_DAP")
        || driver.contains("CMSIS_AGDI")
        || driver.contains("DAPLINK")
    {
        Some("cmsis-dap.cfg".to_owned())
    } else {
        None
    }
}

/// Build a direct `openocd` invocation. The program is passed as one Tcl
/// command through `-c`, so it must exist before the flash starts.
pub fn make_spec(
    config: &OpenOcdConfig,
    root: &Path,
    load_address: Option<&str>,
) -> Result<RunSpec, String> {
    let program = resolve_program_file(&config.program_file, root, true)?;
    let interface = resolve_script(root, &config.interface_config, "interface")?;
    let target = resolve_script(root, &config.target_config, "target")?;
    let program_command = openocd_program_command(&program, load_address)?;
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
        env: Vec::new(),
    })
}

fn openocd_program_command(
    program: &ResolvedProgram,
    load_address: Option<&str>,
) -> Result<String, String> {
    let is_bin = program.command.to_ascii_lowercase().ends_with(".bin");
    if is_bin {
        let address = load_address
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .ok_or_else(|| "烧录 .bin 需要在“内存布局”里设置下载起始地址".to_owned())?;
        return Ok(format!(
            "program {} {} verify reset exit",
            tcl_word(&program.command),
            tcl_word(address)
        ));
    }
    Ok(format!(
        "program {} verify reset exit",
        tcl_word(&program.command)
    ))
}

/// Same composition as [`make_spec`] but does not require the build output to
/// exist yet. Used by the configuration modal for a live preview.
pub fn preview(
    config: &OpenOcdConfig,
    root: &Path,
    load_address: Option<&str>,
) -> Result<String, String> {
    let program = resolve_program_file(&config.program_file, root, false)?;
    let interface = resolve_script(root, &config.interface_config, "interface")?;
    let target = resolve_script(root, &config.target_config, "target")?;
    let program_command = openocd_program_command(&program, load_address)?;
    let executable = resolve_executable(effective_executable(config), root);
    Ok(format!(
        "{} -f {} -f {} -c \"{}\"",
        util::relative_display(root, &executable),
        interface,
        target,
        program_command
    ))
}

/// Build an `openocd` GDB server invocation for embedded debugging.
pub fn openocd_server_spec(
    config: &OpenOcdConfig,
    root: &Path,
    port: u16,
    telnet_port: u16,
) -> Result<RunSpec, String> {
    let interface = resolve_script(root, &config.interface_config, "interface")?;
    let target = resolve_script(root, &config.target_config, "target")?;
    let executable = resolve_executable(effective_executable(config), root);
    let server_command = format!("gdb_port {port}; telnet_port {telnet_port}; tcl_port disabled");
    let args: Vec<OsString> = vec![
        "-f".into(),
        interface.clone().into(),
        "-f".into(),
        target.clone().into(),
        "-c".into(),
        server_command.clone().into(),
    ];
    let display = format!(
        "{} -f {} -f {} -c \"{}\"",
        util::relative_display(root, &executable),
        interface,
        target,
        server_command
    );
    Ok(RunSpec {
        program: executable.into_os_string(),
        args,
        cwd: root.to_path_buf(),
        display,
        env_paths: Vec::new(),
        env: Vec::new(),
    })
}

/// Build a `pyocd gdbserver` invocation for embedded debugging.
pub fn pyocd_server_spec(config: &PyOcdConfig, root: &Path, port: u16) -> Result<RunSpec, String> {
    let target = config.target.trim();
    if target.is_empty() {
        return Err("pyOCD 目标芯片不能为空".to_owned());
    }
    let pack = resolve_pack(config, root, true)?;
    let executable = resolve_executable(effective_pyocd_executable(config), root);
    let port = port.to_string();
    let mut args: Vec<OsString> = vec![
        "gdbserver".into(),
        "--port".into(),
        port.clone().into(),
        "-t".into(),
        target.into(),
    ];
    if let Some(pack) = &pack {
        args.push("--pack".into());
        args.push(pack.clone().into());
    }
    let mut display = format!(
        "{} gdbserver --port {port} -t {target}",
        util::relative_display(root, &executable)
    );
    if let Some(pack) = &pack {
        display.push_str(&format!(" --pack {pack}"));
    }
    Ok(RunSpec {
        program: executable.into_os_string(),
        args,
        cwd: root.to_path_buf(),
        display,
        env_paths: Vec::new(),
        env: Vec::new(),
    })
}

/// Build a direct `pyocd flash` invocation.
pub fn make_pyocd_spec(
    config: &PyOcdConfig,
    root: &Path,
    load_address: Option<&str>,
) -> Result<RunSpec, String> {
    let program = resolve_program_file(&config.program_file, root, true)?;
    let target = config.target.trim();
    if target.is_empty() {
        return Err("pyOCD 目标芯片不能为空".to_owned());
    }
    let pack = resolve_pack(config, root, true)?;
    let executable = resolve_executable(effective_pyocd_executable(config), root);
    let base_address = pyocd_base_address(&program, load_address)?;
    let mut args: Vec<OsString> = vec!["flash".into(), "-t".into(), target.into()];
    if let Some(pack) = &pack {
        args.push("--pack".into());
        args.push(pack.clone().into());
    }
    if let Some(address) = &base_address {
        args.push("--base-address".into());
        args.push(address.clone().into());
    }
    args.push(program.command.clone().into());
    let display = pyocd_display(
        config,
        target,
        pack.as_deref(),
        base_address.as_deref(),
        &program.command,
        root,
    );
    Ok(RunSpec {
        program: executable.into_os_string(),
        args,
        cwd: root.to_path_buf(),
        display,
        env_paths: Vec::new(),
        env: Vec::new(),
    })
}

/// Same composition as [`make_pyocd_spec`] without requiring the output or
/// pack file to exist yet.
pub fn preview_pyocd(
    config: &PyOcdConfig,
    root: &Path,
    load_address: Option<&str>,
) -> Result<String, String> {
    let program = resolve_program_file(&config.program_file, root, false)?;
    let target = config.target.trim();
    if target.is_empty() {
        return Err("pyOCD 目标芯片不能为空".to_owned());
    }
    let pack = resolve_pack(config, root, false)?;
    let base_address = pyocd_base_address(&program, load_address)?;
    Ok(pyocd_display(
        config,
        target,
        pack.as_deref(),
        base_address.as_deref(),
        &program.command,
        root,
    ))
}

fn pyocd_base_address(
    program: &ResolvedProgram,
    load_address: Option<&str>,
) -> Result<Option<String>, String> {
    if !program.command.to_ascii_lowercase().ends_with(".bin") {
        return Ok(None);
    }
    let address = load_address
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| "烧录 .bin 需要在“内存布局”里设置下载起始地址".to_owned())?;
    Ok(Some(address.to_owned()))
}

fn pyocd_display(
    config: &PyOcdConfig,
    target: &str,
    pack: Option<&str>,
    base_address: Option<&str>,
    program: &str,
    root: &Path,
) -> String {
    let executable = resolve_executable(effective_pyocd_executable(config), root);
    let mut display = format!(
        "{} flash -t {}",
        util::relative_display(root, &executable),
        target
    );
    if let Some(pack) = pack {
        display.push_str(&format!(" --pack {pack}"));
    }
    if let Some(address) = base_address {
        display.push_str(&format!(" --base-address {address}"));
    }
    display.push(' ');
    display.push_str(program);
    display
}

fn resolve_pack(
    config: &PyOcdConfig,
    root: &Path,
    require_exists: bool,
) -> Result<Option<String>, String> {
    let pack = config.pack.trim();
    if pack.is_empty() {
        return Ok(None);
    }
    let path = PathBuf::from(pack.replace('\\', "/"));
    let absolute = if path.is_absolute() {
        path
    } else {
        root.join(&path)
    };
    if require_exists && !absolute.is_file() {
        return Err(format!(
            "找不到 pyOCD Pack 文件 {}",
            util::relative_display(root, &absolute)
        ));
    }
    Ok(Some(util::relative_display(root, &absolute)))
}

pub fn effective_executable(config: &OpenOcdConfig) -> &str {
    effective_value(&config.executable, "openocd")
}

pub fn effective_pyocd_executable(config: &PyOcdConfig) -> &str {
    effective_value(&config.executable, "pyocd")
}

fn effective_value<'a>(value: &'a str, fallback: &'static str) -> &'a str {
    let value = value.trim();
    if value.is_empty() {
        fallback
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
    executable_available(executable, root, "openocd")
}

pub fn pyocd_available(executable: &str, root: &Path) -> bool {
    executable_available(executable, root, "pyocd")
}

fn executable_available(executable: &str, root: &Path, fallback: &str) -> bool {
    let value = executable.trim();
    if value.is_empty() {
        return find_in_path(fallback).is_some();
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

pub fn pyocd_candidates(configured: &str) -> Vec<String> {
    let mut values = Vec::new();
    let configured = configured.trim();
    if !configured.is_empty() {
        push_unique(&mut values, configured.to_owned());
    }
    if find_in_path("pyocd").is_some() {
        push_unique(&mut values, "pyocd".to_owned());
    }

    #[cfg(target_os = "windows")]
    {
        if let Some(base) = std::env::var_os("APPDATA") {
            scan_pyocd_script_dirs(&Path::new(&base).join("Python"), &mut values);
        }
        if let Some(base) = std::env::var_os("LOCALAPPDATA") {
            scan_pyocd_script_dirs(&Path::new(&base).join("Programs/Python"), &mut values);
        }
        if let Some(base) = std::env::var_os("USERPROFILE") {
            scan_pyocd_script_dirs(
                &Path::new(&base).join("AppData/Roaming/Python"),
                &mut values,
            );
        }
        if let Some(base) = std::env::var_os("ProgramFiles") {
            scan_pyocd_script_dirs(Path::new(&base), &mut values);
        }
    }
    #[cfg(not(target_os = "windows"))]
    {
        push_if_file(&mut values, PathBuf::from("/usr/bin/pyocd"));
        push_if_file(&mut values, PathBuf::from("/usr/local/bin/pyocd"));
    }

    if values.is_empty() {
        values.push("pyocd".to_owned());
    }
    values
}

fn scan_pyocd_script_dirs(base: &Path, values: &mut Vec<String>) {
    push_if_file(values, base.join("Scripts/pyocd.exe"));
    let Ok(entries) = std::fs::read_dir(base) else {
        return;
    };
    for entry in entries.flatten() {
        let name = entry.file_name().to_string_lossy().to_ascii_lowercase();
        if name.starts_with("python") {
            push_if_file(values, entry.path().join("Scripts/pyocd.exe"));
        }
    }
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

fn resolve_program_file(
    template: &str,
    root: &Path,
    require_exists: bool,
) -> Result<ResolvedProgram, String> {
    let template = template.trim();
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
        assert_eq!(
            infer_interface_config(Some("CMSIS-DAP")).as_deref(),
            Some("cmsis-dap.cfg")
        );
    }

    #[test]
    fn openocd_default_does_not_assume_stlink() {
        assert!(OpenOcdConfig::default().interface_config.is_empty());
    }

    #[test]
    fn detects_common_probe_usb_ids() {
        let stlink = detected_from_usb_text("USB\\VID_0483&PID_3748\\ST-Link").unwrap();
        assert_eq!(stlink.config, "stlink.cfg");
        assert_eq!(stlink.label, "ST-Link");

        let dap = detected_from_usb_text("USB\\VID_0D28&PID_0204\\DAPLink").unwrap();
        assert_eq!(dap.config, "cmsis-dap.cfg");
        assert_eq!(dap.label, "CMSIS-DAP / DAPLink");

        let jlink = detected_from_usb_text("USB\\VID_1366&PID_0101\\J-Link").unwrap();
        assert_eq!(jlink.config, "jlink.cfg");
        assert_eq!(jlink.label, "J-Link");

        assert!(detected_from_usb_text("USB\\VID_1234&PID_0001").is_none());
    }

    #[test]
    fn recognizes_interface_presets() {
        assert_eq!(interface_label("stlink.cfg"), Some("ST-Link"));
        assert_eq!(
            interface_label("interface/cmsis-dap.cfg"),
            Some("CMSIS-DAP / DAPLink")
        );
        assert_eq!(interface_label("custom.cfg"), None);
    }

    #[test]
    fn resolves_artifact_and_builds_openocd_spec() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(dir.path().join("build")).unwrap();
        std::fs::write(dir.path().join("build/demo.hex"), b":00000001FF").unwrap();

        let spec = make_spec(&sample_config(), dir.path(), None).unwrap();
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
    fn openocd_server_spec_uses_requested_ports_and_disables_tcl() {
        let dir = tempfile::tempdir().unwrap();
        let spec = openocd_server_spec(&sample_config(), dir.path(), 3456, 3457).unwrap();
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
                "gdb_port 3456; telnet_port 3457; tcl_port disabled",
            ]
        );
        assert!(spec.display.contains("gdb_port 3456"));
        assert!(spec.display.contains("telnet_port 3457"));
    }

    #[test]
    fn pyocd_server_spec_uses_gdbserver_subcommand() {
        let dir = tempfile::tempdir().unwrap();
        let spec = pyocd_server_spec(&sample_pyocd(), dir.path(), 3456).unwrap();
        let args = spec
            .args
            .iter()
            .map(|value| value.to_string_lossy().into_owned())
            .collect::<Vec<_>>();
        assert_eq!(
            args,
            vec!["gdbserver", "--port", "3456", "-t", "stm32f103xb"]
        );
        assert!(spec.display.contains("gdbserver --port 3456"));
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
        let preview = preview(&config, dir.path(), None).unwrap();
        assert!(preview.contains("-f interface/stlink.cfg"));
        assert!(preview.contains("-f board.cfg"));
        assert!(preview.contains("program {build/my demo.hex} verify reset exit"));
    }

    #[test]
    fn reports_missing_program_file_on_flash() {
        let dir = tempfile::tempdir().unwrap();
        let error = make_spec(&sample_config(), dir.path(), None)
            .err()
            .expect("missing program file should fail");
        assert!(error.contains("找不到程序文件"));
        assert!(preview(&sample_config(), dir.path(), None).is_ok());
    }

    #[test]
    fn raw_bin_uses_the_configured_download_address() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(dir.path().join("build")).unwrap();
        std::fs::write(dir.path().join("build/demo.bin"), b"\x00").unwrap();
        let config = OpenOcdConfig {
            program_file: "${ExecutableName}.bin".to_owned(),
            ..sample_config()
        };
        let spec = make_spec(&config, dir.path(), Some("0x08000000")).unwrap();
        assert!(spec.args.iter().any(|arg| arg
            .to_string_lossy()
            .contains("program build/demo.bin 0x08000000")));
        let error = make_spec(&config, dir.path(), None)
            .err()
            .expect("bin without address should fail");
        assert!(error.contains("下载起始地址"));
    }

    #[test]
    fn pyocd_raw_bin_passes_base_address() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(dir.path().join("build")).unwrap();
        std::fs::write(dir.path().join("build/demo.bin"), b"\x00").unwrap();
        let config = PyOcdConfig {
            program_file: "${ExecutableName}.bin".to_owned(),
            ..sample_pyocd()
        };
        let spec = make_pyocd_spec(&config, dir.path(), Some("0x08000000")).unwrap();
        assert!(spec
            .args
            .iter()
            .any(|arg| arg.to_string_lossy() == "--base-address"));
        assert!(spec
            .args
            .iter()
            .any(|arg| arg.to_string_lossy() == "0x08000000"));
    }

    #[test]
    fn ready_requires_the_three_openocd_fields() {
        let mut config = sample_config();
        assert!(openocd_ready(&config));
        config.target_config.clear();
        assert!(!openocd_ready(&config));
    }

    fn sample_pyocd() -> PyOcdConfig {
        PyOcdConfig {
            executable: "pyocd".to_owned(),
            target: "stm32f103xb".to_owned(),
            pack: String::new(),
            program_file: "${ExecutableName}.hex".to_owned(),
        }
    }

    #[test]
    fn builds_pyocd_spec_with_optional_pack() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(dir.path().join("build")).unwrap();
        std::fs::write(dir.path().join("build/demo.hex"), b":00000001FF").unwrap();
        std::fs::write(dir.path().join("demo.pack"), b"pack").unwrap();

        let config = sample_pyocd();
        let spec = make_pyocd_spec(&config, dir.path(), None).unwrap();
        assert_eq!(spec.program, OsString::from("pyocd"));
        let args = spec
            .args
            .iter()
            .map(|value| value.to_string_lossy().into_owned())
            .collect::<Vec<_>>();
        assert_eq!(args, vec!["flash", "-t", "stm32f103xb", "build/demo.hex"]);
        assert!(preview_pyocd(&config, dir.path(), None)
            .unwrap()
            .contains("pyocd flash -t stm32f103xb build/demo.hex"));

        let config = PyOcdConfig {
            pack: "demo.pack".to_owned(),
            ..sample_pyocd()
        };
        let spec = make_pyocd_spec(&config, dir.path(), None).unwrap();
        let args = spec
            .args
            .iter()
            .map(|value| value.to_string_lossy().into_owned())
            .collect::<Vec<_>>();
        assert_eq!(
            args,
            vec![
                "flash",
                "-t",
                "stm32f103xb",
                "--pack",
                "demo.pack",
                "build/demo.hex"
            ]
        );
    }

    #[test]
    fn pyocd_ready_and_target_inference() {
        let mut config = sample_pyocd();
        assert!(pyocd_ready(&config));
        config.target.clear();
        assert!(!pyocd_ready(&config));
        assert_eq!(
            infer_pyocd_target(Some("STM32F103xB")).as_deref(),
            Some("stm32f103xb")
        );
        assert_eq!(infer_pyocd_target(Some("ESP32")), None);
    }

    #[test]
    fn checks_pyocd_availability() {
        let dir = tempfile::tempdir().unwrap();
        let executable = dir.path().join("pyocd.exe");
        std::fs::write(&executable, b"fake").unwrap();
        assert!(pyocd_available(
            &util::display_path(&executable),
            dir.path()
        ));
        assert!(!pyocd_available(
            &util::display_path(&dir.path().join("missing.exe")),
            dir.path()
        ));
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
