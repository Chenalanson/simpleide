//! Per-user Windows installation and Explorer integration.
//!
//! The application can install itself into `%LOCALAPPDATA%\\Programs\\SimpleEmbedded`,
//! add a folder context menu, create a Start Menu shortcut and register an
//! entry in Windows "Apps & features". Everything is written to HKCU, so no
//! administrator elevation is required.

use std::path::{Path, PathBuf};
use std::process::Command;

use crate::util::{display_path, hide_console};

const EXE_NAME: &str = "simple-embedded.exe";
const INSTALL_DIR_NAME: &str = "SimpleEmbedded";
const START_MENU_NAME: &str = "Simple embedded.lnk";
const FOLDER_MENU_KEY: &str = r"HKCU\Software\Classes\Directory\shell\SimpleEmbedded";
const FOLDER_BACKGROUND_KEY: &str =
    r"HKCU\Software\Classes\Directory\Background\shell\SimpleEmbedded";
const UNINSTALL_KEY: &str =
    r"HKCU\Software\Microsoft\Windows\CurrentVersion\Uninstall\SimpleEmbedded";

/// Directory used by the per-user installer.
pub fn install_dir() -> Result<PathBuf, String> {
    if let Some(path) = std::env::var_os("SIMPLE_EMBEDDED_INSTALL_DIR") {
        let path = PathBuf::from(path);
        if !path.as_os_str().is_empty() {
            return Ok(path);
        }
    }
    let base = std::env::var_os("LOCALAPPDATA")
        .or_else(|| std::env::var_os("APPDATA"))
        .ok_or_else(|| "无法定位 LOCALAPPDATA / APPDATA".to_owned())?;
    Ok(PathBuf::from(base).join("Programs").join(INSTALL_DIR_NAME))
}

pub fn installed_exe() -> Result<PathBuf, String> {
    Ok(install_dir()?.join(EXE_NAME))
}

pub fn is_installed() -> bool {
    installed_exe().is_ok_and(|path| path.is_file())
}

pub fn current_is_installed() -> bool {
    let Ok(current) = std::env::current_exe() else {
        return false;
    };
    installed_exe().is_ok_and(|target| same_path(&current, &target))
}

/// Copy the running build into the per-user install directory and register
/// Explorer integration. Safe to run again to update an existing install.
pub fn install_system() -> Result<String, String> {
    if !cfg!(target_os = "windows") {
        return Err("系统安装仅支持 Windows".to_owned());
    }
    let source = std::env::current_exe().map_err(|err| format!("无法获取程序路径：{err}"))?;
    let directory = install_dir()?;
    std::fs::create_dir_all(&directory)
        .map_err(|err| format!("无法创建安装目录 {}：{err}", display_path(&directory)))?;
    let target = directory.join(EXE_NAME);
    let mut notes = Vec::new();

    if !same_path(&source, &target) {
        let linked = install_file(&source, &target)?;
        if linked {
            notes.push("程序文件已链接".to_owned());
        }
    }

    if let Some(source_tools) = source.parent().map(|parent| parent.join("tools")) {
        if source_tools.is_dir() {
            let target_tools = directory.join("tools");
            if !same_path(&source_tools, &target_tools) {
                let linked = install_tree(&source_tools, &target_tools)?;
                notes.push(if linked {
                    "自带工具链已链接".to_owned()
                } else {
                    "自带工具链已复制".to_owned()
                });
            }
        }
    }

    register_folder_context_menu(&target)?;
    if let Err(err) = crate::config::register_workspace_association_for(&target) {
        notes.push(format!("工作区关联失败：{err}"));
    }
    match create_start_menu_shortcut(&target, &directory) {
        Ok(_) => notes.push("已创建开始菜单快捷方式".to_owned()),
        Err(err) => notes.push(format!("开始菜单快捷方式创建失败：{err}")),
    }
    register_uninstall_entry(&target, &directory)?;
    crate::config::notify_shell_association_changed();

    let mut message = format!("已安装到 {}", display_path(&directory));
    if !notes.is_empty() {
        message.push('（');
        message.push_str(&notes.join("；"));
        message.push('）');
    }
    Ok(message)
}

/// Remove Explorer menu items, the Start Menu shortcut and the Apps & features
/// entry. Installed files are kept; use [`uninstall_system`] for a full
/// uninstall.
pub fn remove_integration() -> Result<String, String> {
    if !cfg!(target_os = "windows") {
        return Err("系统集成仅支持 Windows".to_owned());
    }
    for key in [FOLDER_MENU_KEY, FOLDER_BACKGROUND_KEY, UNINSTALL_KEY] {
        reg_delete(key);
    }
    crate::config::unregister_workspace_association();
    if let Ok(shortcut) = start_menu_shortcut_path() {
        remove_file_if_exists(&shortcut)?;
    }
    crate::config::notify_shell_association_changed();
    let directory = install_dir()?;
    Ok(format!(
        "已移除文件夹右键菜单、开始菜单快捷方式和应用列表条目；安装文件保留在 {}",
        display_path(&directory)
    ))
}

/// Full uninstall used by `--uninstall-system` and Windows "Apps & features".
/// When the running executable is inside the install directory, deletion is
/// scheduled after the process exits.
pub fn uninstall_system() -> Result<String, String> {
    if !cfg!(target_os = "windows") {
        return Err("系统卸载仅支持 Windows".to_owned());
    }
    let directory = install_dir()?;
    let current = std::env::current_exe().ok();
    let mut errors = Vec::new();

    if let Err(err) = remove_integration() {
        errors.push(err);
    }

    if directory.exists() {
        let running_from_install = current
            .as_ref()
            .is_some_and(|path| path_is_inside(path, &directory));
        if running_from_install {
            if let Err(err) = schedule_directory_removal(&directory) {
                errors.push(format!("无法安排删除安装目录：{err}"));
            }
        } else if let Err(err) = std::fs::remove_dir_all(&directory) {
            if err.kind() != std::io::ErrorKind::NotFound {
                errors.push(format!(
                    "无法删除安装目录 {}：{err}",
                    display_path(&directory)
                ));
            }
        }
    }

    if errors.is_empty() {
        Ok("Simple embedded 已卸载".to_owned())
    } else {
        Err(errors.join("；"))
    }
}

fn register_folder_context_menu(exe: &Path) -> Result<(), String> {
    let exe = display_path(exe);
    let icon = format!("\"{exe}\",0");
    let folder_command = format!("\"{exe}\" \"%1\"");
    let background_command = format!("\"{exe}\" \"%V\"");

    reg_add_value(FOLDER_MENU_KEY, None, "用 Simple embedded 打开")?;
    reg_add_value(FOLDER_MENU_KEY, Some("Icon"), &icon)?;
    reg_add_value(
        &format!(r"{FOLDER_MENU_KEY}\command"),
        None,
        &folder_command,
    )?;

    reg_add_value(FOLDER_BACKGROUND_KEY, None, "在此处用 Simple embedded 打开")?;
    reg_add_value(FOLDER_BACKGROUND_KEY, Some("Icon"), &icon)?;
    reg_add_value(
        &format!(r"{FOLDER_BACKGROUND_KEY}\command"),
        None,
        &background_command,
    )?;
    Ok(())
}

fn register_uninstall_entry(exe: &Path, directory: &Path) -> Result<(), String> {
    let exe = display_path(exe);
    let directory = display_path(directory);
    reg_add_value(UNINSTALL_KEY, Some("DisplayName"), "Simple embedded")?;
    reg_add_value(
        UNINSTALL_KEY,
        Some("DisplayVersion"),
        env!("CARGO_PKG_VERSION"),
    )?;
    reg_add_value(UNINSTALL_KEY, Some("Publisher"), crate::config::APP_AUTHOR)?;
    reg_add_value(UNINSTALL_KEY, Some("InstallLocation"), &directory)?;
    reg_add_value(UNINSTALL_KEY, Some("DisplayIcon"), &format!("\"{exe}\",0"))?;
    reg_add_value(
        UNINSTALL_KEY,
        Some("UninstallString"),
        &format!("\"{exe}\" --uninstall-system"),
    )?;
    reg_add_dword(UNINSTALL_KEY, "NoModify", 1)?;
    reg_add_dword(UNINSTALL_KEY, "NoRepair", 1)?;
    Ok(())
}

fn start_menu_shortcut_path() -> Result<PathBuf, String> {
    let appdata = std::env::var_os("APPDATA")
        .ok_or_else(|| "无法定位 APPDATA，不能创建开始菜单快捷方式".to_owned())?;
    Ok(PathBuf::from(appdata)
        .join("Microsoft")
        .join("Windows")
        .join("Start Menu")
        .join("Programs")
        .join(START_MENU_NAME))
}

fn create_start_menu_shortcut(exe: &Path, directory: &Path) -> Result<PathBuf, String> {
    let shortcut = start_menu_shortcut_path()?;
    create_shortcut_file(&shortcut, exe, directory)?;
    Ok(shortcut)
}

fn create_shortcut_file(shortcut: &Path, exe: &Path, directory: &Path) -> Result<(), String> {
    if let Some(parent) = shortcut.parent() {
        std::fs::create_dir_all(parent)
            .map_err(|err| format!("无法创建开始菜单目录 {}：{err}", display_path(parent)))?;
    }
    let target = display_path(exe);
    let working = display_path(directory);
    let shortcut_text = display_path(shortcut);
    let script = format!(
        "$s=(New-Object -ComObject WScript.Shell).CreateShortcut({});\
         $s.TargetPath={};$s.WorkingDirectory={};$s.IconLocation='{},0';\
         $s.Description='Simple embedded';$s.Save()",
        powershell_quote(&shortcut_text),
        powershell_quote(&target),
        powershell_quote(&working),
        target.replace('\'', "''")
    );
    let mut command = Command::new("powershell.exe");
    command.args([
        "-NoLogo",
        "-NoProfile",
        "-NonInteractive",
        "-ExecutionPolicy",
        "Bypass",
        "-Command",
        &script,
    ]);
    hide_console(&mut command);
    let output = command
        .output()
        .map_err(|err| format!("无法运行 PowerShell：{err}"))?;
    if !output.status.success() {
        let message = String::from_utf8_lossy(&output.stderr).trim().to_owned();
        return Err(if message.is_empty() {
            "PowerShell 创建快捷方式失败".to_owned()
        } else {
            message
        });
    }
    Ok(())
}

fn powershell_quote(value: &str) -> String {
    format!("'{}'", value.replace('\'', "''"))
}

fn reg_add_value(key: &str, name: Option<&str>, value: &str) -> Result<(), String> {
    let mut command = Command::new("reg");
    command.arg("add").arg(key);
    if let Some(name) = name {
        command.args(["/v", name]);
    } else {
        command.arg("/ve");
    }
    command.args(["/d", value, "/f"]);
    hide_console(&mut command);
    let output = command
        .output()
        .map_err(|err| format!("无法调用 reg.exe：{err}"))?;
    if !output.status.success() {
        let message = String::from_utf8_lossy(&output.stderr).trim().to_owned();
        return Err(if message.is_empty() {
            format!("写入注册表失败：{key}")
        } else {
            format!("写入注册表失败：{key}（{message}）")
        });
    }
    Ok(())
}

fn reg_add_dword(key: &str, name: &str, value: u32) -> Result<(), String> {
    let mut command = Command::new("reg");
    command.args([
        "add",
        key,
        "/v",
        name,
        "/t",
        "REG_DWORD",
        "/d",
        &value.to_string(),
        "/f",
    ]);
    hide_console(&mut command);
    let output = command
        .output()
        .map_err(|err| format!("无法调用 reg.exe：{err}"))?;
    if !output.status.success() {
        return Err(format!("写入注册表失败：{key}\\{name}"));
    }
    Ok(())
}

fn reg_delete(key: &str) {
    let mut command = Command::new("reg");
    command.args(["delete", key, "/f"]);
    hide_console(&mut command);
    let _ = command.output();
}

/// Install a file, preferring a hard link when both paths are on the same
/// volume (the bundled compiler is over 1 GB). Returns true when linked.
fn install_file(source: &Path, target: &Path) -> Result<bool, String> {
    if let Some(parent) = target.parent() {
        std::fs::create_dir_all(parent)
            .map_err(|err| format!("无法创建目录 {}：{err}", display_path(parent)))?;
    }
    if target.exists() {
        std::fs::remove_file(target).map_err(|err| {
            format!(
                "无法替换 {}：{err}。请先关闭已经安装并正在运行的版本。",
                display_path(target)
            )
        })?;
    }
    if std::fs::hard_link(source, target).is_ok() {
        return Ok(true);
    }
    std::fs::copy(source, target)
        .map_err(|err| format!("无法复制 {}：{err}", display_path(source)))?;
    Ok(false)
}

/// Recursively install a directory. Returns true when at least one file was
/// hard-linked. Staging directories from `package-tools.ps1` are skipped.
fn install_tree(source: &Path, target: &Path) -> Result<bool, String> {
    std::fs::create_dir_all(target)
        .map_err(|err| format!("无法创建目录 {}：{err}", display_path(target)))?;
    let mut linked = false;
    let entries = std::fs::read_dir(source)
        .map_err(|err| format!("无法读取目录 {}：{err}", display_path(source)))?;
    for entry in entries.flatten() {
        let path = entry.path();
        let name = entry.file_name();
        let name_text = name.to_string_lossy();
        if name_text == ".compiler.tmp" || name_text == ".make.tmp" {
            continue;
        }
        let Ok(file_type) = entry.file_type() else {
            continue;
        };
        if file_type.is_symlink() {
            continue;
        }
        let destination = target.join(&name);
        if file_type.is_dir() {
            if install_tree(&path, &destination)? {
                linked = true;
            }
        } else if file_type.is_file() && install_file(&path, &destination)? {
            linked = true;
        }
    }
    Ok(linked)
}

fn remove_file_if_exists(path: &Path) -> Result<(), String> {
    match std::fs::remove_file(path) {
        Ok(()) => Ok(()),
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(err) => Err(format!("无法删除 {}：{err}", display_path(path))),
    }
}

fn schedule_directory_removal(directory: &Path) -> Result<(), String> {
    let script_path = std::env::temp_dir().join(format!(
        "simple-embedded-uninstall-{}.ps1",
        std::process::id()
    ));
    let directory = display_path(directory);
    let script_path_text = display_path(&script_path);
    // PowerShell 5.1 needs a BOM to read non-ASCII paths from a script file.
    let script = format!(
        "\u{FEFF}Start-Sleep -Seconds 2\r\n\
         Remove-Item -LiteralPath {} -Recurse -Force -ErrorAction SilentlyContinue\r\n\
         Remove-Item -LiteralPath {} -Force -ErrorAction SilentlyContinue\r\n",
        powershell_quote(&directory),
        powershell_quote(&script_path_text)
    );
    std::fs::write(&script_path, script)
        .map_err(|err| format!("无法创建卸载脚本 {}：{err}", display_path(&script_path)))?;
    let mut command = Command::new("powershell.exe");
    command.args([
        "-NoLogo",
        "-NoProfile",
        "-NonInteractive",
        "-ExecutionPolicy",
        "Bypass",
        "-File",
        &script_path_text,
    ]);
    hide_console(&mut command);
    command
        .spawn()
        .map_err(|err| format!("无法启动卸载脚本：{err}"))?;
    Ok(())
}

fn same_path(left: &Path, right: &Path) -> bool {
    display_path(left).eq_ignore_ascii_case(&display_path(right))
}

fn path_is_inside(path: &Path, directory: &Path) -> bool {
    let mut root = display_path(directory).replace('/', "\\");
    while root.ends_with('\\') {
        root.pop();
    }
    let path = display_path(path).replace('/', "\\");
    path.to_ascii_lowercase()
        .starts_with(&format!("{}\\", root.to_ascii_lowercase()))
}

pub fn show_info_dialog(title: &str, message: &str) {
    message_box(title, message, false);
}

pub fn show_error_dialog(title: &str, message: &str) {
    message_box(title, message, true);
}

#[cfg(target_os = "windows")]
fn message_box(title: &str, message: &str, error: bool) {
    use std::ffi::c_void;

    #[link(name = "user32")]
    unsafe extern "system" {
        fn MessageBoxW(hwnd: *mut c_void, text: *const u16, caption: *const u16, flags: u32)
            -> i32;
    }
    const MB_OK: u32 = 0x0000_0000;
    const MB_ICONERROR: u32 = 0x0000_0010;
    const MB_ICONINFORMATION: u32 = 0x0000_0040;
    let text = message
        .encode_utf16()
        .chain(std::iter::once(0))
        .collect::<Vec<_>>();
    let caption = title
        .encode_utf16()
        .chain(std::iter::once(0))
        .collect::<Vec<_>>();
    let flags = MB_OK
        | if error {
            MB_ICONERROR
        } else {
            MB_ICONINFORMATION
        };
    // SAFETY: both strings are NUL terminated and live for the duration of the call.
    unsafe {
        MessageBoxW(std::ptr::null_mut(), text.as_ptr(), caption.as_ptr(), flags);
    }
}

#[cfg(not(target_os = "windows"))]
fn message_box(title: &str, message: &str, error: bool) {
    let _ = error;
    eprintln!("{title}: {message}");
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::util::{write_text_file, LineEnding, TextEncoding};

    fn write(path: &Path, text: &str) {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).unwrap();
        }
        write_text_file(path, text, LineEnding::Lf, false, TextEncoding::Utf8).unwrap();
    }

    #[test]
    fn copies_tree_and_skips_packaging_staging_dirs() {
        let dir = tempfile::tempdir().unwrap();
        let source = dir.path().join("tools");
        let target = dir.path().join("installed/tools");
        write(&source.join("toolchain.json"), "{}");
        write(&source.join("make/make.exe"), "bin");
        write(&source.join(".make.tmp/make.exe"), "tmp");
        write(&source.join(".compiler.tmp/old.exe"), "tmp");

        let linked = install_tree(&source, &target).unwrap();

        assert!(target.join("toolchain.json").is_file());
        assert!(target.join("make/make.exe").is_file());
        assert!(!target.join(".make.tmp").exists());
        assert!(!target.join(".compiler.tmp").exists());
        let _ = linked;
    }

    #[test]
    fn powershell_quotes_apostrophes() {
        assert_eq!(powershell_quote("C:\\A B\\x.exe"), "'C:\\A B\\x.exe'");
        assert_eq!(powershell_quote("C:\\O'Brien\\x"), "'C:\\O''Brien\\x'");
    }

    #[cfg(target_os = "windows")]
    #[test]
    fn creates_windows_shortcut_file() {
        let dir = tempfile::tempdir().unwrap();
        let exe = dir.path().join("dummy.exe");
        std::fs::write(&exe, b"test").unwrap();
        let shortcut = dir.path().join("test.lnk");
        create_shortcut_file(&shortcut, &exe, dir.path()).unwrap();
        assert!(shortcut.is_file());
    }

    #[test]
    fn detects_paths_inside_install_directory() {
        assert!(path_is_inside(
            Path::new(r"C:\Users\me\AppData\Local\Programs\SimpleEmbedded\simple-embedded.exe"),
            Path::new(r"C:\Users\me\AppData\Local\Programs\SimpleEmbedded")
        ));
        assert!(!path_is_inside(
            Path::new(r"C:\Users\me\AppData\Local\Programs\SimpleEmbedded2\simple-embedded.exe"),
            Path::new(r"C:\Users\me\AppData\Local\Programs\SimpleEmbedded")
        ));
    }
}
