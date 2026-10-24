//! Background build execution, streaming stdout/stderr and cancellation.

use std::ffi::OsString;
use std::io::{BufRead, BufReader, Read};
use std::process::{Child, Command, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, Receiver, Sender, TryRecvError};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant};

use crate::log::{LogStore, Severity};
use crate::util::{format_duration, hide_console};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TaskKind {
    Build,
    Clean,
    Rebuild,
    Flash,
}

impl TaskKind {
    pub fn label(self) -> &'static str {
        match self {
            Self::Build => "构建",
            Self::Clean => "清理",
            Self::Rebuild => "重新构建",
            Self::Flash => "烧录",
        }
    }

    pub fn verb(self) -> &'static str {
        match self {
            Self::Build => "构建",
            Self::Clean => "清理",
            Self::Rebuild => "重新构建",
            Self::Flash => "烧录",
        }
    }
}

pub struct RunSpec {
    pub program: OsString,
    pub args: Vec<OsString>,
    pub cwd: std::path::PathBuf,
    pub display: String,
}

enum BuildEvent {
    Line(String),
    Finished {
        success: bool,
        code: Option<i32>,
        cancelled: bool,
        duration: Duration,
    },
}

struct Session {
    kind: TaskKind,
    started: Instant,
    receiver: Receiver<BuildEvent>,
    child: Arc<Mutex<Option<Child>>>,
    cancelled: Arc<AtomicBool>,
    stop_requested: bool,
}

#[derive(Clone, Debug)]
pub enum BuildOutcome {
    Success,
    Failed(Option<i32>),
    Cancelled,
    Error(String),
}

#[derive(Clone, Debug)]
pub struct LastBuild {
    pub kind: TaskKind,
    pub outcome: BuildOutcome,
    pub duration: Duration,
}

pub struct BuildController {
    pub log: LogStore,
    pub auto_scroll: bool,
    pub output_open: bool,
    session: Option<Session>,
    last: Option<LastBuild>,
}

impl Default for BuildController {
    fn default() -> Self {
        Self {
            log: LogStore::default(),
            auto_scroll: true,
            output_open: true,
            session: None,
            last: None,
        }
    }
}

impl Drop for BuildController {
    fn drop(&mut self) {
        if let Some(session) = self.session.take() {
            session.cancelled.store(true, Ordering::SeqCst);
            if let Ok(mut child) = session.child.lock() {
                if let Some(child) = child.as_mut() {
                    terminate_process(child);
                }
            }
        }
    }
}

impl BuildController {
    pub fn is_running(&self) -> bool {
        self.session.is_some()
    }

    pub fn running_kind(&self) -> Option<TaskKind> {
        self.session.as_ref().map(|session| session.kind)
    }

    pub fn elapsed(&self) -> Option<Duration> {
        self.session
            .as_ref()
            .map(|session| session.started.elapsed())
    }

    pub fn last(&self) -> Option<&LastBuild> {
        self.last.as_ref()
    }

    pub fn start(&mut self, spec: RunSpec, kind: TaskKind) -> Result<(), String> {
        if self.session.is_some() {
            return Err("已有任务正在运行".to_owned());
        }

        if !self.log.is_empty() {
            self.log.push_blank();
        }
        self.log.push_message(
            format!("▶ {} · {}", kind.label(), spec.display),
            Severity::Command,
        );

        let started = Instant::now();
        match spawn_process(&spec) {
            Ok((receiver, child, cancelled)) => {
                self.session = Some(Session {
                    kind,
                    started,
                    receiver,
                    child,
                    cancelled,
                    stop_requested: false,
                });
                Ok(())
            }
            Err(err) => {
                self.log
                    .push_message(format!("无法启动进程：{err}"), Severity::Error);
                self.last = Some(LastBuild {
                    kind,
                    outcome: BuildOutcome::Error(err.clone()),
                    duration: started.elapsed(),
                });
                Err(err)
            }
        }
    }

    pub fn stop(&mut self) {
        let Some(session) = self.session.as_mut() else {
            return;
        };
        if session.stop_requested {
            return;
        }
        session.stop_requested = true;
        session.cancelled.store(true, Ordering::SeqCst);
        self.log.push_message("正在停止进程树…", Severity::Warning);

        if let Ok(mut child) = session.child.lock() {
            if let Some(child) = child.as_mut() {
                terminate_process(child);
            }
        }
    }

    /// Kill a running task immediately, without waiting for the summary event.
    pub fn abort(&mut self) {
        if let Some(session) = self.session.take() {
            session.cancelled.store(true, Ordering::SeqCst);
            if let Ok(mut child) = session.child.lock() {
                if let Some(child) = child.as_mut() {
                    terminate_process(child);
                }
            }
        }
    }

    /// Forget the current project's logs and result, killing any running task.
    pub fn reset(&mut self) {
        self.abort();
        self.log.clear();
        self.last = None;
    }

    /// Drain all pending events. Returns true if a task finished this frame.
    #[allow(clippy::while_let_loop)] // The loop mutates `self.session`; a `while let` borrow fights this.
    pub fn poll(&mut self) -> bool {
        let mut finished = false;
        loop {
            let event = match self.session.as_ref() {
                Some(session) => match session.receiver.try_recv() {
                    Ok(event) => event,
                    Err(TryRecvError::Empty) => break,
                    Err(TryRecvError::Disconnected) => {
                        let session = self.session.take().expect("session exists");
                        let outcome = if session.cancelled.load(Ordering::SeqCst) {
                            BuildOutcome::Cancelled
                        } else {
                            BuildOutcome::Error("构建进程意外退出".to_owned())
                        };
                        let result = LastBuild {
                            kind: session.kind,
                            outcome,
                            duration: session.started.elapsed(),
                        };
                        self.write_summary(&result);
                        self.last = Some(result);
                        finished = true;
                        break;
                    }
                },
                None => break,
            };

            match event {
                BuildEvent::Line(line) => {
                    self.log.push_raw(&line);
                }
                BuildEvent::Finished {
                    success,
                    code,
                    cancelled,
                    duration,
                } => {
                    let Some(session) = self.session.take() else {
                        break;
                    };
                    let outcome = if cancelled {
                        BuildOutcome::Cancelled
                    } else if success {
                        BuildOutcome::Success
                    } else {
                        BuildOutcome::Failed(code)
                    };
                    let result = LastBuild {
                        kind: session.kind,
                        outcome,
                        duration,
                    };
                    self.write_summary(&result);
                    self.last = Some(result);
                    finished = true;
                }
            }
        }
        finished
    }

    fn write_summary(&mut self, result: &LastBuild) {
        let task = result.kind.verb();
        let duration = format_duration(result.duration);
        match &result.outcome {
            BuildOutcome::Success => self
                .log
                .push_message(format!("{task}完成 · 用时 {duration}"), Severity::Success),
            BuildOutcome::Failed(Some(code)) => self.log.push_message(
                format!("{task}失败 · 退出码 {code} · 用时 {duration}"),
                Severity::Error,
            ),
            BuildOutcome::Failed(None) => self.log.push_message(
                format!("{task}失败 · 进程被终止 · 用时 {duration}"),
                Severity::Error,
            ),
            BuildOutcome::Cancelled => self
                .log
                .push_message(format!("{task}已停止 · 用时 {duration}"), Severity::Warning),
            BuildOutcome::Error(err) => {
                self.log
                    .push_message(format!("{task}无法执行：{err}"), Severity::Error);
            }
        }
    }
}

type SpawnedProcess = (
    Receiver<BuildEvent>,
    Arc<Mutex<Option<Child>>>,
    Arc<AtomicBool>,
);

fn spawn_process(spec: &RunSpec) -> Result<SpawnedProcess, String> {
    let mut command = Command::new(&spec.program);
    command
        .args(&spec.args)
        .current_dir(&spec.cwd)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .env("CARGO_TERM_COLOR", "always")
        .env("CLICOLOR_FORCE", "1")
        .env("TERM", "xterm-256color");
    hide_console(&mut command);

    let mut child = command
        .spawn()
        .map_err(|err| format!("{}：{err}", spec.program.to_string_lossy()))?;

    let stdout = child.stdout.take();
    let stderr = child.stderr.take();
    let child = Arc::new(Mutex::new(Some(child)));
    let cancelled = Arc::new(AtomicBool::new(false));
    let (sender, receiver) = mpsc::channel();

    let stdout_thread = stdout.map(|stream| {
        let sender = sender.clone();
        thread::spawn(move || read_stream(stream, sender))
    });
    let stderr_thread = stderr.map(|stream| {
        let sender = sender.clone();
        thread::spawn(move || read_stream(stream, sender))
    });

    let waiter_child = Arc::clone(&child);
    let waiter_cancelled = Arc::clone(&cancelled);
    let waiter_sender = sender.clone();
    thread::spawn(move || {
        let started = Instant::now();
        let status = loop {
            let mut guard = match waiter_child.lock() {
                Ok(guard) => guard,
                Err(_) => break None,
            };
            let Some(child) = guard.as_mut() else {
                break None;
            };
            match child.try_wait() {
                Ok(Some(status)) => {
                    *guard = None;
                    break Some(status);
                }
                Ok(None) => {}
                Err(_) => {
                    *guard = None;
                    break None;
                }
            }
            drop(guard);
            thread::sleep(Duration::from_millis(35));
        };

        if let Some(handle) = stdout_thread {
            let _ = handle.join();
        }
        if let Some(handle) = stderr_thread {
            let _ = handle.join();
        }

        let cancelled_now = waiter_cancelled.load(Ordering::SeqCst);
        let success = status
            .as_ref()
            .is_some_and(std::process::ExitStatus::success)
            && !cancelled_now;
        let code = status.as_ref().and_then(std::process::ExitStatus::code);
        let _ = waiter_sender.send(BuildEvent::Finished {
            success,
            code,
            cancelled: cancelled_now,
            duration: started.elapsed(),
        });
    });

    Ok((receiver, child, cancelled))
}

fn read_stream<R: Read>(stream: R, sender: Sender<BuildEvent>) {
    let mut reader = BufReader::new(stream);
    let mut bytes = Vec::new();
    loop {
        bytes.clear();
        match reader.read_until(b'\n', &mut bytes) {
            Ok(0) => break,
            Ok(_) => {
                let decoded = decode_output(&bytes);
                let text = decoded.trim_end_matches(['\r', '\n']);
                if !text.is_empty() && sender.send(BuildEvent::Line(text.to_owned())).is_err() {
                    break;
                }
            }
            Err(_) => break,
        }
    }
}

fn decode_output(bytes: &[u8]) -> String {
    if let Ok(text) = std::str::from_utf8(bytes) {
        return text.to_owned();
    }
    #[cfg(target_os = "windows")]
    {
        // Native Windows toolchains (especially MSVC) often emit GBK when the
        // system locale is Chinese. Prefer a readable decode over replacement
        // characters in the log.
        let (decoded, _, _) = encoding_rs::GBK.decode(bytes);
        decoded.into_owned()
    }
    #[cfg(not(target_os = "windows"))]
    {
        String::from_utf8_lossy(bytes).into_owned()
    }
}

fn terminate_process(child: &mut Child) {
    #[cfg(target_os = "windows")]
    {
        let pid = child.id().to_string();
        let mut taskkill = Command::new("taskkill.exe");
        taskkill
            .args(["/PID", &pid, "/T", "/F"])
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null());
        hide_console(&mut taskkill);
        let _ = taskkill.status();
    }

    #[cfg(unix)]
    {
        let pid = child.id().to_string();
        let _ = Command::new("pkill")
            .args(["-TERM", "-P", &pid])
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status();
    }

    let _ = child.kill();
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn streams_a_command_to_completion() {
        #[cfg(target_os = "windows")]
        let spec = RunSpec {
            program: "cmd".into(),
            args: vec!["/C".into(), "echo build-ok".into()],
            cwd: std::env::current_dir().unwrap(),
            display: "echo build-ok".into(),
        };
        #[cfg(not(target_os = "windows"))]
        let spec = RunSpec {
            program: "sh".into(),
            args: vec!["-c".into(), "echo build-ok".into()],
            cwd: std::env::current_dir().unwrap(),
            display: "echo build-ok".into(),
        };

        let mut controller = BuildController::default();
        controller.start(spec, TaskKind::Build).unwrap();
        let deadline = Instant::now() + Duration::from_secs(5);
        while controller.is_running() && Instant::now() < deadline {
            controller.poll();
            thread::sleep(Duration::from_millis(20));
        }
        controller.poll();
        assert!(!controller.is_running());
        assert!(matches!(
            controller.last(),
            Some(LastBuild {
                outcome: BuildOutcome::Success,
                ..
            })
        ));
        assert!(controller.log.all_text().contains("build-ok"));
    }
}
