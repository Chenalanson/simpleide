//! Local build toolchain discovery and optional bundled tool install.

use std::path::{Path, PathBuf};
use std::sync::mpsc::{channel, Receiver};

use serde::{Deserialize, Serialize};

use crate::config::{settings_path, Settings};

const MANIFEST: &str = "toolchain.json";
const MAKE_NAMES: [&str; 4] = ["make.exe", "mingw32-make.exe", "gmake.exe", "make"];
const COMPILER_NAMES: &[&str] = &[
    "arm-none-eabi-gcc.exe",
    "gcc.exe",
    "clang.exe",
    "armclang.exe",
    "armcc.exe",
    "cl.exe",
    "arm-none-eabi-gcc",
    "gcc",
    "armclang",
    "armcc",
];

#[derive(Clone, Debug, Default)]
pub struct BundledTools {
    pub compiler: Option<PathBuf>,
    pub make: Option<PathBuf>,
}

#[derive(Debug, Serialize, Deserialize)]
struct Manifest {
    compiler: String,
    make: String,
}

#[derive(Debug)]
#[allow(dead_code)]
pub enum InstallEvent {
    Progress(String),
    Done(Result<(), String>),
}

pub fn portable_tools_dir() -> Option<PathBuf> {
    std::env::current_exe()
        .ok()
        .and_then(|exe| exe.parent().map(|parent| parent.join("tools")))
        .filter(|path| path.is_dir())
}

pub fn user_tools_dir() -> PathBuf {
    settings_path()
        .parent()
        .map(|parent| parent.join("tools"))
        .unwrap_or_else(|| PathBuf::from("tools"))
}

pub fn default_tools_dir() -> PathBuf {
    portable_tools_dir().unwrap_or_else(user_tools_dir)
}

/// Resolve a configured tool directory. Empty means the per-user default;
/// relative paths are resolved against the executable directory so a portable
/// release can ship a sibling `tools` folder.
pub fn resolve_tools_dir(configured: &str) -> PathBuf {
    let trimmed = configured.trim();
    if trimmed.is_empty() {
        return default_tools_dir();
    }
    let path = PathBuf::from(trimmed);
    if path.is_absolute() {
        path
    } else {
        std::env::current_exe()
            .ok()
            .and_then(|exe| exe.parent().map(|parent| parent.join(&path)))
            .unwrap_or(path)
    }
}

pub fn detect(root: &Path) -> BundledTools {
    let mut tools = BundledTools {
        compiler: None,
        make: None,
    };
    if let Ok(bytes) = std::fs::read(root.join(MANIFEST)) {
        if let Ok(manifest) = serde_json::from_slice::<Manifest>(&bytes) {
            let compiler = root.join(
                manifest
                    .compiler
                    .replace('/', std::path::MAIN_SEPARATOR_STR),
            );
            let make = root.join(manifest.make.replace('/', std::path::MAIN_SEPARATOR_STR));
            if compiler.is_file() {
                tools.compiler = Some(compiler);
            }
            if make.is_file() {
                tools.make = Some(make);
            }
        }
    }
    if tools.make.is_none() {
        tools.make = first_existing(root, &MAKE_NAMES, &["make", "bin"], true);
    }
    if tools.compiler.is_none() {
        tools.compiler = first_existing(root, COMPILER_NAMES, &["compiler/bin", "bin"], false);
    }
    tools
}

fn first_existing(root: &Path, names: &[&str], dirs: &[&str], make: bool) -> Option<PathBuf> {
    for dir in dirs {
        for name in names {
            let candidate = root.join(dir).join(name);
            if candidate.is_file() {
                return Some(candidate);
            }
        }
    }
    if make {
        // Some portable packages place make directly in the tools root.
        for name in names {
            let candidate = root.join(name);
            if candidate.is_file() {
                return Some(candidate);
            }
        }
    }
    None
}

pub fn detect_installed() -> (Option<PathBuf>, Option<PathBuf>) {
    let Some(paths) = std::env::var_os("PATH") else {
        return (None, None);
    };
    let dirs = std::env::split_paths(&paths).collect::<Vec<_>>();
    let compiler = COMPILER_NAMES.iter().find_map(|name| find_in(&dirs, name));
    let make = MAKE_NAMES.iter().find_map(|name| find_in(&dirs, name));
    (compiler, make)
}

fn find_in(dirs: &[PathBuf], name: &str) -> Option<PathBuf> {
    dirs.iter()
        .map(|dir| dir.join(name))
        .find(|candidate| candidate.is_file())
}

pub fn build_env_dirs(settings: &Settings) -> Vec<PathBuf> {
    let mut dirs = Vec::new();
    let mut bundled_complete = false;
    if settings.use_bundled_toolchain {
        let root = resolve_tools_dir(&settings.bundled_tools_dir);
        let tools = detect(&root);
        if let Some(make) = tools.make {
            push_parent(&mut dirs, &make);
        }
        if let Some(compiler) = tools.compiler {
            push_parent(&mut dirs, &compiler);
        }
        bundled_complete = dirs.len() >= 2;
    }

    // The old "构建工具链" page is gone, so these fields are only a
    // compatibility fallback. Missing paths are skipped instead of blocking
    // the build; the project's extra PATH and the system PATH still apply.
    if !bundled_complete {
        for value in [settings.compiler_path.trim(), settings.make_path.trim()] {
            if value.is_empty() {
                continue;
            }
            let path = Path::new(value);
            if path.is_file() {
                push_parent(&mut dirs, path);
            }
        }
    }
    dedup_dirs(&mut dirs);
    dirs
}

fn push_parent(dirs: &mut Vec<PathBuf>, file: &Path) {
    if let Some(parent) = file.parent() {
        dirs.push(parent.to_path_buf());
    }
}

fn dedup_dirs(dirs: &mut Vec<PathBuf>) {
    let mut seen = std::collections::BTreeSet::new();
    dirs.retain(|dir| seen.insert(dir.to_string_lossy().to_lowercase()));
}

#[allow(dead_code)]
pub fn install_async(compiler: PathBuf, make: PathBuf, root: PathBuf) -> Receiver<InstallEvent> {
    let (tx, rx) = channel();
    std::thread::spawn(move || {
        let mut last = String::new();
        let result = install(&compiler, &make, &root, &mut |message| {
            if message != last {
                last = message.clone();
                let _ = tx.send(InstallEvent::Progress(message));
            }
        });
        let _ = tx.send(InstallEvent::Done(result));
    });
    rx
}

pub fn install(
    compiler: &Path,
    make: &Path,
    root: &Path,
    progress: &mut dyn FnMut(String),
) -> Result<(), String> {
    if !compiler.is_file() {
        return Err(format!("编译器不存在：{}", compiler.display()));
    }
    if !make.is_file() {
        return Err(format!("make 不存在：{}", make.display()));
    }
    let compiler_root = compiler_root(compiler);
    let make_dir = make
        .parent()
        .ok_or_else(|| format!("无法确定 make 目录：{}", make.display()))?;
    std::fs::create_dir_all(root).map_err(|err| format!("无法创建 {}：{err}", root.display()))?;

    let tmp_compiler = root.join(".compiler.tmp");
    let tmp_make = root.join(".make.tmp");
    remove_dir_if_exists(&tmp_compiler)?;
    remove_dir_if_exists(&tmp_make)?;
    std::fs::create_dir_all(&tmp_compiler)
        .map_err(|err| format!("无法创建 {}：{err}", tmp_compiler.display()))?;
    std::fs::create_dir_all(&tmp_make)
        .map_err(|err| format!("无法创建 {}：{err}", tmp_make.display()))?;

    progress("正在复制编译器…".to_owned());
    let mut copied = 0usize;
    copy_tree(&compiler_root, &tmp_compiler, &mut |count| {
        copied = count;
        if count % 500 == 0 {
            progress(format!("正在复制编译器… {count} 个文件"));
        }
        let _ = copied;
    })?;
    progress(format!("编译器复制完成，共 {copied} 个文件"));

    progress("正在复制 make…".to_owned());
    let make_name = make
        .file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .ok_or_else(|| format!("无法确定 make 文件名：{}", make.display()))?;
    copy_file(make, &tmp_make.join(&make_name))?;
    let canonical_make = if make_name != "make.exe" {
        copy_file(make, &tmp_make.join("make.exe"))?;
        "make/make.exe".to_owned()
    } else {
        format!("make/{make_name}")
    };
    if let Ok(entries) = std::fs::read_dir(make_dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_file()
                && path
                    .extension()
                    .is_some_and(|ext| ext.eq_ignore_ascii_case("dll"))
            {
                if let Some(name) = path.file_name() {
                    copy_file(&path, &tmp_make.join(name))?;
                }
            }
        }
    }

    let dest_compiler = root.join("compiler");
    let dest_make = root.join("make");
    replace_dir(&tmp_compiler, &dest_compiler)?;
    replace_dir(&tmp_make, &dest_make)?;

    let compiler_relative = compiler
        .strip_prefix(&compiler_root)
        .unwrap_or(compiler)
        .to_string_lossy()
        .replace('\\', "/");
    let manifest = Manifest {
        compiler: format!("compiler/{compiler_relative}"),
        make: canonical_make,
    };
    let bytes =
        serde_json::to_vec_pretty(&manifest).map_err(|err| format!("无法生成工具链清单：{err}"))?;
    std::fs::write(root.join(MANIFEST), bytes)
        .map_err(|err| format!("无法写入 {}：{err}", root.join(MANIFEST).display()))?;
    progress("工具安装完成".to_owned());
    Ok(())
}

fn compiler_root(compiler: &Path) -> PathBuf {
    let parent = compiler.parent().unwrap_or_else(|| Path::new("."));
    if parent
        .file_name()
        .is_some_and(|name| name.eq_ignore_ascii_case("bin"))
    {
        parent.parent().unwrap_or(parent).to_path_buf()
    } else {
        parent.to_path_buf()
    }
}

fn copy_file(source: &Path, target: &Path) -> Result<(), String> {
    std::fs::copy(source, target).map_err(|err| {
        format!(
            "复制 {} 到 {} 失败：{err}",
            source.display(),
            target.display()
        )
    })?;
    Ok(())
}

fn copy_tree(source: &Path, target: &Path, progress: &mut dyn FnMut(usize)) -> Result<(), String> {
    let entries = std::fs::read_dir(source)
        .map_err(|err| format!("读取目录 {} 失败：{err}", source.display()))?;
    let mut count = 0usize;
    for entry in entries.flatten() {
        let path = entry.path();
        let file_type = entry
            .file_type()
            .map_err(|err| format!("读取 {} 类型失败：{err}", path.display()))?;
        let destination = target.join(entry.file_name());
        if file_type.is_dir() {
            std::fs::create_dir_all(&destination)
                .map_err(|err| format!("创建目录 {} 失败：{err}", destination.display()))?;
            copy_tree(&path, &destination, &mut |sub| {
                count += sub;
                progress(count);
            })?;
        } else if file_type.is_file() {
            copy_file(&path, &destination)?;
            count += 1;
            progress(count);
        }
    }
    Ok(())
}

fn replace_dir(source: &Path, destination: &Path) -> Result<(), String> {
    let old = destination.with_extension("old");
    remove_dir_if_exists(&old)?;
    if destination.exists() {
        std::fs::rename(destination, &old)
            .map_err(|err| format!("备份旧工具目录 {} 失败：{err}", destination.display()))?;
    }
    std::fs::rename(source, destination)
        .map_err(|err| format!("安装 {} 失败：{err}", destination.display()))?;
    remove_dir_if_exists(&old)?;
    Ok(())
}

fn remove_dir_if_exists(path: &Path) -> Result<(), String> {
    if path.exists() {
        std::fs::remove_dir_all(path)
            .map_err(|err| format!("删除旧目录 {} 失败：{err}", path.display()))?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn installs_and_detects_tools() {
        let dir = tempfile::tempdir().unwrap();
        let compiler_root = dir.path().join("compiler-src");
        let compiler_bin = compiler_root.join("bin");
        std::fs::create_dir_all(&compiler_bin).unwrap();
        std::fs::write(compiler_bin.join("arm-none-eabi-gcc.exe"), b"fake gcc").unwrap();
        let make_dir = dir.path().join("make-src");
        std::fs::create_dir_all(&make_dir).unwrap();
        std::fs::write(make_dir.join("mingw32-make.exe"), b"fake make").unwrap();
        std::fs::write(make_dir.join("libexample.dll"), b"fake dll").unwrap();
        let tools = dir.path().join("tools");
        install(
            &compiler_bin.join("arm-none-eabi-gcc.exe"),
            &make_dir.join("mingw32-make.exe"),
            &tools,
            &mut |_| {},
        )
        .unwrap();
        let detected = detect(&tools);
        assert_eq!(
            detected.compiler.as_deref(),
            Some(tools.join("compiler/bin/arm-none-eabi-gcc.exe").as_path())
        );
        assert_eq!(
            detected.make.as_deref(),
            Some(tools.join("make/make.exe").as_path())
        );
        assert!(tools.join("make/libexample.dll").is_file());
        assert!(tools.join(MANIFEST).is_file());
    }

    #[test]
    fn resolves_relative_tools_dir_next_to_executable() {
        let resolved = resolve_tools_dir("tools");
        assert!(resolved.ends_with("tools"));
        assert_eq!(resolve_tools_dir(""), default_tools_dir());
    }
}
