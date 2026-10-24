//! Small cross-platform helpers shared by the application.

use std::ffi::OsStr;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::Duration;

pub const MAX_FILE_BYTES: u64 = 16 * 1024 * 1024;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LineEnding {
    Lf,
    Crlf,
}

impl LineEnding {
    pub fn label(self) -> &'static str {
        match self {
            Self::Lf => "LF",
            Self::Crlf => "CRLF",
        }
    }
}

pub struct LoadedText {
    pub text: String,
    pub line_ending: LineEnding,
    pub bom: bool,
}

pub fn read_text_file(path: &Path) -> Result<LoadedText, String> {
    let metadata = std::fs::metadata(path).map_err(|err| format!("无法读取文件信息：{err}"))?;
    if metadata.len() > MAX_FILE_BYTES {
        return Err(format!(
            "文件过大（{} MB），本编辑器最多打开 {} MB 的文本文件",
            metadata.len() / 1024 / 1024,
            MAX_FILE_BYTES / 1024 / 1024
        ));
    }

    let bytes = std::fs::read(path).map_err(|err| format!("无法读取文件：{err}"))?;
    if bytes.iter().take(8192).any(|byte| *byte == 0) {
        return Err("这是二进制文件，无法以文本方式打开".to_owned());
    }

    let (bom, bytes) = if bytes.starts_with(&[0xEF, 0xBB, 0xBF]) {
        (true, &bytes[3..])
    } else {
        (false, bytes.as_slice())
    };

    let raw =
        std::str::from_utf8(bytes).map_err(|_| "文件不是 UTF-8 编码，暂不支持该编码".to_owned())?;

    let had_crlf = raw.contains("\r\n");
    let text = if raw.contains('\r') {
        raw.replace("\r\n", "\n").replace('\r', "\n")
    } else {
        raw.to_owned()
    };

    Ok(LoadedText {
        text,
        line_ending: if had_crlf {
            LineEnding::Crlf
        } else {
            LineEnding::Lf
        },
        bom,
    })
}

pub fn write_text_file(
    path: &Path,
    text: &str,
    line_ending: LineEnding,
    bom: bool,
) -> Result<(), String> {
    let mut bytes = Vec::with_capacity(text.len() + 3);
    if bom {
        bytes.extend_from_slice(&[0xEF, 0xBB, 0xBF]);
    }

    match line_ending {
        LineEnding::Lf => bytes.extend_from_slice(text.as_bytes()),
        LineEnding::Crlf => {
            for part in text.split_inclusive('\n') {
                if let Some(line) = part.strip_suffix('\n') {
                    bytes.extend_from_slice(line.as_bytes());
                    bytes.extend_from_slice(b"\r\n");
                } else {
                    bytes.extend_from_slice(part.as_bytes());
                }
            }
        }
    }

    std::fs::write(path, bytes).map_err(|err| format!("保存失败：{err}"))
}

pub fn canonical_project(path: &Path) -> Result<PathBuf, String> {
    let canonical = std::fs::canonicalize(path)
        .map_err(|err| format!("无法打开目录 {}：{err}", path.display()))?;
    if !canonical.is_dir() {
        return Err(format!("{} 不是文件夹", canonical.display()));
    }
    Ok(canonical)
}

pub fn display_path(path: &Path) -> String {
    let value = path.to_string_lossy();
    if let Some(rest) = value.strip_prefix(r"\\?\UNC\") {
        format!(r"\\{rest}")
    } else if let Some(rest) = value.strip_prefix(r"\\?\") {
        rest.to_owned()
    } else {
        value.into_owned()
    }
}

pub fn relative_display(root: &Path, path: &Path) -> String {
    path.strip_prefix(root)
        .unwrap_or(path)
        .to_string_lossy()
        .replace('\\', "/")
}

pub fn file_name(path: &Path) -> String {
    path.file_name()
        .unwrap_or_else(|| OsStr::new("未命名"))
        .to_string_lossy()
        .into_owned()
}

pub fn format_duration(duration: Duration) -> String {
    let seconds = duration.as_secs_f64();
    if seconds < 60.0 {
        format!("{seconds:.1}s")
    } else {
        let minutes = (seconds / 60.0).floor() as u64;
        let rest = seconds - minutes as f64 * 60.0;
        format!("{minutes}m {rest:.0}s")
    }
}

pub fn shell_quote_posix(value: &str) -> String {
    format!("'{}'", value.replace('\'', "'\\''"))
}

pub fn open_in_file_manager(path: &Path) {
    #[cfg(target_os = "windows")]
    {
        let mut command = Command::new("explorer.exe");
        if path.is_file() {
            command.arg(format!("/select,{}", path.display()));
        } else {
            command.arg(path);
        }
        let _ = command.spawn();
    }

    #[cfg(target_os = "macos")]
    {
        let _ = Command::new("open").arg("-R").arg(path).spawn();
    }

    #[cfg(all(unix, not(target_os = "macos")))]
    {
        let target = if path.is_file() {
            path.parent().unwrap_or(path)
        } else {
            path
        };
        let _ = Command::new("xdg-open").arg(target).spawn();
    }
}

pub fn open_in_vscode(path: &Path) {
    #[cfg(target_os = "windows")]
    {
        let escaped = path.to_string_lossy().replace('"', "");
        let command = format!("code --reuse-window \"{escaped}\"");
        let mut cmd = Command::new("cmd");
        cmd.args(["/C", &command]);
        hide_console(&mut cmd);
        let _ = cmd.spawn();
    }

    #[cfg(not(target_os = "windows"))]
    {
        let _ = Command::new("code").arg("--reuse-window").arg(path).spawn();
    }
}

pub fn hide_console(command: &mut Command) {
    #[cfg(target_os = "windows")]
    {
        use std::os::windows::process::CommandExt;
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        command.creation_flags(CREATE_NO_WINDOW);
    }
    #[cfg(not(target_os = "windows"))]
    {
        let _ = command;
    }
}

/// Converts a character index (the unit used by egui's TextEdit) into (line, column),
/// both 1-based. Kept O(n) but used only once per frame for the active document.
pub fn char_index_at_line(text: &str, line: usize) -> usize {
    if line <= 1 {
        return 0;
    }
    let mut index = 0usize;
    let mut current = 1usize;
    for ch in text.chars() {
        if current == line {
            break;
        }
        index += 1;
        if ch == '\n' {
            current += 1;
        }
    }
    index
}

pub fn line_column_at_char(text: &str, char_index: usize) -> (usize, usize) {
    let mut line = 1usize;
    let mut column = 1usize;
    for (seen, ch) in text.chars().enumerate() {
        if seen >= char_index {
            break;
        }
        if ch == '\n' {
            line += 1;
            column = 1;
        } else {
            column += 1;
        }
    }
    (line, column)
}

pub fn project_key(path: &Path) -> String {
    let value = path.to_string_lossy().replace('/', "\\");
    if cfg!(target_os = "windows") {
        value.to_lowercase()
    } else {
        value
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn line_columns_are_one_based() {
        assert_eq!(line_column_at_char("abc", 0), (1, 1));
        assert_eq!(line_column_at_char("abc\ndef", 4), (2, 1));
        assert_eq!(line_column_at_char("abc\ndef", 6), (2, 3));
    }

    #[test]
    fn char_index_at_line_starts_at_line_beginning() {
        let text = "abc\ndef\nghi";
        assert_eq!(char_index_at_line(text, 1), 0);
        assert_eq!(char_index_at_line(text, 2), 4);
        assert_eq!(char_index_at_line(text, 3), 8);
        assert_eq!(char_index_at_line(text, 99), 11);
    }

    #[test]
    fn crlf_roundtrip() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("sample.c");
        std::fs::write(&path, b"int main() {\r\n  return 0;\r\n}\r\n").unwrap();
        let loaded = read_text_file(&path).unwrap();
        assert_eq!(loaded.text, "int main() {\n  return 0;\n}\n");
        assert_eq!(loaded.line_ending, LineEnding::Crlf);
        write_text_file(&path, &loaded.text, loaded.line_ending, loaded.bom).unwrap();
        assert_eq!(
            std::fs::read(&path).unwrap(),
            b"int main() {\r\n  return 0;\r\n}\r\n"
        );
    }

    #[test]
    fn posix_quote_handles_quotes() {
        assert_eq!(shell_quote_posix("a'b"), "'a'\\''b'");
    }

    #[test]
    fn strips_windows_verbatim_prefix_for_display() {
        assert_eq!(
            display_path(Path::new(r"\\?\C:\firmware\main.c")),
            r"C:\firmware\main.c"
        );
        assert_eq!(
            display_path(Path::new(r"\\?\UNC\server\share\main.c")),
            r"\\server\share\main.c"
        );
    }
}
