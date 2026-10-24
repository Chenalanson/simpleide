//! clang-format integration used by the editor's format action and save hook.

use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::OnceLock;

const COMMON_WINDOWS_PATHS: &[&str] = &[
    r"C:\Program Files\LLVM\bin\clang-format.exe",
    r"C:\Program Files (x86)\LLVM\bin\clang-format.exe",
    r"C:\msys64\mingw64\bin\clang-format.exe",
    r"C:\msys64\ucrt64\bin\clang-format.exe",
];

pub fn is_c_family(path: &Path) -> bool {
    matches!(
        path.extension()
            .map(|extension| extension.to_string_lossy().to_lowercase())
            .as_deref(),
        Some("c")
            | Some("h")
            | Some("cc")
            | Some("cpp")
            | Some("cxx")
            | Some("hh")
            | Some("hpp")
            | Some("hxx")
            | Some("ino")
    )
}

/// Format a C/C++ source with clang-format. `configured` may be empty to
/// auto-detect the executable from PATH or common LLVM install locations.
pub fn format_source(path: &Path, text: &str, configured: &str) -> Result<String, String> {
    let program = resolve_program(configured)?;
    let mut command = Command::new(&program);
    command
        .arg("--style=file")
        .arg(format!("-assume-filename={}", path.display()))
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    crate::util::hide_console(&mut command);
    let mut child = command
        .spawn()
        .map_err(|err| format!("无法启动 clang-format（{}）：{err}", program.display()))?;
    if let Some(mut stdin) = child.stdin.take() {
        stdin
            .write_all(text.as_bytes())
            .map_err(|err| format!("写入 clang-format 失败：{err}"))?;
    }
    let output = child
        .wait_with_output()
        .map_err(|err| format!("等待 clang-format 失败：{err}"))?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr).trim().to_owned();
        let message = if stderr.is_empty() {
            format!("退出码 {:?}", output.status.code())
        } else {
            stderr
        };
        return Err(format!("clang-format 失败：{message}"));
    }
    let formatted =
        String::from_utf8(output.stdout).map_err(|_| "clang-format 输出不是 UTF-8".to_owned())?;
    if formatted.is_empty() && !text.is_empty() {
        return Err("clang-format 返回了空内容".to_owned());
    }
    Ok(formatted)
}

fn resolve_program(configured: &str) -> Result<PathBuf, String> {
    let configured = configured.trim();
    if !configured.is_empty() {
        return Ok(PathBuf::from(configured));
    }
    static CACHE: OnceLock<Option<PathBuf>> = OnceLock::new();
    CACHE.get_or_init(detect_program).clone().ok_or_else(|| {
        "未找到 clang-format，请安装 LLVM 或在设置中指定 clang-format 路径".to_owned()
    })
}

fn detect_program() -> Option<PathBuf> {
    for path in COMMON_WINDOWS_PATHS {
        let path = PathBuf::from(path);
        if path.is_file() {
            return Some(path);
        }
    }
    let mut command = Command::new("clang-format");
    command
        .arg("--version")
        .stdout(Stdio::null())
        .stderr(Stdio::null());
    crate::util::hide_console(&mut command);
    if command.status().is_ok_and(|status| status.success()) {
        Some(PathBuf::from("clang-format"))
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn recognizes_c_family_extensions() {
        for name in [
            "main.c",
            "main.h",
            "main.cpp",
            "main.hpp",
            "main.cc",
            "sketch.ino",
        ] {
            assert!(is_c_family(Path::new(name)), "{name}");
        }
        for name in ["main.rs", "main.asm", "Makefile", "main.c.bak"] {
            assert!(!is_c_family(Path::new(name)), "{name}");
        }
    }

    #[test]
    fn missing_configured_program_reports_error() {
        let result = format_source(
            Path::new("sample.c"),
            "int main(void){return 0;}",
            "this-program-does-not-exist-clang-format",
        );
        assert!(result.is_err());
    }
}
