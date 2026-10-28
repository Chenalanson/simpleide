//! Persisted user preferences, per-project settings, and workspace files.
//!
//! A folder is still the project. Build and flash configuration may live with
//! the project in `.simple-embedded/settings.json`; the user-wide settings file
//! remains the fallback for older projects and for global editor preferences.

use std::collections::BTreeMap;
use std::ffi::OsString;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::build::{RunSpec, TaskKind};
use crate::flash::{FlashTool, OpenOcdConfig, PyOcdConfig};
use crate::project_meta::ProjectMeta;
use crate::util::{display_path, file_name, project_key, shell_quote_posix};

pub const APP_NAME: &str = "Simple embedded";
pub const APP_AUTHOR: &str = "smallYYY_";
const APP_DATA_DIR: &str = "SimpleEmbedded";
const LEGACY_APP_DATA_DIR: &str = "MiniEIDE";
const MAX_RECENT_PROJECTS: usize = 8;

/// Folder inside the project that holds project-local application settings.
pub const PROJECT_CONFIG_DIR: &str = ".simple-embedded";
/// File name of the project-local settings file.
pub const PROJECT_CONFIG_FILE: &str = "settings.json";
/// Extension of workspace files that can be opened by the application.
pub const WORKSPACE_EXTENSION: &str = "se-workspace";
const WORKSPACE_TYPE_NAME: &str = "SimpleEmbedded.Workspace";

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum BuildTarget {
    #[default]
    Local,
    Ssh,
}

impl BuildTarget {
    pub fn label(self) -> &'static str {
        match self {
            Self::Local => "本地",
            Self::Ssh => "远程 SSH",
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(default)]
pub struct BuildOptions {
    pub c_standard: String,
    pub cpp_standard: String,
    pub optimization: String,
    pub warning: String,
    pub lto: bool,
    pub function_sections: bool,
    pub split_load_store: bool,
    pub plain_char_signed: bool,
    pub short_enums: bool,
    pub rtti: bool,
    pub use_microlib: bool,
    pub printf_float: bool,
    pub scanf_float: bool,
    pub big_endian: bool,
    pub debug_info: String,
    pub assembler_defines: Vec<String>,
    pub assembler_type: String,
    pub assembler_options: String,
    pub linker_output_format: String,
    pub linker_no_binary: bool,
    pub linker_binary_excludes: Vec<String>,
    pub linker_xo_base: String,
    pub linker_ro_base: String,
    pub linker_rw_base: String,
    pub linker_options: String,
}

impl Default for BuildOptions {
    fn default() -> Self {
        Self {
            c_standard: "c99".to_owned(),
            cpp_standard: "c++03".to_owned(),
            optimization: "-O2".to_owned(),
            warning: "-Wall -Wextra".to_owned(),
            lto: false,
            function_sections: true,
            split_load_store: false,
            plain_char_signed: false,
            short_enums: false,
            rtti: false,
            use_microlib: false,
            printf_float: false,
            scanf_float: false,
            big_endian: false,
            debug_info: "enable".to_owned(),
            assembler_defines: Vec::new(),
            assembler_type: "arm-auto (auto detect)".to_owned(),
            assembler_options: String::new(),
            linker_output_format: "AXF".to_owned(),
            linker_no_binary: false,
            linker_binary_excludes: Vec::new(),
            linker_xo_base: String::new(),
            linker_ro_base: String::new(),
            linker_rw_base: String::new(),
            linker_options: String::new(),
        }
    }
}

impl BuildOptions {
    pub fn summary_lines(&self) -> Vec<String> {
        let mut lines = vec![
            self.c_standard.clone(),
            self.cpp_standard.clone(),
            self.optimization.clone(),
            self.warning.clone(),
        ];
        if self.lto {
            lines.push("LTO".to_owned());
        }
        if self.function_sections {
            lines.push("function-sections".to_owned());
        }
        if self.split_load_store {
            lines.push("split-load-store".to_owned());
        }
        if self.plain_char_signed {
            lines.push("plain-char-signed".to_owned());
        }
        if self.short_enums {
            lines.push("short-enums".to_owned());
        }
        if self.rtti {
            lines.push("RTTI".to_owned());
        }
        if self.use_microlib {
            lines.push("MicroLIB".to_owned());
        }
        if self.printf_float {
            lines.push("printf 浮点".to_owned());
        }
        if self.scanf_float {
            lines.push("scanf 浮点".to_owned());
        }
        if self.big_endian {
            lines.push("Big Endian".to_owned());
        }
        if self.assembler_type != "arm-auto (auto detect)" {
            lines.push(self.assembler_type.clone());
        }
        if self.linker_no_binary {
            lines.push("不生成二进制文件".to_owned());
        }
        lines.push(self.linker_output_format.clone());
        lines.retain(|line| !line.trim().is_empty());
        lines
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum BuildSystem {
    Make,
    Keil,
    Cargo,
    CMake,
    #[default]
    Other,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ToolchainKind {
    #[default]
    Auto,
    MakeGcc,
    MakeClang,
    MakeAc5,
    MakeAc6,
    KeilAc5,
    KeilAc6,
    Cargo,
    CMake,
    Micropython,
    Custom,
}

impl ToolchainKind {
    pub const ALL: [Self; 11] = [
        Self::Auto,
        Self::MakeGcc,
        Self::MakeClang,
        Self::MakeAc5,
        Self::MakeAc6,
        Self::KeilAc6,
        Self::KeilAc5,
        Self::Cargo,
        Self::CMake,
        Self::Micropython,
        Self::Custom,
    ];

    pub fn label(self) -> &'static str {
        match self {
            Self::Auto => "自动检测",
            Self::MakeGcc => "Make · GCC",
            Self::MakeClang => "Make · Clang",
            Self::MakeAc5 => "Make · AC5",
            Self::MakeAc6 => "Make · AC6",
            Self::KeilAc5 => "Keil MDK · AC5",
            Self::KeilAc6 => "Keil MDK · AC6",
            Self::Cargo => "Cargo",
            Self::CMake => "CMake",
            Self::Micropython => "MicroPython",
            Self::Custom => "自定义",
        }
    }

    pub fn build_system(self) -> BuildSystem {
        match self {
            Self::MakeGcc | Self::MakeClang | Self::MakeAc5 | Self::MakeAc6 => BuildSystem::Make,
            Self::KeilAc5 | Self::KeilAc6 => BuildSystem::Keil,
            Self::Cargo => BuildSystem::Cargo,
            Self::CMake => BuildSystem::CMake,
            Self::Auto | Self::Micropython | Self::Custom => BuildSystem::Other,
        }
    }

    pub fn is_arm_compiler(self) -> bool {
        matches!(
            self,
            Self::MakeAc5 | Self::MakeAc6 | Self::KeilAc5 | Self::KeilAc6
        )
    }

    pub fn is_arm_ac5(self) -> bool {
        matches!(self, Self::MakeAc5 | Self::KeilAc5)
    }

    pub fn is_make_gcc_like(self) -> bool {
        matches!(self, Self::MakeGcc | Self::MakeClang)
    }

    pub fn is_micropython(self) -> bool {
        matches!(self, Self::Micropython)
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(default)]
pub struct FirmwareConfig {
    /// esptool executable, `esptool.py`, or a name on PATH.
    pub esptool: String,
    /// Combined MicroPython firmware image (`.bin`).
    pub file: String,
    /// esptool `--chip` value.
    pub chip: String,
    /// Flash offset for the combined image.
    pub address: String,
    pub erase: bool,
    pub verify: bool,
}

impl Default for FirmwareConfig {
    fn default() -> Self {
        Self {
            esptool: "esptool".to_owned(),
            file: String::new(),
            chip: "esp32".to_owned(),
            address: "0x1000".to_owned(),
            erase: false,
            verify: true,
        }
    }
}

impl FirmwareConfig {
    pub const CHIPS: &'static [(&'static str, &'static str)] = &[
        ("esp32", "ESP32"),
        ("esp32s2", "ESP32-S2"),
        ("esp32s3", "ESP32-S3"),
        ("esp32c3", "ESP32-C3"),
        ("esp32c6", "ESP32-C6"),
        ("esp32h2", "ESP32-H2"),
    ];

    pub fn default_address(chip: &str) -> &'static str {
        match chip.to_ascii_lowercase().as_str() {
            "esp32" | "esp32s2" => "0x1000",
            _ => "0x0",
        }
    }

    pub fn is_ready(&self) -> bool {
        !self.esptool.trim().is_empty()
            && !self.file.trim().is_empty()
            && !self.chip.trim().is_empty()
            && !self.address.trim().is_empty()
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(default)]
pub struct BuildConfig {
    pub target: BuildTarget,
    pub build_command: String,
    /// Explicit compiler/build-system choice. `Auto` follows the build command.
    pub toolchain: ToolchainKind,
    pub clean_command: String,
    pub rebuild_command: String,
    /// Custom flash command. Used when `flash_tool` is `Custom`.
    pub flash_command: String,
    pub flash_tool: FlashTool,
    pub openocd: OpenOcdConfig,
    pub pyocd: PyOcdConfig,
    /// Serial port used by MicroPython / ESP32 targets.
    pub serial: crate::serial::SerialConfig,
    /// MicroPython firmware flashing configuration.
    pub firmware: FirmwareConfig,
    /// Parallel build jobs. Zero keeps the command unchanged.
    pub build_jobs: u16,
    pub ssh_host: String,
    pub ssh_port: u16,
    pub remote_dir: String,
    /// Extra directories prepended to PATH for local build commands.
    pub extra_paths: Vec<String>,
    /// User overrides for the includes shown in the project info tree.
    pub meta_includes: Option<Vec<String>>,
    /// User overrides for the library search directories shown in the project info tree.
    pub meta_library_dirs: Option<Vec<String>>,
    /// User overrides for the defines shown in the project info tree.
    pub meta_defines: Option<Vec<String>>,
    /// Compiler options edited in the project info tree.
    pub build_options: BuildOptions,
    /// Editable Flash/RAM layout and optional raw-binary download address.
    pub memory: MemoryConfig,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct MemoryConfig {
    pub flash_start: String,
    pub flash_size: String,
    pub ram_start: String,
    pub ram_size: String,
    pub download_address: String,
    pub regions: Vec<MemoryRegionConfig>,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum MemoryKind {
    #[default]
    Ram,
    Rom,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct MemoryRegionConfig {
    pub name: String,
    pub kind: MemoryKind,
    pub on_chip: bool,
    pub enabled: bool,
    pub start: String,
    pub size: String,
    pub no_init: bool,
    pub startup: bool,
}

impl Default for MemoryRegionConfig {
    fn default() -> Self {
        Self {
            name: String::new(),
            kind: MemoryKind::Ram,
            on_chip: true,
            enabled: false,
            start: String::new(),
            size: String::new(),
            no_init: false,
            startup: false,
        }
    }
}

impl Default for BuildConfig {
    fn default() -> Self {
        Self {
            target: BuildTarget::Local,
            build_command: String::new(),
            toolchain: ToolchainKind::Auto,
            clean_command: String::new(),
            rebuild_command: String::new(),
            flash_command: String::new(),
            flash_tool: FlashTool::Custom,
            openocd: OpenOcdConfig::default(),
            pyocd: PyOcdConfig::default(),
            serial: crate::serial::SerialConfig::default(),
            firmware: FirmwareConfig::default(),
            build_jobs: 0,
            ssh_host: String::new(),
            ssh_port: 22,
            remote_dir: String::new(),
            extra_paths: Vec::new(),
            meta_includes: None,
            meta_library_dirs: None,
            meta_defines: None,
            build_options: BuildOptions::default(),
            memory: MemoryConfig::default(),
        }
    }
}

impl BuildConfig {
    pub fn download_address(&self) -> Option<String> {
        crate::memory::effective_download_address(&self.memory)
    }

    pub fn command(&self, kind: TaskKind) -> &str {
        match kind {
            TaskKind::Build => &self.build_command,
            TaskKind::Clean => &self.clean_command,
            TaskKind::Rebuild => &self.rebuild_command,
            TaskKind::Flash => &self.flash_command,
        }
    }

    pub fn has_command(&self, kind: TaskKind) -> bool {
        if self.toolchain.is_micropython() {
            return false;
        }
        if kind == TaskKind::Flash {
            return match self.flash_tool {
                FlashTool::Custom => !self.flash_command.trim().is_empty(),
                FlashTool::OpenOcd => crate::flash::openocd_ready(&self.openocd),
                FlashTool::PyOcd => crate::flash::pyocd_ready(&self.pyocd),
            };
        }
        !self.command(kind).trim().is_empty()
    }

    pub fn validate(&self) -> Result<(), String> {
        if self.toolchain.is_micropython() {
            return self.serial.validate();
        }
        if self.build_command.trim().is_empty() {
            return Err("Build 命令不能为空".to_owned());
        }
        if self.target == BuildTarget::Ssh {
            if self.ssh_host.trim().is_empty() {
                return Err("远程 SSH 模式需要填写主机".to_owned());
            }
            if self.remote_dir.trim().is_empty() {
                return Err("远程 SSH 模式需要填写远程工程目录".to_owned());
            }
        }
        Ok(())
    }

    pub fn tool_name(&self) -> &'static str {
        if self.toolchain.is_micropython() {
            return "MicroPython";
        }
        let lower = self.build_command.to_ascii_lowercase();
        if lower.contains("cargo") {
            "Cargo"
        } else if lower.contains("cmake") {
            "CMake"
        } else if lower.contains("uv4") || lower.contains(".uvprojx") {
            "Keil"
        } else if lower.contains("make") {
            "Make"
        } else if lower.contains("ninja") {
            "Ninja"
        } else if lower.contains("idf.py") {
            "ESP-IDF"
        } else if lower.contains("pio") {
            "PlatformIO"
        } else if lower.trim().is_empty() {
            "未配置"
        } else {
            "自定义"
        }
    }

    pub fn build_system(&self) -> BuildSystem {
        match self.tool_name() {
            "Make" => BuildSystem::Make,
            "Keil" => BuildSystem::Keil,
            "Cargo" => BuildSystem::Cargo,
            "CMake" => BuildSystem::CMake,
            _ => BuildSystem::Other,
        }
    }

    pub fn effective_build_system(&self) -> BuildSystem {
        if self.toolchain == ToolchainKind::Auto {
            self.build_system()
        } else {
            self.toolchain.build_system()
        }
    }

    /// Resolve `Auto` against the metadata for the selected build system. The
    /// caller must refresh `ProjectMeta` with [`BuildConfig::effective_build_system`]
    /// before calling this.
    pub fn effective_toolchain(&self, meta: &ProjectMeta) -> ToolchainKind {
        if self.toolchain != ToolchainKind::Auto {
            return self.toolchain;
        }
        match self.effective_build_system() {
            BuildSystem::Make => compiler_toolchain(meta.compiler.as_deref()),
            BuildSystem::Keil => {
                if meta
                    .toolchain
                    .as_deref()
                    .is_some_and(|value| value.contains("AC5"))
                {
                    ToolchainKind::KeilAc5
                } else {
                    ToolchainKind::KeilAc6
                }
            }
            BuildSystem::Cargo => ToolchainKind::Cargo,
            BuildSystem::CMake => ToolchainKind::CMake,
            BuildSystem::Other => {
                let Some(toolchain) = meta.toolchain.as_deref() else {
                    return ToolchainKind::Custom;
                };
                if toolchain.contains("Keil") && toolchain.contains("AC5") {
                    ToolchainKind::KeilAc5
                } else if toolchain.contains("Keil") && toolchain.contains("AC6") {
                    ToolchainKind::KeilAc6
                } else if toolchain.contains("AC5") {
                    ToolchainKind::MakeAc5
                } else if toolchain.contains("AC6") {
                    ToolchainKind::MakeAc6
                } else if toolchain.contains("Clang") {
                    ToolchainKind::MakeClang
                } else if toolchain.contains("GCC") {
                    ToolchainKind::MakeGcc
                } else {
                    ToolchainKind::Custom
                }
            }
        }
    }

    pub fn toolchain_mismatch(&self) -> bool {
        !matches!(
            self.toolchain,
            ToolchainKind::Auto | ToolchainKind::Micropython | ToolchainKind::Custom
        ) && self.build_system() != self.toolchain.build_system()
    }

    pub fn mode_summary(&self) -> String {
        if self.toolchain.is_micropython() {
            return format!("MicroPython · {}", self.serial.summary());
        }
        format!("{} · {}", self.tool_name(), self.target.label())
    }

    fn jobs_env(&self, raw: &str) -> Vec<(OsString, OsString)> {
        if self.build_jobs == 0 || command_has_jobs_flag(raw) {
            return Vec::new();
        }
        let jobs = self.build_jobs.min(128).to_string();
        match self.tool_name() {
            "Make" => {
                let existing = std::env::var("MAKEFLAGS").unwrap_or_default();
                let existing = existing.trim();
                if existing.split_whitespace().any(is_jobs_flag) {
                    return Vec::new();
                }
                let value = if existing.is_empty() {
                    format!("-j{jobs}")
                } else {
                    format!("{existing} -j{jobs}")
                };
                vec![(OsString::from("MAKEFLAGS"), OsString::from(value))]
            }
            "Cargo" => vec![(OsString::from("CARGO_BUILD_JOBS"), OsString::from(jobs))],
            "CMake" => vec![(
                OsString::from("CMAKE_BUILD_PARALLEL_LEVEL"),
                OsString::from(jobs),
            )],
            _ => Vec::new(),
        }
    }

    fn jobs_shell_prefix(&self, raw: &str) -> Option<String> {
        if self.build_jobs == 0 || command_has_jobs_flag(raw) {
            return None;
        }
        let jobs = self.build_jobs.min(128);
        match self.tool_name() {
            "Make" => Some(format!("export MAKEFLAGS=\"${{MAKEFLAGS}} -j{jobs}\"; ")),
            "Cargo" => Some(format!("export CARGO_BUILD_JOBS={jobs}; ")),
            "CMake" => Some(format!("export CMAKE_BUILD_PARALLEL_LEVEL={jobs}; ")),
            _ => None,
        }
    }

    pub fn make_spec(&self, kind: TaskKind, project_root: &Path) -> Result<RunSpec, String> {
        if self.toolchain.is_micropython() {
            return Err(
                "MicroPython 工具链当前用于串口连接配置，尚未接入构建或烧录命令".to_owned(),
            );
        }
        if kind == TaskKind::Flash {
            let load_address = self.download_address();
            let spec = match self.flash_tool {
                FlashTool::OpenOcd => Some(crate::flash::make_spec(
                    &self.openocd,
                    project_root,
                    load_address.as_deref(),
                )?),
                FlashTool::PyOcd => Some(crate::flash::make_pyocd_spec(
                    &self.pyocd,
                    project_root,
                    load_address.as_deref(),
                )?),
                FlashTool::Custom => None,
            };
            if let Some(mut spec) = spec {
                spec.env_paths = self
                    .extra_paths
                    .iter()
                    .map(|path| path.trim())
                    .filter(|path| !path.is_empty())
                    .map(PathBuf::from)
                    .collect();
                return Ok(spec);
            }
        }

        let raw = self.command(kind).trim();
        if raw.is_empty() {
            return Err(format!("{} 命令尚未配置", kind.label()));
        }

        // Flashing is intentionally local: J-Link / OpenOCD usually needs USB
        // access, even when compilation happens in a Linux VM.
        let remote = self.target == BuildTarget::Ssh && kind != TaskKind::Flash;
        let jobs_env = if kind == TaskKind::Flash {
            Vec::new()
        } else {
            self.jobs_env(raw)
        };
        if remote {
            let host = self.ssh_host.trim();
            let remote_dir = self.remote_dir.trim();
            if host.is_empty() {
                return Err("远程 SSH 模式需要填写主机".to_owned());
            }
            if remote_dir.is_empty() {
                return Err("远程 SSH 模式需要填写远程工程目录".to_owned());
            }

            let jobs_prefix = if kind == TaskKind::Flash {
                None
            } else {
                self.jobs_shell_prefix(raw)
            };
            let remote_command = match jobs_prefix {
                Some(prefix) => {
                    format!("{}cd {} && {}", prefix, shell_quote_posix(remote_dir), raw)
                }
                None => format!("cd {} && {}", shell_quote_posix(remote_dir), raw),
            };
            let mut args: Vec<OsString> = vec![
                "-o".into(),
                "BatchMode=yes".into(),
                "-o".into(),
                "ConnectTimeout=10".into(),
            ];
            if self.ssh_port != 0 && self.ssh_port != 22 {
                args.push("-p".into());
                args.push(self.ssh_port.to_string().into());
            }
            args.push(host.into());
            args.push(remote_command.into());

            Ok(RunSpec {
                program: "ssh".into(),
                args,
                cwd: project_root.to_path_buf(),
                display: format!("ssh {host} · {raw}"),
                env_paths: Vec::new(),
                env: Vec::new(),
            })
        } else {
            let mut env_paths: Vec<PathBuf> = self
                .extra_paths
                .iter()
                .map(|path| path.trim())
                .filter(|path| !path.is_empty())
                .map(PathBuf::from)
                .collect();

            // POSIX-style Makefiles commonly invoke `rm`, `cp`, etc. Windows
            // Make implementations cannot run those unless a Unix userland is
            // on PATH. Git for Windows is the most common source, so add its
            // usr/bin automatically for local Make builds.
            #[cfg(target_os = "windows")]
            if looks_like_make(raw) {
                if let Some(dir) = find_posix_tools_dir() {
                    if !env_paths.iter().any(|path| {
                        path.to_string_lossy()
                            .eq_ignore_ascii_case(&dir.to_string_lossy())
                    }) {
                        env_paths.push(dir);
                    }
                }
            }

            #[cfg(target_os = "windows")]
            let spec = RunSpec {
                program: "cmd".into(),
                args: vec!["/S".into(), "/C".into(), raw.into()],
                cwd: project_root.to_path_buf(),
                display: raw.to_owned(),
                env_paths,
                env: jobs_env,
            };

            #[cfg(not(target_os = "windows"))]
            let spec = RunSpec {
                program: "sh".into(),
                args: vec!["-c".into(), raw.into()],
                cwd: project_root.to_path_buf(),
                display: raw.to_owned(),
                env_paths,
                env: jobs_env,
            };

            Ok(spec)
        }
    }
}

fn command_has_jobs_flag(command: &str) -> bool {
    command
        .split(|ch: char| ch.is_whitespace() || matches!(ch, '&' | '|' | ';'))
        .map(|token| token.trim_matches('"'))
        .any(is_jobs_flag)
}

fn is_jobs_flag(token: &str) -> bool {
    token == "-j"
        || token.starts_with("-j")
        || token == "--jobs"
        || token.starts_with("--jobs=")
        || token == "--parallel"
        || token.starts_with("--parallel=")
}

#[cfg(target_os = "windows")]
fn looks_like_make(command: &str) -> bool {
    command
        .split(|ch: char| ch.is_whitespace() || matches!(ch, '&' | '|' | ';'))
        .filter(|token| !token.is_empty())
        .map(|token| token.trim_matches('"').to_ascii_lowercase())
        .any(|token| {
            token == "make"
                || token == "make.exe"
                || token.ends_with("-make")
                || token.ends_with("-make.exe")
                || token == "gmake"
                || token == "gmake.exe"
                || token.ends_with("\\make.exe")
                || token.ends_with("/make.exe")
        })
}

#[cfg(target_os = "windows")]
fn find_posix_tools_dir() -> Option<PathBuf> {
    if let Some(paths) = std::env::var_os("PATH") {
        for dir in std::env::split_paths(&paths) {
            if dir.join("rm.exe").is_file() {
                return Some(dir);
            }
            if dir.join("git.exe").is_file() {
                if let Some(root) = dir.parent() {
                    let candidate = root.join("usr").join("bin");
                    if candidate.join("rm.exe").is_file() {
                        return Some(candidate);
                    }
                }
            }
        }
    }

    let mut candidates = Vec::new();
    if let Some(program_files) = std::env::var_os("ProgramFiles") {
        candidates.push(
            PathBuf::from(program_files)
                .join("Git")
                .join("usr")
                .join("bin"),
        );
    }
    #[cfg(target_pointer_width = "64")]
    if let Some(program_files) = std::env::var_os("ProgramFiles(x86)") {
        candidates.push(
            PathBuf::from(program_files)
                .join("Git")
                .join("usr")
                .join("bin"),
        );
    }
    if let Some(local_app_data) = std::env::var_os("LOCALAPPDATA") {
        candidates.push(
            PathBuf::from(local_app_data)
                .join("Programs")
                .join("Git")
                .join("usr")
                .join("bin"),
        );
    }
    candidates.extend(
        [
            r"C:\msys64\usr\bin",
            r"C:\msys32\usr\bin",
            r"C:\cygwin64\bin",
            r"C:\cygwin\bin",
        ]
        .into_iter()
        .map(PathBuf::from),
    );

    candidates
        .into_iter()
        .find(|dir| dir.join("rm.exe").is_file())
}

/// Project-local settings stored in `<project>/.simple-embedded/settings.json`.
///
/// The file is intentionally plain JSON so it can be edited by hand and
/// committed next to the sources. Unknown fields are ignored on load.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(default)]
pub struct ProjectSettings {
    pub version: u32,
    pub build: BuildConfig,
    pub debug: ProjectDebugSettings,
}

impl Default for ProjectSettings {
    fn default() -> Self {
        Self {
            version: 1,
            build: BuildConfig::default(),
            debug: ProjectDebugSettings::default(),
        }
    }
}

/// Debug configuration that belongs to a project.
///
/// `program` is the ELF/AXF/EXE being debugged. `gdb_path` is stored here as
/// well because a project may need a specific cross-gdb (for example
/// `arm-none-eabi-gdb`) even when the machine-default gdb is different.
/// `watches` keeps the Live Watch expressions with the project.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct ProjectDebugSettings {
    pub program: String,
    pub gdb_path: String,
    pub auto_refresh: bool,
    pub watches: Vec<String>,
}

impl Default for ProjectDebugSettings {
    fn default() -> Self {
        Self {
            program: String::new(),
            gdb_path: String::new(),
            auto_refresh: true,
            watches: Vec::new(),
        }
    }
}

impl ProjectSettings {
    pub fn path(project_root: &Path) -> PathBuf {
        project_root
            .join(PROJECT_CONFIG_DIR)
            .join(PROJECT_CONFIG_FILE)
    }

    pub fn exists(project_root: &Path) -> bool {
        Self::path(project_root).is_file()
    }

    /// Load the project file. `Ok(None)` means the project has no local
    /// settings yet; an error means the file exists but cannot be parsed.
    pub fn load(project_root: &Path) -> Result<Option<Self>, String> {
        let path = Self::path(project_root);
        let Ok(bytes) = std::fs::read(&path) else {
            return Ok(None);
        };
        let bytes = bytes.strip_prefix(&[0xEF, 0xBB, 0xBF]).unwrap_or(&bytes);
        match serde_json::from_slice::<Self>(bytes) {
            Ok(mut settings) => {
                settings.build.build_jobs = settings.build.build_jobs.min(128);
                Ok(Some(settings))
            }
            Err(err) => Err(format!("无法解析 {}：{err}", display_path(&path))),
        }
    }

    pub fn save(
        project_root: &Path,
        build: &BuildConfig,
        debug: &ProjectDebugSettings,
    ) -> Result<PathBuf, String> {
        let path = Self::path(project_root);
        let Some(parent) = path.parent() else {
            return Err("工程配置路径无效".to_owned());
        };
        std::fs::create_dir_all(parent)
            .map_err(|err| format!("无法创建 {}：{err}", display_path(parent)))?;
        let settings = Self {
            version: 1,
            build: build.clone(),
            debug: debug.clone(),
        };
        let mut bytes = serde_json::to_vec_pretty(&settings)
            .map_err(|err| format!("无法序列化工程配置：{err}"))?;
        bytes.push(b'\n');
        std::fs::write(&path, bytes)
            .map_err(|err| format!("无法写入 {}：{err}", display_path(&path)))?;
        Ok(path)
    }
}

/// Minimal VS Code style workspace file. Only the first folder is used.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(default)]
pub struct WorkspaceFile {
    pub version: u32,
    pub name: String,
    pub folders: Vec<WorkspaceFolder>,
    /// Optional path (relative to the workspace file) of a project settings
    /// file. The application always prefers `<folder>/.simple-embedded`.
    pub settings: Option<String>,
}

impl Default for WorkspaceFile {
    fn default() -> Self {
        Self {
            version: 1,
            name: String::new(),
            folders: vec![WorkspaceFolder {
                path: ".".to_owned(),
            }],
            settings: Some(format!("{PROJECT_CONFIG_DIR}/{PROJECT_CONFIG_FILE}")),
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(default)]
pub struct WorkspaceFolder {
    pub path: String,
}

impl Default for WorkspaceFolder {
    fn default() -> Self {
        Self {
            path: ".".to_owned(),
        }
    }
}

pub fn is_workspace_file(path: &Path) -> bool {
    path.extension().is_some_and(|extension| {
        extension.eq_ignore_ascii_case(WORKSPACE_EXTENSION)
            || extension.eq_ignore_ascii_case("code-workspace")
    })
}

/// Resolve a workspace file to its project folder.
pub fn resolve_workspace_file(path: &Path) -> Result<PathBuf, String> {
    let bytes = std::fs::read(path)
        .map_err(|err| format!("无法读取工作区文件 {}：{err}", display_path(path)))?;
    let bytes = bytes.strip_prefix(&[0xEF, 0xBB, 0xBF]).unwrap_or(&bytes);
    let workspace: WorkspaceFile = serde_json::from_slice(bytes)
        .map_err(|err| format!("无法解析工作区文件 {}：{err}", display_path(path)))?;
    let folder = workspace
        .folders
        .first()
        .map(|folder| folder.path.trim())
        .filter(|folder| !folder.is_empty())
        .unwrap_or(".");
    let folder = PathBuf::from(folder);
    let folder = if folder.is_absolute() {
        folder
    } else {
        path.parent().unwrap_or_else(|| Path::new(".")).join(folder)
    };
    if !folder.is_dir() {
        return Err(format!("工作区指向的目录不存在：{}", display_path(&folder)));
    }
    Ok(folder)
}

/// Write `<project>/<name>.se-workspace` and return its path.
pub fn write_workspace_file(project_root: &Path) -> Result<PathBuf, String> {
    let name = file_name(project_root);
    let path = project_root.join(format!("{name}.{WORKSPACE_EXTENSION}"));
    let workspace = WorkspaceFile {
        version: 1,
        name: name.clone(),
        folders: vec![WorkspaceFolder {
            path: ".".to_owned(),
        }],
        settings: Some(format!("{PROJECT_CONFIG_DIR}/{PROJECT_CONFIG_FILE}")),
    };
    let mut bytes =
        serde_json::to_vec_pretty(&workspace).map_err(|err| format!("无法序列化工作区：{err}"))?;
    bytes.push(b'\n');
    std::fs::write(&path, bytes)
        .map_err(|err| format!("无法写入 {}：{err}", display_path(&path)))?;
    Ok(path)
}

/// Register `.se-workspace` with the current executable so double-clicking a
/// workspace file opens this application. This only touches HKCU.
pub fn register_workspace_association() -> Result<PathBuf, String> {
    if !cfg!(target_os = "windows") {
        return Err("双击关联仅支持 Windows".to_owned());
    }
    let exe = std::env::current_exe().map_err(|err| format!("无法获取程序路径：{err}"))?;
    let exe = display_path(&exe);
    let extension_key = format!(r"HKCU\Software\Classes\.{WORKSPACE_EXTENSION}");
    let class_key = format!(r"HKCU\Software\Classes\{WORKSPACE_TYPE_NAME}");
    let icon_key = format!(r"HKCU\Software\Classes\{WORKSPACE_TYPE_NAME}\DefaultIcon");
    let command_key = format!(r"HKCU\Software\Classes\{WORKSPACE_TYPE_NAME}\shell\open\command");
    let command = format!("\"{exe}\" \"%1\"");
    let icon = crate::write_app_icon_ico()
        .map(|path| format!("\"{}\",0", display_path(&path)))
        .unwrap_or_else(|_| format!("\"{exe}\",0"));
    let entries = [
        (extension_key.clone(), WORKSPACE_TYPE_NAME.to_owned()),
        (class_key, "Simple embedded 工作区".to_owned()),
        (icon_key, icon),
        (command_key, command),
    ];
    for (key, value) in &entries {
        let status = std::process::Command::new("reg")
            .args(["add", key, "/ve", "/d", value, "/f"])
            .status()
            .map_err(|err| format!("无法调用 reg.exe：{err}"))?;
        if !status.success() {
            return Err(format!("写入注册表失败：{key}"));
        }
    }
    notify_shell_association_changed();
    Ok(PathBuf::from(exe))
}

#[cfg(target_os = "windows")]
fn notify_shell_association_changed() {
    #[link(name = "shell32")]
    unsafe extern "system" {
        fn SHChangeNotify(
            event: i32,
            flags: u32,
            item1: *const std::ffi::c_void,
            item2: *const std::ffi::c_void,
        );
    }
    const SHCNE_ASSOCCHANGED: i32 = 0x0800_0000;
    const SHCNF_IDLIST: u32 = 0;
    // SAFETY: both item pointers are allowed to be null for this notification.
    unsafe {
        SHChangeNotify(
            SHCNE_ASSOCCHANGED,
            SHCNF_IDLIST,
            std::ptr::null(),
            std::ptr::null(),
        );
    }
}

#[cfg(not(target_os = "windows"))]
fn notify_shell_association_changed() {}

pub fn autodetect(project_root: &Path) -> BuildConfig {
    let mut extra_paths = Vec::new();

    // Root detection comes first: the project files decide which build system
    // is expected. Only then look for that tool in PATH / common install dirs.
    if project_root.join("Cargo.toml").is_file() {
        add_tool_dir(&mut extra_paths, find_in_path("cargo"));
        return BuildConfig {
            build_command: "cargo build".to_owned(),
            toolchain: ToolchainKind::Cargo,
            clean_command: "cargo clean".to_owned(),
            rebuild_command: "cargo clean && cargo build".to_owned(),
            extra_paths,
            ..BuildConfig::default()
        };
    }

    let has_makefile = ["Makefile", "makefile", "GNUmakefile"]
        .iter()
        .any(|name| project_root.join(name).is_file());
    if has_makefile {
        let make_path = find_one_of(&["make", "gmake", "mingw32-make"]);
        let make = make_path
            .as_ref()
            .and_then(|path| path.file_stem())
            .map(|name| name.to_string_lossy().into_owned())
            .unwrap_or_else(|| "make".to_owned());
        add_tool_dir(&mut extra_paths, make_path);
        add_tool_dir(
            &mut extra_paths,
            find_one_of(&["arm-none-eabi-gcc", "gcc", "clang", "armclang", "armcc"]),
        );
        let meta = ProjectMeta::detect_for(project_root, BuildSystem::Make);
        return BuildConfig {
            build_command: make.clone(),
            toolchain: compiler_toolchain(meta.compiler.as_deref()),
            clean_command: format!("{make} clean"),
            rebuild_command: format!("{make} clean && {make}"),
            extra_paths,
            ..BuildConfig::default()
        };
    }

    if project_root.join("CMakeLists.txt").is_file() {
        add_tool_dir(&mut extra_paths, find_in_path("cmake"));
        let has_cache = project_root.join("build").join("CMakeCache.txt").is_file();
        return BuildConfig {
            build_command: if has_cache {
                "cmake --build build".to_owned()
            } else {
                "cmake -S . -B build && cmake --build build".to_owned()
            },
            toolchain: ToolchainKind::CMake,
            clean_command: "cmake --build build --target clean".to_owned(),
            rebuild_command: "cmake --build build --clean-first".to_owned(),
            extra_paths,
            ..BuildConfig::default()
        };
    }

    if let Some(uvprojx) = crate::project_meta::find_uvprojx(project_root) {
        let meta = ProjectMeta::detect_for(project_root, BuildSystem::Keil);
        let toolchain = if meta
            .toolchain
            .as_deref()
            .is_some_and(|value| value.contains("AC5"))
        {
            ToolchainKind::KeilAc5
        } else {
            ToolchainKind::KeilAc6
        };
        let uv = find_one_of(&["UV4", "UV5"]).or_else(find_keil_uv4);
        add_tool_dir(&mut extra_paths, uv.clone());
        let project_arg = uvprojx.strip_prefix(project_root).unwrap_or(&uvprojx);
        let build_command = uv
            .map(|uv| format!("{} -b \"{}\"", display_path(&uv), project_arg.display()))
            .unwrap_or_default();
        return BuildConfig {
            build_command,
            toolchain,
            extra_paths,
            ..BuildConfig::default()
        };
    }

    // MicroPython projects commonly only contain boot.py / main.py.
    if project_root.join("boot.py").is_file() || project_root.join("main.py").is_file() {
        return BuildConfig {
            toolchain: ToolchainKind::Micropython,
            ..BuildConfig::default()
        };
    }

    // No project build file: fall back to tools available on PATH.
    if let Some(cargo) = find_in_path("cargo") {
        add_tool_dir(&mut extra_paths, Some(cargo));
        return BuildConfig {
            build_command: "cargo build".to_owned(),
            toolchain: ToolchainKind::Cargo,
            clean_command: "cargo clean".to_owned(),
            rebuild_command: "cargo clean && cargo build".to_owned(),
            extra_paths,
            ..BuildConfig::default()
        };
    }
    if let Some(cmake) = find_in_path("cmake") {
        add_tool_dir(&mut extra_paths, Some(cmake));
        return BuildConfig {
            build_command: "cmake -S . -B build && cmake --build build".to_owned(),
            toolchain: ToolchainKind::CMake,
            clean_command: "cmake --build build --target clean".to_owned(),
            rebuild_command: "cmake --build build --clean-first".to_owned(),
            extra_paths,
            ..BuildConfig::default()
        };
    }
    if let Some(make_path) = find_one_of(&["make", "gmake", "mingw32-make"]) {
        let make = make_path
            .file_stem()
            .map(|name| name.to_string_lossy().into_owned())
            .unwrap_or_else(|| "make".to_owned());
        add_tool_dir(&mut extra_paths, Some(make_path));
        return BuildConfig {
            build_command: make.clone(),
            toolchain: ToolchainKind::MakeGcc,
            clean_command: format!("{make} clean"),
            rebuild_command: format!("{make} clean && {make}"),
            extra_paths,
            ..BuildConfig::default()
        };
    }

    BuildConfig::default()
}

fn compiler_toolchain(compiler: Option<&str>) -> ToolchainKind {
    let compiler = compiler.unwrap_or_default().to_ascii_lowercase();
    if compiler.contains("armclang") {
        ToolchainKind::MakeAc6
    } else if compiler.contains("armcc") {
        ToolchainKind::MakeAc5
    } else if compiler.contains("clang") {
        ToolchainKind::MakeClang
    } else {
        ToolchainKind::MakeGcc
    }
}

fn find_keil_uv4() -> Option<PathBuf> {
    let mut candidates = vec![
        PathBuf::from(r"C:\Keil_v5\UV4\UV4.exe"),
        PathBuf::from(r"C:\Keil\UV4\UV4.exe"),
    ];
    for env in ["ProgramFiles", "ProgramFiles(x86)"] {
        if let Some(root) = std::env::var_os(env) {
            candidates.push(
                PathBuf::from(root)
                    .join("Keil_v5")
                    .join("UV4")
                    .join("UV4.exe"),
            );
        }
    }
    candidates.into_iter().find(|path| path.is_file())
}

/// Best-effort check whether the first token of a build command exists.
pub fn command_available(command: &str) -> bool {
    let command = command.trim();
    if command.is_empty() {
        return false;
    }
    let first = if let Some(rest) = command.strip_prefix('"') {
        rest.split('"').next().unwrap_or(rest)
    } else {
        command.split_whitespace().next().unwrap_or(command)
    };
    let path = Path::new(first);
    if path.is_file() {
        return true;
    }
    if first.contains(['/', '\\']) {
        return false;
    }
    find_in_path(first).is_some()
}

fn find_one_of(names: &[&str]) -> Option<PathBuf> {
    names.iter().find_map(|name| find_in_path(name))
}

fn find_in_path(name: &str) -> Option<PathBuf> {
    let paths = std::env::var_os("PATH")?;
    let mut candidates = vec![name.to_owned()];
    #[cfg(target_os = "windows")]
    {
        if Path::new(name).extension().is_none() {
            candidates.push(format!("{name}.exe"));
            candidates.push(format!("{name}.cmd"));
            candidates.push(format!("{name}.bat"));
        }
    }
    for dir in std::env::split_paths(&paths) {
        for candidate in &candidates {
            let path = dir.join(candidate);
            if path.is_file() {
                return Some(path);
            }
        }
    }
    None
}

fn add_tool_dir(paths: &mut Vec<String>, executable: Option<PathBuf>) {
    let Some(executable) = executable else {
        return;
    };
    let Some(parent) = executable.parent() else {
        return;
    };
    let path = display_path(parent);
    if !paths
        .iter()
        .any(|existing| existing.eq_ignore_ascii_case(&path))
    {
        paths.push(path);
    }
}

pub fn detected_tool(project_root: &Path) -> Option<&'static str> {
    if project_root.join("Cargo.toml").is_file() {
        Some("Cargo")
    } else if ["Makefile", "makefile", "GNUmakefile"]
        .iter()
        .any(|name| project_root.join(name).is_file())
    {
        Some("Make")
    } else if project_root.join("CMakeLists.txt").is_file() {
        Some("CMake")
    } else {
        None
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(default)]
pub struct Settings {
    pub last_project: Option<PathBuf>,
    pub recent_projects: Vec<PathBuf>,
    pub build_configs: BTreeMap<String, BuildConfig>,
    pub show_hidden: bool,
    pub respect_gitignore: bool,
    pub ignore_patterns: String,
    pub font_size: f32,
    pub editor_theme: String,
    pub word_wrap: bool,
    pub relative_line_numbers: bool,
    pub show_whitespace: bool,
    pub check_duplicate_macro_values: bool,
    pub ruler_column: u16,
    pub use_spaces: bool,
    pub indent_width: u8,
    pub autosave: bool,
    pub autosave_delay_ms: u64,
    pub undo_limit: usize,
    pub format_on_save: bool,
    pub clang_format_path: String,
    pub clangd_enabled: bool,
    pub clangd_path: String,
    pub compiler_path: String,
    pub make_path: String,
    pub use_bundled_toolchain: bool,
    pub bundled_tools_dir: String,
    pub debug_program: String,
    pub gdb_path: String,
    pub auto_scroll_output: bool,
    pub show_minimap: bool,
    pub show_indent_guides: bool,
    pub show_hover_info: bool,
    pub output_open: bool,
    pub output_height: f32,
    pub left_panel_width: f32,
    pub left_split: f32,
    pub resource_open: bool,
    pub info_open: bool,
    pub auto_start_watch: bool,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            last_project: None,
            recent_projects: Vec::new(),
            build_configs: BTreeMap::new(),
            show_hidden: false,
            respect_gitignore: true,
            ignore_patterns: String::new(),
            font_size: 14.0,
            editor_theme: "示波器原色".to_owned(),
            word_wrap: true,
            relative_line_numbers: false,
            show_whitespace: false,
            check_duplicate_macro_values: false,
            ruler_column: 0,
            use_spaces: true,
            indent_width: 4,
            autosave: false,
            autosave_delay_ms: 1500,
            undo_limit: 64,
            format_on_save: false,
            clang_format_path: String::new(),
            clangd_enabled: true,
            clangd_path: String::new(),
            compiler_path: String::new(),
            make_path: String::new(),
            use_bundled_toolchain: true,
            bundled_tools_dir: String::new(),
            debug_program: String::new(),
            gdb_path: String::new(),
            auto_scroll_output: true,
            show_minimap: true,
            show_indent_guides: true,
            show_hover_info: true,
            output_open: true,
            output_height: 214.0,
            left_panel_width: 300.0,
            left_split: 0.5,
            resource_open: true,
            info_open: true,
            auto_start_watch: false,
        }
    }
}

impl Settings {
    pub fn load() -> Self {
        migrate_legacy_settings();
        let path = settings_path();
        let Ok(bytes) = std::fs::read(&path) else {
            return Self::default();
        };
        let bytes = bytes.strip_prefix(&[0xEF, 0xBB, 0xBF]).unwrap_or(&bytes);
        match serde_json::from_slice::<Self>(bytes) {
            Ok(mut settings) => {
                settings.sanitize();
                settings
            }
            Err(err) => {
                eprintln!("Could not parse {}: {err}", path.display());
                Self::default()
            }
        }
    }

    pub fn save(&self) {
        let path = settings_path();
        if let Some(parent) = path.parent() {
            if let Err(err) = std::fs::create_dir_all(parent) {
                eprintln!("Could not create {}: {err}", parent.display());
                return;
            }
        }
        match serde_json::to_vec_pretty(self) {
            Ok(bytes) => {
                if let Err(err) = std::fs::write(&path, bytes) {
                    eprintln!("Could not write {}: {err}", path.display());
                }
            }
            Err(err) => eprintln!("Could not serialize settings: {err}"),
        }
    }

    pub fn remember_project(&mut self, path: &Path) {
        self.last_project = Some(path.to_path_buf());
        self.recent_projects.retain(|item| item != path);
        self.recent_projects.insert(0, path.to_path_buf());
        self.recent_projects.truncate(MAX_RECENT_PROJECTS);
    }

    pub fn build_config_for(&self, path: &Path) -> BuildConfig {
        self.build_configs
            .get(&project_key(path))
            .cloned()
            .unwrap_or_else(|| autodetect(path))
    }

    pub fn set_build_config(&mut self, path: &Path, config: &BuildConfig) {
        self.build_configs.insert(project_key(path), config.clone());
    }

    fn sanitize(&mut self) {
        self.font_size = self.font_size.clamp(10.0, 28.0);
        if !matches!(self.ruler_column, 80 | 120) {
            self.ruler_column = 0;
        }
        self.indent_width = self.indent_width.clamp(1, 8);
        self.autosave_delay_ms = self.autosave_delay_ms.clamp(500, 10_000);
        self.undo_limit = self.undo_limit.clamp(1, 64);
        self.clangd_path = self.clangd_path.trim().to_owned();
        if !self.output_height.is_finite() || self.output_height <= 0.0 {
            self.output_height = 214.0;
        }
        self.output_height = self.output_height.clamp(40.0, 4000.0);
        if !self.left_panel_width.is_finite() || self.left_panel_width <= 0.0 {
            self.left_panel_width = 300.0;
        }
        self.left_panel_width = self.left_panel_width.clamp(300.0, 520.0);
        if !self.left_split.is_finite() || self.left_split <= 0.0 {
            self.left_split = 0.5;
        }
        self.left_split = self.left_split.clamp(0.18, 0.82);
        for config in self.build_configs.values_mut() {
            config.build_jobs = config.build_jobs.min(128);
        }
        self.recent_projects.retain(|path| path.is_dir());
        self.recent_projects.dedup();
        self.recent_projects.truncate(MAX_RECENT_PROJECTS);
        if self
            .last_project
            .as_ref()
            .is_some_and(|path| !path.is_dir())
        {
            self.last_project = None;
        }
    }
}

pub fn settings_path() -> PathBuf {
    config_dir().join("settings.json")
}

fn config_dir() -> PathBuf {
    #[cfg(target_os = "windows")]
    {
        if let Some(appdata) = std::env::var_os("APPDATA") {
            return PathBuf::from(appdata).join(APP_DATA_DIR);
        }
    }

    if let Some(config) = std::env::var_os("XDG_CONFIG_HOME") {
        return PathBuf::from(config).join("simple-embedded");
    }
    if let Some(home) = std::env::var_os("HOME").or_else(|| std::env::var_os("USERPROFILE")) {
        return PathBuf::from(home).join(".config").join("simple-embedded");
    }

    PathBuf::from(".simple-embedded")
}

fn legacy_settings_path() -> PathBuf {
    #[cfg(target_os = "windows")]
    {
        if let Some(appdata) = std::env::var_os("APPDATA") {
            return PathBuf::from(appdata)
                .join(LEGACY_APP_DATA_DIR)
                .join("settings.json");
        }
    }

    if let Some(config) = std::env::var_os("XDG_CONFIG_HOME") {
        return PathBuf::from(config)
            .join("mini-eide")
            .join("settings.json");
    }
    if let Some(home) = std::env::var_os("HOME").or_else(|| std::env::var_os("USERPROFILE")) {
        return PathBuf::from(home)
            .join(".config")
            .join("mini-eide")
            .join("settings.json");
    }

    PathBuf::from(".mini-eide-settings.json")
}

fn migrate_legacy_settings() {
    let new_path = settings_path();
    if new_path.exists() {
        return;
    }
    let old_path = legacy_settings_path();
    if !old_path.is_file() {
        return;
    }
    if let Some(parent) = new_path.parent() {
        if std::fs::create_dir_all(parent).is_err() {
            return;
        }
    }
    let _ = std::fs::copy(old_path, new_path);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn settings_defaults_keep_hover_and_optional_macro_check() {
        let settings: Settings = serde_json::from_str("{}").unwrap();
        assert!(settings.show_hover_info);
        assert!(!settings.check_duplicate_macro_values);
        assert_eq!(settings.editor_theme, "示波器原色");
        assert_eq!(settings.left_panel_width, 300.0);
        assert_eq!(settings.undo_limit, 64);
    }

    #[test]
    fn settings_clamps_undo_limit_to_64() {
        let mut settings = Settings {
            undo_limit: 0,
            ..Settings::default()
        };
        settings.sanitize();
        assert_eq!(settings.undo_limit, 1);
        settings.undo_limit = 999;
        settings.sanitize();
        assert_eq!(settings.undo_limit, 64);
    }

    #[test]
    fn detects_cargo() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("Cargo.toml"), "[package]\n").unwrap();
        let cfg = autodetect(dir.path());
        assert_eq!(cfg.build_command, "cargo build");
        assert_eq!(detected_tool(dir.path()), Some("Cargo"));
    }

    #[test]
    fn detects_make_before_cmake() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("Makefile"), "all:\n").unwrap();
        std::fs::write(dir.path().join("CMakeLists.txt"), "").unwrap();
        let cfg = autodetect(dir.path());
        assert_eq!(cfg.tool_name(), "Make");
    }

    #[test]
    fn resolves_effective_toolchain_from_build_system() {
        let mut cfg = BuildConfig {
            build_command: "make".into(),
            ..BuildConfig::default()
        };
        let mut meta = ProjectMeta {
            compiler: Some("arm-none-eabi-gcc".into()),
            toolchain: Some("Make · GCC".into()),
            ..ProjectMeta::default()
        };
        assert_eq!(cfg.effective_toolchain(&meta), ToolchainKind::MakeGcc);
        assert!(!cfg.toolchain_mismatch());

        meta.compiler = Some("armclang".into());
        assert_eq!(cfg.effective_toolchain(&meta), ToolchainKind::MakeAc6);

        cfg.toolchain = ToolchainKind::KeilAc6;
        assert!(cfg.toolchain_mismatch());
        meta.toolchain = Some("Keil MDK · AC6".into());
        assert_eq!(cfg.effective_toolchain(&meta), ToolchainKind::KeilAc6);

        cfg.toolchain = ToolchainKind::Auto;
        cfg.build_command = "UV4 -b demo.uvprojx".into();
        meta.toolchain = Some("Keil MDK · AC5".into());
        assert_eq!(cfg.effective_toolchain(&meta), ToolchainKind::KeilAc5);
    }

    #[test]
    fn autodetect_prefers_root_build_system_and_sets_toolchain() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(
            dir.path().join("Makefile"),
            "TARGET = demo\nCC = armclang\n",
        )
        .unwrap();
        let mdk = dir.path().join("MDK-ARM");
        std::fs::create_dir_all(&mdk).unwrap();
        std::fs::write(
            mdk.join("demo.uvprojx"),
            "<Project><Targets><Target><uAC6>1</uAC6></Target></Targets></Project>",
        )
        .unwrap();
        let cfg = autodetect(dir.path());
        assert_eq!(cfg.toolchain, ToolchainKind::MakeAc6);
        assert_eq!(cfg.tool_name(), "Make");
    }

    #[test]
    fn checks_absolute_command_availability() {
        let dir = tempfile::tempdir().unwrap();
        let exe = dir.path().join("tool.exe");
        std::fs::write(&exe, b"x").unwrap();
        assert!(command_available(&format!(
            "\"{}\" -b project",
            exe.display()
        )));
        assert!(!command_available("definitely-not-a-real-tool-xyz build"));
    }

    #[test]
    fn autodetect_keil_project_without_makefile() {
        let dir = tempfile::tempdir().unwrap();
        let mdk = dir.path().join("MDK-ARM");
        std::fs::create_dir_all(&mdk).unwrap();
        std::fs::write(
            mdk.join("demo.uvprojx"),
            "<Project><Targets><Target><uAC6>1</uAC6></Target></Targets></Project>",
        )
        .unwrap();
        let cfg = autodetect(dir.path());
        assert_eq!(cfg.toolchain, ToolchainKind::KeilAc6);
    }

    #[test]
    fn autodetect_recognizes_micropython_main_py() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("main.py"), "print('esp32')\n").unwrap();
        let cfg = autodetect(dir.path());
        assert_eq!(cfg.toolchain, ToolchainKind::Micropython);
        assert!(cfg.serial.port.is_empty());
        assert_eq!(cfg.serial.baud, 115_200);
    }

    #[test]
    fn validates_ssh_fields() {
        let mut cfg = BuildConfig {
            target: BuildTarget::Ssh,
            build_command: "make".into(),
            ..BuildConfig::default()
        };
        assert!(cfg.validate().is_err());
        cfg.ssh_host = "dev@vm".into();
        cfg.remote_dir = "/home/dev/project".into();
        assert!(cfg.validate().is_ok());
    }

    #[test]
    fn micropython_requires_serial_port_and_skips_build_commands() {
        let mut cfg = BuildConfig {
            toolchain: ToolchainKind::Micropython,
            ..BuildConfig::default()
        };
        assert!(cfg.validate().is_err());
        cfg.serial.port = "COM7".into();
        cfg.serial.baud = 115_200;
        assert!(cfg.validate().is_ok());
        assert!(!cfg.has_command(TaskKind::Build));
        assert!(!cfg.has_command(TaskKind::Flash));
        assert_eq!(cfg.tool_name(), "MicroPython");
        assert!(cfg.mode_summary().contains("COM7"));
        assert!(cfg.mode_summary().contains("115200"));
    }

    #[test]
    fn micropython_serial_config_round_trips_through_json() {
        let cfg: BuildConfig = serde_json::from_str(
            r#"{"toolchain":"micropython","serial":{"port":"COM4","baud":921600}}"#,
        )
        .unwrap();
        assert_eq!(cfg.toolchain, ToolchainKind::Micropython);
        assert_eq!(cfg.serial.port, "COM4");
        assert_eq!(cfg.serial.baud, 921_600);
        assert_eq!(cfg.firmware.chip, "esp32");
        assert_eq!(cfg.firmware.address, "0x1000");
        assert!(cfg.firmware.verify);
        let json = serde_json::to_string(&cfg).unwrap();
        assert!(json.contains(r#""toolchain":"micropython""#));
        assert!(json.contains(r#""serial":{"port":"COM4","baud":921600}"#));
        assert!(json.contains(r#""firmware""#));
    }

    #[test]
    fn micropython_firmware_defaults_follow_the_chip() {
        assert_eq!(FirmwareConfig::default_address("esp32"), "0x1000");
        assert_eq!(FirmwareConfig::default_address("esp32s2"), "0x1000");
        assert_eq!(FirmwareConfig::default_address("esp32s3"), "0x0");
        assert_eq!(FirmwareConfig::default_address("esp32c3"), "0x0");
        assert!(!FirmwareConfig::default().is_ready());
        let config = FirmwareConfig {
            file: "esp32.bin".into(),
            ..FirmwareConfig::default()
        };
        assert!(config.is_ready());
    }

    #[test]
    fn builds_quoted_remote_command() {
        let cfg = BuildConfig {
            target: BuildTarget::Ssh,
            build_command: "make -j4".into(),
            ssh_host: "dev@vm".into(),
            remote_dir: "/home/dev/my firmware".into(),
            ..BuildConfig::default()
        };
        let spec = cfg.make_spec(TaskKind::Build, Path::new(".")).unwrap();
        assert_eq!(spec.program, "ssh");
        assert_eq!(spec.display, "ssh dev@vm · make -j4");
        assert!(spec
            .args
            .last()
            .unwrap()
            .to_string_lossy()
            .contains("cd '/home/dev/my firmware' && make -j4"));
    }

    #[cfg(target_os = "windows")]
    #[test]
    fn identifies_make_commands() {
        assert!(looks_like_make("mingw32-make clean"));
        assert!(looks_like_make("gmake -j4"));
        assert!(looks_like_make("make.exe"));
        assert!(!looks_like_make("cmake --build build"));
    }

    #[cfg(target_os = "windows")]
    #[test]
    fn local_make_adds_posix_tools_when_available() {
        if find_posix_tools_dir().is_none() {
            return;
        }
        let cfg = BuildConfig {
            build_command: "mingw32-make".into(),
            clean_command: "mingw32-make clean".into(),
            ..BuildConfig::default()
        };
        let spec = cfg.make_spec(TaskKind::Clean, Path::new(".")).unwrap();
        assert!(spec
            .env_paths
            .iter()
            .any(|dir| dir.join("rm.exe").is_file()));
    }

    #[test]
    fn flash_is_local_even_in_ssh_mode() {
        let cfg = BuildConfig {
            target: BuildTarget::Ssh,
            build_command: "make".into(),
            flash_command: "JLink.exe -CommanderScript flash.jlink".into(),
            ssh_host: "dev@vm".into(),
            remote_dir: "/home/dev/project".into(),
            ..BuildConfig::default()
        };
        let spec = cfg.make_spec(TaskKind::Flash, Path::new(".")).unwrap();
        if cfg!(target_os = "windows") {
            assert_eq!(spec.program, "cmd");
        } else {
            assert_eq!(spec.program, "sh");
        }
    }

    #[test]
    fn parallel_jobs_are_applied_to_cargo_and_cmake() {
        let cargo = BuildConfig {
            build_command: "cargo build".into(),
            build_jobs: 8,
            ..BuildConfig::default()
        };
        let env = cargo.jobs_env("cargo build");
        assert_eq!(
            env,
            vec![(OsString::from("CARGO_BUILD_JOBS"), OsString::from("8"))]
        );

        let cmake = BuildConfig {
            build_command: "cmake --build build".into(),
            build_jobs: 6,
            ..BuildConfig::default()
        };
        let env = cmake.jobs_env("cmake --build build");
        assert_eq!(
            env,
            vec![(
                OsString::from("CMAKE_BUILD_PARALLEL_LEVEL"),
                OsString::from("6")
            )]
        );
    }

    #[test]
    fn explicit_jobs_flags_are_respected() {
        let cfg = BuildConfig {
            build_command: "make -j4".into(),
            build_jobs: 8,
            ..BuildConfig::default()
        };
        assert!(cfg.jobs_env("make -j4").is_empty());
        assert!(cfg.jobs_shell_prefix("make -j4").is_none());
        assert!(command_has_jobs_flag("make --jobs=4"));
        assert!(command_has_jobs_flag("cmake --build build --parallel 4"));
        assert!(!command_has_jobs_flag("make all"));
    }

    #[test]
    fn remote_jobs_prefix_uses_tool_environment() {
        let cfg = BuildConfig {
            build_command: "cargo build".into(),
            build_jobs: 8,
            target: BuildTarget::Ssh,
            ssh_host: "dev@vm".into(),
            remote_dir: "/home/dev/project".into(),
            ..BuildConfig::default()
        };
        let spec = cfg.make_spec(TaskKind::Build, Path::new(".")).unwrap();
        assert_eq!(
            spec.args
                .last()
                .map(|value| value.to_string_lossy().into_owned()),
            Some("export CARGO_BUILD_JOBS=8; cd '/home/dev/project' && cargo build".to_owned())
        );
    }

    #[test]
    fn old_settings_default_to_custom_flash() {
        let config: BuildConfig = serde_json::from_str(
            r#"{"build_command":"make","flash_command":"JLink.exe flash.jlink"}"#,
        )
        .unwrap();
        assert_eq!(config.flash_tool, FlashTool::Custom);
        assert!(config.has_command(TaskKind::Flash));
    }

    #[test]
    fn openocd_flash_uses_direct_spec_even_over_ssh() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(dir.path().join("build")).unwrap();
        std::fs::write(dir.path().join("build/demo.hex"), b":00000001FF").unwrap();
        let cfg = BuildConfig {
            target: BuildTarget::Ssh,
            build_command: "make".into(),
            flash_tool: FlashTool::OpenOcd,
            openocd: OpenOcdConfig {
                executable: "openocd".into(),
                program_file: "${ExecutableName}.hex".into(),
                target_config: "stm32f1x.cfg".into(),
                interface_config: "stlink.cfg".into(),
            },
            ssh_host: "dev@vm".into(),
            remote_dir: "/home/dev/project".into(),
            ..BuildConfig::default()
        };
        assert!(cfg.has_command(TaskKind::Flash));
        let spec = cfg.make_spec(TaskKind::Flash, dir.path()).unwrap();
        assert_eq!(spec.program, "openocd");
        assert!(spec.display.contains("program build/demo.hex"));
        assert!(!spec.display.starts_with("ssh"));
    }

    #[test]
    fn project_settings_round_trip_preserves_flash_config() {
        let dir = tempfile::tempdir().unwrap();
        let config = BuildConfig {
            build_command: "make".into(),
            flash_tool: FlashTool::OpenOcd,
            openocd: OpenOcdConfig {
                executable: "C:/tools/openocd.exe".into(),
                program_file: "${ExecutableName}.hex".into(),
                target_config: "stm32f1x.cfg".into(),
                interface_config: "stlink.cfg".into(),
            },
            memory: MemoryConfig {
                flash_start: "0x08000000".into(),
                flash_size: "0x00010000".into(),
                ram_start: "0x20000000".into(),
                ram_size: "0x00005000".into(),
                download_address: "0x08000000".into(),
                regions: vec![MemoryRegionConfig {
                    name: "IROM1".into(),
                    kind: MemoryKind::Rom,
                    on_chip: true,
                    enabled: true,
                    start: "0x08000000".into(),
                    size: "64K".into(),
                    no_init: false,
                    startup: true,
                }],
            },
            ..BuildConfig::default()
        };
        let debug = ProjectDebugSettings {
            program: "build/demo.axf".into(),
            gdb_path: "C:/tools/arm-none-eabi-gdb.exe".into(),
            auto_refresh: true,
            watches: vec!["counter".into(), "state".into()],
        };
        let path = ProjectSettings::save(dir.path(), &config, &debug).unwrap();
        assert!(path.ends_with("settings.json"));
        assert!(path.parent().unwrap().ends_with(".simple-embedded"));
        let loaded = ProjectSettings::load(dir.path()).unwrap().unwrap();
        assert_eq!(loaded.build.flash_tool, FlashTool::OpenOcd);
        assert_eq!(
            loaded.build.openocd.executable,
            "C:/tools/openocd.exe".to_owned()
        );
        assert_eq!(loaded.build.openocd.target_config, "stm32f1x.cfg");
        assert_eq!(loaded.build.memory.flash_size, "0x00010000");
        assert_eq!(loaded.build.memory.download_address, "0x08000000");
        assert_eq!(loaded.build.memory.regions.len(), 1);
        assert!(loaded.build.memory.regions[0].startup);
        assert_eq!(loaded.debug.program, "build/demo.axf");
        assert_eq!(loaded.debug.gdb_path, "C:/tools/arm-none-eabi-gdb.exe");
        assert!(loaded.debug.auto_refresh);
        assert_eq!(loaded.debug.watches, vec!["counter", "state"]);
    }

    #[test]
    fn malformed_project_settings_are_reported_without_being_replaced() {
        let dir = tempfile::tempdir().unwrap();
        let path = ProjectSettings::path(dir.path());
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(&path, b"{not json").unwrap();
        assert!(ProjectSettings::load(dir.path()).is_err());
        assert_eq!(std::fs::read(&path).unwrap(), b"{not json");
    }

    #[test]
    fn workspace_file_resolves_relative_project_folder() {
        let dir = tempfile::tempdir().unwrap();
        let project = dir.path().join("demo");
        std::fs::create_dir_all(&project).unwrap();
        let path = dir.path().join("demo.se-workspace");
        std::fs::write(
            &path,
            br#"{"version":1,"name":"demo","folders":[{"path":"demo"}]}"#,
        )
        .unwrap();
        assert!(is_workspace_file(&path));
        assert_eq!(resolve_workspace_file(&path).unwrap(), project);
    }

    #[test]
    fn generated_workspace_file_points_at_its_project() {
        let dir = tempfile::tempdir().unwrap();
        let path = write_workspace_file(dir.path()).unwrap();
        let expected = format!("{}.{WORKSPACE_EXTENSION}", file_name(dir.path()));
        assert_eq!(
            path.file_name().and_then(|name| name.to_str()),
            Some(expected.as_str())
        );
        let workspace: WorkspaceFile =
            serde_json::from_slice(&std::fs::read(&path).unwrap()).unwrap();
        assert_eq!(workspace.name, file_name(dir.path()));
        assert_eq!(workspace.folders[0].path, ".");
        assert_eq!(resolve_workspace_file(&path).unwrap(), dir.path());
    }
}
