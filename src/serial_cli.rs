//! Serial CLI configuration and a small background reader/writer.
//!
//! The firmware side is generated from `references/serial-cli.md`; this module
//! only transports commands and lines for the AI-mode CLI panel.

use std::io::{Read, Write};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, Receiver, Sender};
use std::sync::Arc;
use std::thread;
use std::time::Duration;

use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct CliConfig {
    pub port: String,
    pub baud: u32,
    /// `CRLF`, `LF` or `CR`.
    pub line_ending: String,
    pub timeout_ms: u64,
}

impl Default for CliConfig {
    fn default() -> Self {
        Self {
            port: String::new(),
            baud: 115_200,
            line_ending: "CRLF".to_owned(),
            timeout_ms: 800,
        }
    }
}

impl CliConfig {
    pub fn is_ready(&self) -> bool {
        !self.port.trim().is_empty() && (1_200..=4_000_000).contains(&self.baud)
    }

    pub fn validate(&self) -> Result<(), String> {
        if self.port.trim().is_empty() {
            return Err("CLI 需要选择串口端口".to_owned());
        }
        if !(1_200..=4_000_000).contains(&self.baud) {
            return Err("CLI 波特率需要在 1200 到 4000000 之间".to_owned());
        }
        if !matches!(self.line_ending.as_str(), "CRLF" | "LF" | "CR") {
            return Err("CLI 行尾必须是 CRLF、LF 或 CR".to_owned());
        }
        Ok(())
    }

    pub fn summary(&self) -> String {
        if self.port.trim().is_empty() {
            "串口未选择".to_owned()
        } else {
            format!("{} · {}", self.port.trim(), self.baud)
        }
    }

    pub fn ending_bytes(&self) -> &'static [u8] {
        match self.line_ending.as_str() {
            "LF" => b"\n",
            "CR" => b"\r",
            _ => b"\r\n",
        }
    }
}

pub enum CliEvent {
    Line(String),
    Error(String),
    Closed,
}

pub struct CliSession {
    commands: Sender<Vec<u8>>,
    events: Receiver<CliEvent>,
    stop: Arc<AtomicBool>,
    thread: Option<thread::JoinHandle<()>>,
}

impl CliSession {
    pub fn open(config: &CliConfig) -> Result<Self, String> {
        config.validate()?;
        let mut port = serialport::new(config.port.trim(), config.baud)
            .timeout(Duration::from_millis(30))
            .open()
            .map_err(|err| format!("打开 CLI 串口失败：{err}"))?;
        let (command_tx, command_rx) = mpsc::channel::<Vec<u8>>();
        let (event_tx, event_rx) = mpsc::channel();
        let stop = Arc::new(AtomicBool::new(false));
        let thread_stop = Arc::clone(&stop);
        let thread = thread::Builder::new()
            .name("simple-embedded-cli".to_owned())
            .spawn(move || {
                let mut bytes = Vec::new();
                let mut read_buf = [0u8; 1024];
                loop {
                    if thread_stop.load(Ordering::SeqCst) {
                        break;
                    }
                    while let Ok(command) = command_rx.try_recv() {
                        if let Err(err) = port.write_all(&command).and_then(|_| port.flush()) {
                            let _ = event_tx.send(CliEvent::Error(format!("CLI 写入失败：{err}")));
                            return;
                        }
                    }
                    match port.read(&mut read_buf) {
                        Ok(0) => {}
                        Ok(count) => {
                            bytes.extend_from_slice(&read_buf[..count]);
                            let text = String::from_utf8_lossy(&bytes);
                            let mut consumed = 0usize;
                            for (index, ch) in text.char_indices() {
                                if ch == '\n' || ch == '\r' {
                                    let line =
                                        text[consumed..index].trim_end_matches('\r').to_owned();
                                    consumed = index + 1;
                                    if !line.is_empty() {
                                        let _ = event_tx.send(CliEvent::Line(line));
                                    }
                                }
                            }
                            if consumed > 0 {
                                let remaining = text[consumed..].to_owned();
                                bytes.clear();
                                bytes.extend_from_slice(remaining.as_bytes());
                            }
                            // Keep an unterminated partial line bounded.
                            if bytes.len() > 32_768 {
                                bytes.clear();
                            }
                        }
                        Err(err) if err.kind() == std::io::ErrorKind::TimedOut => {}
                        Err(err) => {
                            let _ = event_tx.send(CliEvent::Error(format!("CLI 读取失败：{err}")));
                            return;
                        }
                    }
                }
                let _ = event_tx.send(CliEvent::Closed);
            })
            .map_err(|err| format!("无法启动 CLI 线程：{err}"))?;
        Ok(Self {
            commands: command_tx,
            events: event_rx,
            stop,
            thread: Some(thread),
        })
    }

    pub fn send(&self, command: &str, ending: &[u8]) -> Result<(), String> {
        let mut bytes = command.trim().as_bytes().to_vec();
        bytes.extend_from_slice(ending);
        self.commands
            .send(bytes)
            .map_err(|_| "CLI 会话已关闭".to_owned())
    }

    pub fn poll(&self) -> Vec<CliEvent> {
        let mut events = Vec::new();
        while let Ok(event) = self.events.try_recv() {
            events.push(event);
        }
        events
    }
}

impl Drop for CliSession {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::SeqCst);
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn line_endings_and_validation() {
        let mut config = CliConfig::default();
        assert!(!config.is_ready());
        config.port = "COM3".to_owned();
        assert!(config.is_ready());
        config.line_ending = "LF".to_owned();
        assert_eq!(config.ending_bytes(), b"\n");
        config.line_ending = "CR".to_owned();
        assert_eq!(config.ending_bytes(), b"\r");
        config.baud = 0;
        assert!(config.validate().is_err());
    }
}
