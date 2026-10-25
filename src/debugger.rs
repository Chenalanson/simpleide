//! Minimal GDB console integration (plain CLI, no MI parsing).
//!
//! Host targets use plain `gdb`. Bare-metal ELF targets reuse the project's
//! OpenOCD / pyOCD configuration: the application starts a GDB server, then
//! connects `arm-none-eabi-gdb` to it and loads the program.

use std::io::{BufRead, BufReader, Read, Write};
use std::net::{SocketAddr, TcpListener, TcpStream};
use std::path::{Path, PathBuf};
use std::process::{Child, ChildStdin, Command, Stdio};
use std::sync::mpsc::{channel, Receiver, Sender};
use std::time::{Duration, Instant};

use crate::build::{terminate_process, RunSpec};

const MAX_LOG_LINES: usize = 4000;
const SERVER_START_TIMEOUT: Duration = Duration::from_secs(8);

pub enum DebugEvent {
    Line(String),
    Watch(String),
    Preg(String),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DebugTargetKind {
    Host,
    Embedded,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ServerKind {
    OpenOcd,
    PyOcd,
}

impl ServerKind {
    fn label(self) -> &'static str {
        match self {
            Self::OpenOcd => "OpenOCD",
            Self::PyOcd => "pyOCD",
        }
    }
}

pub struct DebugSession {
    child: Child,
    stdin: Option<ChildStdin>,
    rx: Receiver<DebugEvent>,
    pub log: Vec<String>,
    /// Lines captured between Live Watch markers. Hidden from the normal log.
    pub watch_output: Vec<String>,
    /// Lines captured between peripheral-register markers.
    pub preg_output: Vec<String>,
    pub started: bool,
    server: Option<Child>,
}

impl DebugSession {
    /// Start a host gdb session for a PE/ELF executable.
    pub fn start(program: &Path, gdb: &Path) -> Result<Self, String> {
        let (tx, rx) = channel();
        Self::spawn_gdb(program, gdb, tx, rx)
    }

    /// Start an embedded session backed by OpenOCD / pyOCD.
    pub fn start_embedded(
        program: &Path,
        gdb: &Path,
        server_spec: RunSpec,
        port: u16,
        kind: ServerKind,
    ) -> Result<Self, String> {
        let (tx, rx) = channel();
        let mut session = Self::spawn_gdb(program, gdb, tx.clone(), rx)?;
        let mut server = match spawn_server(&server_spec, kind, tx) {
            Ok(server) => server,
            Err(err) => {
                session.stop();
                return Err(err);
            }
        };
        if let Err(err) = wait_for_server(port, SERVER_START_TIMEOUT, &mut server, &session.rx) {
            terminate_process(&mut server);
            session.poll();
            let details = if session.log.is_empty() {
                String::new()
            } else {
                format!("\n{}", session.log.join("\n"))
            };
            session.stop();
            return Err(format!("{err}{details}"));
        }
        session.server = Some(server);
        session.send(&format!("target extended-remote 127.0.0.1:{port}"));
        match kind {
            ServerKind::OpenOcd => session.send("monitor reset halt"),
            ServerKind::PyOcd => {
                session.send("monitor reset");
                session.send("monitor halt");
            }
        }
        session.send("load");
        match kind {
            ServerKind::OpenOcd => session.send("monitor reset halt"),
            ServerKind::PyOcd => {
                session.send("monitor reset");
                session.send("monitor halt");
            }
        }
        session.started = true;
        Ok(session)
    }

    fn spawn_gdb(
        program: &Path,
        gdb: &Path,
        tx: Sender<DebugEvent>,
        rx: Receiver<DebugEvent>,
    ) -> Result<Self, String> {
        let mut command = Command::new(gdb);
        command
            .arg("--quiet")
            .arg("--nx")
            .arg("--args")
            .arg(crate::util::display_path(program))
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        crate::util::hide_console(&mut command);
        let mut child = command
            .spawn()
            .map_err(|err| format!("无法启动 gdb（{}）：{err}", gdb.display()))?;
        let stdin = child.stdin.take();
        let Some(stdout) = child.stdout.take() else {
            terminate_process(&mut child);
            return Err("gdb 没有 stdout".to_owned());
        };
        let Some(stderr) = child.stderr.take() else {
            terminate_process(&mut child);
            return Err("gdb 没有 stderr".to_owned());
        };
        let out_tx = tx.clone();
        std::thread::spawn(move || forward_gdb(stdout, out_tx));
        std::thread::spawn(move || forward_gdb(stderr, tx));
        let mut session = Self {
            child,
            stdin,
            rx,
            log: Vec::new(),
            watch_output: Vec::new(),
            preg_output: Vec::new(),
            started: false,
            server: None,
        };
        session.send("set pagination off");
        session.send("set confirm off");
        session.send("set print pretty on");
        session.send("set breakpoint pending on");
        Ok(session)
    }

    pub fn send(&mut self, command: &str) {
        if let Some(stdin) = &mut self.stdin {
            let _ = writeln!(stdin, "{command}");
            let _ = stdin.flush();
        }
    }

    /// Interrupt a running target. GDB treats Ctrl+C on stdin as an interrupt
    /// request even when the CLI prompt is not active.
    pub fn interrupt(&mut self) {
        if let Some(stdin) = &mut self.stdin {
            let _ = stdin.write_all(&[3]);
            let _ = stdin.flush();
        }
    }

    pub fn poll(&mut self) {
        while let Ok(event) = self.rx.try_recv() {
            match event {
                DebugEvent::Line(line) => {
                    let line = line.trim_end_matches('\r').to_owned();
                    if line.is_empty() || line == "(gdb) " || line == "(gdb)" {
                        continue;
                    }
                    self.log.push(line);
                }
                DebugEvent::Watch(line) => {
                    self.watch_output
                        .push(line.trim_end_matches('\r').to_owned());
                }
                DebugEvent::Preg(line) => {
                    self.preg_output
                        .push(line.trim_end_matches('\r').to_owned());
                }
            }
        }
        if self.log.len() > MAX_LOG_LINES {
            let excess = self.log.len() - MAX_LOG_LINES;
            self.log.drain(..excess);
        }
        if self.watch_output.len() > MAX_LOG_LINES * 4 {
            let excess = self.watch_output.len() - MAX_LOG_LINES * 4;
            self.watch_output.drain(..excess);
        }
        if self.preg_output.len() > MAX_LOG_LINES * 4 {
            let excess = self.preg_output.len() - MAX_LOG_LINES * 4;
            self.preg_output.drain(..excess);
        }
    }

    pub fn is_running(&mut self) -> bool {
        let running = self.child.try_wait().ok().flatten().is_none();
        if !running {
            if let Some(mut server) = self.server.take() {
                terminate_process(&mut server);
            }
        }
        running
    }

    pub fn stop(&mut self) {
        self.send("quit");
        if let Some(mut server) = self.server.take() {
            terminate_process(&mut server);
        }
        terminate_process(&mut self.child);
        self.stdin = None;
    }
}

impl Drop for DebugSession {
    fn drop(&mut self) {
        self.stop();
    }
}

fn forward_gdb(reader: impl Read, tx: Sender<DebugEvent>) {
    let reader = BufReader::new(reader);
    let mut in_watch = false;
    let mut in_preg = false;
    for line in reader.lines().map_while(Result::ok) {
        if line.contains("__SE_PREG_BEGIN_") {
            in_preg = true;
            let _ = tx.send(DebugEvent::Preg(line));
            continue;
        }
        if in_preg {
            let is_end = line.contains("__SE_PREG_END_");
            let _ = tx.send(DebugEvent::Preg(line));
            if is_end {
                in_preg = false;
            }
            continue;
        }
        if line.contains("__SE_WATCH_BEGIN_") {
            in_watch = true;
            let _ = tx.send(DebugEvent::Watch(line));
            continue;
        }
        if in_watch {
            let is_end = line.contains("__SE_WATCH_END_");
            let _ = tx.send(DebugEvent::Watch(line));
            if is_end {
                in_watch = false;
            }
        } else {
            let _ = tx.send(DebugEvent::Line(line));
        }
    }
}

fn forward_server(reader: impl Read, tx: Sender<DebugEvent>, label: &'static str) {
    let reader = BufReader::new(reader);
    for line in reader.lines().map_while(Result::ok) {
        // Peripheral-register polling intentionally probes optional blocks;
        // OpenOCD reports each unmapped word as a failed read.
        if line.contains("Error: Failed to read memory at 0x") {
            continue;
        }
        let _ = tx.send(DebugEvent::Line(format!("[{label}] {line}")));
    }
}

fn spawn_server(spec: &RunSpec, kind: ServerKind, tx: Sender<DebugEvent>) -> Result<Child, String> {
    let mut command = Command::new(&spec.program);
    command
        .args(&spec.args)
        .current_dir(&spec.cwd)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    let mut paths = Vec::new();
    if let Some(parent) = Path::new(spec.program.as_os_str()).parent() {
        if parent.is_dir() {
            paths.push(parent.to_path_buf());
        }
    }
    paths.extend(spec.env_paths.iter().cloned());
    if !paths.is_empty() {
        if let Some(existing) = std::env::var_os("PATH") {
            paths.extend(std::env::split_paths(&existing));
        }
        if let Ok(joined) = std::env::join_paths(&paths) {
            command.env("PATH", joined);
        }
    }
    for (key, value) in &spec.env {
        command.env(key, value);
    }
    crate::util::hide_console(&mut command);
    let mut child = command
        .spawn()
        .map_err(|err| format!("无法启动调试服务（{}）：{err}", spec.display))?;
    let Some(stdout) = child.stdout.take() else {
        terminate_process(&mut child);
        return Err("调试服务没有 stdout".to_owned());
    };
    let Some(stderr) = child.stderr.take() else {
        terminate_process(&mut child);
        return Err("调试服务没有 stderr".to_owned());
    };
    let label = kind.label();
    let out_tx = tx.clone();
    std::thread::spawn(move || forward_server(stdout, out_tx, label));
    std::thread::spawn(move || forward_server(stderr, tx, label));
    Ok(child)
}

fn wait_for_server(
    port: u16,
    timeout: Duration,
    server: &mut Child,
    rx: &Receiver<DebugEvent>,
) -> Result<(), String> {
    let address = SocketAddr::from(([127, 0, 0, 1], port));
    let started = Instant::now();
    loop {
        if TcpStream::connect_timeout(&address, Duration::from_millis(180)).is_ok() {
            return Ok(());
        }
        if let Ok(Some(status)) = server.try_wait() {
            let details = drain_logs(rx);
            return Err(format!("调试服务已退出（{status}）{details}"));
        }
        if started.elapsed() >= timeout {
            let details = drain_logs(rx);
            return Err(format!("等待调试服务端口 {port} 超时{details}"));
        }
        std::thread::sleep(Duration::from_millis(80));
    }
}

fn drain_logs(rx: &Receiver<DebugEvent>) -> String {
    let mut lines = Vec::new();
    while let Ok(DebugEvent::Line(line)) = rx.try_recv() {
        lines.push(line);
    }
    if lines.is_empty() {
        return String::new();
    }
    if lines.len() > 20 {
        lines.drain(..lines.len() - 20);
    }
    format!("：\n{}", lines.join("\n"))
}

/// Extract a stopped source location from a plain gdb console line.
///
/// GDB prints stop locations as `... at <path>:<line>`; breakpoint creation
/// messages (`Breakpoint 1 at 0x...: file ..., line N.`) deliberately do not
/// match. Windows drive colons are accepted because the path is split at the
/// last colon before a trailing line number.
pub fn parse_stop_location(line: &str) -> Option<(String, usize)> {
    let line = line.trim();
    if line.is_empty() {
        return None;
    }
    let after = line.rfind(" at ").map(|index| &line[index + 4..])?;
    let after = after.trim();
    let (path, line_no) = after.rsplit_once(':')?;
    let path = path.trim().trim_matches('"');
    if path.is_empty() || !path.contains(['.', '\\', '/']) {
        return None;
    }
    let line_no = line_no
        .split(|character: char| !character.is_ascii_digit())
        .next()?
        .parse::<usize>()
        .ok()?;
    if line_no == 0 {
        return None;
    }
    Some((path.to_owned(), line_no))
}

/// Parse gdb's warning for a breakpoint that has no compiled code.
///
/// Examples:
/// `No compiled code for line 16 in file "D:/project/Core/Src/main.c".`
/// `No line 16 in file "D:/project/Core/Src/main.c".`
pub fn parse_uncompiled_line(line: &str) -> Option<(String, usize)> {
    let line = line.trim();
    let rest = line
        .strip_prefix("No compiled code for line ")
        .or_else(|| line.strip_prefix("No line "))?;
    let (line_no, rest) = rest.split_once(" in file ")?;
    let path = rest.trim().trim_end_matches('.').trim_matches('"');
    if path.is_empty() {
        return None;
    }
    let line_no = line_no.trim().parse::<usize>().ok()?;
    if line_no == 0 {
        return None;
    }
    Some((path.to_owned(), line_no))
}

pub fn free_tcp_port() -> Result<u16, String> {
    let listener =
        TcpListener::bind(("127.0.0.1", 0)).map_err(|err| format!("无法分配调试端口：{err}"))?;
    listener
        .local_addr()
        .map(|address| address.port())
        .map_err(|err| format!("无法读取调试端口：{err}"))
}

/// Decide whether a program should be debugged via a remote embedded target.
/// On Windows any ELF is treated as firmware; PE `.exe` files use host gdb.
pub fn target_kind(path: &Path) -> DebugTargetKind {
    if path
        .extension()
        .is_some_and(|extension| extension.eq_ignore_ascii_case("exe"))
    {
        return DebugTargetKind::Host;
    }
    if let Some(machine) = elf_machine(path) {
        #[cfg(not(target_os = "windows"))]
        if matches!(machine, 3 | 62) {
            return DebugTargetKind::Host;
        }
        let _ = machine;
        return DebugTargetKind::Embedded;
    }
    DebugTargetKind::Host
}

fn elf_machine(path: &Path) -> Option<u16> {
    let mut file = std::fs::File::open(path).ok()?;
    let mut header = [0u8; 20];
    file.read_exact(&mut header).ok()?;
    if &header[..4] != b"\x7fELF" {
        return None;
    }
    let little_endian = header[5] != 2;
    Some(if little_endian {
        u16::from_le_bytes([header[18], header[19]])
    } else {
        u16::from_be_bytes([header[18], header[19]])
    })
}

pub fn detect_gdb(configured: &str) -> Option<std::path::PathBuf> {
    let configured = configured.trim();
    if !configured.is_empty() {
        let path = std::path::PathBuf::from(configured);
        if path.is_dir() {
            for name in ["gdb.exe", "gdb"] {
                let candidate = path.join(name);
                if candidate.is_file() {
                    return Some(candidate);
                }
            }
        }
        return Some(path);
    }
    for candidate in [
        r"C:\complier\mingw64\bin\gdb.exe",
        r"C:\msys64\mingw64\bin\gdb.exe",
        r"C:\msys64\ucrt64\bin\gdb.exe",
    ] {
        let path = std::path::PathBuf::from(candidate);
        if path.is_file() {
            return Some(path);
        }
    }
    let mut command = Command::new("gdb");
    command
        .arg("--version")
        .stdout(Stdio::null())
        .stderr(Stdio::null());
    crate::util::hide_console(&mut command);
    if command.status().is_ok_and(|status| status.success()) {
        Some(std::path::PathBuf::from("gdb"))
    } else {
        None
    }
}

/// Detect an ARM-capable gdb. `extra_dirs` usually contains the selected
/// compiler's `bin` directory and the project's extra PATH entries.
pub fn detect_arm_gdb(configured: &str, extra_dirs: &[PathBuf]) -> Option<PathBuf> {
    let configured = configured.trim();
    if !configured.is_empty() {
        let path = PathBuf::from(configured);
        if path.is_dir() {
            if let Some(found) = find_arm_gdb_in(&path) {
                return Some(found);
            }
        }
        if is_arm_gdb_name(&path) && (path.is_file() || !path.is_absolute()) {
            return Some(path);
        }
    }
    for dir in extra_dirs {
        if let Some(path) = find_arm_gdb_in(dir) {
            return Some(path);
        }
    }
    for name in ["arm-none-eabi-gdb", "arm-none-eabi-gdb.exe"] {
        if let Some(path) = find_in_path(name) {
            return Some(path);
        }
    }
    if let Some(path) = find_arm_gdb_in_common_dirs() {
        return Some(path);
    }
    None
}

fn is_arm_gdb_name(path: &Path) -> bool {
    path.file_name()
        .map(|name| name.to_string_lossy().to_ascii_lowercase())
        .is_some_and(|name| name.contains("arm-none-eabi-gdb") || name.contains("gdb-multiarch"))
}

fn find_arm_gdb_in(dir: &Path) -> Option<PathBuf> {
    for name in [
        "arm-none-eabi-gdb.exe",
        "arm-none-eabi-gdb",
        "gdb-multiarch.exe",
        "gdb-multiarch",
    ] {
        let candidate = dir.join(name);
        if candidate.is_file() {
            return Some(candidate);
        }
    }
    None
}

fn find_in_path(name: &str) -> Option<PathBuf> {
    let path = std::env::var_os("PATH")?;
    for dir in std::env::split_paths(&path) {
        let candidate = dir.join(name);
        if candidate.is_file() {
            return Some(candidate);
        }
    }
    None
}

fn find_arm_gdb_in_common_dirs() -> Option<PathBuf> {
    let mut bases = Vec::new();
    for key in ["ProgramFiles", "ProgramFiles(x86)"] {
        if let Some(path) = std::env::var_os(key) {
            bases.push(PathBuf::from(path));
        }
    }
    if let Some(path) = std::env::var_os("LOCALAPPDATA") {
        bases.push(PathBuf::from(path).join("Programs"));
    }
    for base in bases {
        let Ok(entries) = std::fs::read_dir(&base) else {
            continue;
        };
        for entry in entries.flatten() {
            let name = entry.file_name().to_string_lossy().to_ascii_lowercase();
            if !name.contains("arm")
                || !(name.contains("toolchain") || name.contains("gnu") || name.contains("gcc"))
            {
                continue;
            }
            let root = entry.path();
            if let Some(path) = find_arm_gdb_in(&root.join("bin")) {
                return Some(path);
            }
            let Ok(children) = std::fs::read_dir(&root) else {
                continue;
            };
            for child in children.flatten() {
                if let Some(path) = find_arm_gdb_in(&child.path().join("bin")) {
                    return Some(path);
                }
            }
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detect_returns_configured_path_even_if_missing() {
        let path = detect_gdb("Z:\\no-such-gdb.exe");
        assert_eq!(path, Some(std::path::PathBuf::from("Z:\\no-such-gdb.exe")));
    }

    #[test]
    fn detects_elf_as_embedded_target() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("demo.axf");
        let mut header = [0u8; 20];
        header[..4].copy_from_slice(b"\x7fELF");
        header[4] = 1;
        header[5] = 1;
        header[18] = 40;
        header[19] = 0;
        std::fs::write(&path, header).unwrap();
        assert_eq!(target_kind(&path), DebugTargetKind::Embedded);
    }

    #[test]
    fn detects_pe_as_host_target() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("demo.exe");
        std::fs::write(&path, b"MZ").unwrap();
        assert_eq!(target_kind(&path), DebugTargetKind::Host);
    }

    #[test]
    fn finds_arm_gdb_in_compiler_directory() {
        let dir = tempfile::tempdir().unwrap();
        let gdb = dir.path().join("arm-none-eabi-gdb.exe");
        std::fs::write(&gdb, b"fake").unwrap();
        assert_eq!(
            detect_arm_gdb("", &[dir.path().to_path_buf()]),
            Some(gdb.clone())
        );
        assert_eq!(
            detect_arm_gdb(&dir.path().to_string_lossy(), &[]),
            Some(gdb)
        );
    }

    #[test]
    fn parses_source_locations_from_stop_lines() {
        assert_eq!(
            parse_stop_location("Breakpoint 1, main () at ../Core/Src/main.c:95"),
            Some(("../Core/Src/main.c".to_owned(), 95))
        );
        assert_eq!(
            parse_stop_location("0x08000b24 in HAL_Delay () at C:\\work\\Src\\stm32f1xx_hal.c:402"),
            Some(("C:\\work\\Src\\stm32f1xx_hal.c".to_owned(), 402))
        );
        assert_eq!(
            parse_stop_location("Breakpoint 1 at 0x08000b24: file ../Core/Src/main.c, line 95."),
            None
        );
        assert_eq!(parse_stop_location("Continuing."), None);
    }

    #[test]
    fn parses_uncompiled_breakpoint_warnings() {
        assert_eq!(
            parse_uncompiled_line(
                "No compiled code for line 16 in file \"D:/project/Core/Src/main.c\"."
            ),
            Some(("D:/project/Core/Src/main.c".to_owned(), 16))
        );
        assert_eq!(
            parse_uncompiled_line("No line 42 in file \"C:\\work\\main.c\"."),
            Some(("C:\\work\\main.c".to_owned(), 42))
        );
        assert_eq!(parse_uncompiled_line("Continuing."), None);
    }

    #[test]
    fn allocates_a_usable_local_port() {
        let port = free_tcp_port().unwrap();
        assert!(port > 0);
        assert!(TcpListener::bind(("127.0.0.1", port)).is_ok());
    }

    #[test]
    fn separates_watch_output_from_the_normal_log() {
        let input = b"normal\n__SE_WATCH_BEGIN_1__\n$1 = 2\n__SE_WATCH_END_1__\nafter\n";
        let (tx, rx) = channel();
        forward_gdb(&input[..], tx);
        let mut lines = 0;
        let mut watches = 0;
        for event in rx.try_iter() {
            match event {
                DebugEvent::Line(_) => lines += 1,
                DebugEvent::Watch(_) => watches += 1,
                DebugEvent::Preg(_) => {}
            }
        }
        assert_eq!(lines, 2);
        assert_eq!(watches, 3);
    }

    #[test]
    fn separates_peripheral_register_output() {
        let input =
            b"__SE_PREG_BEGIN_2__\n__SE_PREG_V_2_40010800__\n0x40010800:\t0x44444444\n__SE_PREG_END_2__\n";
        let (tx, rx) = channel();
        forward_gdb(&input[..], tx);
        let mut preg = Vec::new();
        for event in rx.try_iter() {
            if let DebugEvent::Preg(line) = event {
                preg.push(line);
            }
        }
        assert_eq!(preg.len(), 4);
        assert!(preg.iter().any(|line| line.contains("0x44444444")));
    }
}
