//! Persisted user preferences and per-project build commands.
//!
//! Nothing is written into the project itself: a folder is the project. Settings
//! live under the user's application-data directory.

use std::collections::BTreeMap;
use std::ffi::OsString;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::build::{RunSpec, TaskKind};
use crate::util::{display_path, project_key, shell_quote_posix};

pub const APP_NAME: &str = "Simple embedded";
pub const APP_AUTHOR: &str = "smallYYY";
const APP_DATA_DIR: &str = "SimpleEmbedded";
const LEGACY_APP_DATA_DIR: &str = "MiniEIDE";
const MAX_RECENT_PROJECTS: usize = 8;

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

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(default)]
pub struct BuildConfig {
    pub target: BuildTarget,
    pub build_command: String,
    pub clean_command: String,
    pub rebuild_command: String,
    pub flash_command: String,
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
}

impl Default for BuildConfig {
    fn default() -> Self {
        Self {
            target: BuildTarget::Local,
            build_command: String::new(),
            clean_command: String::new(),
            rebuild_command: String::new(),
            flash_command: String::new(),
            ssh_host: String::new(),
            ssh_port: 22,
            remote_dir: String::new(),
            extra_paths: Vec::new(),
            meta_includes: None,
            meta_library_dirs: None,
            meta_defines: None,
            build_options: BuildOptions::default(),
        }
    }
}

impl BuildConfig {
    pub fn command(&self, kind: TaskKind) -> &str {
        match kind {
            TaskKind::Build => &self.build_command,
            TaskKind::Clean => &self.clean_command,
            TaskKind::Rebuild => &self.rebuild_command,
            TaskKind::Flash => &self.flash_command,
        }
    }

    pub fn has_command(&self, kind: TaskKind) -> bool {
        !self.command(kind).trim().is_empty()
    }

    pub fn validate(&self) -> Result<(), String> {
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

    pub fn mode_summary(&self) -> String {
        format!("{} · {}", self.tool_name(), self.target.label())
    }

    pub fn make_spec(&self, kind: TaskKind, project_root: &Path) -> Result<RunSpec, String> {
        let raw = self.command(kind).trim();
        if raw.is_empty() {
            return Err(format!("{} 命令尚未配置", kind.label()));
        }

        // Flashing is intentionally local: J-Link / OpenOCD usually needs USB
        // access, even when compilation happens in a Linux VM.
        let remote = self.target == BuildTarget::Ssh && kind != TaskKind::Flash;
        if remote {
            let host = self.ssh_host.trim();
            let remote_dir = self.remote_dir.trim();
            if host.is_empty() {
                return Err("远程 SSH 模式需要填写主机".to_owned());
            }
            if remote_dir.is_empty() {
                return Err("远程 SSH 模式需要填写远程工程目录".to_owned());
            }

            let remote_command = format!("cd {} && {}", shell_quote_posix(remote_dir), raw);
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
            };

            #[cfg(not(target_os = "windows"))]
            let spec = RunSpec {
                program: "sh".into(),
                args: vec!["-c".into(), raw.into()],
                cwd: project_root.to_path_buf(),
                display: raw.to_owned(),
                env_paths,
            };

            Ok(spec)
        }
    }
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

pub fn autodetect(project_root: &Path) -> BuildConfig {
    let mut extra_paths = Vec::new();

    if project_root.join("Cargo.toml").is_file() {
        add_tool_dir(&mut extra_paths, find_in_path("cargo"));
        return BuildConfig {
            build_command: "cargo build".to_owned(),
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
            find_one_of(&["arm-none-eabi-gcc", "gcc", "clang"]),
        );
        return BuildConfig {
            build_command: make.clone(),
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
            clean_command: "cmake --build build --target clean".to_owned(),
            rebuild_command: "cmake --build build --clean-first".to_owned(),
            extra_paths,
            ..BuildConfig::default()
        };
    }

    BuildConfig::default()
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
    pub font_size: f32,
    pub word_wrap: bool,
    pub auto_scroll_output: bool,
    pub output_open: bool,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            last_project: None,
            recent_projects: Vec::new(),
            build_configs: BTreeMap::new(),
            show_hidden: false,
            font_size: 14.0,
            word_wrap: true,
            auto_scroll_output: true,
            output_open: true,
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
}
