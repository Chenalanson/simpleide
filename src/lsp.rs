//! Minimal clangd Language Server Protocol client.
//!
//! The IDE already knows the project include paths, defines and CPU flags.
//! `write_compilation_database` turns that into a per-project
//! `compile_commands.json` under the user configuration directory, so clangd
//! parses firmware sources with the same `-I` / `-D` / `--target` setup as the
//! real build instead of falling back to the host compiler.

use serde_json::{json, Value};
use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::hash::{Hash, Hasher};
use std::io::{BufRead, BufReader, Write};
use std::path::{Path, PathBuf};
use std::process::{Child, ChildStdin, Command, Stdio};
use std::sync::mpsc::{channel, Receiver};
use std::time::{Duration, Instant};

const SYNC_DELAY: Duration = Duration::from_millis(280);
const COMPLETION_DELAY: Duration = Duration::from_millis(90);
const MAX_DATABASE_FILES: usize = 1500;

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Status {
    Starting,
    Ready,
    Failed(String),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Location {
    pub path: PathBuf,
    pub line: usize,
    pub column: usize,
    pub preview: String,
}

#[derive(Clone, Debug)]
pub struct CompletionEntry {
    pub label: String,
    pub detail: Option<String>,
    pub kind: u32,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LspPosition {
    /// Zero based line, matching LSP.
    pub line: usize,
    /// Zero based UTF-16 column, matching LSP.
    pub character: usize,
}

#[derive(Clone, Debug)]
pub struct TextEdit {
    pub start: LspPosition,
    pub end: LspPosition,
    pub new_text: String,
}

#[derive(Clone, Debug)]
pub struct FileEdits {
    pub path: PathBuf,
    pub edits: Vec<TextEdit>,
}

#[derive(Clone)]
pub struct RefsView {
    pub title: String,
    pub query: String,
    pub items: Vec<Location>,
}

pub struct HoverView {
    pub path: PathBuf,
    pub word: String,
    pub line: usize,
    pub text: String,
}

pub struct CompletionView {
    pub path: PathBuf,
    pub cursor: usize,
    pub revision: u64,
    pub prefix_start: usize,
    pub items: Vec<CompletionEntry>,
}

pub enum Event {
    Ready,
    Failed(String),
    Definition(Vec<Location>),
    References(RefsView),
    Hover(HoverView),
    Completion(CompletionView),
    Rename(Vec<FileEdits>),
    Error(String),
}

enum Pending {
    Initialize,
    Definition,
    References {
        query: String,
    },
    Hover {
        path: PathBuf,
        word: String,
        line: usize,
    },
    Completion {
        path: PathBuf,
        cursor: usize,
        revision: u64,
        prefix_start: usize,
    },
    Rename,
}

pub struct Client {
    child: Child,
    stdin: ChildStdin,
    rx: Receiver<Value>,
    next_id: i64,
}

impl Client {
    fn spawn(program: &Path, args: &[String], root: &Path) -> Result<Self, String> {
        let mut command = Command::new(program);
        command
            .args(args)
            .current_dir(root)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null());
        #[cfg(windows)]
        {
            use std::os::windows::process::CommandExt;
            // CREATE_NO_WINDOW: clangd must not flash a console window.
            command.creation_flags(0x0800_0000);
        }
        let mut child = command
            .spawn()
            .map_err(|err| format!("无法启动 clangd：{err}"))?;
        let stdin = child
            .stdin
            .take()
            .ok_or_else(|| "clangd 标准输入不可用".to_owned())?;
        let stdout = child
            .stdout
            .take()
            .ok_or_else(|| "clangd 标准输出不可用".to_owned())?;
        let (tx, rx) = channel();
        std::thread::spawn(move || {
            let mut reader = BufReader::new(stdout);
            loop {
                match read_message(&mut reader) {
                    Ok(Some(value)) => {
                        if tx.send(value).is_err() {
                            break;
                        }
                    }
                    Ok(None) => break,
                    Err(_) => break,
                }
            }
        });
        Ok(Self {
            child,
            stdin,
            rx,
            next_id: 1,
        })
    }

    fn send(&mut self, value: &Value) -> Result<(), String> {
        let body = serde_json::to_string(value).map_err(|err| err.to_string())?;
        write!(self.stdin, "Content-Length: {}\r\n\r\n{}", body.len(), body)
            .map_err(|err| err.to_string())?;
        self.stdin.flush().map_err(|err| err.to_string())
    }

    fn request(&mut self, method: &str, params: Value) -> Result<i64, String> {
        let id = self.next_id;
        self.next_id += 1;
        self.send(&json!({
            "jsonrpc": "2.0",
            "id": id,
            "method": method,
            "params": params,
        }))?;
        Ok(id)
    }

    fn notify(&mut self, method: &str, params: Value) -> Result<(), String> {
        self.send(&json!({
            "jsonrpc": "2.0",
            "method": method,
            "params": params,
        }))
    }

    fn try_recv(&self) -> Option<Value> {
        self.rx.try_recv().ok()
    }

    fn exited(&mut self) -> bool {
        self.child.try_wait().ok().flatten().is_some()
    }

    fn stop(&mut self) {
        let _ = self.request("shutdown", Value::Null);
        let _ = self.notify("exit", Value::Null);
        let deadline = Instant::now() + Duration::from_millis(400);
        while Instant::now() < deadline {
            if self.child.try_wait().ok().flatten().is_some() {
                return;
            }
            std::thread::sleep(Duration::from_millis(20));
        }
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

fn read_message(reader: &mut impl BufRead) -> std::io::Result<Option<Value>> {
    let mut content_length = None;
    loop {
        let mut line = String::new();
        let read = reader.read_line(&mut line)?;
        if read == 0 {
            return Ok(None);
        }
        let trimmed = line.trim_end();
        if trimmed.is_empty() {
            break;
        }
        if let Some(rest) = trimmed.strip_prefix("Content-Length:") {
            content_length = rest.trim().parse::<usize>().ok();
        }
    }
    let Some(length) = content_length else {
        return Ok(None);
    };
    let mut buffer = vec![0u8; length];
    reader.read_exact(&mut buffer)?;
    Ok(serde_json::from_slice(&buffer).ok())
}

/// Project inputs used to build `compile_commands.json`.
pub struct WorkspaceConfig {
    pub root: PathBuf,
    pub includes: Vec<String>,
    pub defines: Vec<String>,
    pub cpu_flags: Vec<String>,
    /// Path to `arm-none-eabi-gcc`; used to discover newlib headers.
    pub compiler: Option<PathBuf>,
}

/// Detect clangd from an explicit path, PATH, or common LLVM install roots.
pub fn detect_clangd(configured: &str) -> Option<PathBuf> {
    let configured = configured.trim();
    if !configured.is_empty() {
        let path = PathBuf::from(configured);
        if path.is_file() {
            return Some(path);
        }
        if path.is_dir() {
            if let Some(found) = find_program_in(&path, "clangd") {
                return Some(found);
            }
        }
    }
    if let Some(paths) = std::env::var_os("PATH") {
        for dir in std::env::split_paths(&paths) {
            if let Some(found) = find_program_in(&dir, "clangd") {
                return Some(found);
            }
        }
    }
    let roots = [
        r"C:\Program Files\LLVM\bin",
        r"C:\Program Files (x86)\LLVM\bin",
        r"C:\Program Files\Microsoft Visual Studio\2022\Community\VC\Tools\Llvm\x64\bin",
        r"C:\Program Files\Microsoft Visual Studio\2022\Professional\VC\Tools\Llvm\x64\bin",
        r"C:\Program Files\Microsoft Visual Studio\2022\Enterprise\VC\Tools\Llvm\x64\bin",
    ];
    for root in roots {
        if let Some(found) = find_program_in(Path::new(root), "clangd") {
            return Some(found);
        }
    }
    if let Some(local) = std::env::var_os("LOCALAPPDATA") {
        if let Some(found) = find_program_in(
            &PathBuf::from(local)
                .join("Programs")
                .join("LLVM")
                .join("bin"),
            "clangd",
        ) {
            return Some(found);
        }
    }
    // The VS Code clangd extension ships a working clangd binary.
    if let Some(home) = std::env::var_os("USERPROFILE") {
        let home = PathBuf::from(home);
        for folder in [".vscode", ".vscode-insiders", ".cursor"] {
            if let Some(found) = find_extension_clangd(&home.join(folder).join("extensions")) {
                return Some(found);
            }
        }
    }
    None
}

fn find_extension_clangd(extensions: &Path) -> Option<PathBuf> {
    let mut candidates = fs::read_dir(extensions)
        .ok()?
        .flatten()
        .map(|entry| entry.path())
        .filter(|path| {
            path.file_name().is_some_and(|name| {
                name.to_string_lossy()
                    .starts_with("llvm-vs-code-extensions.vscode-clangd-")
            })
        })
        .collect::<Vec<_>>();
    candidates.sort();
    for candidate in candidates.into_iter().rev() {
        let Ok(entries) = fs::read_dir(candidate.join("install")) else {
            continue;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                if let Some(found) = find_program_in(&path.join("bin"), "clangd") {
                    return Some(found);
                }
            }
        }
    }
    None
}

fn find_program_in(dir: &Path, name: &str) -> Option<PathBuf> {
    for candidate in [format!("{name}.exe"), name.to_owned()] {
        let path = dir.join(candidate);
        if path.is_file() {
            return Some(path);
        }
    }
    None
}

/// Compile flags shared by every compilation database entry.
pub fn compile_flags(config: &WorkspaceConfig) -> Vec<String> {
    let arm = config
        .compiler
        .as_ref()
        .map(|path| {
            path.to_string_lossy()
                .to_ascii_lowercase()
                .contains("arm-none-eabi")
        })
        .unwrap_or(false)
        || config
            .cpu_flags
            .iter()
            .any(|flag| flag.starts_with("-mcpu=cortex-m"));
    let mut flags = Vec::new();
    if arm {
        flags.push("--target=arm-none-eabi".to_owned());
    }
    flags.push("-std=gnu11".to_owned());
    flags.push("-fno-limit-debug-info".to_owned());
    for flag in &config.cpu_flags {
        let flag = flag.trim();
        if flag.is_empty() || flags.iter().any(|existing| existing == flag) {
            continue;
        }
        flags.push(flag.to_owned());
    }
    for define in &config.defines {
        let define = define.trim();
        if define.is_empty() {
            continue;
        }
        if define.starts_with("-D") {
            flags.push(define.to_owned());
        } else {
            flags.push(format!("-D{define}"));
        }
    }
    for include in &config.includes {
        let include = include.trim();
        if include.is_empty() {
            continue;
        }
        let path = Path::new(include);
        let path = if path.is_absolute() {
            path.to_path_buf()
        } else {
            config.root.join(path)
        };
        flags.push(format!("-I{}", path.display()));
    }
    if let Some(compiler) = &config.compiler {
        for path in gcc_system_includes(compiler) {
            flags.push("-isystem".to_owned());
            flags.push(path.display().to_string());
        }
    }
    flags
}

fn compilation_entry(config: &WorkspaceConfig, file: &Path) -> Value {
    let flags = compile_flags(config);
    let mut arguments = Vec::with_capacity(flags.len() + 2);
    arguments.push("clang".to_owned());
    arguments.extend(flags);
    let language = match file
        .extension()
        .map(|value| value.to_string_lossy().to_ascii_lowercase())
        .as_deref()
    {
        Some("cpp") | Some("cc") | Some("cxx") => Some("c++"),
        _ => Some("c"),
    };
    if let Some(language) = language {
        arguments.push(format!("-x{language}"));
    }
    arguments.push(file.display().to_string());
    json!({
        "directory": config.root.display().to_string(),
        "file": file.display().to_string(),
        "arguments": arguments,
    })
}

/// Write a clangd compilation database for `config`.
pub fn write_compilation_database(
    config: &WorkspaceConfig,
    ignore: &crate::ignore::IgnoreRules,
) -> Result<PathBuf, String> {
    write_database_to(config, ignore, &database_dir(&config.root))
}

/// Write the compilation database to an explicit directory (used by tests).
pub fn write_database_to(
    config: &WorkspaceConfig,
    ignore: &crate::ignore::IgnoreRules,
    dir: &Path,
) -> Result<PathBuf, String> {
    fs::create_dir_all(dir).map_err(|err| format!("无法创建 clangd 目录：{err}"))?;
    let mut files = source_files(&config.root, ignore);
    files.truncate(MAX_DATABASE_FILES);
    let entries = files
        .iter()
        .map(|file| compilation_entry(config, file))
        .collect::<Vec<_>>();
    let text = serde_json::to_string_pretty(&Value::Array(entries))
        .map_err(|err| format!("无法生成 compile_commands.json：{err}"))?;
    let path = dir.join("compile_commands.json");
    fs::write(&path, text).map_err(|err| format!("无法写入 {}：{err}", path.display()))?;
    Ok(path)
}

pub fn database_dir(root: &Path) -> PathBuf {
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    root.hash(&mut hasher);
    let key = hasher.finish();
    crate::config::settings_path()
        .parent()
        .map(|parent| parent.join("clangd"))
        .unwrap_or_else(|| PathBuf::from("clangd"))
        .join(format!("{key:016x}"))
}

fn source_files(root: &Path, ignore: &crate::ignore::IgnoreRules) -> Vec<PathBuf> {
    let mut files = Vec::new();
    let mut stack = vec![root.to_path_buf()];
    while let Some(dir) = stack.pop() {
        let Ok(read_dir) = fs::read_dir(&dir) else {
            continue;
        };
        let mut entries = read_dir.flatten().collect::<Vec<_>>();
        entries.sort_by_key(|entry| entry.file_name());
        for entry in entries {
            let path = entry.path();
            let Ok(file_type) = entry.file_type() else {
                continue;
            };
            if file_type.is_symlink() {
                continue;
            }
            let relative = path.strip_prefix(root).unwrap_or(path.as_path());
            if file_type.is_dir() {
                if !ignore.is_ignored(relative, true) {
                    stack.push(path);
                }
                continue;
            }
            let extension = path
                .extension()
                .map(|value| value.to_string_lossy().to_ascii_lowercase())
                .unwrap_or_default();
            if file_type.is_file()
                && !ignore.is_ignored(relative, false)
                && matches!(extension.as_str(), "c" | "cc" | "cpp" | "cxx")
            {
                files.push(path);
            }
        }
    }
    // Prefer the project's own sources over vendor trees when hitting the cap.
    files.sort_by_key(|path| {
        let text = path.to_string_lossy().to_ascii_lowercase();
        text.contains("\\drivers\\")
            || text.contains("/drivers/")
            || text.contains("\\middlewares\\")
            || text.contains("/middlewares/")
            || text.contains("\\cmsis\\")
            || text.contains("/cmsis/")
    });
    files
}

/// Ask the ARM GCC driver for its system include directories.
pub fn gcc_system_includes(compiler: &Path) -> Vec<PathBuf> {
    let input = if cfg!(windows) { "NUL" } else { "/dev/null" };
    let output = Command::new(compiler)
        .args(["-E", "-Wp,-v", "-xc", input])
        .output();
    let Ok(output) = output else {
        return Vec::new();
    };
    let text = String::from_utf8_lossy(&output.stderr);
    let mut in_list = false;
    let mut paths = Vec::new();
    for line in text.lines() {
        let trimmed = line.trim();
        if trimmed.starts_with("#include <...> search starts here:") {
            in_list = true;
            continue;
        }
        if trimmed.starts_with("End of search list.") {
            break;
        }
        if !in_list || trimmed.is_empty() {
            continue;
        }
        let cleaned = trimmed
            .strip_suffix(" (framework directory)")
            .unwrap_or(trimmed);
        let path = PathBuf::from(cleaned);
        if path.is_dir() {
            paths.push(path);
        }
    }
    paths
}

/// Extract `-mcpu`, `-mfpu`, `-mfloat-abi` and `-mthumb` from a CubeMX Makefile.
pub fn makefile_cpu_flags(root: &Path) -> Vec<String> {
    let Ok(text) = fs::read_to_string(root.join("Makefile")) else {
        return Vec::new();
    };
    let mut vars = BTreeMap::new();
    for line in text.lines() {
        let Some((name, value)) = line.split_once('=') else {
            continue;
        };
        let name = name.trim();
        if name.is_empty()
            || name.contains(' ')
            || name.starts_with('#')
            || name.starts_with("ifeq")
        {
            continue;
        }
        vars.insert(
            name.to_owned(),
            value.trim().trim_end_matches('\\').trim().to_owned(),
        );
    }
    let expand = |value: &str| -> String {
        let mut out = value.to_owned();
        for _ in 0..8 {
            let mut changed = false;
            for (name, replacement) in &vars {
                let pattern = format!("$({name})");
                if out.contains(&pattern) {
                    out = out.replace(&pattern, replacement);
                    changed = true;
                }
            }
            if !changed {
                break;
            }
        }
        out
    };
    let mut flags = Vec::new();
    if let Some(mcu) = vars.get("MCU") {
        let expanded = expand(mcu);
        for token in expanded.split_whitespace() {
            if (token.starts_with("-mcpu=")
                || token.starts_with("-mfpu=")
                || token.starts_with("-mfloat-abi=")
                || token == "-mthumb")
                && !flags.iter().any(|flag: &String| flag == token)
            {
                flags.push(token.to_owned());
            }
        }
    }
    for name in ["CPU", "FPU", "FLOAT-ABI"] {
        if let Some(value) = vars.get(name) {
            let expanded = expand(value);
            for token in expanded.split_whitespace() {
                if (token.starts_with("-mcpu=")
                    || token.starts_with("-mfpu=")
                    || token.starts_with("-mfloat-abi="))
                    && !flags.iter().any(|flag: &String| flag == token)
                {
                    flags.push(token.to_owned());
                }
            }
        }
    }
    if flags.iter().any(|flag| flag.starts_with("-mcpu=")) && !flags.iter().any(|f| f == "-mthumb")
    {
        flags.push("-mthumb".to_owned());
    }
    flags
}

/// Find `arm-none-eabi-gcc` in the bundled tools, configured paths or PATH.
pub fn detect_arm_gcc(extra_paths: &[String]) -> Option<PathBuf> {
    let bundle = crate::toolchain::default_tools_dir()
        .join("compiler")
        .join("bin");
    if let Some(found) = find_program_in(&bundle, "arm-none-eabi-gcc") {
        return Some(found);
    }
    for extra in extra_paths {
        let dir = PathBuf::from(extra.trim());
        if let Some(found) = find_program_in(&dir, "arm-none-eabi-gcc") {
            return Some(found);
        }
    }
    if let Some(paths) = std::env::var_os("PATH") {
        for dir in std::env::split_paths(&paths) {
            if let Some(found) = find_program_in(&dir, "arm-none-eabi-gcc") {
                return Some(found);
            }
        }
    }
    None
}

pub struct Session {
    client: Client,
    pub status: Status,
    versions: BTreeMap<PathBuf, i32>,
    opened: BTreeSet<PathBuf>,
    last_sync: BTreeMap<PathBuf, (u64, Instant)>,
    pending: BTreeMap<i64, Pending>,
    last_hover: Option<(PathBuf, String, usize, Instant)>,
    last_completion: Option<Instant>,
}

impl Drop for Session {
    fn drop(&mut self) {
        self.client.stop();
    }
}

impl Session {
    pub fn start(
        clangd: &Path,
        config: &WorkspaceConfig,
        ignore: &crate::ignore::IgnoreRules,
    ) -> Result<Self, String> {
        let database = write_compilation_database(config, ignore)?;
        let database_dir = database
            .parent()
            .map(Path::to_path_buf)
            .unwrap_or_else(|| database.clone());
        let mut args = vec![
            format!("--compile-commands-dir={}", database_dir.display()),
            "--header-insertion=never".to_owned(),
            "--completion-style=detailed".to_owned(),
            "--log=error".to_owned(),
        ];
        if !cfg!(windows) {
            args.push("--background-index".to_owned());
        }
        let mut client = Client::spawn(clangd, &args, &config.root)?;
        let root_uri = path_to_uri(&config.root);
        let initialize_id = client.request(
            "initialize",
            json!({
                "processId": std::process::id(),
                "clientInfo": { "name": "Simple embedded" },
                "rootUri": root_uri,
                "workspaceFolders": [{
                    "uri": root_uri,
                    "name": config.root.file_name().map(|name| name.to_string_lossy().to_string()).unwrap_or_default(),
                }],
                "capabilities": {
                    "textDocument": {
                        "synchronization": { "dynamicRegistration": false },
                        "definition": { "linkSupport": true },
                        "references": {},
                        "hover": { "contentFormat": ["markdown", "plaintext"] },
                        "completion": {
                            "completionItem": {
                                "snippetSupport": false,
                                "documentationFormat": ["markdown", "plaintext"]
                            },
                            "contextSupport": false
                        },
                        "rename": { "prepareSupport": false },
                        "publishDiagnostics": { "relatedInformation": true, "versionSupport": true }
                    },
                    "workspace": {
                        "workspaceFolders": true,
                        "configuration": false,
                        "workspaceEdit": { "documentChanges": true }
                    }
                }
            }),
        )?;
        Ok(Self {
            client,
            status: Status::Starting,
            versions: BTreeMap::new(),
            opened: BTreeSet::new(),
            last_sync: BTreeMap::new(),
            pending: BTreeMap::from([(initialize_id, Pending::Initialize)]),
            last_hover: None,
            last_completion: None,
        })
    }

    pub fn is_ready(&self) -> bool {
        self.status == Status::Ready
    }

    /// True while the initialize handshake or a request response is pending.
    pub fn has_pending(&self) -> bool {
        self.status == Status::Starting || !self.pending.is_empty()
    }

    pub fn stop(&mut self) {
        self.client.stop();
        self.status = Status::Failed("clangd 已停止".to_owned());
    }

    pub fn sync(&mut self, docs: &[(PathBuf, u64)]) -> Vec<PathBuf> {
        if !self.is_ready() {
            return Vec::new();
        }
        // Never send Makefiles, Python files or project JSON to clangd. It
        // would parse them as C and produce a large diagnostics stream on every
        // keystroke, which made editing non-C files feel sluggish.
        let docs = docs
            .iter()
            .filter(|(path, _)| supports_lsp(path))
            .collect::<Vec<_>>();
        let now = Instant::now();
        let mut needed = Vec::new();
        let current = docs
            .iter()
            .map(|(path, _)| (*path).clone())
            .collect::<BTreeSet<_>>();
        let stale = self
            .opened
            .iter()
            .filter(|path| !current.contains(*path))
            .cloned()
            .collect::<Vec<_>>();
        for path in stale {
            self.opened.remove(&path);
            self.versions.remove(&path);
            self.last_sync.remove(&path);
            let uri = path_to_uri(&path);
            let _ = self.client.notify(
                "textDocument/didClose",
                json!({ "textDocument": { "uri": uri } }),
            );
        }
        for (path, revision) in docs {
            if !self.opened.contains(path) {
                needed.push(path.clone());
                continue;
            }
            let Some((last_revision, since)) = self.last_sync.get(path) else {
                needed.push(path.clone());
                continue;
            };
            if *last_revision != *revision && now.duration_since(*since) >= SYNC_DELAY {
                needed.push(path.clone());
            }
        }
        needed
    }

    pub fn feed(&mut self, path: &Path, text: &str, revision: u64) {
        if !supports_lsp(path) {
            return;
        }
        let uri = path_to_uri(path);
        if !self.opened.contains(path) {
            self.opened.insert(path.to_path_buf());
            self.versions.insert(path.to_path_buf(), 1);
            let language = language_id(path);
            let _ = self.client.notify(
                "textDocument/didOpen",
                json!({
                    "textDocument": {
                        "uri": uri,
                        "languageId": language,
                        "version": 1,
                        "text": text,
                    }
                }),
            );
        } else {
            let version = self.versions.entry(path.to_path_buf()).or_insert(1);
            *version += 1;
            let version = *version;
            let _ = self.client.notify(
                "textDocument/didChange",
                json!({
                    "textDocument": { "uri": uri, "version": version },
                    "contentChanges": [{ "text": text }],
                }),
            );
        }
        self.last_sync
            .insert(path.to_path_buf(), (revision, Instant::now()));
    }

    pub fn definition(&mut self, path: &Path, text: &str, cursor: usize) -> Result<(), String> {
        if !supports_lsp(path) {
            return Err("当前文件不是 C/C++，使用轻量符号索引".to_owned());
        }
        self.text_request(
            "textDocument/definition",
            path,
            text,
            cursor,
            Pending::Definition,
        )
    }

    pub fn references(&mut self, path: &Path, text: &str, cursor: usize) -> Result<(), String> {
        if !supports_lsp(path) {
            return Err("当前文件不是 C/C++，使用工程搜索".to_owned());
        }
        if !self.is_ready() {
            return Err("clangd 尚未就绪".to_owned());
        }
        let word = crate::navigation::word_at(text, cursor)
            .map(|value| value.0)
            .unwrap_or_default();
        let mut params = text_position_params(path, text, cursor);
        params["context"] = json!({ "includeDeclaration": true });
        let id = self.client.request("textDocument/references", params)?;
        self.pending.insert(id, Pending::References { query: word });
        Ok(())
    }

    pub fn hover(&mut self, path: &Path, text: &str, cursor: usize) -> Result<(), String> {
        if !self.is_ready() || !supports_lsp(path) {
            return Ok(());
        }
        let line_column = char_to_position(text, cursor);
        let word = crate::navigation::word_at(text, cursor)
            .map(|value| value.0)
            .unwrap_or_default();
        if word.is_empty() {
            return Ok(());
        }
        let key = (path.to_path_buf(), word.clone(), line_column.line);
        if self
            .last_hover
            .as_ref()
            .is_some_and(|(path, w, line, _)| *path == key.0 && *w == key.1 && *line == key.2)
        {
            return Ok(());
        }
        self.last_hover = Some((key.0.clone(), key.1.clone(), key.2, Instant::now()));
        let params = text_position_params(path, text, cursor);
        let id = self.client.request("textDocument/hover", params)?;
        self.pending.insert(
            id,
            Pending::Hover {
                path: path.to_path_buf(),
                word,
                line: line_column.line + 1,
            },
        );
        Ok(())
    }

    pub fn completion(
        &mut self,
        path: &Path,
        text: &str,
        cursor: usize,
        revision: u64,
        prefix_start: usize,
    ) -> Result<(), String> {
        if !self.is_ready() || !supports_lsp(path) {
            return Ok(());
        }
        if self
            .last_completion
            .is_some_and(|at| at.elapsed() < COMPLETION_DELAY)
        {
            return Ok(());
        }
        self.last_completion = Some(Instant::now());
        let params = text_position_params(path, text, cursor);
        let id = self.client.request("textDocument/completion", params)?;
        self.pending.insert(
            id,
            Pending::Completion {
                path: path.to_path_buf(),
                cursor,
                revision,
                prefix_start,
            },
        );
        Ok(())
    }

    pub fn rename(
        &mut self,
        path: &Path,
        text: &str,
        cursor: usize,
        new_name: &str,
    ) -> Result<(), String> {
        if !supports_lsp(path) {
            return Err("当前文件不是 C/C++".to_owned());
        }
        let mut params = text_position_params(path, text, cursor);
        params["newName"] = Value::String(new_name.to_owned());
        let id = self.client.request("textDocument/rename", params)?;
        self.pending.insert(id, Pending::Rename);
        Ok(())
    }

    fn text_request(
        &mut self,
        method: &str,
        path: &Path,
        text: &str,
        cursor: usize,
        pending: Pending,
    ) -> Result<(), String> {
        if !self.is_ready() {
            return Err("clangd 尚未就绪".to_owned());
        }
        let params = text_position_params(path, text, cursor);
        let id = self.client.request(method, params)?;
        self.pending.insert(id, pending);
        Ok(())
    }

    pub fn poll(&mut self) -> Vec<Event> {
        let mut events = Vec::new();
        while let Some(message) = self.client.try_recv() {
            if let Some(method) = message.get("method").and_then(Value::as_str) {
                if message.get("id").is_some() {
                    // Server-initiated request: answer with null so clangd does
                    // not wait forever. Diagnostics are the only notification
                    // this client consumes.
                    let id = message.get("id").cloned().unwrap_or(Value::Null);
                    let _ = self.client.send(&json!({
                        "jsonrpc": "2.0",
                        "id": id,
                        "result": Value::Null,
                    }));
                    continue;
                }
                if method == "window/showMessage" {
                    let text = message
                        .pointer("/params/message")
                        .and_then(Value::as_str)
                        .unwrap_or_default()
                        .to_owned();
                    if !text.is_empty() {
                        events.push(Event::Error(text));
                    }
                }
                continue;
            }
            let Some(id) = message.get("id").and_then(Value::as_i64) else {
                continue;
            };
            let Some(pending) = self.pending.remove(&id) else {
                continue;
            };
            if let Some(error) = message.get("error") {
                let text = error
                    .get("message")
                    .and_then(Value::as_str)
                    .unwrap_or("clangd 请求失败");
                events.push(Event::Error(text.to_owned()));
                continue;
            }
            let result = message.get("result").cloned().unwrap_or(Value::Null);
            match pending {
                Pending::Initialize => {
                    self.status = Status::Ready;
                    let _ = self.client.notify("initialized", json!({}));
                    events.push(Event::Ready);
                }
                Pending::Definition => {
                    events.push(Event::Definition(parse_locations(&result)));
                }
                Pending::References { query } => {
                    events.push(Event::References(RefsView {
                        title: "LSP 引用".to_owned(),
                        query,
                        items: parse_locations(&result),
                    }));
                }
                Pending::Hover { path, word, line } => {
                    let text = hover_text(&result);
                    if !text.is_empty() {
                        events.push(Event::Hover(HoverView {
                            path,
                            word,
                            line,
                            text,
                        }));
                    }
                }
                Pending::Completion {
                    path,
                    cursor,
                    revision,
                    prefix_start,
                } => {
                    events.push(Event::Completion(CompletionView {
                        path,
                        cursor,
                        revision,
                        prefix_start,
                        items: parse_completion(&result),
                    }));
                }
                Pending::Rename => {
                    events.push(Event::Rename(parse_workspace_edit(&result)));
                }
            }
        }
        if self.status == Status::Starting && self.client.exited() {
            self.status = Status::Failed("clangd 进程已退出".to_owned());
            events.push(Event::Failed("clangd 进程已退出".to_owned()));
        }
        events
    }
}

/// clangd only understands C/C++ translation units; feeding Makefiles or
/// Python files to it makes it parse them as C and burns CPU on diagnostics.
pub fn supports_lsp(path: &Path) -> bool {
    matches!(
        path.extension()
            .map(|value| value.to_string_lossy().to_ascii_lowercase())
            .as_deref(),
        Some("c" | "h" | "cc" | "cpp" | "cxx" | "hh" | "hpp" | "hxx" | "ino")
    )
}

fn language_id(path: &Path) -> &'static str {
    match path
        .extension()
        .map(|value| value.to_string_lossy().to_ascii_lowercase())
        .as_deref()
    {
        Some("cpp") | Some("cc") | Some("cxx") | Some("hpp") | Some("hxx") => "cpp",
        _ => "c",
    }
}

fn text_position_params(path: &Path, text: &str, cursor: usize) -> Value {
    let position = char_to_position(text, cursor);
    json!({
        "textDocument": { "uri": path_to_uri(path) },
        "position": {
            "line": position.line,
            "character": position.character,
        }
    })
}

pub fn char_to_position(text: &str, char_index: usize) -> LspPosition {
    let chars = text.chars().collect::<Vec<_>>();
    let char_index = char_index.min(chars.len());
    let mut line = 0usize;
    let mut character = 0usize;
    for ch in &chars[..char_index] {
        if *ch == '\n' {
            line += 1;
            character = 0;
        } else {
            character += ch.len_utf16();
        }
    }
    LspPosition { line, character }
}

pub fn position_to_char(text: &str, position: &LspPosition) -> usize {
    let mut line = 0usize;
    let mut character = 0usize;
    let mut index = 0usize;
    for ch in text.chars() {
        if line == position.line && character >= position.character {
            return index;
        }
        if ch == '\n' {
            if line == position.line {
                return index;
            }
            line += 1;
            character = 0;
        } else {
            character += ch.len_utf16();
        }
        index += 1;
    }
    index
}

pub fn path_to_uri(path: &Path) -> String {
    let text = path.to_string_lossy().replace('\\', "/");
    let mut out = String::from("file:///");
    if let Some(rest) = text.strip_prefix('/') {
        out.push_str(rest);
    } else {
        out.push_str(&text);
    }
    let mut encoded = String::new();
    for byte in out.bytes() {
        match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'.' | b'_' | b'~' | b'/' | b':' => {
                encoded.push(byte as char)
            }
            _ => encoded.push_str(&format!("%{byte:02X}")),
        }
    }
    encoded
}

pub fn uri_to_path(uri: &str) -> Option<PathBuf> {
    let rest = uri.strip_prefix("file://")?;
    let rest = rest.strip_prefix('/').unwrap_or(rest);
    let mut bytes = Vec::new();
    let raw = rest.as_bytes();
    let mut index = 0;
    while index < raw.len() {
        if raw[index] == b'%' && index + 2 < raw.len() {
            if let Ok(value) = u8::from_str_radix(&rest[index + 1..index + 3], 16) {
                bytes.push(value);
                index += 3;
                continue;
            }
        }
        bytes.push(raw[index]);
        index += 1;
    }
    let text = String::from_utf8_lossy(&bytes).replace('/', "\\");
    let text = text.strip_prefix('\\').map(str::to_owned).unwrap_or(text);
    Some(PathBuf::from(text))
}

fn parse_locations(value: &Value) -> Vec<Location> {
    let mut items = Vec::new();
    match value {
        Value::Array(array) => {
            for item in array {
                items.push(parse_location(item));
            }
        }
        Value::Null => {}
        _ => items.push(parse_location(value)),
    }
    items.into_iter().flatten().collect()
}

fn parse_location(value: &Value) -> Option<Location> {
    let uri = value
        .get("uri")
        .or_else(|| value.get("targetUri"))
        .and_then(Value::as_str)?;
    let path = uri_to_path(uri)?;
    let range = value
        .get("range")
        .or_else(|| value.get("targetSelectionRange"))
        .or_else(|| value.get("targetRange"))?;
    let line = range.pointer("/start/line").and_then(Value::as_u64)? as usize;
    let character = range
        .pointer("/start/character")
        .and_then(Value::as_u64)
        .unwrap_or(0) as usize;
    Some(Location {
        path,
        line: line + 1,
        column: character + 1,
        preview: String::new(),
    })
}

fn hover_text(value: &Value) -> String {
    let contents = value.get("contents").unwrap_or(value);
    let mut text = String::new();
    match contents {
        Value::String(value) => text.push_str(value),
        Value::Array(items) => {
            for item in items {
                let part = item
                    .get("value")
                    .and_then(Value::as_str)
                    .or_else(|| item.as_str())
                    .unwrap_or_default();
                if !part.is_empty() {
                    if !text.is_empty() {
                        text.push('\n');
                    }
                    text.push_str(part);
                }
            }
        }
        Value::Object(_) => {
            text.push_str(
                contents
                    .get("value")
                    .and_then(Value::as_str)
                    .unwrap_or_default(),
            );
        }
        _ => {}
    }
    clean_markdown(&text)
}

/// Turn clangd markdown into compact plain text for the hover tooltip.
pub fn clean_markdown(markdown: &str) -> String {
    let mut lines = Vec::new();
    for line in markdown.lines() {
        let trimmed = line.trim();
        if trimmed.starts_with("```") {
            continue;
        }
        if trimmed.is_empty() {
            if lines.last().is_some_and(|line: &String| !line.is_empty()) {
                lines.push(String::new());
            }
            continue;
        }
        let mut cleaned = trimmed.replace("**", "").replace('`', "");
        if let Some(rest) = cleaned.strip_prefix("- ") {
            cleaned = format!("• {rest}");
        }
        lines.push(cleaned);
    }
    while lines.last().is_some_and(|line| line.is_empty()) {
        lines.pop();
    }
    lines.join("\n")
}

fn parse_completion(value: &Value) -> Vec<CompletionEntry> {
    let items = value
        .get("items")
        .and_then(Value::as_array)
        .or_else(|| value.as_array())
        .cloned()
        .unwrap_or_default();
    items
        .into_iter()
        .filter_map(|item| {
            let label = item
                .pointer("/textEdit/newText")
                .and_then(Value::as_str)
                .or_else(|| item.get("insertText").and_then(Value::as_str))
                .or_else(|| item.get("label").and_then(Value::as_str))?
                .to_owned();
            let detail = item
                .get("detail")
                .and_then(Value::as_str)
                .map(str::to_owned);
            let kind = item.get("kind").and_then(Value::as_u64).unwrap_or(0) as u32;
            Some(CompletionEntry {
                label,
                detail,
                kind,
            })
        })
        .collect()
}

fn parse_workspace_edit(value: &Value) -> Vec<FileEdits> {
    let mut files: BTreeMap<PathBuf, Vec<TextEdit>> = BTreeMap::new();
    if let Some(changes) = value.get("changes").and_then(Value::as_object) {
        for (uri, edits) in changes {
            let Some(path) = uri_to_path(uri) else {
                continue;
            };
            for edit in edits.as_array().into_iter().flatten() {
                if let Some(edit) = parse_text_edit(edit) {
                    files.entry(path.clone()).or_default().push(edit);
                }
            }
        }
    }
    if let Some(changes) = value.get("documentChanges").and_then(Value::as_array) {
        for change in changes {
            let Some(uri) = change.pointer("/textDocument/uri").and_then(Value::as_str) else {
                continue;
            };
            let Some(path) = uri_to_path(uri) else {
                continue;
            };
            for edit in change
                .get("edits")
                .and_then(Value::as_array)
                .into_iter()
                .flatten()
            {
                if let Some(edit) = parse_text_edit(edit) {
                    files.entry(path.clone()).or_default().push(edit);
                }
            }
        }
    }
    files
        .into_iter()
        .filter(|(_, edits)| !edits.is_empty())
        .map(|(path, edits)| FileEdits { path, edits })
        .collect()
}

fn parse_text_edit(value: &Value) -> Option<TextEdit> {
    let start = value.get("range")?.get("start")?;
    let end = value.get("range")?.get("end")?;
    let position = |value: &Value| LspPosition {
        line: value.get("line").and_then(Value::as_u64).unwrap_or(0) as usize,
        character: value.get("character").and_then(Value::as_u64).unwrap_or(0) as usize,
    };
    Some(TextEdit {
        start: position(start),
        end: position(end),
        new_text: value
            .get("newText")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_owned(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn compilation_entry_carries_includes_defines_and_arm_target() {
        let root = PathBuf::from(r"D:\demo");
        let config = WorkspaceConfig {
            root: root.clone(),
            includes: vec!["Core/Inc".to_owned()],
            defines: vec!["USE_HAL_DRIVER".to_owned(), "-DSTM32G431xx".to_owned()],
            cpu_flags: vec!["-mcpu=cortex-m4".to_owned(), "-mthumb".to_owned()],
            compiler: None,
        };
        let entry = compilation_entry(&config, &root.join("Core").join("Src").join("main.c"));
        let args = entry["arguments"].as_array().unwrap();
        let text = args
            .iter()
            .filter_map(Value::as_str)
            .collect::<Vec<_>>()
            .join(" ");
        assert!(text.contains("--target=arm-none-eabi"));
        assert!(text.contains("-mcpu=cortex-m4"));
        assert!(text.contains("-DUSE_HAL_DRIVER"));
        assert!(text.contains("-DSTM32G431xx"));
        let normalized = text.replace('\\', "/");
        assert!(normalized.contains("-ID:/demo/Core/Inc"));
    }

    #[test]
    fn only_c_family_files_go_to_clangd() {
        assert!(supports_lsp(Path::new("Core/Src/main.c")));
        assert!(supports_lsp(Path::new("BSP/bsp_usart.h")));
        assert!(supports_lsp(Path::new("lib/foo.cpp")));
        assert!(!supports_lsp(Path::new("Makefile")));
        assert!(!supports_lsp(Path::new("main.py")));
        assert!(!supports_lsp(Path::new(".simple-embedded/settings.json")));
    }

    #[test]
    fn writes_a_compilation_database_from_a_cube_makefile() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        fs::create_dir_all(root.join("Core").join("Src")).unwrap();
        fs::create_dir_all(root.join("Core").join("Inc")).unwrap();
        fs::write(
            root.join("Makefile"),
            "CPU = -mcpu=cortex-m4\nFPU = -mfpu=fpv4-sp-d16\nFLOAT-ABI = -mfloat-abi=hard\nMCU = $(CPU) -mthumb $(FPU) $(FLOAT-ABI)\n",
        )
        .unwrap();
        fs::write(
            root.join("Core").join("Src").join("main.c"),
            "int main(void) { return 0; }\n",
        )
        .unwrap();
        fs::write(
            root.join("Core").join("Inc").join("usart.h"),
            "#pragma once\n",
        )
        .unwrap();
        let ignore = crate::ignore::IgnoreRules::load(root, false, "");
        let config = WorkspaceConfig {
            root: root.to_path_buf(),
            includes: vec!["Core/Inc".to_owned()],
            defines: vec!["USE_HAL_DRIVER".to_owned()],
            cpu_flags: makefile_cpu_flags(root),
            compiler: None,
        };
        let path = write_database_to(&config, &ignore, &root.join("db")).unwrap();
        let value: Value = serde_json::from_slice(&fs::read(path).unwrap()).unwrap();
        let entry = &value.as_array().unwrap()[0];
        let arguments = entry["arguments"]
            .as_array()
            .unwrap()
            .iter()
            .filter_map(Value::as_str)
            .collect::<Vec<_>>()
            .join(" ");
        assert!(arguments.contains("main.c"));
        assert!(arguments.contains("--target=arm-none-eabi"));
        assert!(arguments.contains("-mcpu=cortex-m4"));
        assert!(arguments.contains("-DUSE_HAL_DRIVER"));
        assert!(arguments.replace('\\', "/").contains("Core/Inc"));
    }

    #[test]
    fn parses_location_links_and_plain_locations() {
        let value = json!([
            {
                "uri": "file:///D:/demo/Core/Src/main.c",
                "range": { "start": { "line": 9, "character": 4 }, "end": { "line": 9, "character": 8 } }
            },
            {
                "targetUri": "file:///D:/demo/Core/Inc/usart.h",
                "targetSelectionRange": { "start": { "line": 2, "character": 0 }, "end": { "line": 2, "character": 5 } }
            }
        ]);
        let items = parse_locations(&value);
        assert_eq!(items.len(), 2);
        assert_eq!(items[0].line, 10);
        assert_eq!(items[1].path, PathBuf::from(r"D:\demo\Core\Inc\usart.h"));
    }

    #[test]
    fn hover_markdown_drops_fences() {
        let text = clean_markdown("```c\nvoid usart_test(void)\n```\n---\n发送测试数据");
        assert!(text.contains("void usart_test(void)"));
        assert!(text.contains("发送测试数据"));
        assert!(!text.contains("```"));
    }

    #[test]
    fn utf16_positions_and_uri_round_trip() {
        let text = "中文 x\nint y;";
        let index = text.chars().position(|ch| ch == 'y').unwrap();
        let position = char_to_position(text, index);
        assert_eq!(position.line, 1);
        assert_eq!(position.character, 4);
        assert_eq!(position_to_char(text, &position), index);
        let path = PathBuf::from(r"D:\demo\Core\Src\main.c");
        assert_eq!(uri_to_path(&path_to_uri(&path)), Some(path));
    }

    #[test]
    fn workspace_edit_reads_changes_and_document_changes() {
        let value = json!({
            "changes": {
                "file:///D:/demo/a.c": [{
                    "range": { "start": { "line": 0, "character": 1 }, "end": { "line": 0, "character": 3 } },
                    "newText": "xy"
                }]
            }
        });
        let files = parse_workspace_edit(&value);
        assert_eq!(files.len(), 1);
        assert_eq!(files[0].edits[0].new_text, "xy");
    }

    #[test]
    fn makefile_cpu_flags_expand_mcu_variable() {
        let dir = std::env::temp_dir().join(format!("simple-embedded-lsp-{}", std::process::id()));
        let _ = fs::create_dir_all(&dir);
        fs::write(
            dir.join("Makefile"),
            "CPU = -mcpu=cortex-m4\nFPU = -mfpu=fpv4-sp-d16\nFLOAT-ABI = -mfloat-abi=hard\nMCU = $(CPU) -mthumb $(FPU) $(FLOAT-ABI)\n",
        )
        .unwrap();
        let flags = makefile_cpu_flags(&dir);
        assert!(flags.contains(&"-mcpu=cortex-m4".to_owned()));
        assert!(flags.contains(&"-mthumb".to_owned()));
        assert!(flags.contains(&"-mfloat-abi=hard".to_owned()));
        let _ = fs::remove_dir_all(&dir);
    }
}
