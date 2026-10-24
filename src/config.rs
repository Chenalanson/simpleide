//! Persisted user preferences and per-project build commands.
//!
//! Nothing is written into the project itself: a folder is the project. Settings
//! live under the user's application-data directory.

use std::collections::BTreeMap;
use std::ffi::OsString;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::build::{RunSpec, TaskKind};
use crate::util::{project_key, shell_quote_posix};

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
pub struct BuildConfig {
    pub target: BuildTarget,
    pub build_command: String,
    pub clean_command: String,
    pub rebuild_command: String,
    pub flash_command: String,
    pub ssh_host: String,
    pub ssh_port: u16,
    pub remote_dir: String,
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
            })
        } else {
            #[cfg(target_os = "windows")]
            let spec = RunSpec {
                program: "cmd".into(),
                args: vec!["/S".into(), "/C".into(), raw.into()],
                cwd: project_root.to_path_buf(),
                display: raw.to_owned(),
            };

            #[cfg(not(target_os = "windows"))]
            let spec = RunSpec {
                program: "sh".into(),
                args: vec!["-c".into(), raw.into()],
                cwd: project_root.to_path_buf(),
                display: raw.to_owned(),
            };

            Ok(spec)
        }
    }
}

pub fn autodetect(project_root: &Path) -> BuildConfig {
    if project_root.join("Cargo.toml").is_file() {
        return BuildConfig {
            build_command: "cargo build".to_owned(),
            clean_command: "cargo clean".to_owned(),
            rebuild_command: "cargo clean && cargo build".to_owned(),
            ..BuildConfig::default()
        };
    }

    let has_makefile = ["Makefile", "makefile", "GNUmakefile"]
        .iter()
        .any(|name| project_root.join(name).is_file());
    if has_makefile {
        let make = make_program();
        return BuildConfig {
            build_command: make.to_owned(),
            clean_command: format!("{make} clean"),
            rebuild_command: format!("{make} clean && {make}"),
            ..BuildConfig::default()
        };
    }

    if project_root.join("CMakeLists.txt").is_file() {
        let has_cache = project_root.join("build").join("CMakeCache.txt").is_file();
        return BuildConfig {
            build_command: if has_cache {
                "cmake --build build".to_owned()
            } else {
                "cmake -S . -B build && cmake --build build".to_owned()
            },
            clean_command: "cmake --build build --target clean".to_owned(),
            rebuild_command: "cmake --build build --clean-first".to_owned(),
            ..BuildConfig::default()
        };
    }

    BuildConfig::default()
}

fn make_program() -> &'static str {
    for candidate in ["make", "gmake", "mingw32-make"] {
        if executable_in_path(candidate) {
            return match candidate {
                "make" => "make",
                "gmake" => "gmake",
                _ => "mingw32-make",
            };
        }
    }
    "make"
}

fn executable_in_path(name: &str) -> bool {
    let Some(paths) = std::env::var_os("PATH") else {
        return false;
    };
    for dir in std::env::split_paths(&paths) {
        if dir.join(name).is_file() {
            return true;
        }
        #[cfg(target_os = "windows")]
        if dir.join(format!("{name}.exe")).is_file() {
            return true;
        }
    }
    false
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
    #[cfg(target_os = "windows")]
    {
        if let Some(appdata) = std::env::var_os("APPDATA") {
            return PathBuf::from(appdata)
                .join("MiniEIDE")
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
