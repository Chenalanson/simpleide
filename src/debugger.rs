//! Minimal GDB console integration (plain CLI, no MI parsing).

use std::io::{BufRead, BufReader, Write};
use std::path::Path;
use std::process::{Child, ChildStdin, Command, Stdio};
use std::sync::mpsc::{channel, Receiver};

const MAX_LOG_LINES: usize = 4000;

pub enum DebugEvent {
    Line(String),
}

pub struct DebugSession {
    child: Child,
    stdin: Option<ChildStdin>,
    rx: Receiver<DebugEvent>,
    pub log: Vec<String>,
    pub started: bool,
}

impl DebugSession {
    pub fn start(program: &Path, gdb: &Path) -> Result<Self, String> {
        let mut command = Command::new(gdb);
        command
            .arg("--quiet")
            .arg("--args")
            .arg(program)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        crate::util::hide_console(&mut command);
        let mut child = command
            .spawn()
            .map_err(|err| format!("无法启动 gdb（{}）：{err}", gdb.display()))?;
        let stdin = child.stdin.take();
        let stdout = child
            .stdout
            .take()
            .ok_or_else(|| "gdb 没有 stdout".to_owned())?;
        let stderr = child
            .stderr
            .take()
            .ok_or_else(|| "gdb 没有 stderr".to_owned())?;
        let (tx, rx) = channel();
        let out_tx = tx.clone();
        std::thread::spawn(move || {
            let reader = BufReader::new(stdout);
            for line in reader.lines().map_while(Result::ok) {
                let _ = out_tx.send(DebugEvent::Line(line));
            }
        });
        std::thread::spawn(move || {
            let reader = BufReader::new(stderr);
            for line in reader.lines().map_while(Result::ok) {
                let _ = tx.send(DebugEvent::Line(line));
            }
        });
        let mut session = Self {
            child,
            stdin,
            rx,
            log: Vec::new(),
            started: false,
        };
        session.send("set pagination off");
        session.send("set confirm off");
        session.send("set print pretty on");
        Ok(session)
    }

    pub fn send(&mut self, command: &str) {
        if let Some(stdin) = &mut self.stdin {
            let _ = writeln!(stdin, "{command}");
            let _ = stdin.flush();
        }
    }

    pub fn poll(&mut self) {
        while let Ok(DebugEvent::Line(line)) = self.rx.try_recv() {
            let line = line.trim_end_matches('\r').to_owned();
            if line.is_empty() || line == "(gdb) " || line == "(gdb)" {
                continue;
            }
            self.log.push(line);
        }
        if self.log.len() > MAX_LOG_LINES {
            let excess = self.log.len() - MAX_LOG_LINES;
            self.log.drain(..excess);
        }
    }

    pub fn is_running(&mut self) -> bool {
        self.child.try_wait().ok().flatten().is_none()
    }

    pub fn stop(&mut self) {
        self.send("quit");
        let _ = self.child.kill();
        let _ = self.child.wait();
        self.stdin = None;
    }
}

impl Drop for DebugSession {
    fn drop(&mut self) {
        self.stop();
    }
}

pub fn detect_gdb(configured: &str) -> Option<std::path::PathBuf> {
    let configured = configured.trim();
    if !configured.is_empty() {
        return Some(std::path::PathBuf::from(configured));
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detect_returns_configured_path_even_if_missing() {
        let path = detect_gdb("Z:\\no-such-gdb.exe");
        assert_eq!(path, Some(std::path::PathBuf::from("Z:\\no-such-gdb.exe")));
    }
}
