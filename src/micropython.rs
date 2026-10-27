//! Minimal MicroPython raw-REPL client used by the ESP32 run/upload actions.

use std::ffi::OsString;
use std::io::{ErrorKind, Read, Write};
use std::path::{Path, PathBuf};
use std::process::Command;
use std::thread;
use std::time::{Duration, Instant};

use serialport::SerialPort;

const MAX_UPLOAD_BYTES: usize = 128 * 1024;
pub const MAX_READ_BYTES: usize = 128 * 1024;
const UPLOAD_CHUNK_BYTES: usize = 4 * 1024;
const READ_TIMEOUT: Duration = Duration::from_millis(100);

const FILE_LIST_SCRIPT: &str = "\
import os
def walk(d, prefix, depth):
    if depth > 2:
        return
    try:
        for e in os.ilistdir(d):
            name = e[0]
            typ = e[1]
            size = e[3] if len(e) > 3 else 0
            p = prefix + name
            print(p + '\\t' + str(typ) + '\\t' + str(size))
            if (typ & 0x4000) and depth < 2:
                walk(d.rstrip('/') + '/' + name, p + '/', depth + 1)
    except OSError:
        pass
walk('/', '', 0)
";

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BoardFile {
    pub path: String,
    pub name: String,
    pub is_dir: bool,
    pub size: u32,
}

pub fn list_files(port: &str, baud: u32) -> Result<Vec<BoardFile>, String> {
    let mut port = open(port, baud)?;
    drain(&mut port, Duration::from_millis(120));
    enter_raw_repl(&mut port)?;
    let raw = execute_raw(&mut port, FILE_LIST_SCRIPT, baud)?;
    let _ = port.write_all(b"\x02");
    let report = decode_raw(&raw)?;
    if !report.success {
        return Err(format!("读取芯片文件失败：{}", report.error));
    }
    Ok(parse_file_list(&report.output))
}

pub fn read_file(port: &str, baud: u32, path: &str) -> Result<Vec<u8>, String> {
    let script = format!(
        "import ubinascii\n\
p = {}\n\
f = open(p, 'rb')\n\
f.seek(0, 2)\n\
size = f.tell()\n\
f.seek(0)\n\
if size > {MAX_READ_BYTES}:\n\
    print('ERROR:too large')\n\
else:\n\
    print(ubinascii.b2a_base64(f.read()).decode())\n\
f.close()\n",
        python_string(path)
    );
    let mut port = open(port, baud)?;
    drain(&mut port, Duration::from_millis(120));
    enter_raw_repl(&mut port)?;
    let raw = execute_raw(&mut port, &script, baud)?;
    let _ = port.write_all(b"\x02");
    let report = decode_raw(&raw)?;
    if !report.success {
        return Err(format!("读取芯片文件失败：{}", report.error));
    }
    let output = report.output.trim();
    if output.starts_with("ERROR:") {
        return Err(format!(
            "芯片文件超过 {} KB，暂不支持读取",
            MAX_READ_BYTES / 1024
        ));
    }
    let encoded = output
        .chars()
        .filter(|ch| !ch.is_whitespace())
        .collect::<String>();
    let data = base64_decode(&encoded)?;
    if data.len() > MAX_READ_BYTES {
        return Err(format!(
            "芯片文件超过 {} KB，暂不支持读取",
            MAX_READ_BYTES / 1024
        ));
    }
    Ok(data)
}

fn parse_file_list(output: &str) -> Vec<BoardFile> {
    let mut files = output
        .lines()
        .filter_map(|line| {
            let mut parts = line.splitn(3, '\t');
            let path = parts.next()?.trim();
            if path.is_empty() {
                return None;
            }
            let typ = parts
                .next()
                .and_then(|value| value.trim().parse::<u32>().ok())
                .unwrap_or(0);
            let size = parts
                .next()
                .and_then(|value| value.trim().parse::<u32>().ok())
                .unwrap_or(0);
            let name = path.rsplit('/').next().unwrap_or(path).to_owned();
            Some(BoardFile {
                path: path.to_owned(),
                name,
                is_dir: typ & 0x4000 != 0,
                size,
            })
        })
        .collect::<Vec<_>>();
    files.sort_by(|a, b| {
        b.is_dir.cmp(&a.is_dir).then_with(|| {
            a.path
                .to_ascii_lowercase()
                .cmp(&b.path.to_ascii_lowercase())
        })
    });
    files
}

fn base64_decode(value: &str) -> Result<Vec<u8>, String> {
    let mut out = Vec::new();
    let mut buffer = 0u32;
    let mut bits = 0u8;
    for byte in value.bytes() {
        let digit = match byte {
            b'A'..=b'Z' => byte - b'A',
            b'a'..=b'z' => byte - b'a' + 26,
            b'0'..=b'9' => byte - b'0' + 52,
            b'+' => 62,
            b'/' => 63,
            b'=' => break,
            b'\r' | b'\n' | b' ' | b'\t' => continue,
            _ => return Err("芯片文件返回了无效的 base64 数据".to_owned()),
        };
        buffer = (buffer << 6) | u32::from(digit);
        bits += 6;
        if bits >= 8 {
            bits -= 8;
            out.push((buffer >> bits) as u8);
        }
    }
    Ok(out)
}

#[derive(Clone, Debug, Default)]
pub struct Report {
    pub output: String,
    pub error: String,
    pub success: bool,
}

pub fn run(port: &str, baud: u32, code: &str) -> Result<Report, String> {
    let upload = upload(port, baud, "main.py", code)?;
    if !upload.success {
        return Ok(upload);
    }
    let reset = soft_reset(port, baud)?;
    let mut report = Report {
        output: join_output(&upload.output, &reset.output),
        error: join_output(&upload.error, &reset.error),
        success: reset.success,
    };
    if report.output.is_empty() && report.error.is_empty() {
        report.output = "已上传 main.py 并发送软复位".to_owned();
    }
    Ok(report)
}

pub fn upload(port: &str, baud: u32, file_name: &str, code: &str) -> Result<Report, String> {
    if code.len() > MAX_UPLOAD_BYTES {
        return Err(format!(
            "文件超过 {} KB 串口上传上限",
            MAX_UPLOAD_BYTES / 1024
        ));
    }
    let mut port = open(port, baud)?;
    drain(&mut port, Duration::from_millis(120));
    enter_raw_repl(&mut port)?;

    let open = open_script(file_name);
    let raw = execute_raw(&mut port, &open, baud)?;
    let opened = decode_raw(&raw)?;
    if !opened.success {
        let _ = port.write_all(b"\x02");
        return Ok(opened);
    }

    for chunk in code.as_bytes().chunks(UPLOAD_CHUNK_BYTES) {
        let script = chunk_script(chunk);
        let raw = execute_raw(&mut port, &script, baud)?;
        let step = decode_raw(&raw)?;
        if !step.success {
            let _ = port.write_all(b"\x02");
            return Ok(step);
        }
    }

    let raw = execute_raw(&mut port, "f.close()\nprint('uploaded')\n", baud)?;
    let _ = port.write_all(b"\x02");
    decode_raw(&raw)
}

pub fn soft_reset(port: &str, baud: u32) -> Result<Report, String> {
    let mut port = open(port, baud)?;
    drain(&mut port, Duration::from_millis(120));
    port.write_all(b"\x02\x03\x03")
        .map_err(|err| format!("发送串口中断失败：{err}"))?;
    thread::sleep(Duration::from_millis(120));
    drain(&mut port, Duration::from_millis(120));
    port.write_all(b"\r\x04")
        .map_err(|err| format!("发送软复位失败：{err}"))?;
    let output = read_for(&mut port, Duration::from_millis(1600))?;
    let output = clean_text(&output);
    let failed = output.contains("Traceback (most recent call last)");
    Ok(Report {
        output,
        error: if failed {
            "运行输出包含 Traceback".to_owned()
        } else {
            String::new()
        },
        success: !failed,
    })
}

fn open(port: &str, baud: u32) -> Result<Box<dyn SerialPort>, String> {
    serialport::new(port, baud)
        .timeout(READ_TIMEOUT)
        .open()
        .map_err(|err| format!("打开串口 {port} 失败：{err}"))
}

fn enter_raw_repl(port: &mut Box<dyn SerialPort>) -> Result<(), String> {
    // Ctrl-C stops a running script / interrupts the friendly REPL.
    port.write_all(b"\r\x03")
        .map_err(|err| format!("发送串口中断失败：{err}"))?;
    thread::sleep(Duration::from_millis(80));
    port.write_all(b"\x03")
        .map_err(|err| format!("发送串口中断失败：{err}"))?;
    thread::sleep(Duration::from_millis(80));
    drain(port, Duration::from_millis(160));

    // Ctrl-A enters the raw REPL. Wait for the banner and the following `>`
    // prompt; the friendly `>>>` prompt may still be in the buffer.
    port.write_all(b"\x01")
        .map_err(|err| format!("进入 MicroPython raw REPL 失败：{err}"))?;
    let deadline = Instant::now() + Duration::from_secs(3);
    let mut data = Vec::new();
    let mut buf = [0u8; 1024];
    let mut banner_end = None;
    while Instant::now() < deadline {
        match port.read(&mut buf) {
            Ok(read) if read > 0 => {
                data.extend_from_slice(&buf[..read]);
                if banner_end.is_none() {
                    if let Some(index) = find_bytes(&data, b"raw REPL") {
                        banner_end = Some(index + b"raw REPL".len());
                    }
                }
                if let Some(start) = banner_end {
                    if data[start..].contains(&b'>') {
                        return Ok(());
                    }
                }
            }
            Ok(_) => {}
            Err(err) if matches!(err.kind(), ErrorKind::TimedOut | ErrorKind::WouldBlock) => {}
            Err(err) => return Err(format!("读取串口失败：{err}")),
        }
    }
    Err(format!(
        "未进入 MicroPython raw REPL；已收到：{}",
        clean_text(&String::from_utf8_lossy(&data))
    ))
}

fn find_bytes(haystack: &[u8], needle: &[u8]) -> Option<usize> {
    haystack
        .windows(needle.len())
        .position(|window| window == needle)
}

fn execute_raw(port: &mut Box<dyn SerialPort>, script: &str, baud: u32) -> Result<Vec<u8>, String> {
    port.write_all(script.as_bytes())
        .map_err(|err| format!("发送代码失败：{err}"))?;
    port.write_all(b"\x04")
        .map_err(|err| format!("提交代码失败：{err}"))?;
    let wire_ms = script.len() as u64 * 10_000 / u64::from(baud.max(1_200));
    let timeout = Duration::from_millis(3_000 + wire_ms.min(30_000));
    read_until(port, b"\x04>", timeout).map_err(|err| {
        let _ = port.write_all(b"\x03");
        format!("等待 MicroPython 返回超时：{err}")
    })
}

fn read_until(
    port: &mut Box<dyn SerialPort>,
    needle: &[u8],
    timeout: Duration,
) -> Result<Vec<u8>, String> {
    let deadline = Instant::now() + timeout;
    let mut data = Vec::new();
    let mut buf = [0u8; 1024];
    while Instant::now() < deadline {
        match port.read(&mut buf) {
            Ok(read) if read > 0 => {
                data.extend_from_slice(&buf[..read]);
                if data.windows(needle.len()).any(|window| window == needle) {
                    return Ok(data);
                }
            }
            Ok(_) => {}
            Err(err) if matches!(err.kind(), ErrorKind::TimedOut | ErrorKind::WouldBlock) => {}
            Err(err) => return Err(format!("读取串口失败：{err}")),
        }
    }
    Err(format!(
        "未收到结束标记；已收到：{}",
        clean_text(&String::from_utf8_lossy(&data))
    ))
}

fn read_for(port: &mut Box<dyn SerialPort>, duration: Duration) -> Result<String, String> {
    let deadline = Instant::now() + duration;
    let mut data = Vec::new();
    let mut buf = [0u8; 1024];
    while Instant::now() < deadline {
        match port.read(&mut buf) {
            Ok(read) if read > 0 => data.extend_from_slice(&buf[..read]),
            Ok(_) => {}
            Err(err) if matches!(err.kind(), ErrorKind::TimedOut | ErrorKind::WouldBlock) => {}
            Err(err) => return Err(format!("读取串口失败：{err}")),
        }
    }
    Ok(String::from_utf8_lossy(&data).into_owned())
}

fn drain(port: &mut Box<dyn SerialPort>, duration: Duration) {
    let deadline = Instant::now() + duration;
    let mut buf = [0u8; 1024];
    while Instant::now() < deadline {
        match port.read(&mut buf) {
            Ok(read) if read > 0 => {}
            Ok(_) => {}
            Err(err) if matches!(err.kind(), ErrorKind::TimedOut | ErrorKind::WouldBlock) => {
                thread::sleep(Duration::from_millis(10));
            }
            Err(_) => return,
        }
    }
}

fn open_script(file_name: &str) -> String {
    format!(
        "try:\n    import ubinascii as binascii\nexcept ImportError:\n    import binascii\n\
f = open({}, 'wb')\n",
        python_string(file_name)
    )
}

fn chunk_script(chunk: &[u8]) -> String {
    let encoded = base64_encode(chunk);
    format!("f.write(binascii.a2b_base64(\"{encoded}\"))\n")
}

fn python_string(value: &str) -> String {
    let mut out = String::from("\"");
    for ch in value.chars() {
        match ch {
            '\\' => out.push_str("\\\\"),
            '"' => out.push_str("\\\""),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            ch if (ch as u32) < 0x20 => out.push_str(&format!("\\x{:02x}", ch as u32)),
            ch => out.push(ch),
        }
    }
    out.push('"');
    out
}

fn decode_raw(raw: &[u8]) -> Result<Report, String> {
    let mut text = String::from_utf8_lossy(raw).into_owned();
    if let Some(index) = text.rfind("\x04>") {
        text.truncate(index);
    }
    if let Some(rest) = text.strip_prefix("OK") {
        text = rest.to_owned();
    }
    let (output, error) = match text.find('\x04') {
        Some(index) => (&text[..index], &text[index + 1..]),
        None => (text.as_str(), ""),
    };
    let output = clean_text(output);
    let error = clean_text(error);
    Ok(Report {
        success: error.is_empty(),
        output,
        error,
    })
}

fn clean_text(text: &str) -> String {
    text.chars()
        .filter(|ch| !ch.is_control() || matches!(ch, '\n' | '\r' | '\t'))
        .collect::<String>()
        .replace("\r\n", "\n")
        .replace('\r', "\n")
        .lines()
        .map(str::trim_end)
        .collect::<Vec<_>>()
        .join("\n")
        .trim()
        .to_owned()
}

fn join_output(a: &str, b: &str) -> String {
    match (a.trim(), b.trim()) {
        ("", "") => String::new(),
        ("", b) => b.to_owned(),
        (a, "") => a.to_owned(),
        (a, b) => format!("{a}\n{b}"),
    }
}

pub fn detect_esptool() -> Option<String> {
    for candidate in ["esptool", "esptool.exe", "esptool.py"] {
        if crate::config::command_available(candidate) {
            return Some(candidate.to_owned());
        }
    }
    common_esptool_paths()
        .into_iter()
        .find(|path| path.is_file())
        .map(|path| crate::util::display_path(&path))
}

#[cfg(windows)]
fn common_esptool_paths() -> Vec<PathBuf> {
    let mut out = Vec::new();
    if let Some(local) = std::env::var_os("LOCALAPPDATA") {
        scan_python_installs(
            &PathBuf::from(local).join("Programs").join("Python"),
            &mut out,
        );
    }
    if let Some(roaming) = std::env::var_os("APPDATA") {
        scan_python_installs(&PathBuf::from(roaming).join("Python"), &mut out);
    }
    out
}

#[cfg(not(windows))]
fn common_esptool_paths() -> Vec<PathBuf> {
    Vec::new()
}

#[cfg(windows)]
fn scan_python_installs(base: &Path, out: &mut Vec<PathBuf>) {
    let Ok(entries) = std::fs::read_dir(base) else {
        return;
    };
    for entry in entries.flatten() {
        let candidate = entry.path().join("Scripts").join("esptool.exe");
        if candidate.is_file() {
            out.push(candidate);
        }
    }
}

fn esptool_invocation(esptool: &str) -> Result<(String, Vec<OsString>), String> {
    let lower = esptool.to_ascii_lowercase();
    if !lower.ends_with(".py") {
        return Ok((esptool.to_owned(), Vec::new()));
    }
    let python = if crate::config::command_available("python") {
        "python"
    } else if crate::config::command_available("py") {
        "py"
    } else {
        return Err("系统未找到 python 或 py，无法运行 esptool.py".to_owned());
    };
    let mut prefix = Vec::new();
    if esptool.contains('\\') || esptool.contains('/') {
        prefix.push(OsString::from(esptool));
    } else {
        // `esptool.py` on PATH usually means the pip module is installed.
        prefix.push(OsString::from("-m"));
        prefix.push(OsString::from("esptool"));
    }
    Ok((python.to_owned(), prefix))
}

fn esptool_major_version(program: &str, prefix: &[OsString]) -> Option<u32> {
    let mut command = Command::new(program);
    command.args(prefix);
    command.arg("version");
    crate::util::hide_console(&mut command);
    let output = command.output().ok()?;
    let text = format!(
        "{}\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    parse_esptool_major(&text)
}

fn parse_esptool_major(text: &str) -> Option<u32> {
    for line in text.lines() {
        let lower = line.to_ascii_lowercase();
        let Some(index) = lower.find("esptool") else {
            continue;
        };
        let rest = &line[index + "esptool".len()..];
        let Some(version) = rest.find('v') else {
            continue;
        };
        let digits: String = rest[version + 1..]
            .chars()
            .take_while(|ch| ch.is_ascii_digit())
            .collect();
        if let Ok(major) = digits.parse() {
            return Some(major);
        }
    }
    None
}

fn erase_command(major: Option<u32>) -> &'static str {
    if major.is_some_and(|major| major >= 5) {
        "erase-flash"
    } else {
        "erase_flash"
    }
}

fn firmware_write_args(major: Option<u32>, verify: bool, address: &str, file: &str) -> Vec<String> {
    let mut args = vec![if major.is_some_and(|major| major >= 5) {
        "write-flash".to_owned()
    } else {
        "write_flash".to_owned()
    }];
    // esptool 5 verifies automatically and no longer accepts --verify/--no-verify;
    // esptool 4 verifies by default, so only opt out explicitly.
    if !verify && !major.is_some_and(|major| major >= 5) {
        args.push("--no-verify".to_owned());
    }
    args.push("-z".to_owned());
    args.push(address.to_owned());
    args.push(file.to_owned());
    args
}

pub fn flash_firmware(
    port: &str,
    baud: u32,
    config: &crate::config::FirmwareConfig,
) -> Result<Report, String> {
    let file = config.file.trim();
    if file.is_empty() {
        return Err("请选择 MicroPython 固件 .bin 文件".to_owned());
    }
    if !Path::new(file).is_file() {
        return Err(format!("固件文件不存在：{file}"));
    }
    if config.esptool.trim().is_empty() {
        return Err("请填写 esptool 路径".to_owned());
    }
    if config.chip.trim().is_empty() || config.address.trim().is_empty() {
        return Err("请选择芯片并填写烧录地址".to_owned());
    }
    let (program, prefix) = esptool_invocation(config.esptool.trim())?;
    let major = esptool_major_version(&program, &prefix);
    let base = vec![
        "--chip".to_owned(),
        config.chip.trim().to_owned(),
        "--port".to_owned(),
        port.to_owned(),
        "--baud".to_owned(),
        baud.to_string(),
    ];
    let mut output = String::new();
    if major.is_some_and(|major| major >= 5) && !config.verify {
        output.push_str("esptool 5 会自动校验写入内容，忽略“写入校验”开关。\n");
    }

    if config.erase {
        let mut args = base.clone();
        args.push(erase_command(major).to_owned());
        let (success, text) = run_esptool(&program, &prefix, &args)?;
        output = join_output(&output, &text);
        if !success {
            return Ok(Report {
                output,
                error: "擦除 ESP32 Flash 失败".to_owned(),
                success: false,
            });
        }
    }

    let mut args = base.clone();
    args.extend(firmware_write_args(
        major,
        config.verify,
        config.address.trim(),
        file,
    ));
    let (success, text) = run_esptool(&program, &prefix, &args)?;
    output = join_output(&output, &text);
    Ok(Report {
        error: if success {
            String::new()
        } else if text.trim().is_empty() {
            "esptool 烧录失败".to_owned()
        } else {
            text.lines().last().unwrap_or("esptool 烧录失败").to_owned()
        },
        output,
        success,
    })
}

fn run_esptool(
    program: &str,
    prefix: &[OsString],
    args: &[String],
) -> Result<(bool, String), String> {
    let mut command = Command::new(program);
    command.args(prefix);
    command.args(args);
    crate::util::hide_console(&mut command);
    let output = command
        .output()
        .map_err(|err| format!("启动 esptool 失败：{err}"))?;
    let mut text = String::from_utf8_lossy(&output.stdout).into_owned();
    let stderr = String::from_utf8_lossy(&output.stderr).into_owned();
    if !stderr.trim().is_empty() {
        if !text.trim().is_empty() {
            text.push('\n');
        }
        text.push_str(&stderr);
    }
    Ok((output.status.success(), clean_text(&text)))
}

fn base64_encode(data: &[u8]) -> String {
    const TABLE: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::with_capacity(data.len().div_ceil(3) * 4);
    for chunk in data.chunks(3) {
        let first = chunk[0];
        out.push(TABLE[(first >> 2) as usize] as char);
        if chunk.len() == 1 {
            out.push(TABLE[((first & 0x03) << 4) as usize] as char);
            out.push_str("==");
            continue;
        }
        let second = chunk[1];
        out.push(TABLE[(((first & 0x03) << 4) | (second >> 4)) as usize] as char);
        if chunk.len() == 2 {
            out.push(TABLE[((second & 0x0f) << 2) as usize] as char);
            out.push('=');
            continue;
        }
        let third = chunk[2];
        out.push(TABLE[(((second & 0x0f) << 2) | (third >> 6)) as usize] as char);
        out.push(TABLE[(third & 0x3f) as usize] as char);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn encodes_base64_padding() {
        assert_eq!(base64_encode(b"hello"), "aGVsbG8=");
        assert_eq!(base64_encode(b"hi"), "aGk=");
        assert_eq!(base64_encode(b"a"), "YQ==");
        assert_eq!(base64_encode(b"abc"), "YWJj");
    }

    #[test]
    fn decodes_base64_output_from_the_board() {
        for data in [
            b"".as_slice(),
            b"hello".as_slice(),
            b"\0\xffbinary".as_slice(),
        ] {
            let encoded = base64_encode(data);
            assert_eq!(base64_decode(&encoded).unwrap(), data);
        }
        assert!(base64_decode("not base64!").is_err());
    }

    #[test]
    fn parses_board_file_list_with_directories_and_sizes() {
        let files =
            parse_file_list("main.py\t32768\t123\nlib\t16384\t0\nlib/helper.py\t32768\t45\n");
        assert_eq!(files.len(), 3);
        assert_eq!(files[0].path, "lib");
        assert!(files[0].is_dir);
        assert_eq!(files[1].path, "lib/helper.py");
        assert_eq!(files[1].size, 45);
        assert_eq!(files[2].path, "main.py");
        assert_eq!(files[2].size, 123);
    }

    #[test]
    fn builds_open_and_chunk_scripts_with_escaped_name_and_base64() {
        let open = open_script("main.py");
        assert!(open.contains("a2b_base64") || open.contains("import ubinascii"));
        assert!(open.contains("\"main.py\""));
        let chunk = chunk_script(b"print('hello')\n");
        assert!(chunk.contains("a2b_base64"));
        assert!(chunk.contains("cHJpbnQoJ2hlbGxvJykK"));
    }

    #[test]
    fn decodes_success_and_error_raw_repl_responses() {
        let success = decode_raw(b"OK\r\nuploaded 12\r\n\x04\x04>").unwrap();
        assert!(success.success);
        assert!(success.output.contains("uploaded 12"));
        assert!(success.error.is_empty());

        let failure =
            decode_raw(b"OK\x04Traceback (most recent call last):\nOSError\x04>").unwrap();
        assert!(!failure.success);
        assert!(failure.error.contains("Traceback"));
    }

    #[test]
    fn escapes_python_strings() {
        assert_eq!(python_string("main.py"), "\"main.py\"");
        assert_eq!(python_string("a\\b\"c"), "\"a\\\\b\\\"c\"");
    }

    #[test]
    fn rejects_files_over_the_upload_limit_before_opening_the_port() {
        let code = "x".repeat(MAX_UPLOAD_BYTES + 1);
        let error = upload("THIS_PORT_DOES_NOT_EXIST", 115_200, "main.py", &code)
            .expect_err("oversized files must be rejected");
        assert!(error.contains("128 KB"));
    }

    #[test]
    fn missing_serial_port_reports_an_open_error() {
        let error =
            soft_reset("THIS_PORT_DOES_NOT_EXIST", 115_200).expect_err("invalid port must fail");
        assert!(error.contains("打开串口") || error.contains("does not exist"));
    }

    #[test]
    fn firmware_flash_requires_an_existing_firmware_file() {
        let config = crate::config::FirmwareConfig {
            file: "definitely-missing-firmware.bin".to_owned(),
            ..crate::config::FirmwareConfig::default()
        };
        let error = flash_firmware("COM_DOES_NOT_EXIST", 115_200, &config).unwrap_err();
        assert!(error.contains("固件文件不存在"));
    }

    #[test]
    fn parses_esptool_major_versions() {
        assert_eq!(parse_esptool_major("esptool.py v4.8.1"), Some(4));
        assert_eq!(parse_esptool_major("esptool v5.6.0"), Some(5));
        assert_eq!(
            parse_esptool_major("Detecting chip type...\nesptool.py v3.3"),
            Some(3)
        );
        assert_eq!(parse_esptool_major("no version here"), None);
    }

    #[test]
    fn firmware_commands_follow_the_esptool_major_version() {
        assert_eq!(erase_command(Some(4)), "erase_flash");
        assert_eq!(erase_command(Some(5)), "erase-flash");
        assert_eq!(erase_command(None), "erase_flash");

        let legacy = firmware_write_args(Some(4), false, "0x1000", "fw.bin");
        assert_eq!(legacy[0], "write_flash");
        assert!(legacy.contains(&"--no-verify".to_owned()));
        let legacy_verify = firmware_write_args(Some(4), true, "0x1000", "fw.bin");
        assert!(!legacy_verify.contains(&"--no-verify".to_owned()));
        assert!(!legacy_verify.contains(&"--verify".to_owned()));

        let modern = firmware_write_args(Some(5), false, "0x0", "fw.bin");
        assert_eq!(modern[0], "write-flash");
        assert!(!modern.contains(&"--no-verify".to_owned()));
        assert_eq!(modern[modern.len() - 2], "0x0");
        assert_eq!(modern[modern.len() - 1], "fw.bin");
    }
}
