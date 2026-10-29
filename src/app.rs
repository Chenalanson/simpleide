//! Simple embedded application shell: workspace, setup steps, tabs and build output.

mod ai_ui;
mod ui;

use std::collections::{BTreeMap, BTreeSet};
use std::hash::{Hash, Hasher};
use std::path::{Path, PathBuf};
use std::sync::mpsc::{self, Receiver};
use std::sync::Arc;
use std::time::{Duration, Instant, SystemTime};

use eframe::egui::{
    self, Align, Button, Color32, CornerRadius, FontId, Frame, Id, Label, Layout, Margin, RichText,
    ScrollArea, Sense, Stroke, TextEdit, ViewportCommand,
};
#[cfg(target_os = "windows")]
use raw_window_handle::{HasWindowHandle, RawWindowHandle};

use crate::build::{BuildController, BuildOutcome, LastBuild, TaskKind};
use crate::config::{
    detected_tool, settings_path, BuildConfig, BuildOptions, BuildSystem, BuildTarget, Settings,
    ToolchainKind, APP_AUTHOR, APP_NAME,
};
use crate::editor::Document;
use crate::file_tree::{FileTree, TreeAction};
use crate::flash::{self, FlashTool, OpenOcdConfig, PyOcdConfig};
use crate::formatting;
use crate::git::{self, GitStatus};
use crate::highlight::{self, Highlighter};
use crate::ignore::IgnoreRules;
use crate::log::{LogFilter, Severity};
use crate::makefile;
use crate::project_meta::ProjectMeta;
use crate::search::{search_project, SearchHit, SearchOptions};
use crate::theme;
use crate::util::{self, format_duration, project_key, TextEncoding};

struct Project {
    root: PathBuf,
    name: String,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum SplitDirection {
    Right,
    Left,
    Up,
    Down,
}

impl Project {
    fn new(root: PathBuf) -> Self {
        let name = root
            .file_name()
            .map(|name| name.to_string_lossy().into_owned())
            .filter(|name| !name.is_empty())
            .unwrap_or_else(|| root.display().to_string());
        Self { root, name }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum SettingsTab {
    Build,
    Editor,
    Debug,
    Other,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
enum OutputView {
    #[default]
    Build,
    Debug,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum MicroPythonAction {
    Run,
    Upload,
    Reset,
}

impl MicroPythonAction {
    fn label(self) -> &'static str {
        match self {
            Self::Run => "运行",
            Self::Upload => "上传",
            Self::Reset => "复位",
        }
    }
}

enum MicroPythonJobResult {
    Report(Result<crate::micropython::Report, String>),
    Files(Result<Vec<crate::micropython::BoardFile>, String>),
    File {
        result: Result<Vec<u8>, String>,
        remote_path: String,
        local_path: PathBuf,
    },
}

#[derive(Clone)]
struct RenameDialog {
    path: PathBuf,
    cursor: usize,
    original: String,
    name: String,
    error: Option<String>,
}

struct SettingsDraft {
    tab: SettingsTab,
    config: BuildConfig,
    extra_paths_text: String,
    font_size: f32,
    word_wrap: bool,
    relative_line_numbers: bool,
    show_whitespace: bool,
    check_duplicate_macro_values: bool,
    ruler_column: u16,
    use_spaces: bool,
    indent_width: u8,
    autosave: bool,
    autosave_delay_ms: u64,
    undo_limit: usize,
    format_on_save: bool,
    clang_format_path: String,
    clangd_enabled: bool,
    clangd_path: String,
    debug_program: String,
    gdb_path: String,
    auto_scroll_output: bool,
    show_minimap: bool,
    show_indent_guides: bool,
    show_hover_info: bool,
    show_hidden: bool,
    respect_gitignore: bool,
    ignore_patterns: String,
    serial_ports: Vec<crate::serial::PortChoice>,
    serial_note: Option<String>,
    note: Option<String>,
    error: Option<String>,
}

impl SettingsDraft {
    fn refresh_serial_ports(&mut self, select_best: bool) {
        match crate::serial::scan() {
            Ok(ports) => {
                let chosen = if select_best || self.config.serial.port.trim().is_empty() {
                    crate::serial::preferred_port(&ports).cloned()
                } else {
                    None
                };
                self.serial_note = if let Some(choice) = &chosen {
                    self.config.serial.port = choice.port.clone();
                    Some(if choice.likely_esp32 {
                        format!("已选择 {}（ESP32 / USB 串口）", choice.label)
                    } else {
                        format!("已选择 {}", choice.label)
                    })
                } else if ports.is_empty() {
                    Some("未检测到串口；请检查 ESP32 的 USB 数据线和串口驱动，或点端口下拉手动输入。".to_owned())
                } else if let Some(choice) = crate::serial::preferred_port(&ports) {
                    Some(format!(
                        "检测到 {} 个串口；当前端口 {}，可点击自动检测切换到 {}",
                        ports.len(),
                        self.config.serial.port.trim(),
                        choice.port
                    ))
                } else {
                    Some(format!("检测到 {} 个串口", ports.len()))
                };
                self.serial_ports = ports;
            }
            Err(err) => {
                self.serial_ports.clear();
                self.serial_note = Some(err);
            }
        }
    }
}

struct MetaDraft {
    includes: String,
    library_dirs: String,
    defines: String,
    note: Option<String>,
}

struct FlashDraft {
    tool: FlashTool,
    command: String,
    openocd: OpenOcdConfig,
    pyocd: PyOcdConfig,
    error: Option<String>,
}

struct ThemeSettingsDraft {
    selected: String,
    original: String,
    filter: String,
}

impl FlashDraft {
    fn from_config(config: &BuildConfig) -> Self {
        Self {
            tool: config.flash_tool,
            command: config.flash_command.clone(),
            openocd: config.openocd.clone(),
            pyocd: config.pyocd.clone(),
            error: None,
        }
    }
}

#[derive(Default)]
struct ProjectSearchState {
    open: bool,
    query: String,
    case_sensitive: bool,
    use_regex: bool,
    results: Vec<SearchHit>,
    error: Option<String>,
    truncated: bool,
    scanned_files: usize,
    searched: bool,
    focus_query: bool,
}

#[derive(Default)]
struct TodoState {
    open: bool,
    items: Vec<crate::todo::TodoItem>,
    truncated: bool,
    error: Option<String>,
    filter: Option<crate::todo::TodoKind>,
}

const MAX_ARRAY_BYTES: usize = 8192;

struct ArrayState {
    open: bool,
    name: String,
    hex: bool,
    per_line: usize,
    source: Vec<u8>,
    source_label: String,
    truncated: bool,
}

impl Default for ArrayState {
    fn default() -> Self {
        Self {
            open: false,
            name: "data".to_owned(),
            hex: true,
            per_line: 16,
            source: Vec::new(),
            source_label: String::new(),
            truncated: false,
        }
    }
}

#[derive(Default)]
struct HexState {
    open: bool,
    dump: crate::hexview::HexDump,
    error: Option<String>,
}

struct ColumnEditState {
    open: bool,
    line_start: usize,
    line_end: usize,
    column: usize,
    count: usize,
    operation: crate::column::ColumnOperation,
    insert: String,
    source_label: String,
}

impl Default for ColumnEditState {
    fn default() -> Self {
        Self {
            open: false,
            line_start: 0,
            line_end: 0,
            column: 1,
            count: 1,
            operation: crate::column::ColumnOperation::Insert,
            insert: String::new(),
            source_label: String::new(),
        }
    }
}

#[derive(Clone, Debug)]
struct ColumnSelectionState {
    path: PathBuf,
    anchor_line: usize,
    anchor_col: usize,
    cursor_line: usize,
    cursor_col: usize,
}

#[derive(Clone, Debug)]
struct MultiCursorState {
    path: PathBuf,
    /// Primary caret (head); the actual primary selection lives in egui.
    primary: usize,
    /// Secondary selections as normalized `(start, end)` character ranges.
    secondary: Vec<(usize, usize)>,
}

#[derive(Clone)]
struct UndoSnapshot {
    revision: u64,
    text: String,
    cursor: usize,
}

#[derive(Clone)]
struct UndoEntry {
    path: PathBuf,
    before_text: String,
    after_text: String,
    before_cursor: usize,
    after_cursor: usize,
}

#[derive(Default)]
struct UndoState {
    undo: Vec<UndoEntry>,
    redo: Vec<UndoEntry>,
    snapshots: std::collections::BTreeMap<PathBuf, UndoSnapshot>,
    applying: bool,
}

#[derive(Default)]
struct GotoLineState {
    open: bool,
    input: String,
    error: Option<String>,
    focus_input: bool,
}

#[derive(Default)]
struct CompletionState {
    open: bool,
    query: String,
    prefix_start: usize,
    cursor: usize,
    selected: usize,
    items: Vec<crate::completion::CompletionItem>,
    member_chain: Option<String>,
    anchor: Option<egui::Pos2>,
    path: Option<PathBuf>,
    last_revision: u64,
    last_cursor: usize,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
enum DebugSideTab {
    #[default]
    Variables,
    Registers,
    Peripherals,
}

#[derive(Clone, Debug, Default)]
struct WatchEntry {
    expr: String,
    value: String,
    type_name: String,
}

impl WatchEntry {
    fn new(expr: String) -> Self {
        Self {
            expr: expr.trim().replace(['\n', '\r'], " "),
            value: String::new(),
            type_name: String::new(),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum WatchSection {
    None,
    Value(usize),
    Type(usize),
    Registers,
}

#[derive(Clone, Debug, Default)]
struct RegisterValue {
    name: String,
    value: String,
    raw: String,
}

fn marker_number(line: &str, prefix: &str) -> Option<u64> {
    let start = line.find(prefix)? + prefix.len();
    let rest = &line[start..];
    let end = rest.find("__")?;
    rest[..end].parse().ok()
}

/// Extract the value part of a gdb `print` response such as
/// `$1 = 0 '\000'`. Multi-line struct output is intentionally limited to its
/// first line; watchpoint expressions are normally scalars.
fn parse_print_value(line: &str) -> Option<String> {
    let line = line.trim().trim_start_matches("(gdb) ").trim();
    let (name, value) = line.split_once(" = ")?;
    if !name.starts_with('$') {
        return None;
    }
    let value = value.trim();
    if value.is_empty() {
        return None;
    }
    Some(value.to_owned())
}

#[derive(Clone)]
struct PeripheralEditDraft {
    peripheral: String,
    register: crate::registers::SvdRegister,
    address: u64,
    field: Option<crate::registers::SvdField>,
    input: String,
    register_value: Option<u64>,
    error: Option<String>,
}

#[derive(Default)]
struct DebugState {
    open: bool,
    session: Option<crate::debugger::DebugSession>,
    input: String,
    error: Option<String>,
    breakpoints: BTreeSet<(PathBuf, usize)>,
    target: Option<PathBuf>,
    log: Vec<String>,
    starting: bool,
    pending_continue: bool,
    /// Target is executing and the primary debug button should show pause.
    running: bool,
    watch_tab: DebugSideTab,
    watches: Vec<WatchEntry>,
    watch_input: String,
    watch_filter: String,
    register_filter: String,
    registers: Vec<RegisterValue>,
    watch_refresh: Option<u64>,
    watch_refresh_id: u64,
    last_watch_refresh: Option<Instant>,
    auto_watch: bool,
    side_width: f32,
    peripheral_filter: String,
    peripheral_values: BTreeMap<u64, String>,
    peripheral_open: BTreeSet<String>,
    peripheral_registers_open: BTreeSet<String>,
    peripheral_edit: Option<PeripheralEditDraft>,
    peripheral_initialized: bool,
    peripheral_refresh: Option<u64>,
    peripheral_id: u64,
    peripheral_last_refresh: Option<Instant>,
    /// Last source location reported by gdb, used for the execution marker.
    current_source: Option<(PathBuf, usize)>,
    /// Number of console lines already scanned for stop locations.
    log_scan: usize,
    /// Arm saved watchpoints only after the first real stop (normally `main`),
    /// so startup code that clears `.bss` does not trip them.
    pending_watch_rearm: bool,
    /// Ignore late reset/load log lines until gdb reports the temporary `main`
    /// breakpoint after an automatic start.
    waiting_for_main: bool,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
enum MemoryCheckTab {
    #[default]
    Static,
    Dynamic,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
enum WatchKind {
    #[default]
    Write,
    Read,
    Access,
}

impl WatchKind {
    fn command(self) -> &'static str {
        match self {
            Self::Write => "watch",
            Self::Read => "rwatch",
            Self::Access => "awatch",
        }
    }

    fn label(self) -> &'static str {
        match self {
            Self::Write => "写入",
            Self::Read => "读取",
            Self::Access => "读写",
        }
    }
}

#[derive(Clone, Debug)]
struct WatchEvent {
    id: u64,
    time: String,
    old_value: Option<String>,
    new_value: Option<String>,
    location: Option<String>,
    stack: Vec<String>,
}

#[derive(Clone, Debug)]
struct HwWatchpoint {
    /// Stable id used for GDB convenience variables such as `$__se_wp_7`.
    uid: u64,
    number: Option<usize>,
    expression: String,
    kind: WatchKind,
    hardware: Option<bool>,
    hits: usize,
    old_value: Option<String>,
    new_value: Option<String>,
    history: Vec<WatchEvent>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum WatchProbeRole {
    Old,
    New,
}

#[derive(Clone, Debug)]
struct WatchProbeRequest {
    query_id: u64,
    watch_uid: u64,
    role: WatchProbeRole,
}

/// OpenOCD reports Cortex-M watchpoint hits as `SIGTRAP` instead of gdb's
/// `Old value / New value` pair. After such a stop we ask gdb for both the
/// saved old value and the current expression value, then record the change.
#[derive(Debug)]
struct PendingWatchProbe {
    id: u64,
    time: String,
    location: Option<String>,
    requests: Vec<WatchProbeRequest>,
    results: Vec<(u64, Option<String>)>,
    collecting: Option<u64>,
    bt_collecting: bool,
    bt_done: bool,
    bt_lines: Vec<String>,
    ticks: u32,
}

#[derive(Debug)]
struct PendingWatchBt {
    event_id: u64,
    expression: String,
    collecting: bool,
    lines: Vec<String>,
}

#[derive(Default)]
struct MemoryCheckState {
    open: bool,
    tab: MemoryCheckTab,
    findings: Vec<crate::memory_check::Finding>,
    files: usize,
    arrays: usize,
    error: Option<String>,
    query: String,
    watch_input: String,
    watch_kind: WatchKind,
    watchpoints: Vec<HwWatchpoint>,
    watch_filter: String,
    watch_error: Option<String>,
    watch_log_scan: usize,
    pending_bt: Option<PendingWatchBt>,
    pending_probe: Option<PendingWatchProbe>,
    watch_seq: u64,
}

struct FirmwareViewState {
    path: Option<PathBuf>,
    modified: Option<SystemTime>,
    checked_at: Option<Instant>,
    analysis: Option<crate::memory::FirmwareAnalysis>,
    error: Option<String>,
    side_width: f32,
}

impl Default for FirmwareViewState {
    fn default() -> Self {
        Self {
            path: None,
            modified: None,
            checked_at: None,
            analysis: None,
            error: None,
            side_width: 380.0,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum CreateKind {
    File,
    Folder,
}

impl CreateKind {
    fn title(self) -> &'static str {
        match self {
            Self::File => "新建文件",
            Self::Folder => "新建文件夹",
        }
    }

    fn action(self) -> &'static str {
        match self {
            Self::File => "创建文件",
            Self::Folder => "创建文件夹",
        }
    }
}

struct CreateEntryDraft {
    kind: CreateKind,
    dir: PathBuf,
    name: String,
    error: Option<String>,
    focus: bool,
}

struct ImagePreviewState {
    path: PathBuf,
    texture: Option<egui::TextureHandle>,
    size: [usize; 2],
    error: Option<String>,
}

#[derive(Clone, Debug)]
struct FileClipboard {
    path: PathBuf,
    cut: bool,
}

struct RenameEntryDraft {
    path: PathBuf,
    name: String,
    error: Option<String>,
    focus: bool,
}

struct DeleteEntryDraft {
    path: PathBuf,
    is_dir: bool,
}

struct MemoryDraft {
    regions: Vec<crate::config::MemoryRegionConfig>,
    download_address: String,
    auto: Option<crate::memory::ResolvedMemory>,
    restored: bool,
    error: Option<String>,
}

#[derive(Default)]
struct RegisterState {
    open: bool,
    items: Vec<crate::registers::RegisterItem>,
    peripherals: Vec<crate::registers::SvdPeripheral>,
    source: String,
    truncated: bool,
    error: Option<String>,
    query: String,
}

#[derive(Default)]
struct SymbolState {
    open: bool,
    items: Vec<crate::symbols::Symbol>,
    truncated: bool,
    error: Option<String>,
    query: String,
    scanned: bool,
}

#[derive(Default)]
struct RelationState {
    index: Option<Arc<crate::relations::RelationIndex>>,
    receiver: Option<Receiver<Result<crate::relations::RelationIndex, String>>>,
    root: Option<PathBuf>,
    error: Option<String>,
    dirty_at: Option<Instant>,
}

#[derive(Default)]
struct InstallState {
    receiver: Option<Receiver<Result<String, String>>>,
}

#[derive(Default)]
struct ReportState {
    receiver: Option<Receiver<Result<PathBuf, String>>>,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
enum AiEditStatus {
    #[default]
    Pending,
    Applied,
    Cancelled,
}

#[derive(Clone, Debug)]
struct AiChatMessage {
    role: String,
    content: String,
    reasoning: String,
    streaming: bool,
    error: bool,
    edits: Vec<crate::ai_edit::AiEdit>,
    edit_status: AiEditStatus,
    apply_error: Option<String>,
    backup_dir: Option<PathBuf>,
    evolutions: Vec<String>,
    evolution_status: AiEditStatus,
    evolution_error: Option<String>,
}

impl AiChatMessage {
    fn text(role: &str, content: String, error: bool) -> Self {
        Self {
            role: role.to_owned(),
            content,
            reasoning: String::new(),
            streaming: false,
            error,
            edits: Vec::new(),
            edit_status: AiEditStatus::Pending,
            apply_error: None,
            backup_dir: None,
            evolutions: Vec::new(),
            evolution_status: AiEditStatus::Pending,
            evolution_error: None,
        }
    }
}

struct AiTransition {
    started: Instant,
    from: egui::Rect,
}

#[derive(Default)]
struct AiState {
    enabled: bool,
    messages: Vec<AiChatMessage>,
    input: String,
    sending: bool,
    receiver: Option<Receiver<crate::ai::AiStreamEvent>>,
    transition: Option<AiTransition>,
    focus_input: bool,
    context_file: Option<PathBuf>,
    skill_source: Option<String>,
    skill_files: usize,
    skill_error: Option<String>,
    skill_builtin: bool,
    fog_texture: Option<egui::TextureHandle>,
    window_layered: bool,
    framework: Option<crate::framework::FrameworkDoc>,
    framework_path: Option<PathBuf>,
    framework_error: Option<String>,
    framework_checked_at: Option<Instant>,
    framework_mtime: Option<SystemTime>,
    framework_missing: bool,
    extra_skills: Vec<crate::ai::ResourceEntry>,
    extra_references: Vec<crate::ai::ResourceEntry>,
    import_receiver: Option<Receiver<Result<crate::ai::ImportOutcome, String>>>,
    import_kind: Option<crate::ai::AiResourceKind>,
    import_status: Option<String>,
    import_error: Option<String>,
    orb_clicks: Vec<Instant>,
}

struct AiSettingsDraft {
    api_key: String,
    base_url: String,
    model: String,
    show_key: bool,
    error: Option<String>,
}

#[derive(Clone, Debug)]
struct Diagnostic {
    path: PathBuf,
    line: usize,
    severity: Severity,
}

#[derive(Clone, Debug)]
struct FindMatch {
    start_byte: usize,
    end_byte: usize,
    start_char: usize,
}

struct FindState {
    open: bool,
    replace_open: bool,
    query: String,
    replacement: String,
    case_sensitive: bool,
    use_regex: bool,
    whole_word: bool,
    preserve_case: bool,
    matches: Vec<FindMatch>,
    active: usize,
    error: Option<String>,
    compiled: Option<regex::Regex>,
    version: u64,
    last_revision: u64,
    dirty: bool,
    focus_query: bool,
}

impl Default for FindState {
    fn default() -> Self {
        Self {
            open: false,
            replace_open: false,
            query: String::new(),
            replacement: String::new(),
            case_sensitive: false,
            use_regex: false,
            whole_word: false,
            preserve_case: false,
            matches: Vec::new(),
            active: 0,
            error: None,
            compiled: None,
            version: 0,
            last_revision: u64::MAX,
            dirty: false,
            focus_query: false,
        }
    }
}

impl FindState {
    fn ensure_matches(&mut self, text: &str, revision: u64) {
        if !self.open {
            self.matches.clear();
            self.compiled = None;
            self.last_revision = revision;
            return;
        }
        if !self.dirty && self.last_revision == revision {
            return;
        }
        self.rebuild(text, revision);
    }

    fn rebuild(&mut self, text: &str, revision: u64) {
        self.matches.clear();
        self.compiled = None;
        self.error = None;
        self.active = 0;
        self.last_revision = revision;
        self.dirty = false;
        self.version = self.version.wrapping_add(1);
        if self.query.is_empty() {
            return;
        }
        let base = if self.use_regex {
            self.query.clone()
        } else {
            regex::escape(&self.query)
        };
        let pattern = if self.whole_word {
            format!(r"\b(?:{base})\b")
        } else {
            base
        };
        let compiled = regex::RegexBuilder::new(&pattern)
            .case_insensitive(!self.case_sensitive)
            .build();
        let compiled = match compiled {
            Ok(compiled) => compiled,
            Err(err) => {
                self.error = Some(err.to_string());
                return;
            }
        };
        for matched in compiled.find_iter(text) {
            if matched.start() == matched.end() {
                continue;
            }
            let start_char = text[..matched.start()].chars().count();
            self.matches.push(FindMatch {
                start_byte: matched.start(),
                end_byte: matched.end(),
                start_char,
            });
        }
        self.compiled = Some(compiled);
    }

    fn current(&self) -> Option<&FindMatch> {
        self.matches.get(self.active)
    }

    fn decorations(&self) -> Vec<(std::ops::Range<usize>, bool)> {
        self.matches
            .iter()
            .enumerate()
            .map(|(index, matched)| (matched.start_byte..matched.end_byte, index == self.active))
            .collect()
    }

    fn select_next(&mut self, cursor: usize, forward: bool) -> bool {
        if self.matches.is_empty() {
            return false;
        }
        let matches = self
            .matches
            .iter()
            .enumerate()
            .filter(|(_, matched)| matched.start_char >= cursor)
            .map(|(index, _)| index)
            .collect::<Vec<_>>();
        let next = if forward {
            matches.first().copied().unwrap_or(0)
        } else {
            let before = self
                .matches
                .iter()
                .enumerate()
                .filter(|(_, matched)| matched.start_char < cursor)
                .map(|(index, _)| index)
                .collect::<Vec<_>>();
            before.last().copied().unwrap_or(self.matches.len() - 1)
        };
        self.active = next;
        self.version = self.version.wrapping_add(1);
        true
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum MetaListField {
    Includes,
    LibraryDirs,
    Defines,
}

impl MetaListField {
    fn label(self) -> &'static str {
        match self {
            Self::Includes => "包含目录",
            Self::LibraryDirs => "库搜索目录",
            Self::Defines => "预处理器定义",
        }
    }
}

struct MetaAddDraft {
    field: MetaListField,
    value: String,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
enum BuildOptionsTab {
    #[default]
    Global,
    Compiler,
    Assembler,
    Linker,
}

enum PendingAction {
    OpenProject(PathBuf),
    CloseTab(usize),
    CloseTabs {
        paths: Vec<PathBuf>,
        focus: Option<PathBuf>,
    },
    CloseWindow,
}

struct PendingDialog {
    action: PendingAction,
    error: Option<String>,
}

struct Toast {
    message: String,
    severity: Severity,
    born: Instant,
}

enum Action {
    OpenPicker,
    OpenWorkspacePicker,
    CreateWorkspace,
    OpenProjectConfig,
    OpenFile(PathBuf),
    OpenAt(String, usize, usize),
    SetEncoding(TextEncoding),
    ToggleReadOnly,
    FormatDocument,
    OpenGitStatus,
    OpenTodos,
    OpenSymbols,
    LspRename,
    OpenRegisters,
    OpenHex(PathBuf),
    OpenArrayGenerator,
    OpenDebug,
    DebugContinue,
    DebugPause,
    DebugJumpToCurrent,
    DebugStepOver,
    DebugStepInto,
    DebugFinish,
    DebugStop,
    DebugSend(String),
    ToggleBreakpointAt(PathBuf, usize),
    SelectTab(usize),
    CloseTab(usize),
    CloseOtherTabs(usize),
    CloseTabsToRight(usize),
    CloseSavedTabs,
    CloseAllTabs,
    SplitTab(PathBuf, SplitDirection),
    MoveSplit(SplitDirection),
    SplitGroup,
    CloseSplit,
    SaveActive,
    SaveAs(usize),
    CopyText(String),
    RevealInTree(PathBuf),
    Run(TaskKind),
    StopTask,
    RefreshTree,
    ToggleHidden,
    OpenSettings,
    OpenThemeSettings,
    EnterAiMode,
    OpenFirmwareConfirm,
    RefreshBoardFiles,
    ReadBoardFile(crate::micropython::BoardFile),
    EditMeta,
    DetectIncludeDirs,
    SyncMakefileSources,
    AddMetaItem(MetaListField),
    EditBuildOptions,
    AddSupportPack,
    ImportSupportPack(PathBuf),
    EditFlashConfig,
    EditMemoryLayout,
    GenerateResourceReport,
    RunMemoryCheck,
    SetFlashTool(FlashTool),
    SetOpenOcdPath(String),
    BrowseOpenOcd,
    SetPyOcdPath(String),
    BrowsePyOcd,
    ClearLog,
    CopyLog,
    ClearDebugLog,
    CopyDebugLog,
    Reveal(PathBuf),
    CopyPath(PathBuf),
    CopyRelativePath(PathBuf),
    NewFile(PathBuf),
    NewFolder(PathBuf),
    OpenToSide(PathBuf),
    CutFile(PathBuf),
    CopyFile(PathBuf),
    PasteFile(PathBuf),
    RenameFile(PathBuf),
    DeleteFile(PathBuf),
    PreviewImage(PathBuf),
    OpenVscode(PathBuf),
    ToggleWrap,
}

pub struct App {
    settings: Settings,
    highlighter: Arc<Highlighter>,
    project: Option<Project>,
    tabs: Vec<Document>,
    active: Option<usize>,
    tree: Option<FileTree>,
    project_meta: ProjectMeta,
    support_packs: Vec<crate::support_pack::SupportPack>,
    build: BuildController,
    build_config: BuildConfig,
    project_debug: crate::config::ProjectDebugSettings,
    settings_draft: Option<SettingsDraft>,
    theme_settings: Option<ThemeSettingsDraft>,
    firmware_confirm: bool,
    meta_draft: Option<MetaDraft>,
    meta_add: Option<MetaAddDraft>,
    flash_draft: Option<FlashDraft>,
    build_options_draft: Option<BuildOptions>,
    build_options_tab: BuildOptionsTab,
    find: FindState,
    project_search: ProjectSearchState,
    todo: TodoState,
    symbols: SymbolState,
    registers: RegisterState,
    relations: RelationState,
    install: InstallState,
    report: ReportState,
    ai: AiState,
    ai_settings_draft: Option<AiSettingsDraft>,
    /// Zoom factor to restore when leaving AI mode. Kept outside `AiState` so
    /// switching projects cannot lose the user's normal editor zoom.
    ai_zoom_before: Option<f32>,
    hex: HexState,
    array: ArrayState,
    debug: DebugState,
    debug_start: Option<Receiver<Result<(crate::debugger::DebugSession, PathBuf), String>>>,
    micropython_busy: bool,
    micropython_action: Option<String>,
    micropython_rx: Option<Receiver<MicroPythonJobResult>>,
    board_files: Option<Vec<crate::micropython::BoardFile>>,
    board_error: Option<String>,
    board_file_error: Option<String>,
    board_remote_paths: BTreeMap<PathBuf, String>,
    create_entry: Option<CreateEntryDraft>,
    rename_entry: Option<RenameEntryDraft>,
    delete_entry: Option<DeleteEntryDraft>,
    memory_draft: Option<MemoryDraft>,
    memory_layout: Option<crate::memory::ResolvedMemory>,
    memory_usage_cache: Option<(PathBuf, SystemTime, Option<crate::memory::MemoryUsage>)>,
    firmware_view: FirmwareViewState,
    memory_check: MemoryCheckState,
    file_clipboard: Option<FileClipboard>,
    image_preview: Option<ImagePreviewState>,
    output_view: OutputView,
    completion: CompletionState,
    member_index: Option<crate::completion::MemberIndex>,
    goto_line: GotoLineState,
    column_edit: ColumnEditState,
    column_selection: Option<ColumnSelectionState>,
    multi_cursor: Option<MultiCursorState>,
    undo: UndoState,
    lsp: Option<crate::lsp::Session>,
    lsp_config_key: u64,
    lsp_refs: Option<crate::lsp::RefsView>,
    lsp_hover: Option<crate::lsp::HoverView>,
    lsp_rename: Option<RenameDialog>,
    split_path: Option<PathBuf>,
    split_primary: Option<PathBuf>,
    split_direction: SplitDirection,
    split_focus: usize,
    scroll_to_cursor: bool,
    /// Vertical scroll offset to restore after switching between the normal
    /// and folded editors (folding must not jump to the top).
    pending_scroll: Option<f32>,
    last_editor_scroll: f32,
    immersive: bool,
    chord_k_at: Option<Instant>,
    log_filter: LogFilter,
    diagnostics: Vec<Diagnostic>,
    diagnostics_revision: u64,
    dirty_signature: u64,
    autosave_since: Option<Instant>,
    git: GitStatus,
    git_line_marks: BTreeSet<usize>,
    git_line_key: Option<(PathBuf, u64)>,
    git_open: bool,
    left_split: f32,
    resource_open: bool,
    info_open: bool,
    pending: Option<PendingDialog>,
    toast: Option<Toast>,
    allow_close: bool,
    focus_editor: bool,
    last_editor_rect: Option<egui::Rect>,
    last_title: String,
    native_theme_requested: bool,
    native_titlebar_styled: bool,
}

impl App {
    pub fn new(
        cc: &eframe::CreationContext<'_>,
        initial_project: Option<PathBuf>,
        initial_file: Option<PathBuf>,
    ) -> Self {
        let settings = Settings::load();
        let left_split = settings.left_split.clamp(0.18, 0.82);
        let resource_open = settings.resource_open;
        let info_open = settings.info_open;
        let mut app = Self {
            build: BuildController::default(),
            highlighter: Arc::new(Highlighter::new()),
            project: None,
            tabs: Vec::new(),
            active: None,
            tree: None,
            project_meta: ProjectMeta::default(),
            support_packs: Vec::new(),
            build_config: BuildConfig::default(),
            project_debug: crate::config::ProjectDebugSettings::default(),
            settings_draft: None,
            theme_settings: None,
            firmware_confirm: false,
            meta_draft: None,
            meta_add: None,
            flash_draft: None,
            build_options_draft: None,
            build_options_tab: BuildOptionsTab::default(),
            find: FindState::default(),
            project_search: ProjectSearchState::default(),
            todo: TodoState::default(),
            symbols: SymbolState::default(),
            registers: RegisterState::default(),
            relations: RelationState::default(),
            install: InstallState::default(),
            report: ReportState::default(),
            ai: AiState::default(),
            ai_settings_draft: None,
            ai_zoom_before: None,
            hex: HexState::default(),
            array: ArrayState::default(),
            debug: DebugState::default(),
            debug_start: None,
            micropython_busy: false,
            micropython_action: None,
            micropython_rx: None,
            board_files: None,
            board_error: None,
            board_file_error: None,
            board_remote_paths: BTreeMap::new(),
            create_entry: None,
            rename_entry: None,
            delete_entry: None,
            memory_draft: None,
            memory_layout: None,
            memory_usage_cache: None,
            firmware_view: FirmwareViewState::default(),
            memory_check: MemoryCheckState::default(),
            file_clipboard: None,
            image_preview: None,
            output_view: OutputView::default(),
            completion: CompletionState::default(),
            member_index: None,
            goto_line: GotoLineState::default(),
            column_edit: ColumnEditState::default(),
            column_selection: None,
            multi_cursor: None,
            undo: UndoState::default(),
            lsp: None,
            lsp_config_key: 0,
            lsp_refs: None,
            lsp_hover: None,
            lsp_rename: None,
            split_path: None,
            split_primary: None,
            split_direction: SplitDirection::Right,
            split_focus: 0,
            scroll_to_cursor: false,
            pending_scroll: None,
            last_editor_scroll: 0.0,
            immersive: false,
            chord_k_at: None,
            log_filter: LogFilter::All,
            diagnostics: Vec::new(),
            diagnostics_revision: u64::MAX,
            dirty_signature: 0,
            autosave_since: None,
            git: GitStatus::default(),
            git_line_marks: BTreeSet::new(),
            git_line_key: None,
            git_open: false,
            left_split,
            resource_open,
            info_open,
            pending: None,
            toast: None,
            allow_close: false,
            focus_editor: false,
            last_editor_rect: None,
            last_title: String::new(),
            native_theme_requested: false,
            native_titlebar_styled: false,
            settings,
        };
        app.build.auto_scroll = app.settings.auto_scroll_output;
        app.build.output_open = app.settings.output_open;
        let code_theme = crate::codetheme::by_name(&app.settings.editor_theme)
            .unwrap_or(crate::codetheme::default_theme());
        app.highlighter.set_code_theme(code_theme);
        crate::theme::apply(&cc.egui_ctx, code_theme);

        if let Some(path) = initial_project {
            app.load_project(&path, false);
        } else if let Some(last) = app.settings.last_project.clone() {
            app.load_project(&last, false);
        }
        if let Some(file) = initial_file {
            if app.project.is_some() {
                app.open_file(file);
                app.focus_editor = true;
            }
        }
        app
    }

    fn load_project(&mut self, path: &Path, announce: bool) -> bool {
        self.micropython_busy = false;
        self.micropython_rx = None;
        self.micropython_action = None;
        self.board_files = None;
        self.board_error = None;
        self.board_file_error = None;
        self.board_remote_paths.clear();
        let path = if path.is_file() && crate::config::is_workspace_file(path) {
            match crate::config::resolve_workspace_file(path) {
                Ok(path) => path,
                Err(err) => {
                    self.set_toast(err, Severity::Error);
                    return false;
                }
            }
        } else {
            path.to_path_buf()
        };
        let root = match util::canonical_project(&path) {
            Ok(root) => root,
            Err(err) => {
                self.set_toast(err, Severity::Error);
                return false;
            }
        };

        if self
            .project
            .as_ref()
            .is_some_and(|project| same_path(&project.root, &root))
        {
            if announce {
                self.set_toast("该工程已经打开", Severity::Info);
            }
            return true;
        }

        self.build.reset();
        self.debug_stop();
        self.debug.open = false;
        self.debug.target = None;
        self.create_entry = None;
        self.image_preview = None;
        self.memory_check = MemoryCheckState::default();
        self.firmware_view = FirmwareViewState::default();
        self.registers = RegisterState::default();
        self.relations = RelationState::default();
        self.report = ReportState::default();
        self.ai = AiState::default();
        self.ai_settings_draft = None;
        self.output_view = OutputView::Build;

        let mut config_error = None;
        let mut created_config = false;
        let mut project_file_debug = None;
        let (mut config, from_project) = match crate::config::ProjectSettings::load(&root) {
            Ok(Some(settings)) => {
                project_file_debug = Some(settings.debug);
                (settings.build, true)
            }
            Ok(None) => (self.settings.build_config_for(&root), false),
            Err(err) => {
                config_error = Some(err);
                (self.settings.build_config_for(&root), false)
            }
        };
        if let Some(makefile) = makefile::find_makefile(&root) {
            let fallback = config.build_options.clone();
            if let Some(options) = makefile::read_options(&makefile, &fallback) {
                config.build_options = options;
            }
        }
        let original_debug = project_file_debug.unwrap_or_default();
        let mut project_debug = original_debug.clone();
        if project_debug.gdb_path.trim().is_empty() {
            project_debug.gdb_path = self.settings.gdb_path.clone();
        }
        if project_debug.program.trim().is_empty() && !from_project {
            // The debug program is project-specific: only inherit it from the
            // global fallback when this project has no settings file and the
            // path actually belongs to this project.
            let configured = self.settings.debug_program.trim();
            if !configured.is_empty() && Path::new(configured).starts_with(&root) {
                project_debug.program = configured.to_owned();
            }
        }
        if project_debug.program.trim().is_empty() {
            if let Some(path) = detect_debug_program(&root) {
                project_debug.program = util::display_path(&path);
            }
        }
        let debug_needs_save = project_debug != original_debug;
        self.project_debug = project_debug;
        self.debug.watches = self
            .project_debug
            .watches
            .iter()
            .cloned()
            .map(WatchEntry::new)
            .collect();
        self.debug.auto_watch = self.project_debug.auto_refresh;
        self.debug.watch_refresh = None;
        self.debug.last_watch_refresh = None;
        self.debug.registers.clear();
        self.debug.peripheral_values.clear();
        self.debug.peripheral_open.clear();
        self.debug.peripheral_registers_open.clear();
        self.debug.peripheral_edit = None;
        self.debug.peripheral_initialized = false;
        self.debug.peripheral_refresh = None;
        self.debug.peripheral_last_refresh = None;
        let mut config_in_project = from_project;
        if !from_project && config_error.is_none() {
            match crate::config::ProjectSettings::save(&root, &config, &self.project_debug) {
                Ok(_) => {
                    created_config = true;
                    config_in_project = true;
                }
                Err(err) => config_error = Some(err),
            }
        } else if from_project && debug_needs_save && config_error.is_none() {
            // Backfill debug settings into project files created by an older
            // version that only stored the build configuration.
            if let Err(err) =
                crate::config::ProjectSettings::save(&root, &config, &self.project_debug)
            {
                config_error = Some(err);
            }
        }
        if config_in_project {
            // The project file is authoritative; drop the old user-directory
            // cache so deleting `.simple-embedded` really resets the project.
            self.settings.build_configs.remove(&project_key(&root));
        } else {
            // Keep a fallback for read-only projects or malformed project files.
            self.settings.set_build_config(&root, &config);
        }
        let tool_known = detected_tool(&root).is_some();

        let project_name = Project::new(root.clone()).name;
        self.project_meta = ProjectMeta::detect_for(&root, config.effective_build_system());
        self.support_packs = crate::support_pack::scan(&root);
        if self.support_packs.is_empty() {
            self.support_packs =
                crate::support_pack::scan_keil_installed(self.project_meta.device.as_deref());
        }
        self.project = Some(Project::new(root.clone()));
        self.debug.target = self.debug_target();
        self.member_index = None;
        self.completion = CompletionState::default();
        self.build_config = config;
        self.refresh_memory_layout();
        self.tree = Some(FileTree::new(
            root.clone(),
            self.settings.show_hidden,
            self.settings.respect_gitignore,
            &self.settings.ignore_patterns,
        ));
        self.tabs.clear();
        self.active = None;
        self.symbols.items.clear();
        self.symbols.scanned = false;
        self.split_path = None;
        self.split_primary = None;
        self.split_focus = 0;
        self.column_selection = None;
        self.multi_cursor = None;
        self.undo = UndoState::default();
        self.focus_editor = false;
        self.git_line_marks.clear();
        self.git_line_key = None;
        self.refresh_git_status();
        self.settings.remember_project(&root);
        self.settings.save();
        self.start_relations_scan();

        if let Some(err) = config_error {
            self.set_toast(err, Severity::Warning);
        } else if announce {
            if tool_known || !self.build_config.build_command.trim().is_empty() {
                let suffix = if created_config {
                    "，已创建 .simple-embedded/settings.json"
                } else {
                    ""
                };
                self.set_toast(
                    format!("已打开工程 {project_name}{suffix}"),
                    Severity::Success,
                );
            } else {
                self.set_toast(
                    "工程已打开，但没有检测到构建系统，请打开配置填写命令",
                    Severity::Warning,
                );
            }
        }
        self.start_lsp();
        true
    }

    fn request_open_project(&mut self, path: PathBuf) {
        let path = if path.is_file() && crate::config::is_workspace_file(&path) {
            match crate::config::resolve_workspace_file(&path) {
                Ok(path) => path,
                Err(err) => {
                    self.set_toast(err, Severity::Error);
                    return;
                }
            }
        } else {
            path
        };
        if let Some(project) = &self.project {
            if same_path(&project.root, &path) {
                self.set_toast("该工程已经打开", Severity::Info);
                return;
            }
        }
        if self.has_unsaved() {
            self.pending = Some(PendingDialog {
                action: PendingAction::OpenProject(path),
                error: None,
            });
        } else {
            self.load_project(&path, true);
        }
    }

    fn has_unsaved(&self) -> bool {
        self.tabs.iter().any(Document::is_dirty)
    }

    fn dirty_count(&self) -> usize {
        self.tabs.iter().filter(|tab| tab.is_dirty()).count()
    }

    fn active_document(&self) -> Option<&Document> {
        self.active.and_then(|index| self.tabs.get(index))
    }

    fn open_file(&mut self, path: PathBuf) {
        let Some(project_root) = self.project.as_ref().map(|project| project.root.clone()) else {
            self.set_toast("请先打开一个工程文件夹", Severity::Warning);
            return;
        };
        let path = std::fs::canonicalize(&path).unwrap_or(path);
        if !path.is_file() {
            self.set_toast(
                format!("无法打开 {}：不是文件", util::display_path(&path)),
                Severity::Error,
            );
            return;
        }

        if let Some(index) = self.tabs.iter().position(|doc| same_path(&doc.path, &path)) {
            self.active = Some(index);
            self.focus_editor = true;
            return;
        }

        match Document::open(&path, &project_root, &self.highlighter) {
            Ok(document) => {
                self.tabs.push(document);
                self.active = Some(self.tabs.len() - 1);
                self.focus_editor = true;
                if let Some(tree) = &mut self.tree {
                    tree.reveal(&path);
                }
            }
            Err(err) => self.set_toast(
                format!("无法打开 {}：{err}", util::display_path(&path)),
                Severity::Error,
            ),
        }
    }

    fn refresh_diagnostics(&mut self) {
        let revision = self.build.log.diagnostics_revision();
        if revision == self.diagnostics_revision {
            return;
        }
        self.diagnostics_revision = revision;
        self.diagnostics.clear();
        let Some(root) = self.project.as_ref().map(|project| project.root.clone()) else {
            return;
        };
        let entries = self
            .build
            .log
            .diagnostics()
            .iter()
            .map(|diagnostic| {
                (
                    diagnostic.location.path.clone(),
                    diagnostic.location.line,
                    diagnostic.severity,
                )
            })
            .collect::<Vec<_>>();
        for (path, line, severity) in entries {
            let path = PathBuf::from(path);
            let path = if path.is_absolute() {
                path
            } else {
                root.join(path)
            };
            let path = std::fs::canonicalize(&path).unwrap_or(path);
            self.diagnostics.push(Diagnostic {
                path,
                line,
                severity,
            });
        }
    }

    fn open_at(&mut self, ctx: &egui::Context, path: &str, line: usize, column: usize) {
        let Some(root) = self.project.as_ref().map(|project| project.root.clone()) else {
            return;
        };
        let path = PathBuf::from(path);
        let path = if path.is_absolute() {
            path
        } else {
            root.join(path)
        };
        let path = std::fs::canonicalize(&path).unwrap_or(path);
        self.open_file(path.clone());
        let Some(index) = self.tabs.iter().position(|doc| same_path(&doc.path, &path)) else {
            return;
        };
        self.active = Some(index);
        let Some(doc) = self.tabs.get_mut(index) else {
            return;
        };
        let text = doc.buffer.as_string();
        let start = crate::util::char_index_at_line(text, line);
        let line_len = text
            .lines()
            .nth(line.saturating_sub(1))
            .map(|value| value.chars().count())
            .unwrap_or(0);
        let char_index = start + column.saturating_sub(1).min(line_len);
        doc.update_cursor(char_index);
        set_editor_cursor(ctx, &doc.path, char_index);
        self.focus_editor = true;
        self.scroll_to_cursor = true;
        self.set_toast(
            format!("已跳转到 {} 第 {} 行", util::display_path(&path), line),
            Severity::Info,
        );
    }

    fn set_encoding(&mut self, encoding: TextEncoding) {
        let Some(index) = self.active else {
            return;
        };
        let Some(doc) = self.tabs.get_mut(index) else {
            return;
        };
        if doc.read_only {
            self.set_toast("只读文件不能切换编码", Severity::Warning);
            return;
        }
        if doc.encoding == encoding {
            return;
        }
        doc.encoding = encoding;
        doc.buffer.mark_dirty();
        self.set_toast(format!("保存时将使用 {}", encoding.label()), Severity::Info);
    }

    fn format_document(doc: &mut Document, configured: &str) -> Result<bool, String> {
        if !formatting::is_c_family(&doc.path) {
            return Ok(false);
        }
        let text = doc.buffer.as_string().to_owned();
        let formatted = formatting::format_source(&doc.path, &text, configured)?;
        if formatted == text {
            return Ok(false);
        }
        let cursor = doc.cursor_index.min(formatted.chars().count());
        doc.buffer.set_text(formatted);
        doc.update_cursor(cursor);
        Ok(true)
    }

    fn toggle_read_only(&mut self) {
        let Some(index) = self.active else {
            return;
        };
        let Some(doc) = self.tabs.get_mut(index) else {
            return;
        };
        if doc.binary.is_some() {
            self.set_toast("二进制文件始终只读", Severity::Info);
            return;
        }
        if !doc.read_only && doc.is_dirty() {
            self.set_toast("请先保存当前修改，再切换只读", Severity::Warning);
            return;
        }
        doc.read_only = !doc.read_only;
        let name = doc.name.clone();
        let enabled = doc.read_only;
        self.set_toast(
            if enabled {
                format!("{name} 已设为只读")
            } else {
                format!("{name} 已恢复编辑")
            },
            Severity::Info,
        );
    }

    fn format_current_document(&mut self) {
        let Some(index) = self.active else {
            return;
        };
        let configured = self.settings.clang_format_path.clone();
        let result = {
            let Some(doc) = self.tabs.get_mut(index) else {
                return;
            };
            if doc.read_only {
                self.set_toast("只读文件不能格式化", Severity::Warning);
                return;
            }
            if !formatting::is_c_family(&doc.path) {
                self.set_toast("clang-format 仅支持 C/C++ 源文件", Severity::Warning);
                return;
            }
            (doc.name.clone(), Self::format_document(doc, &configured))
        };
        let (name, result) = result;
        match result {
            Ok(true) => self.set_toast(format!("已格式化 {name}"), Severity::Success),
            Ok(false) => self.set_toast(format!("{name} 已经是格式化状态"), Severity::Info),
            Err(err) => self.set_toast(format!("格式化 {name} 失败：{err}"), Severity::Warning),
        }
    }

    fn open_array_generator(&mut self, ctx: &egui::Context) {
        let Some(index) = self.active else {
            self.set_toast("请先打开一个文件", Severity::Warning);
            return;
        };
        let Some(doc) = self.tabs.get(index) else {
            return;
        };
        let text = doc.buffer.as_string();
        let selection = selected_char_range(ctx, &doc.path).filter(|(start, end)| end > start);
        let (mut bytes, source_label) = if let Some((start, end)) = selection {
            (
                text.chars()
                    .skip(start)
                    .take(end.saturating_sub(start))
                    .collect::<String>()
                    .into_bytes(),
                "选中内容".to_owned(),
            )
        } else {
            (text.as_bytes().to_vec(), "当前文件".to_owned())
        };
        let truncated = bytes.len() > MAX_ARRAY_BYTES;
        bytes.truncate(MAX_ARRAY_BYTES);
        let name = doc
            .path
            .file_stem()
            .map(|stem| format!("{}_data", stem.to_string_lossy()))
            .unwrap_or_else(|| "data".to_owned());
        self.array = ArrayState {
            open: true,
            name,
            hex: true,
            per_line: 16,
            source: bytes,
            source_label,
            truncated,
        };
    }

    fn import_svd(&mut self) {
        let Some(root) = self.project.as_ref().map(|project| project.root.clone()) else {
            self.set_toast("请先打开工程", Severity::Warning);
            return;
        };
        let Some(path) = rfd::FileDialog::new()
            .set_title("选择芯片 SVD 文件")
            .add_filter("SVD", &["svd"])
            .pick_file()
        else {
            return;
        };
        let name = util::file_name(&path);
        let destination = root.join(&name);
        if !same_path(&path, &destination) {
            if let Err(err) = std::fs::copy(&path, &destination) {
                self.set_toast(format!("无法复制 SVD 到工程目录：{err}"), Severity::Error);
                return;
            }
        }
        self.registers.peripherals.clear();
        self.registers.items.clear();
        self.refresh_registers();
        self.debug.peripheral_values.clear();
        self.debug.peripheral_open.clear();
        self.debug.peripheral_registers_open.clear();
        self.debug.peripheral_initialized = false;
        self.debug.peripheral_last_refresh = None;
        if self.debug.watch_tab == DebugSideTab::Peripherals {
            self.refresh_debug_peripheral();
        }
        self.set_toast(format!("已加载芯片 SVD：{name}"), Severity::Success);
    }

    fn ensure_registers_loaded(&mut self) {
        if self.registers.items.is_empty() {
            self.refresh_registers();
        }
    }

    fn refresh_registers(&mut self) {
        let Some(root) = self.project.as_ref().map(|project| project.root.clone()) else {
            self.set_toast("请先打开一个工程文件夹", Severity::Warning);
            return;
        };
        let ignore = IgnoreRules::load(
            &root,
            self.settings.respect_gitignore,
            &self.settings.ignore_patterns,
        );
        match crate::registers::scan_project(&root, &ignore, self.project_meta.device.as_deref()) {
            Ok(index) => {
                self.registers.items = index.items;
                self.registers.peripherals = index.peripherals;
                self.registers.source = index.source;
                self.registers.truncated = index.truncated;
                self.registers.error = None;
            }
            Err(err) => {
                self.registers.items.clear();
                self.registers.peripherals.clear();
                self.registers.error = Some(err);
            }
        }
    }

    fn run_memory_check(&mut self) {
        let Some(root) = self.project.as_ref().map(|project| project.root.clone()) else {
            self.set_toast("请先打开一个工程文件夹", Severity::Warning);
            return;
        };
        self.memory_check.tab = MemoryCheckTab::Static;
        let ignore = IgnoreRules::load(
            &root,
            self.settings.respect_gitignore,
            &self.settings.ignore_patterns,
        );
        match crate::memory_check::scan_project(&root, &ignore) {
            Ok(report) => {
                let count = report.findings.len();
                self.memory_check.findings = report.findings;
                self.memory_check.files = report.files;
                self.memory_check.arrays = report.arrays;
                self.memory_check.error = None;
                self.memory_check.open = true;
                self.set_toast(
                    if count == 0 {
                        "未发现明显的数组越界或内存错误模式".to_owned()
                    } else {
                        format!("检测完成：发现 {count} 个可疑点")
                    },
                    if count == 0 {
                        Severity::Info
                    } else {
                        Severity::Warning
                    },
                );
            }
            Err(err) => {
                self.memory_check.error = Some(err.clone());
                self.memory_check.open = true;
                self.set_toast(err, Severity::Error);
            }
        }
    }

    fn open_memory_check(&mut self) {
        if self.project.is_none() {
            self.set_toast("请先打开一个工程", Severity::Warning);
            return;
        }
        if self.effective_toolchain().is_micropython() {
            self.set_toast("MicroPython 串口模式不支持 C 内存检测", Severity::Warning);
            return;
        }
        if self.debug.session.is_some() {
            self.memory_check.open = true;
            self.memory_check.tab = MemoryCheckTab::Dynamic;
            self.memory_check.watch_error = None;
        } else {
            self.run_memory_check();
        }
    }

    fn add_hardware_watchpoint(&mut self, expression: &str, kind: WatchKind) {
        let expression = expression.trim();
        if expression.is_empty() {
            return;
        }
        if self.debug.running {
            self.set_toast("目标正在运行，请先暂停再添加 watchpoint", Severity::Warning);
            return;
        }
        if self.debug.session.is_none() {
            self.set_toast("请先启动调试会话", Severity::Warning);
            return;
        }
        self.memory_check.watch_seq = self.memory_check.watch_seq.wrapping_add(1).max(1);
        let uid = self.memory_check.watch_seq;
        if let Some(session) = &mut self.debug.session {
            session.send("set can-use-hw-watchpoints 1");
            session.send(&format!("{} {expression}", kind.command()));
            // Keep the value at creation time in a gdb convenience variable.
            // When OpenOCD reports a hit as SIGTRAP without old/new values, the
            // probe below can still show what the variable used to be.
            session.send(&format!("set $__se_wp_{uid} = {expression}"));
        }
        self.memory_check.watchpoints.push(HwWatchpoint {
            uid,
            number: None,
            expression: expression.to_owned(),
            kind,
            hardware: None,
            hits: 0,
            old_value: None,
            new_value: None,
            history: Vec::new(),
        });
        self.memory_check.watch_input.clear();
        self.memory_check.watch_error = None;
        self.memory_check.open = true;
        self.memory_check.tab = MemoryCheckTab::Dynamic;
        self.set_toast(
            format!("已发送 {} {expression}，等待 GDB 分配编号", kind.command()),
            Severity::Info,
        );
    }

    fn remove_hardware_watchpoint(&mut self, index: usize) {
        if index >= self.memory_check.watchpoints.len() {
            return;
        }
        let watchpoint = self.memory_check.watchpoints.remove(index);
        if let Some(number) = watchpoint.number {
            if let Some(session) = &mut self.debug.session {
                session.send(&format!("delete {number}"));
            }
        }
    }

    fn clear_hardware_watchpoints(&mut self) {
        let numbers = self
            .memory_check
            .watchpoints
            .iter()
            .filter_map(|watchpoint| watchpoint.number)
            .collect::<Vec<_>>();
        if let Some(session) = &mut self.debug.session {
            for number in numbers {
                session.send(&format!("delete {number}"));
            }
        }
        self.memory_check.watchpoints.clear();
        self.memory_check.watch_error = None;
        self.memory_check.pending_bt = None;
    }

    /// A debug session ended: keep the expressions and history for the next
    /// session, but GDB's watchpoint numbers are no longer valid.
    fn detach_hardware_watchpoints(&mut self) {
        for watchpoint in &mut self.memory_check.watchpoints {
            watchpoint.number = None;
            watchpoint.hardware = None;
            watchpoint.old_value = None;
            watchpoint.new_value = None;
        }
        self.memory_check.pending_bt = None;
        self.memory_check.pending_probe = None;
        self.memory_check.watch_error = None;
        self.memory_check.watch_log_scan = 0;
    }

    /// Re-send the saved watch expressions after a new debug session starts.
    fn rearm_hardware_watchpoints(&mut self) {
        if self.debug.session.is_none() || self.debug.running {
            return;
        }
        let expressions = self
            .memory_check
            .watchpoints
            .iter()
            .map(|watchpoint| {
                (
                    watchpoint.uid,
                    watchpoint.expression.clone(),
                    watchpoint.kind,
                )
            })
            .collect::<Vec<_>>();
        for watchpoint in &mut self.memory_check.watchpoints {
            watchpoint.number = None;
            watchpoint.hardware = None;
            watchpoint.old_value = None;
            watchpoint.new_value = None;
        }
        self.memory_check.watch_error = None;
        if expressions.is_empty() {
            return;
        }
        if let Some(session) = &mut self.debug.session {
            session.send("set can-use-hw-watchpoints 1");
            for (uid, expression, kind) in expressions {
                session.send(&format!("{} {expression}", kind.command()));
                session.send(&format!("set $__se_wp_{uid} = {expression}"));
            }
        }
    }

    /// Follow new gdb console lines for watchpoint creation and hits. This is
    /// intentionally separate from Live Watch polling: watchpoints are driven
    /// by target stops, not by a timer.
    fn poll_watchpoints(&mut self) {
        let Some(session) = self.debug.session.as_ref() else {
            return;
        };
        if self.memory_check.watch_log_scan > session.log.len() {
            self.memory_check.watch_log_scan = 0;
        }
        if session.log.len() <= self.memory_check.watch_log_scan {
            return;
        }
        let fresh = session.log[self.memory_check.watch_log_scan..].to_vec();
        self.memory_check.watch_log_scan = session.log.len();
        self.poll_watch_backtrace(&fresh);

        let sigtrap = fresh
            .iter()
            .any(|line| line.contains("SIGTRAP") || line.contains("Trace/breakpoint trap"));
        let sigtrap_location = fresh
            .iter()
            .rev()
            .find(|line| line.contains(" at ") && line.contains(':'))
            .map(|line| line.trim().to_owned());
        self.poll_watch_probe(&fresh);
        let mut hit = None;
        let mut bt_hit = None;
        let mut created_watch = false;
        let mut deleted_watchpoints = Vec::new();
        let mut evaluation_error = false;
        let mut native_updates = Vec::new();
        for (index, line) in fresh.iter().enumerate() {
            let trimmed = line.trim();
            if trimmed.contains("Value has been optimized out")
                || trimmed.contains("Error evaluating expression for watchpoint")
            {
                evaluation_error = true;
            }
            if let Some(number) = crate::debugger::parse_deleted_watchpoint(line) {
                deleted_watchpoints.push(number);
            }
            let hardware_error = trimmed.contains("Cannot insert hardware watchpoint")
                || trimmed.contains("Could not insert hardware watchpoint")
                || trimmed.contains("No more hardware watchpoints");
            let expression_error = trimmed.contains("No symbol")
                || trimmed.contains("Cannot watch")
                || trimmed.contains("No watchpoint")
                || trimmed.contains("A syntax error")
                || trimmed.contains("Expression cannot be evaluated");
            let has_pending = self
                .memory_check
                .watchpoints
                .iter()
                .any(|watchpoint| watchpoint.number.is_none());
            if (hardware_error || (has_pending && expression_error))
                && self.memory_check.watch_error.as_deref() != Some(trimmed)
            {
                self.memory_check.watch_error = Some(trimmed.to_owned());
                self.set_toast(trimmed.to_owned(), Severity::Warning);
            }
            let Some(notice) = crate::debugger::parse_watchpoint(line) else {
                continue;
            };
            let mut old_value = None;
            let mut new_value = None;
            let mut location = None;
            for value in fresh.iter().skip(index + 1).take(10) {
                let value = value.trim();
                if let Some(old) = value.strip_prefix("Old value = ") {
                    old_value = Some(old.to_owned());
                }
                if let Some(new) = value.strip_prefix("New value = ") {
                    new_value = Some(new.to_owned());
                }
                if value.contains(" at ") && value.contains(':') {
                    location = Some(value.to_owned());
                }
                if old_value.is_some() && new_value.is_some() && location.is_some() {
                    break;
                }
            }
            let position = self.memory_check.watchpoints.iter().position(|watchpoint| {
                watchpoint.number == Some(notice.number)
                    || (watchpoint.number.is_none() && watchpoint.expression == notice.expression)
            });
            let position = position.or_else(|| {
                if old_value.is_none() && new_value.is_none() {
                    self.memory_check
                        .watchpoints
                        .iter()
                        .position(|watchpoint| watchpoint.number.is_none())
                } else {
                    None
                }
            });
            let notice_is_hit = old_value.is_none()
                && new_value.is_none()
                && fresh
                    .iter()
                    .skip(index + 1)
                    .any(|line| line.contains("SIGTRAP") || line.contains("Trace/breakpoint trap"));
            let position = match position {
                Some(position) => position,
                None if old_value.is_none() && new_value.is_none() => {
                    self.memory_check.watch_seq =
                        self.memory_check.watch_seq.wrapping_add(1).max(1);
                    let uid = self.memory_check.watch_seq;
                    self.memory_check.watchpoints.push(HwWatchpoint {
                        uid,
                        number: Some(notice.number),
                        expression: notice.expression.clone(),
                        kind: WatchKind::Write,
                        hardware: Some(notice.hardware),
                        hits: 0,
                        old_value: None,
                        new_value: None,
                        history: Vec::new(),
                    });
                    self.memory_check.watchpoints.len() - 1
                }
                None => continue,
            };
            let event_id = self.memory_check.watch_seq.wrapping_add(1).max(1);
            self.memory_check.watch_seq = event_id;
            let watchpoint = &mut self.memory_check.watchpoints[position];
            watchpoint.number = Some(notice.number);
            watchpoint.hardware = Some(notice.hardware);
            if old_value.is_some() || new_value.is_some() {
                watchpoint.hits = watchpoint.hits.saturating_add(1);
                watchpoint.old_value = old_value.clone();
                watchpoint.new_value = new_value.clone();
                watchpoint.history.push(WatchEvent {
                    id: event_id,
                    time: crate::util::local_timestamp(),
                    old_value: old_value.clone(),
                    new_value: new_value.clone(),
                    location: location.clone(),
                    stack: Vec::new(),
                });
                if watchpoint.history.len() > 200 {
                    watchpoint.history.remove(0);
                }
                let expression = watchpoint.expression.clone();
                native_updates.push((watchpoint.uid, expression.clone()));
                hit = Some((notice.number, expression.clone(), old_value, new_value));
                bt_hit = Some((event_id, expression));
            } else if !notice_is_hit {
                created_watch = true;
            }
        }
        if !deleted_watchpoints.is_empty() {
            self.memory_check.watchpoints.retain(|watchpoint| {
                watchpoint
                    .number
                    .is_none_or(|number| !deleted_watchpoints.contains(&number))
            });
        }
        if evaluation_error {
            let message = "GDB 无法计算该变量（可能已被优化为寄存器或已离开作用域），watchpoint 已被删除。请改用全局 / volatile 变量，或用 -O0 重新编译。";
            self.memory_check.watch_error = Some(message.to_owned());
            self.set_toast(message.to_owned(), Severity::Warning);
        }
        if created_watch && !evaluation_error {
            self.memory_check.watch_error = None;
        }
        if let Some(session) = &mut self.debug.session {
            for (uid, expression) in native_updates {
                session.send(&format!("set $__se_wp_{uid} = {expression}"));
            }
        }
        let native_hit = hit.is_some();
        if let Some((event_id, expression)) = bt_hit {
            self.memory_check.pending_bt = Some(PendingWatchBt {
                event_id,
                expression,
                collecting: false,
                lines: Vec::new(),
            });
            if let Some(session) = &mut self.debug.session {
                session.send(&format!("echo __SE_WP_BT_BEGIN_{event_id}__\\n"));
                session.send("bt 6");
                session.send(&format!("echo __SE_WP_BT_END_{event_id}__\\n"));
            }
        }

        if let Some((number, expression, old_value, new_value)) = hit {
            self.debug.running = false;
            let change = match (old_value, new_value) {
                (Some(old), Some(new)) => format!("{old} -> {new}"),
                _ => "值已改变".to_owned(),
            };
            self.set_toast(
                format!("watchpoint #{number} 命中：{expression}（{change}）"),
                Severity::Warning,
            );
            if self.memory_check.open {
                self.memory_check.tab = MemoryCheckTab::Dynamic;
            }
        }

        // OpenOCD often reports a Cortex-M watchpoint as a bare SIGTRAP.
        // Query the saved and current values ourselves so the user still gets
        // the old -> new pair, source location and call stack.
        if sigtrap && !native_hit {
            self.start_watch_probe(sigtrap_location, crate::util::local_timestamp());
        }
    }

    fn start_watch_probe(&mut self, location: Option<String>, time: String) {
        if self.memory_check.pending_probe.is_some() {
            return;
        }
        let watchpoints = self
            .memory_check
            .watchpoints
            .iter()
            .map(|watchpoint| (watchpoint.uid, watchpoint.expression.clone()))
            .collect::<Vec<_>>();
        if watchpoints.is_empty() {
            return;
        }
        self.memory_check.watch_seq = self.memory_check.watch_seq.wrapping_add(1).max(1);
        let probe_id = self.memory_check.watch_seq;
        let mut requests = Vec::new();
        let mut commands = Vec::new();
        for (uid, expression) in &watchpoints {
            self.memory_check.watch_seq = self.memory_check.watch_seq.wrapping_add(1).max(1);
            let old_query = self.memory_check.watch_seq;
            self.memory_check.watch_seq = self.memory_check.watch_seq.wrapping_add(1).max(1);
            let new_query = self.memory_check.watch_seq;
            requests.push(WatchProbeRequest {
                query_id: old_query,
                watch_uid: *uid,
                role: WatchProbeRole::Old,
            });
            requests.push(WatchProbeRequest {
                query_id: new_query,
                watch_uid: *uid,
                role: WatchProbeRole::New,
            });
            commands.push(format!("echo __SE_WV_BEGIN_{old_query}__\\n"));
            commands.push(format!("print $__se_wp_{uid}"));
            commands.push(format!("echo __SE_WV_END_{old_query}__\\n"));
            commands.push(format!("echo __SE_WV_BEGIN_{new_query}__\\n"));
            commands.push(format!("print {expression}"));
            commands.push(format!("echo __SE_WV_END_{new_query}__\\n"));
            commands.push(format!("set $__se_wp_{uid} = {expression}"));
        }
        commands.push(format!("echo __SE_WP_BT_BEGIN_{probe_id}__\\n"));
        commands.push("bt 6".to_owned());
        commands.push(format!("echo __SE_WP_BT_END_{probe_id}__\\n"));
        if let Some(session) = &mut self.debug.session {
            for command in commands {
                session.send(&command);
            }
        }
        self.memory_check.pending_probe = Some(PendingWatchProbe {
            id: probe_id,
            time,
            location,
            requests,
            results: Vec::new(),
            collecting: None,
            bt_collecting: false,
            bt_done: false,
            bt_lines: Vec::new(),
            ticks: 0,
        });
    }

    fn poll_watch_probe(&mut self, fresh: &[String]) {
        let Some(mut probe) = self.memory_check.pending_probe.take() else {
            return;
        };
        probe.ticks = probe.ticks.saturating_add(1);
        let bt_begin = format!("__SE_WP_BT_BEGIN_{}__", probe.id);
        let bt_end = format!("__SE_WP_BT_END_{}__", probe.id);
        for line in fresh {
            let trimmed = line.trim();
            if let Some(query) = marker_number(trimmed, "__SE_WV_BEGIN_") {
                probe.collecting = Some(query);
                continue;
            }
            if let Some(query) = marker_number(trimmed, "__SE_WV_END_") {
                if !probe.results.iter().any(|(id, _)| *id == query) {
                    probe.results.push((query, None));
                }
                if probe.collecting == Some(query) {
                    probe.collecting = None;
                }
                continue;
            }
            if trimmed.contains(&bt_begin) {
                probe.bt_collecting = true;
                probe.bt_lines.clear();
                continue;
            }
            if trimmed.contains(&bt_end) {
                probe.bt_done = true;
                probe.bt_collecting = false;
                continue;
            }
            if let Some(query) = probe.collecting {
                if let Some(value) = parse_print_value(trimmed) {
                    if let Some(entry) = probe.results.iter_mut().find(|(id, _)| *id == query) {
                        if entry.1.is_none() {
                            entry.1 = Some(value);
                        }
                    } else {
                        probe.results.push((query, Some(value)));
                    }
                }
            }
            if probe.bt_collecting {
                let value = trimmed.trim_start_matches("(gdb) ").trim();
                if value.starts_with('#') || value.contains(" at ") {
                    probe.bt_lines.push(value.to_owned());
                    if probe.bt_lines.len() > 8 {
                        probe.bt_lines.remove(0);
                    }
                }
            }
        }
        let values_ready = probe
            .requests
            .iter()
            .all(|request| probe.results.iter().any(|(id, _)| *id == request.query_id));
        let finished = (values_ready && probe.bt_done) || probe.ticks > 600;
        if finished {
            self.finish_watch_probe(probe);
        } else {
            self.memory_check.pending_probe = Some(probe);
        }
    }

    fn finish_watch_probe(&mut self, probe: PendingWatchProbe) {
        let mut changes = Vec::new();
        for request in &probe.requests {
            if request.role != WatchProbeRole::New {
                continue;
            }
            let Some(new_value) = probe
                .results
                .iter()
                .find(|(id, _)| *id == request.query_id)
                .and_then(|(_, value)| value.clone())
            else {
                continue;
            };
            let old_value = probe
                .requests
                .iter()
                .find(|candidate| {
                    candidate.watch_uid == request.watch_uid
                        && candidate.role == WatchProbeRole::Old
                })
                .and_then(|candidate| {
                    probe
                        .results
                        .iter()
                        .find(|(id, _)| *id == candidate.query_id)
                        .and_then(|(_, value)| value.clone())
                });
            let Some(old_value) = old_value else {
                continue;
            };
            if old_value != new_value {
                changes.push((request.watch_uid, old_value, new_value));
            }
        }
        if changes.is_empty() {
            return;
        }
        let mut event_ids = Vec::with_capacity(changes.len());
        for _ in &changes {
            self.memory_check.watch_seq = self.memory_check.watch_seq.wrapping_add(1).max(1);
            event_ids.push(self.memory_check.watch_seq);
        }
        let mut hit = None;
        for ((uid, old_value, new_value), event_id) in changes.into_iter().zip(event_ids) {
            let Some(watchpoint) = self
                .memory_check
                .watchpoints
                .iter_mut()
                .find(|watchpoint| watchpoint.uid == uid)
            else {
                continue;
            };
            watchpoint.hits = watchpoint.hits.saturating_add(1);
            watchpoint.old_value = Some(old_value.clone());
            watchpoint.new_value = Some(new_value.clone());
            watchpoint.history.push(WatchEvent {
                id: event_id,
                time: probe.time.clone(),
                old_value: Some(old_value.clone()),
                new_value: Some(new_value.clone()),
                location: probe.location.clone(),
                stack: probe.bt_lines.clone(),
            });
            if watchpoint.history.len() > 200 {
                watchpoint.history.remove(0);
            }
            if hit.is_none() {
                let number = watchpoint
                    .number
                    .map(|number| format!("#{number}"))
                    .unwrap_or_else(|| "#?".to_owned());
                hit = Some((number, watchpoint.expression.clone(), old_value, new_value));
            }
        }
        let Some((number, expression, old_value, new_value)) = hit else {
            return;
        };
        self.memory_check.watch_error = None;
        self.debug.running = false;
        self.set_toast(
            format!("watchpoint {number} 命中：{expression}（{old_value} -> {new_value}）"),
            Severity::Warning,
        );
        if self.memory_check.open {
            self.memory_check.tab = MemoryCheckTab::Dynamic;
        }
    }

    /// Collect the `bt` output requested after a watchpoint hit. The markers
    /// keep unrelated gdb output out of the stack.
    fn poll_watch_backtrace(&mut self, fresh: &[String]) {
        let Some(mut pending) = self.memory_check.pending_bt.take() else {
            return;
        };
        let begin = format!("__SE_WP_BT_BEGIN_{}__", pending.event_id);
        let end = format!("__SE_WP_BT_END_{}__", pending.event_id);
        let mut finished = false;
        for line in fresh {
            let trimmed = line.trim();
            if trimmed.contains(&begin) {
                pending.collecting = true;
                pending.lines.clear();
                continue;
            }
            if trimmed.contains(&end) {
                finished = true;
                break;
            }
            if pending.collecting {
                let value = trimmed.trim_start_matches("(gdb) ").trim();
                if value.starts_with('#') || value.contains(" at ") {
                    pending.lines.push(value.to_owned());
                    if pending.lines.len() > 8 {
                        pending.lines.remove(0);
                    }
                }
            }
        }
        if finished {
            if let Some(watchpoint) = self
                .memory_check
                .watchpoints
                .iter_mut()
                .find(|watchpoint| watchpoint.expression == pending.expression)
            {
                if let Some(event) = watchpoint
                    .history
                    .iter_mut()
                    .find(|event| event.id == pending.event_id)
                {
                    event.stack = pending.lines;
                }
            }
        } else {
            self.memory_check.pending_bt = Some(pending);
        }
    }

    fn current_word(&self) -> Option<String> {
        let doc = self.active_document()?;
        crate::navigation::word_at(doc.buffer.as_string(), doc.cursor_index).map(|value| value.0)
    }

    fn hover_info(
        &self,
        pane_index: usize,
        word: &str,
        line: usize,
    ) -> Option<crate::hover::HoverInfo> {
        let doc = self.tabs.get(pane_index)?;
        crate::hover::build(
            &doc.path,
            doc.buffer.as_string(),
            word,
            line,
            &self.symbols.items,
        )
    }

    fn ensure_symbols(&mut self) {
        if !self.symbols.scanned {
            self.refresh_symbols();
        }
    }

    fn definition_at_cursor(&mut self, ctx: &egui::Context) {
        if self.lsp.as_ref().is_some_and(|session| session.is_ready()) {
            let data = self.active_document().map(|doc| {
                (
                    doc.path.clone(),
                    doc.buffer.as_string().to_owned(),
                    doc.cursor_index,
                )
            });
            if let Some((path, text, cursor)) = data {
                if let Some(session) = self.lsp.as_mut() {
                    if session.definition(&path, &text, cursor).is_ok() {
                        return;
                    }
                }
            }
        }
        self.definition_from_index(ctx);
    }

    fn references_at_cursor(&mut self) {
        if self.lsp.as_ref().is_some_and(|session| session.is_ready()) {
            let data = self.active_document().map(|doc| {
                (
                    doc.path.clone(),
                    doc.buffer.as_string().to_owned(),
                    doc.cursor_index,
                )
            });
            if let Some((path, text, cursor)) = data {
                if let Some(session) = self.lsp.as_mut() {
                    if session.references(&path, &text, cursor).is_ok() {
                        return;
                    }
                }
            }
        }
        let Some(word) = self.current_word() else {
            self.set_toast("光标不在标识符上", Severity::Info);
            return;
        };
        self.open_references_for_word(&word);
    }

    fn open_references_for_word(&mut self, word: &str) {
        self.project_search.open = true;
        self.project_search.query = format!(r"\b{}\b", regex::escape(word));
        self.project_search.case_sensitive = true;
        self.project_search.use_regex = true;
        self.project_search.focus_query = false;
        self.run_project_search();
    }

    fn apply_column_edit(&mut self, ctx: &egui::Context) {
        let state = (
            self.column_edit.line_start,
            self.column_edit.line_end,
            self.column_edit.column,
            self.column_edit.count,
            self.column_edit.operation,
            self.column_edit.insert.clone(),
        );
        let Some(index) = self.active else {
            return;
        };
        let Some(doc) = self.tabs.get_mut(index) else {
            return;
        };
        if doc.read_only {
            self.set_toast("只读文件不能进行列编辑", Severity::Warning);
            return;
        }
        let (start, end, column, count, operation, insert) = state;
        let (new_text, cursor) = crate::column::apply(
            doc.buffer.as_string(),
            start,
            end,
            column,
            count,
            operation,
            &insert,
        );
        doc.buffer.set_text(new_text);
        doc.update_cursor(cursor);
        set_editor_cursor(ctx, &doc.path, cursor);
        self.column_edit.open = false;
        self.focus_editor = true;
        self.find.dirty = true;
        self.set_toast("列编辑已应用", Severity::Success);
    }

    fn ensure_member_index(&mut self) {
        if self.member_index.is_some() {
            return;
        }
        let Some(root) = self.project.as_ref().map(|project| project.root.clone()) else {
            return;
        };
        let ignore = IgnoreRules::load(
            &root,
            self.settings.respect_gitignore,
            &self.settings.ignore_patterns,
        );
        self.member_index = Some(crate::completion::build(&root, &ignore));
    }

    fn completion_items(
        &mut self,
        context: &crate::completion::Context,
    ) -> Vec<crate::completion::CompletionItem> {
        if let Some(chain) = &context.member_chain {
            self.ensure_member_index();
            return self
                .member_index
                .as_ref()
                .map(|index| index.resolve_members(chain))
                .unwrap_or_default();
        }

        self.ensure_symbols();
        let mut seen = BTreeSet::new();
        let mut items = Vec::new();
        for symbol in &self.symbols.items {
            if !seen.insert(symbol.name.clone()) {
                continue;
            }
            let kind = match symbol.kind {
                crate::symbols::SymbolKind::Function => crate::completion::CompletionKind::Function,
                crate::symbols::SymbolKind::Struct => crate::completion::CompletionKind::Type,
                crate::symbols::SymbolKind::Macro => crate::completion::CompletionKind::Macro,
                crate::symbols::SymbolKind::Label => crate::completion::CompletionKind::Label,
            };
            items.push(crate::completion::CompletionItem {
                name: symbol.name.clone(),
                kind,
                detail: None,
            });
        }
        items.sort_by_key(|item| item.name.to_lowercase());
        items
    }

    fn open_completion(&mut self) {
        let Some((path, cursor, revision, context)) = self.active_document().map(|doc| {
            (
                doc.path.clone(),
                doc.cursor_index,
                doc.buffer.revision(),
                crate::completion::context_at(doc.buffer.as_string(), doc.cursor_index),
            )
        }) else {
            return;
        };
        if !crate::lsp::supports_lsp(&path) {
            self.set_toast("当前文件不支持代码补全", Severity::Info);
            self.completion.open = false;
            return;
        }
        let items = self.completion_items(&context);
        self.completion.query = context.prefix;
        self.completion.prefix_start = context.prefix_start;
        self.completion.cursor = cursor;
        self.completion.member_chain = context.member_chain;
        self.completion.path = Some(path);
        self.completion.last_revision = revision;
        self.completion.last_cursor = cursor;
        if items.is_empty() {
            if self.lsp.as_ref().is_some_and(|session| session.is_ready()) {
                self.completion.open = false;
                self.request_lsp_completion();
            } else {
                self.set_toast("没有可用的补全项", Severity::Info);
                self.completion.open = false;
            }
            return;
        }
        self.completion.open = true;
        self.completion.selected = 0;
        self.completion.items = items;
        self.request_lsp_completion();
    }

    fn completion_filtered(&self) -> Vec<usize> {
        let query = self.completion.query.to_lowercase();
        self.completion
            .items
            .iter()
            .enumerate()
            .filter(|(_, item)| item.name.to_lowercase().starts_with(&query))
            .map(|(index, _)| index)
            .take(200)
            .collect()
    }

    fn has_modal(&self) -> bool {
        self.settings_draft.is_some()
            || self.theme_settings.is_some()
            || self.firmware_confirm
            || self.pending.is_some()
            || self.meta_draft.is_some()
            || self.meta_add.is_some()
            || self.flash_draft.is_some()
            || self.build_options_draft.is_some()
            || self.create_entry.is_some()
            || self.rename_entry.is_some()
            || self.delete_entry.is_some()
            || self.memory_draft.is_some()
            || self.lsp_rename.is_some()
            || self.debug.peripheral_edit.is_some()
            || self.ai_settings_draft.is_some()
    }

    fn handle_completion_keys(&mut self, ctx: &egui::Context) {
        if !self.completion.open {
            return;
        }
        if self.has_modal() {
            self.completion.open = false;
            return;
        }
        let filtered = self.completion_filtered();
        if filtered.is_empty() {
            self.completion.open = false;
            return;
        }
        self.completion.selected = self.completion.selected.min(filtered.len() - 1);
        let mut delta = 0i32;
        let mut accept = false;
        let mut close = false;
        ctx.input_mut(|input| {
            if input.consume_key(egui::Modifiers::NONE, egui::Key::ArrowDown) {
                delta = 1;
            } else if input.consume_key(egui::Modifiers::NONE, egui::Key::ArrowUp) {
                delta = -1;
            }
            if input.consume_key(egui::Modifiers::NONE, egui::Key::Enter)
                || input.consume_key(egui::Modifiers::NONE, egui::Key::Tab)
            {
                accept = true;
            }
            if input.consume_key(egui::Modifiers::NONE, egui::Key::Escape) {
                close = true;
            }
        });
        if close {
            self.completion.open = false;
            self.focus_editor = true;
            return;
        }
        if delta != 0 {
            let count = filtered.len();
            self.completion.selected = if delta > 0 {
                (self.completion.selected + 1) % count
            } else {
                (self.completion.selected + count - 1) % count
            };
            ctx.request_repaint();
            return;
        }
        if accept {
            let index = self.completion.selected.min(filtered.len() - 1);
            let name = self.completion.items[filtered[index]].name.clone();
            self.insert_completion(ctx, &name);
        }
    }

    fn update_completion(&mut self, ctx: &egui::Context) {
        let Some((path, cursor, revision)) = self
            .active_document()
            .map(|doc| (doc.path.clone(), doc.cursor_index, doc.buffer.revision()))
        else {
            self.completion.open = false;
            return;
        };
        if !crate::lsp::supports_lsp(&path) {
            self.completion.open = false;
            self.completion.path = Some(path);
            self.completion.last_revision = revision;
            self.completion.last_cursor = cursor;
            return;
        }
        let editor_id = egui::Id::new(("simple_embedded_code_editor", &path));
        let focused = ctx.memory(|memory| memory.has_focus(editor_id));
        if !focused || self.has_modal() {
            self.completion.open = false;
            self.completion.path = Some(path);
            self.completion.last_revision = revision;
            self.completion.last_cursor = cursor;
            return;
        }
        if self.completion.path.as_ref() != Some(&path) {
            self.completion.open = false;
            self.completion.path = Some(path);
            self.completion.last_revision = revision;
            self.completion.last_cursor = cursor;
            return;
        }

        let changed = revision != self.completion.last_revision;
        if changed {
            let Some(context) = self
                .active_document()
                .map(|doc| crate::completion::context_at(doc.buffer.as_string(), cursor))
            else {
                self.completion.open = false;
                return;
            };
            let trigger = context.member_chain.is_some() || context.prefix.chars().count() >= 2;
            if trigger {
                let items = self.completion_items(&context);
                let query_changed = self.completion.query != context.prefix
                    || self.completion.member_chain != context.member_chain;
                self.completion.query = context.prefix;
                self.completion.prefix_start = context.prefix_start;
                self.completion.cursor = cursor;
                self.completion.member_chain = context.member_chain;
                self.completion.path = Some(path.clone());
                if items.is_empty() {
                    self.completion.open = false;
                } else {
                    self.completion.open = true;
                    self.completion.items = items;
                    if query_changed {
                        self.completion.selected = 0;
                    }
                }
                self.completion.last_revision = revision;
                self.completion.last_cursor = cursor;
                self.request_lsp_completion();
            } else {
                self.completion.open = false;
                self.completion.last_revision = revision;
                self.completion.last_cursor = cursor;
            }
        } else if self.completion.open && self.completion.cursor != cursor {
            self.completion.open = false;
            self.completion.last_cursor = cursor;
        } else {
            self.completion.last_cursor = cursor;
        }
    }

    fn insert_completion(&mut self, ctx: &egui::Context, name: &str) {
        let Some(index) = self.active else {
            return;
        };
        let Some(doc) = self.tabs.get_mut(index) else {
            return;
        };
        if doc.read_only {
            self.set_toast("只读文件不能补全", Severity::Warning);
            return;
        }
        let text = doc.buffer.as_string().to_owned();
        let start = self.completion.prefix_start.min(self.completion.cursor);
        let end = self.completion.cursor.min(text.chars().count());
        let (new_text, cursor) = crate::navigation::replace_range_chars(&text, start, end, name);
        doc.buffer.set_text(new_text);
        doc.update_cursor(cursor);
        set_editor_cursor(ctx, &doc.path, cursor);
        self.completion.open = false;
        self.completion.last_revision = doc.buffer.revision();
        self.completion.last_cursor = cursor;
        self.focus_editor = true;
        self.find.dirty = true;
    }

    fn debug_target(&self) -> Option<PathBuf> {
        let root = self.project.as_ref()?.root.clone();
        let configured = self.project_debug.program.trim();
        if !configured.is_empty() {
            let path = PathBuf::from(configured);
            let path = if path.is_absolute() {
                path
            } else {
                root.join(path)
            };
            if path.is_file() {
                return Some(std::fs::canonicalize(&path).unwrap_or(path));
            }
        }
        detect_debug_program(&root)
    }

    fn arm_gdb_dirs(&self) -> Vec<PathBuf> {
        let mut dirs = Vec::new();
        let mut add_tool = |path: &Path| {
            let dir = if path.is_file() {
                path.parent().unwrap_or(path)
            } else {
                path
            };
            if dir.is_dir() {
                dirs.push(dir.to_path_buf());
            }
        };
        let compiler = self.settings.compiler_path.trim();
        if !compiler.is_empty() {
            add_tool(Path::new(compiler));
        }
        for path in &self.build_config.extra_paths {
            let path = Path::new(path.trim());
            if path.as_os_str().is_empty() {
                continue;
            }
            add_tool(path);
        }
        if self.settings.use_bundled_toolchain {
            let root = crate::toolchain::resolve_tools_dir(&self.settings.bundled_tools_dir);
            if let Some(compiler) = crate::toolchain::detect(&root).compiler {
                add_tool(&compiler);
            }
        }
        let (compiler, _) = crate::toolchain::detect_installed();
        if let Some(compiler) = compiler {
            add_tool(&compiler);
        }
        dirs.sort();
        dirs.dedup();
        dirs
    }

    fn debug_start_error(&mut self, message: impl Into<String>) -> bool {
        let message = message.into();
        self.debug.starting = false;
        self.debug_start = None;
        self.debug.error = Some(message.clone());
        self.debug.open = true;
        self.set_toast(message, Severity::Error);
        false
    }

    fn start_debug_session(&mut self) -> bool {
        if self.debug.starting || self.debug_start.is_some() {
            return true;
        }
        let Some(target) = self.debug_target() else {
            return self.debug_start_error("未找到调试程序，请在设置中填写 ELF/AXF/EXE 路径");
        };
        if let Some(root) = self.project.as_ref().map(|project| project.root.clone()) {
            if let Some(source) = newest_newer_source(&target, &root) {
                self.set_toast(
                    format!(
                        "调试程序比 {} 旧，断点/变量可能不匹配；请先重新构建",
                        util::display_path(&source)
                    ),
                    Severity::Warning,
                );
            }
        }
        self.ensure_registers_loaded();
        let kind = crate::debugger::target_kind(&target);
        let configured_gdb = if self.project_debug.gdb_path.trim().is_empty() {
            self.settings.gdb_path.clone()
        } else {
            self.project_debug.gdb_path.clone()
        };
        let gdb = if kind == crate::debugger::DebugTargetKind::Embedded {
            let dirs = self.arm_gdb_dirs();
            match crate::debugger::detect_arm_gdb(&configured_gdb, &dirs) {
                Some(gdb) => gdb,
                None => {
                    return self.debug_start_error(
                        "未找到 arm-none-eabi-gdb，请在“配置 → 调试”填写 ARM gdb 路径；MinGW 的 gdb 不能调试 ARM 目标",
                    );
                }
            }
        } else {
            match crate::debugger::detect_gdb(&configured_gdb) {
                Some(gdb) => gdb,
                None => {
                    return self.debug_start_error("未找到 gdb，请在“配置 → 调试”填写 gdb 路径");
                }
            }
        };
        let server = if kind == crate::debugger::DebugTargetKind::Embedded {
            let Some(root) = self.project.as_ref().map(|project| project.root.clone()) else {
                return self.debug_start_error("请先打开工程");
            };
            match self.build_config.flash_tool {
                FlashTool::OpenOcd => {
                    if !crate::flash::openocd_available(
                        &self.build_config.openocd.executable,
                        &root,
                    ) {
                        return self.debug_start_error(
                            "未找到 OpenOCD，请先在“工程信息 → 烧录配置”选择 openocd.exe",
                        );
                    }
                    let port = match crate::debugger::free_tcp_port() {
                        Ok(port) => port,
                        Err(err) => return self.debug_start_error(err),
                    };
                    let mut telnet_port = match crate::debugger::free_tcp_port() {
                        Ok(port) => port,
                        Err(err) => return self.debug_start_error(err),
                    };
                    if telnet_port == port {
                        telnet_port = match crate::debugger::free_tcp_port() {
                            Ok(port) => port,
                            Err(err) => return self.debug_start_error(err),
                        };
                    }
                    match crate::flash::openocd_server_spec(
                        &self.build_config.openocd,
                        &root,
                        port,
                        telnet_port,
                    ) {
                        Ok(spec) => Some((
                            spec,
                            port,
                            crate::debugger::ServerKind::OpenOcd,
                            Some(telnet_port),
                        )),
                        Err(err) => return self.debug_start_error(err),
                    }
                }
                FlashTool::PyOcd => {
                    if !crate::flash::pyocd_available(&self.build_config.pyocd.executable, &root) {
                        return self.debug_start_error(
                            "未找到 pyOCD，请先在“工程信息 → 烧录配置”选择 pyocd.exe",
                        );
                    }
                    let port = match crate::debugger::free_tcp_port() {
                        Ok(port) => port,
                        Err(err) => return self.debug_start_error(err),
                    };
                    match crate::flash::pyocd_server_spec(&self.build_config.pyocd, &root, port) {
                        Ok(spec) => Some((spec, port, crate::debugger::ServerKind::PyOcd, None)),
                        Err(err) => return self.debug_start_error(err),
                    }
                }
                FlashTool::Custom => {
                    return self.debug_start_error(
                        "调试需要 GDB server：请先在“工程信息 → 烧录配置”选择 OpenOCD 或 pyOCD",
                    );
                }
            }
        } else {
            None
        };

        let (tx, rx) = std::sync::mpsc::channel();
        self.debug_start = Some(rx);
        self.debug.starting = true;
        self.debug.pending_continue = false;
        self.debug.running = false;
        self.debug.open = true;
        self.debug.error = None;
        self.debug.pending_watch_rearm = false;
        self.debug.waiting_for_main = false;
        self.detach_hardware_watchpoints();
        self.debug.log.clear();
        self.debug.target = Some(target.clone());
        self.debug.watch_refresh = None;
        self.debug.last_watch_refresh = None;
        self.debug.current_source = None;
        self.debug.log_scan = 0;
        self.debug.registers.clear();
        self.debug.peripheral_values.clear();
        self.debug.peripheral_open.clear();
        self.debug.peripheral_registers_open.clear();
        self.debug.peripheral_edit = None;
        self.debug.peripheral_initialized = false;
        self.debug.peripheral_refresh = None;
        self.debug.peripheral_last_refresh = None;
        for watch in &mut self.debug.watches {
            watch.value.clear();
            watch.type_name.clear();
        }
        self.set_toast("正在启动调试服务…", Severity::Info);

        let target_for_thread = target;
        let gdb_for_thread = gdb;
        std::thread::spawn(move || {
            let result = if let Some((spec, port, server_kind, telnet_port)) = server {
                crate::debugger::DebugSession::start_embedded(
                    &target_for_thread,
                    &gdb_for_thread,
                    spec,
                    port,
                    server_kind,
                    telnet_port,
                )
                .map(|session| (session, target_for_thread))
            } else {
                crate::debugger::DebugSession::start(&target_for_thread, &gdb_for_thread)
                    .map(|session| (session, target_for_thread))
            };
            let _ = tx.send(result);
        });
        true
    }

    fn poll_debug_start(&mut self) {
        let Some(rx) = self.debug_start.as_ref() else {
            return;
        };
        match rx.try_recv() {
            Ok(Ok((mut session, target))) => {
                self.debug_start = None;
                self.debug.starting = false;
                session.poll();
                for (path, line) in &self.debug.breakpoints {
                    session.send(&format!("break \"{}\":{}", gdb_source_path(path), line));
                }
                self.debug.session = Some(session);
                self.debug.target = Some(target);
                self.debug.error = None;
                self.debug.log.clear();
                self.debug.open = true;
                self.debug.log_scan = 0;
                self.debug.current_source = None;
                self.debug.running = false;
                self.refresh_debug_watch();
                if self.settings.auto_start_watch {
                    self.memory_check.open = true;
                    self.memory_check.tab = MemoryCheckTab::Dynamic;
                    // Do not arm at the reset vector: startup clears `.bss`, which
                    // would trip a watchpoint on a global before `main` runs.
                    // Arm once the temporary `main` breakpoint stops the target.
                    self.debug.pending_watch_rearm = true;
                }
                let continue_now = self.debug.pending_continue;
                self.debug.pending_continue = false;
                if continue_now {
                    if let Some(session) = &mut self.debug.session {
                        // Ignore log lines produced by reset/load so the rearm
                        // below only reacts to the stop at `main`.
                        self.debug.log_scan = session.log.len();
                        // Stop once at main so starting a session immediately opens
                        // and highlights the entry source line instead of running
                        // silently until the first user breakpoint.
                        session.send("tbreak main");
                        if session.started {
                            session.send("continue");
                        } else {
                            session.send("run");
                            session.started = true;
                        }
                        self.debug.waiting_for_main = true;
                        self.debug.running = true;
                    }
                }
                self.set_toast("调试会话已启动", Severity::Success);
            }
            Ok(Err(err)) => {
                self.debug_start = None;
                self.debug.starting = false;
                self.debug.pending_continue = false;
                self.debug.running = false;
                self.debug.error = Some(err.clone());
                self.debug.open = true;
                self.set_toast(err, Severity::Error);
            }
            Err(std::sync::mpsc::TryRecvError::Empty) => {}
            Err(std::sync::mpsc::TryRecvError::Disconnected) => {
                self.debug_start = None;
                self.debug.starting = false;
                self.debug.pending_continue = false;
                self.debug.running = false;
                let message = "调试启动失败：后台线程已退出".to_owned();
                self.debug.error = Some(message.clone());
                self.debug.open = true;
                self.set_toast(message, Severity::Error);
            }
        }
    }

    fn focus_debug_output(&mut self) {
        self.output_view = OutputView::Debug;
        if !self.settings.output_open {
            self.settings.output_open = true;
            if self.settings.output_height < 100.0 {
                self.settings.output_height = 214.0;
            }
            self.settings.save();
        }
    }

    fn debug_continue(&mut self) {
        if self.effective_toolchain().is_micropython() {
            self.set_toast("MicroPython 串口模式不使用 gdb 调试", Severity::Warning);
            return;
        }
        self.focus_debug_output();
        if self.debug.running {
            self.debug_pause();
            return;
        }
        if self.debug.starting {
            self.debug.pending_continue = true;
            return;
        }
        if self.debug.session.is_none() {
            if !self.start_debug_session() {
                return;
            }
            self.debug.pending_continue = true;
            return;
        }
        if let Some(session) = &mut self.debug.session {
            if !session.started {
                session.send("run");
                session.started = true;
            } else {
                session.send("continue");
            }
            self.debug.running = true;
        }
    }

    fn debug_pause(&mut self) {
        self.focus_debug_output();
        let Some(session) = &mut self.debug.session else {
            self.set_toast("调试会话尚未启动", Severity::Info);
            return;
        };
        let halted = session.interrupt();
        if halted {
            // OpenOCD stopped the target from its telnet console. Ask gdb for
            // the current frame so the editor can update the source marker.
            session.send("frame");
            self.debug.running = false;
        }
        self.set_toast(
            if halted {
                "已请求暂停"
            } else {
                "正在暂停目标"
            },
            Severity::Info,
        );
    }

    fn debug_command(&mut self, command: &str) {
        self.focus_debug_output();
        if self.debug.starting {
            self.set_toast("正在启动调试服务，请稍候", Severity::Info);
            return;
        }
        let Some(session) = &mut self.debug.session else {
            self.set_toast("请先启动调试会话", Severity::Warning);
            return;
        };
        session.send(command);
        let command = command.trim().to_ascii_lowercase();
        if [
            "continue", "c", "run", "r", "step", "s", "next", "n", "finish",
        ]
        .iter()
        .any(|value| command == *value || command.starts_with(&format!("{value} ")))
        {
            self.debug.running = true;
        }
    }

    /// Step commands often only print the source text (`95  while (1)`) without
    /// `at file:line`. Ask gdb for the current frame afterwards so the editor
    /// can still follow the execution line.
    fn debug_step_command(&mut self, command: &str) {
        if self.debug.running {
            self.set_toast("目标正在运行，请先暂停", Severity::Info);
            return;
        }
        self.debug_command(command);
        if let Some(session) = &mut self.debug.session {
            session.send("frame");
            self.debug.running = true;
        }
    }

    fn debug_stop(&mut self) {
        if self.debug_start.take().is_some() {
            self.debug.starting = false;
            self.debug.pending_continue = false;
            self.debug.running = false;
            self.debug.pending_watch_rearm = false;
            self.debug.waiting_for_main = false;
            self.debug.error = None;
            self.debug.log.push("[调试启动已取消]".to_owned());
            self.detach_hardware_watchpoints();
            return;
        }
        if let Some(mut session) = self.debug.session.take() {
            session.poll();
            self.debug.log = session.log.clone();
            session.stop();
            self.debug.log.push("[调试已停止]".to_owned());
        }
        self.debug.starting = false;
        self.debug.pending_continue = false;
        self.debug.running = false;
        self.debug.pending_watch_rearm = false;
        self.debug.waiting_for_main = false;
        self.debug.watch_refresh = None;
        self.debug.last_watch_refresh = None;
        self.debug.error = None;
        self.debug.current_source = None;
        self.debug.log_scan = 0;
        self.detach_hardware_watchpoints();
        self.debug.peripheral_values.clear();
        self.debug.peripheral_open.clear();
        self.debug.peripheral_registers_open.clear();
        self.debug.peripheral_edit = None;
        self.debug.peripheral_initialized = false;
        self.debug.peripheral_refresh = None;
        self.debug.peripheral_last_refresh = None;
    }

    /// Follow new gdb console output and open the file/line where the target
    /// stopped. Only new lines are scanned so a manual navigation away from
    /// the paused line is not undone on every repaint.
    fn poll_debug_location(&mut self, ctx: &egui::Context) {
        let Some(session) = self.debug.session.as_ref() else {
            return;
        };
        if self.debug.log_scan > session.log.len() {
            self.debug.log_scan = 0;
        }
        if session.log.len() <= self.debug.log_scan {
            return;
        }
        let fresh = session.log[self.debug.log_scan..].to_vec();
        self.debug.log_scan = session.log.len();
        if fresh.iter().any(|line| line.contains("Continuing.")) {
            self.debug.running = true;
        }
        if fresh.iter().any(|line| {
            line.contains("exited normally")
                || line.contains("Program received signal")
                || line.contains("Remote connection closed")
        }) {
            self.debug.running = false;
        }
        let main_hit = fresh.iter().any(|line| {
            line.contains("Temporary breakpoint")
                && (line.contains(", main") || line.contains("in main"))
        });
        if self.debug.waiting_for_main
            && fresh.iter().any(|line| {
                line.contains("Function \"main\" not defined")
                    || line.contains("No symbol \"main\"")
            })
        {
            self.debug.waiting_for_main = false;
        }
        if self.debug.waiting_for_main {
            // `load`/reset output can still be in flight when the automatic
            // `continue` is sent. Ignore it until gdb confirms the temporary
            // breakpoint at main, otherwise watchpoints are armed too early.
            if !main_hit {
                return;
            }
            self.debug.waiting_for_main = false;
        }
        let mut found = None;
        for line in fresh.iter().rev() {
            if let Some((path, line_no)) = crate::debugger::parse_stop_location(line) {
                found = Some((path, line_no));
                break;
            }
        }
        let mut invalid = Vec::new();
        for line in &fresh {
            if let Some((path, line_no)) = crate::debugger::parse_uncompiled_line(line) {
                if let Some(path) = self.resolve_debug_source(&path) {
                    if self.debug.breakpoints.remove(&(path.clone(), line_no)) {
                        invalid.push((path, line_no));
                    }
                }
            }
        }
        if let Some((path, line_no)) = invalid.last() {
            if let Some(session) = &mut self.debug.session {
                session.send(&format!("clear \"{}\":{}", gdb_source_path(path), line_no));
            }
            self.set_toast(
                format!(
                    "{} 第 {} 行没有可执行代码，已移除断点",
                    util::file_name(path),
                    line_no
                ),
                Severity::Warning,
            );
        }
        let Some((raw, line)) = found else {
            return;
        };
        let Some(path) = self.resolve_debug_source(&raw) else {
            return;
        };
        self.debug.running = false;
        self.debug.current_source = Some((path.clone(), line));
        let startup_stop = raw.to_ascii_lowercase().contains("startup");
        if self.debug.pending_watch_rearm && (main_hit || !startup_stop) {
            self.debug.pending_watch_rearm = false;
            self.rearm_hardware_watchpoints();
        }
        self.jump_to_debug_source(ctx, &path, line);
        ctx.request_repaint();
    }

    fn resolve_debug_source(&self, raw: &str) -> Option<PathBuf> {
        let raw = raw.trim().trim_matches('"');
        if raw.is_empty() {
            return None;
        }
        let candidate = PathBuf::from(raw);
        let mut bases = Vec::new();
        if let Some(project) = &self.project {
            bases.push(project.root.clone());
            for sub in [
                "build",
                "build/Debug",
                "build/Release",
                "Debug",
                "Release",
                "out",
            ] {
                bases.push(project.root.join(sub));
            }
        }
        if let Ok(cwd) = std::env::current_dir() {
            bases.push(cwd);
        }
        let absolute = if candidate.is_absolute() {
            Some(candidate.clone())
        } else {
            bases
                .into_iter()
                .map(|base| base.join(&candidate))
                .find(|path| path.is_file())
        };
        let path = absolute.filter(|path| path.is_file())?;
        Some(std::fs::canonicalize(&path).unwrap_or(path))
    }

    fn debug_jump_to_current(&mut self, ctx: &egui::Context) {
        let Some((path, line)) = self.debug.current_source.clone() else {
            self.set_toast("当前没有可定位的执行行", Severity::Info);
            return;
        };
        self.jump_to_debug_source(ctx, &path, line);
        self.set_toast(
            format!("已定位到 {} 第 {} 行", util::file_name(&path), line),
            Severity::Info,
        );
    }

    /// Show the paused source line without replacing the debug console focus.
    fn jump_to_debug_source(&mut self, ctx: &egui::Context, path: &Path, line: usize) {
        // Callers may pass a project-relative path with `/` separators; the
        // open tab stores the canonical Windows path, so normalize first.
        let path = std::fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf());
        self.open_file(path.clone());
        self.focus_editor = false;
        let Some(index) = self.tabs.iter().position(|doc| same_path(&doc.path, &path)) else {
            return;
        };
        self.active = Some(index);
        let Some(doc) = self.tabs.get_mut(index) else {
            return;
        };
        let start = crate::util::char_index_at_line(doc.buffer.as_string(), line);
        doc.update_cursor(start);
        set_editor_cursor(ctx, &doc.path, start);
        self.scroll_to_cursor = true;
        if let Some(tree) = &mut self.tree {
            tree.reveal(&path);
        }
    }

    fn refresh_debug_watch(&mut self) {
        if self.debug.watch_refresh.is_some() {
            let stale = self
                .debug
                .last_watch_refresh
                .is_none_or(|last| last.elapsed() >= Duration::from_secs(4));
            if !stale {
                return;
            }
            // A lost END marker must not disable Live Watch forever.
            self.debug.watch_refresh = None;
            if let Some(session) = &mut self.debug.session {
                session.watch_output.clear();
                session.preg_output.clear();
            }
        }
        let expressions = self
            .debug
            .watches
            .iter()
            .map(|watch| watch.expr.clone())
            .collect::<Vec<_>>();
        let Some(session) = &mut self.debug.session else {
            return;
        };
        self.debug.watch_refresh_id = self.debug.watch_refresh_id.wrapping_add(1).max(1);
        let id = self.debug.watch_refresh_id;
        session.send(&format!("echo __SE_WATCH_BEGIN_{id}__\\n"));
        for (index, expression) in expressions.iter().enumerate() {
            session.send(&format!("echo __SE_WATCH_V_{id}_{index}__\\n"));
            session.send(&format!("p {expression}"));
            session.send(&format!("echo __SE_WATCH_T_{id}_{index}__\\n"));
            session.send(&format!("whatis {expression}"));
        }
        session.send(&format!("echo __SE_WATCH_R_{id}__\\n"));
        session.send("info registers");
        session.send(&format!("echo __SE_WATCH_END_{id}__\\n"));
        self.debug.watch_refresh = Some(id);
        self.debug.last_watch_refresh = Some(Instant::now());
    }

    fn poll_debug_watch(&mut self) {
        let Some(id) = self.debug.watch_refresh else {
            return;
        };
        let Some(session) = &mut self.debug.session else {
            self.debug.watch_refresh = None;
            return;
        };
        let end_marker = format!("__SE_WATCH_END_{id}__");
        let Some(end) = session
            .watch_output
            .iter()
            .position(|line| line.contains(&end_marker))
        else {
            return;
        };
        let lines = session.watch_output.drain(..=end).collect::<Vec<_>>();
        self.debug.watch_refresh = None;
        self.apply_watch_output(id, &lines);
    }

    fn apply_watch_output(&mut self, id: u64, lines: &[String]) {
        let begin = format!("__SE_WATCH_BEGIN_{id}__");
        let end = format!("__SE_WATCH_END_{id}__");
        let registers = format!("__SE_WATCH_R_{id}__");
        let mut section = WatchSection::None;
        let mut values: BTreeMap<usize, Vec<String>> = BTreeMap::new();
        let mut types: BTreeMap<usize, Vec<String>> = BTreeMap::new();
        let mut register_lines = Vec::new();
        for line in lines {
            let marker = strip_gdb_prompts(line.trim());
            if marker.is_empty() {
                continue;
            }
            if marker == begin {
                section = WatchSection::None;
                continue;
            }
            if marker == end {
                break;
            }
            if marker == registers {
                section = WatchSection::Registers;
                continue;
            }
            if let Some(index) = watch_marker_index(marker, "__SE_WATCH_V_", id) {
                section = WatchSection::Value(index);
                continue;
            }
            if let Some(index) = watch_marker_index(marker, "__SE_WATCH_T_", id) {
                section = WatchSection::Type(index);
                continue;
            }
            match section {
                WatchSection::None => {}
                WatchSection::Value(index) => {
                    values.entry(index).or_default().push(marker.to_owned());
                }
                WatchSection::Type(index) => {
                    types.entry(index).or_default().push(marker.to_owned());
                }
                WatchSection::Registers => {
                    register_lines.push(marker.to_owned());
                }
            }
        }
        for (index, watch) in self.debug.watches.iter_mut().enumerate() {
            if let Some(lines) = values.get(&index) {
                let value = clean_watch_value(lines);
                watch.value = if value.is_empty() {
                    "—".to_owned()
                } else {
                    value
                };
            } else {
                watch.value = "—".to_owned();
            }
            if let Some(lines) = types.get(&index) {
                watch.type_name = clean_watch_type(lines);
            }
        }
        self.debug.registers = register_lines
            .iter()
            .filter_map(|line| parse_register_line(line))
            .collect();
    }

    fn tick_debug_watch(&mut self) {
        if !self.debug.auto_watch
            || self.debug.starting
            || !self.settings.output_open
            || self.output_view != OutputView::Debug
            || self.debug.session.is_none()
            || self.debug.watch_refresh.is_some()
        {
            return;
        }
        let due = self
            .debug
            .last_watch_refresh
            .is_none_or(|last| last.elapsed() >= Duration::from_millis(1200));
        if due {
            self.refresh_debug_watch();
        }
    }

    fn tick_debug_peripheral(&mut self) {
        if self.debug.watch_tab != DebugSideTab::Peripherals
            || !self.debug.auto_watch
            || self.debug.starting
            || self.debug.running
            || !self.settings.output_open
            || self.output_view != OutputView::Debug
            || self.debug.session.is_none()
            || self.debug.peripheral_refresh.is_some()
        {
            return;
        }
        let due = self
            .debug
            .peripheral_last_refresh
            .is_none_or(|last| last.elapsed() >= Duration::from_millis(1500));
        if due {
            self.refresh_debug_peripheral();
        }
    }

    fn refresh_debug_peripheral(&mut self) {
        if self.debug.running {
            return;
        }
        if self.debug.peripheral_refresh.is_some() {
            let stale = self
                .debug
                .peripheral_last_refresh
                .is_none_or(|last| last.elapsed() >= Duration::from_secs(4));
            if !stale {
                return;
            }
            self.debug.peripheral_refresh = None;
            if let Some(session) = &mut self.debug.session {
                session.preg_output.clear();
            }
        }
        if self.registers.items.is_empty() {
            return;
        }
        let filter = self.debug.peripheral_filter.trim().to_lowercase();
        let filter_active = !filter.is_empty();
        let mut selected = Vec::new();
        if self.registers.peripherals.is_empty() {
            let mut groups: BTreeMap<String, Vec<u64>> = BTreeMap::new();
            for item in self.registers.items.iter().filter(|item| {
                filter.is_empty()
                    || item.name.to_lowercase().contains(&filter)
                    || item.description.to_lowercase().contains(&filter)
                    || format!("0x{:08X}", item.address)
                        .to_lowercase()
                        .contains(&filter)
            }) {
                let group = item.name.split('.').next().unwrap_or(&item.name).to_owned();
                groups.entry(group).or_default().push(item.address);
            }
            if groups.is_empty() {
                return;
            }
            if !self.debug.peripheral_initialized {
                if let Some(first) = groups.keys().next().cloned() {
                    self.debug.peripheral_open.insert(first);
                }
                self.debug.peripheral_initialized = true;
            }
            for (group, addresses) in &groups {
                if !filter_active && !self.debug.peripheral_open.contains(group) {
                    continue;
                }
                for address in addresses {
                    if selected.len() >= 160 {
                        break;
                    }
                    selected.push(*address);
                }
                if selected.len() >= 160 {
                    break;
                }
            }
        } else {
            if !self.debug.peripheral_initialized {
                if let Some(first) = self.registers.peripherals.first() {
                    self.debug.peripheral_open.insert(first.name.clone());
                }
                self.debug.peripheral_initialized = true;
            }
            for peripheral in &self.registers.peripherals {
                let matched = filter.is_empty()
                    || peripheral.name.to_lowercase().contains(&filter)
                    || peripheral.description.to_lowercase().contains(&filter)
                    || peripheral.registers.iter().any(|register| {
                        register.name.to_lowercase().contains(&filter)
                            || register.description.to_lowercase().contains(&filter)
                            || format!("0x{:08X}", peripheral.base.wrapping_add(register.offset))
                                .to_lowercase()
                                .contains(&filter)
                    });
                if !matched {
                    continue;
                }
                if !filter_active && !self.debug.peripheral_open.contains(&peripheral.name) {
                    continue;
                }
                for register in &peripheral.registers {
                    if selected.len() >= 160 {
                        break;
                    }
                    selected.push(peripheral.base.wrapping_add(register.offset));
                }
                if selected.len() >= 160 {
                    break;
                }
            }
        }
        if selected.is_empty() {
            return;
        }
        let Some(session) = &mut self.debug.session else {
            return;
        };
        self.debug.peripheral_id = self.debug.peripheral_id.wrapping_add(1).max(1);
        let id = self.debug.peripheral_id;
        session.preg_output.clear();
        session.send(&format!("echo __SE_PREG_BEGIN_{id}__\\n"));
        for address in selected {
            session.send(&format!("echo __SE_PREG_V_{id}_{address:08X}__\\n"));
            session.send(&format!("x/1xw 0x{address:08X}"));
        }
        session.send(&format!("echo __SE_PREG_END_{id}__\\n"));
        self.debug.peripheral_refresh = Some(id);
        self.debug.peripheral_last_refresh = Some(Instant::now());
    }

    fn poll_debug_peripheral(&mut self) {
        let Some(id) = self.debug.peripheral_refresh else {
            return;
        };
        let Some(session) = &mut self.debug.session else {
            self.debug.peripheral_refresh = None;
            return;
        };
        let end_marker = format!("__SE_PREG_END_{id}__");
        let Some(end) = session
            .preg_output
            .iter()
            .position(|line| line.contains(&end_marker))
        else {
            if session.preg_output.len() > 4000 {
                session.preg_output.clear();
                self.debug.peripheral_refresh = None;
            }
            return;
        };
        let lines = session.preg_output.drain(..=end).collect::<Vec<_>>();
        self.debug.peripheral_refresh = None;
        let prefix = format!("__SE_PREG_V_{id}_");
        let mut current = None;
        let mut values = BTreeMap::new();
        for line in lines {
            let marker = strip_gdb_prompts(line.trim());
            if marker.contains(&end_marker) {
                break;
            }
            if let Some(rest) = marker.strip_prefix(&prefix) {
                if let Some(hex) = rest.split("__").next() {
                    current = u64::from_str_radix(hex.trim(), 16).ok();
                }
                continue;
            }
            let Some(address) = current else {
                continue;
            };
            if marker.contains("Cannot access memory") {
                values.insert(address, "无法读取".to_owned());
                current = None;
            } else if let Some(value) = parse_memory_word(marker) {
                values.insert(address, value);
                current = None;
            }
        }
        self.debug.peripheral_values = values;
    }

    fn add_debug_watch(&mut self, expression: String) {
        let expression = expression.trim().replace(['\n', '\r'], " ");
        if expression.is_empty() {
            self.set_toast("请输入变量名或表达式", Severity::Info);
            return;
        }
        if self
            .debug
            .watches
            .iter()
            .any(|watch| watch.expr == expression)
        {
            self.set_toast("该变量已在监视列表中", Severity::Info);
            return;
        }
        self.debug.watches.push(WatchEntry::new(expression));
        self.debug.watch_input.clear();
        self.persist_debug_watches();
        self.debug.watch_refresh = None;
        if let Some(session) = &mut self.debug.session {
            session.watch_output.clear();
            session.preg_output.clear();
        }
        self.refresh_debug_watch();
    }

    fn remove_debug_watch(&mut self, index: usize) {
        if index >= self.debug.watches.len() {
            return;
        }
        self.debug.watches.remove(index);
        self.persist_debug_watches();
        self.debug.watch_refresh = None;
        if let Some(session) = &mut self.debug.session {
            session.watch_output.clear();
            session.preg_output.clear();
        }
        self.refresh_debug_watch();
    }

    fn persist_debug_watches(&mut self) {
        self.project_debug.watches = self
            .debug
            .watches
            .iter()
            .map(|watch| watch.expr.clone())
            .collect();
        self.persist_build_config();
    }

    fn start_split(
        &mut self,
        primary: PathBuf,
        secondary: PathBuf,
        direction: SplitDirection,
        focus: usize,
    ) {
        let Some(primary_index) = self
            .tabs
            .iter()
            .position(|doc| same_path(&doc.path, &primary))
        else {
            return;
        };
        let Some(secondary_index) = self
            .tabs
            .iter()
            .position(|doc| same_path(&doc.path, &secondary))
        else {
            return;
        };
        if primary_index == secondary_index {
            self.set_toast("请先打开另一个文件再拆分", Severity::Info);
            return;
        }
        self.split_primary = Some(primary);
        self.split_path = Some(secondary);
        self.split_direction = direction;
        self.split_focus = focus;
        self.active = Some(if focus == 0 {
            primary_index
        } else {
            secondary_index
        });
        self.focus_editor = true;
    }

    fn split_tab(&mut self, path: PathBuf, direction: SplitDirection) {
        let Some(active) = self.active_document() else {
            return;
        };
        let active_path = active.path.clone();
        if same_path(&active_path, &path) {
            self.set_toast("请右键另一个标签页进行拆分", Severity::Info);
            return;
        }
        match direction {
            SplitDirection::Right => {
                self.start_split(active_path, path, direction, 1);
            }
            SplitDirection::Left => {
                self.start_split(path, active_path, direction, 0);
            }
            SplitDirection::Up => {
                self.start_split(path, active_path, direction, 0);
            }
            SplitDirection::Down => {
                self.start_split(active_path, path, direction, 1);
            }
        }
    }

    fn split_active(&mut self, direction: SplitDirection) {
        let Some(active_index) = self.active else {
            return;
        };
        let Some(active_path) = self.tabs.get(active_index).map(|doc| doc.path.clone()) else {
            return;
        };
        let other = self
            .tabs
            .iter()
            .enumerate()
            .find(|(index, _)| *index != active_index)
            .map(|(_, doc)| doc.path.clone());
        let Some(other) = other else {
            self.set_toast("至少打开两个文件才能拆分", Severity::Info);
            return;
        };
        self.start_split(active_path, other, direction, 0);
    }

    fn move_split(&mut self, direction: SplitDirection) {
        let Some(active_index) = self.active else {
            return;
        };
        let Some(active_path) = self.tabs.get(active_index).map(|doc| doc.path.clone()) else {
            return;
        };
        let other = self
            .tabs
            .iter()
            .enumerate()
            .find(|(index, _)| *index != active_index)
            .map(|(_, doc)| doc.path.clone());
        let Some(other) = other else {
            self.set_toast("至少打开两个文件才能移动/拆分窗格", Severity::Info);
            return;
        };
        match direction {
            SplitDirection::Right => self.start_split(other, active_path, direction, 1),
            SplitDirection::Left => self.start_split(active_path, other, direction, 0),
            SplitDirection::Up => self.start_split(active_path, other, direction, 0),
            SplitDirection::Down => self.start_split(other, active_path, direction, 1),
        }
    }

    fn toggle_breakpoint(&mut self) {
        let Some(doc) = self.active_document() else {
            return;
        };
        let path = doc.path.clone();
        let line = doc.cursor_line;
        self.toggle_breakpoint_at(&path, line);
    }

    fn toggle_breakpoint_at(&mut self, path: &Path, line: usize) {
        if line == 0 {
            return;
        }
        let key = (path.to_path_buf(), line);
        let added = if self.debug.breakpoints.contains(&key) {
            self.debug.breakpoints.remove(&key);
            if let Some(session) = &mut self.debug.session {
                session.send(&format!("clear \"{}\":{}", gdb_source_path(path), line));
            }
            false
        } else {
            self.debug.breakpoints.insert(key);
            if let Some(session) = &mut self.debug.session {
                session.send(&format!("break \"{}\":{}", gdb_source_path(path), line));
            }
            true
        };
        self.set_toast(
            format!(
                "{}：{}:{}",
                if added {
                    "已添加断点"
                } else {
                    "已移除断点"
                },
                util::file_name(path),
                line
            ),
            Severity::Info,
        );
    }

    fn open_hex(&mut self, path: &Path) {
        match crate::hexview::HexDump::open(path) {
            Ok(dump) => {
                self.hex.dump = dump;
                self.hex.error = None;
                self.hex.open = true;
            }
            Err(err) => {
                self.hex.error = Some(err);
                self.hex.open = true;
                self.hex.dump = crate::hexview::HexDump::default();
            }
        }
    }

    fn refresh_todos(&mut self) {
        let Some(root) = self.project.as_ref().map(|project| project.root.clone()) else {
            self.set_toast("请先打开一个工程文件夹", Severity::Warning);
            return;
        };
        let ignore = IgnoreRules::load(
            &root,
            self.settings.respect_gitignore,
            &self.settings.ignore_patterns,
        );
        match crate::todo::scan_project(&root, &ignore) {
            Ok(outcome) => {
                self.todo.items = outcome.items;
                self.todo.truncated = outcome.truncated;
                self.todo.error = None;
            }
            Err(err) => {
                self.todo.items.clear();
                self.todo.error = Some(err);
            }
        }
    }

    fn refresh_symbols(&mut self) {
        let Some(root) = self.project.as_ref().map(|project| project.root.clone()) else {
            self.set_toast("请先打开一个工程文件夹", Severity::Warning);
            return;
        };
        let ignore = IgnoreRules::load(
            &root,
            self.settings.respect_gitignore,
            &self.settings.ignore_patterns,
        );
        match crate::symbols::scan_project(&root, &ignore) {
            Ok(outcome) => {
                self.symbols.items = outcome.symbols;
                self.symbols.truncated = outcome.truncated;
                self.symbols.error = None;
                self.symbols.scanned = true;
            }
            Err(err) => {
                self.symbols.items.clear();
                self.symbols.error = Some(err);
                self.symbols.scanned = true;
            }
        }
    }

    fn start_relations_scan(&mut self) {
        let Some(root) = self.project.as_ref().map(|project| project.root.clone()) else {
            self.relations = RelationState::default();
            return;
        };
        if self.relations.receiver.is_some() {
            return;
        }
        let respect_gitignore = self.settings.respect_gitignore;
        let ignore_patterns = self.settings.ignore_patterns.clone();
        let (sender, receiver) = mpsc::channel();
        self.relations.root = Some(root.clone());
        self.relations.receiver = Some(receiver);
        self.relations.error = None;
        self.relations.dirty_at = None;
        let spawned = std::thread::Builder::new()
            .name("simple-embedded-relations".to_owned())
            .spawn(move || {
                let ignore = IgnoreRules::load(&root, respect_gitignore, &ignore_patterns);
                let result = crate::relations::scan_project(&root, &ignore);
                let _ = sender.send(result);
            });
        if let Err(err) = spawned {
            self.relations.receiver = None;
            self.relations.error = Some(format!("无法启动调用/引用索引：{err}"));
        }
    }

    fn poll_relations(&mut self, ctx: &egui::Context) {
        if self.project.is_none() {
            return;
        }
        let received = self
            .relations
            .receiver
            .as_ref()
            .map(|receiver| receiver.try_recv());
        match received {
            Some(Ok(Ok(index))) => {
                let same_root = self.project.as_ref().is_some_and(|project| {
                    self.relations
                        .root
                        .as_ref()
                        .is_some_and(|root| same_path(root, &project.root))
                });
                if same_root {
                    self.relations.index = Some(Arc::new(index));
                    self.relations.error = None;
                }
                self.relations.receiver = None;
                ctx.request_repaint();
            }
            Some(Ok(Err(err))) => {
                self.relations.error = Some(err);
                self.relations.receiver = None;
                ctx.request_repaint();
            }
            Some(Err(std::sync::mpsc::TryRecvError::Disconnected)) => {
                self.relations.error = Some("调用/引用索引线程已结束".to_owned());
                self.relations.receiver = None;
                ctx.request_repaint();
            }
            Some(Err(std::sync::mpsc::TryRecvError::Empty)) => {
                ctx.request_repaint_after(Duration::from_millis(200));
            }
            None => {
                if self
                    .relations
                    .dirty_at
                    .is_some_and(|at| at.elapsed() >= Duration::from_millis(1200))
                {
                    self.start_relations_scan();
                }
            }
        }
    }

    fn mark_relations_dirty(&mut self) {
        if self.project.is_some() {
            self.relations.dirty_at = Some(Instant::now());
        }
    }

    fn hover_relations(&self, word: &str) -> Option<crate::relations::HoverRelations> {
        self.relations.index.as_ref()?.hover(word)
    }

    fn start_install_system(&mut self) {
        if self.install.receiver.is_some() {
            self.set_toast("正在安装到系统，请稍候", Severity::Info);
            return;
        }
        let (sender, receiver) = mpsc::channel();
        self.install.receiver = Some(receiver);
        let spawned = std::thread::Builder::new()
            .name("simple-embedded-install".to_owned())
            .spawn(move || {
                let _ = sender.send(crate::install::install_system());
            });
        if let Err(err) = spawned {
            self.install.receiver = None;
            self.set_toast(format!("无法启动安装任务：{err}"), Severity::Error);
        } else {
            self.set_toast("正在安装到系统，请稍候…", Severity::Info);
        }
    }

    fn poll_install_system(&mut self, ctx: &egui::Context) {
        let received = self
            .install
            .receiver
            .as_ref()
            .map(|receiver| receiver.try_recv());
        match received {
            Some(Ok(Ok(message))) => {
                self.install.receiver = None;
                self.set_toast(message, Severity::Success);
                ctx.request_repaint();
            }
            Some(Ok(Err(err))) => {
                self.install.receiver = None;
                self.set_toast(err, Severity::Error);
                ctx.request_repaint();
            }
            Some(Err(std::sync::mpsc::TryRecvError::Disconnected)) => {
                self.install.receiver = None;
                self.set_toast("安装任务已中断", Severity::Error);
                ctx.request_repaint();
            }
            Some(Err(std::sync::mpsc::TryRecvError::Empty)) => {
                ctx.request_repaint_after(Duration::from_millis(200));
            }
            None => {}
        }
    }

    fn start_resource_report(&mut self) {
        let Some((root, project_name)) = self
            .project
            .as_ref()
            .map(|project| (project.root.clone(), project.name.clone()))
        else {
            self.set_toast("请先打开工程", Severity::Warning);
            return;
        };
        if self.report.receiver.is_some() {
            self.set_toast("正在生成资源报告，请稍候", Severity::Info);
            return;
        }
        let memory_usage = self.build_memory_usage();
        let firmware_path = self.debug_target();
        let firmware_analysis = firmware_path.as_ref().and_then(|path| {
            self.firmware_view
                .path
                .as_ref()
                .filter(|view_path| same_path(view_path, path))
                .and_then(|_| self.firmware_view.analysis.clone())
        });
        let request = crate::project_report::ReportRequest {
            root,
            project_name,
            meta: self.project_meta.clone(),
            build: self.build_config.clone(),
            debug: self.project_debug.clone(),
            memory_layout: self.memory_layout.clone(),
            memory_usage,
            support_packs: self.support_packs.clone(),
            registers: if self.registers.peripherals.is_empty() {
                None
            } else {
                Some(self.registers.peripherals.clone())
            },
            register_source: if self.registers.source.is_empty() {
                None
            } else {
                Some(self.registers.source.clone())
            },
            registers_truncated: self.registers.truncated,
            symbols: if self.symbols.scanned {
                Some(self.symbols.items.clone())
            } else {
                None
            },
            symbols_truncated: self.symbols.truncated,
            relations: self.relations.index.clone(),
            firmware_path,
            firmware_analysis,
            include_dirs: self.effective_includes().to_vec(),
            library_dirs: self.effective_library_dirs().to_vec(),
            defines: self.effective_defines().to_vec(),
            respect_gitignore: self.settings.respect_gitignore,
            ignore_patterns: self.settings.ignore_patterns.clone(),
        };
        let (sender, receiver) = mpsc::channel();
        self.report.receiver = Some(receiver);
        let spawned = std::thread::Builder::new()
            .name("simple-embedded-resource-report".to_owned())
            .spawn(move || {
                let _ = sender.send(crate::project_report::generate_and_write(request));
            });
        if let Err(err) = spawned {
            self.report.receiver = None;
            self.set_toast(format!("无法启动资源分析：{err}"), Severity::Error);
        } else {
            self.set_toast(
                "正在分析工程资源，完成后会保存到工程根目录…",
                Severity::Info,
            );
        }
    }

    pub(crate) fn start_in_ai_mode(&mut self) {
        self.enter_ai_mode();
    }

    fn enter_ai_mode(&mut self) {
        if self.project.is_none() {
            self.set_toast("请先打开工程，再进入 AI 模式", Severity::Warning);
            return;
        }
        if self.ai.enabled {
            return;
        }
        self.ai.enabled = true;
        self.ai.context_file = self.active_document().map(|doc| doc.path.clone());
        self.ai.focus_input = true;
        self.refresh_ai_skill();
        self.refresh_ai_framework(true);
        self.ai.transition = Some(AiTransition {
            started: Instant::now(),
            from: self.last_editor_rect.unwrap_or(egui::Rect::NOTHING),
        });
    }

    fn exit_ai_mode(&mut self) {
        self.ai.enabled = false;
        self.ai.transition = None;
        self.ai.sending = false;
        self.ai.receiver = None;
        self.ai_settings_draft = None;
        self.focus_editor = true;
    }

    fn refresh_ai_skill(&mut self) {
        self.ai.skill_error = None;
        self.ai.skill_builtin = false;
        let Some(root) = self.project.as_ref().map(|project| project.root.clone()) else {
            self.ai.skill_source = None;
            self.ai.skill_files = 0;
            self.ai.skill_error = Some("尚未打开工程".to_owned());
            return;
        };
        match crate::ai::find_skill(&root) {
            Some(path) => match crate::ai::load_skill(&path) {
                Ok(bundle) => {
                    self.ai.skill_source = Some(util::display_path(&bundle.root));
                    self.ai.skill_files = bundle.files.len();
                }
                Err(err) => {
                    self.ai.skill_source = Some(util::display_path(&path));
                    self.ai.skill_files = 0;
                    self.ai.skill_error = Some(format!("读取 skill 失败：{err}"));
                }
            },
            None => {
                let bundle = crate::ai::embedded_skill();
                self.ai.skill_source = Some("内置副本（随程序编译）".to_owned());
                self.ai.skill_files = bundle.files.len();
                self.ai.skill_builtin = true;
            }
        }
    }

    fn refresh_ai_resources(&mut self) {
        self.ai.extra_skills = crate::ai::scan_resources(crate::ai::AiResourceKind::Skill);
        self.ai.extra_references = crate::ai::scan_resources(crate::ai::AiResourceKind::Reference);
    }

    fn start_ai_import(&mut self, kind: crate::ai::AiResourceKind, sources: Vec<PathBuf>) {
        if sources.is_empty() {
            return;
        }
        if self.ai.import_receiver.is_some() {
            self.ai.import_status = Some("上一项导入仍在进行，请稍候…".to_owned());
            return;
        }
        let (sender, receiver) = mpsc::channel();
        self.ai.import_receiver = Some(receiver);
        self.ai.import_kind = Some(kind);
        self.ai.import_status = Some(format!("正在导入 {}（PDF 会自动提取文本）…", kind.label()));
        self.ai.import_error = None;
        let spawned = std::thread::Builder::new()
            .name("simple-embedded-ai-import".to_owned())
            .spawn(move || {
                let result = crate::ai::import_resources(kind, &sources);
                let _ = sender.send(result);
            });
        if let Err(err) = spawned {
            self.ai.import_receiver = None;
            self.ai.import_kind = None;
            self.ai.import_status = None;
            self.ai.import_error = Some(format!("无法启动导入线程：{err}"));
        }
    }

    fn poll_ai_import(&mut self, ctx: &egui::Context) {
        let received = self
            .ai
            .import_receiver
            .as_ref()
            .map(|receiver| receiver.try_recv());
        match received {
            Some(Ok(Ok(outcome))) => {
                let kind = self.ai.import_kind.take();
                self.ai.import_receiver = None;
                self.refresh_ai_resources();
                let label = kind.map_or("资源", |kind| kind.label());
                let mut status = format!(
                    "已导入 {label}：{} 项{}",
                    outcome.imported,
                    if outcome.extracted_pdfs > 0 {
                        format!("，提取 PDF {} 篇", outcome.extracted_pdfs)
                    } else {
                        String::new()
                    }
                );
                if !outcome.warnings.is_empty() {
                    status.push_str(&format!(
                        "；{} 条提示：{}",
                        outcome.warnings.len(),
                        outcome
                            .warnings
                            .iter()
                            .take(3)
                            .cloned()
                            .collect::<Vec<_>>()
                            .join("；")
                    ));
                    self.set_toast(
                        format!("导入完成，但有 {} 条提示", outcome.warnings.len()),
                        Severity::Warning,
                    );
                } else {
                    self.set_toast("AI 资源导入完成", Severity::Success);
                }
                self.ai.import_status = Some(status);
                self.ai.import_error = None;
                ctx.request_repaint();
            }
            Some(Ok(Err(err))) => {
                self.ai.import_receiver = None;
                self.ai.import_kind = None;
                self.ai.import_status = None;
                self.ai.import_error = Some(err);
                ctx.request_repaint();
            }
            Some(Err(std::sync::mpsc::TryRecvError::Disconnected)) => {
                self.ai.import_receiver = None;
                self.ai.import_kind = None;
                self.ai.import_status = None;
                self.ai.import_error = Some("AI 资源导入线程已结束".to_owned());
                ctx.request_repaint();
            }
            Some(Err(std::sync::mpsc::TryRecvError::Empty)) => {
                ctx.request_repaint_after(Duration::from_millis(150));
            }
            None => {}
        }
    }

    fn import_ai_skill_folder(&mut self) {
        if let Some(path) = rfd::FileDialog::new()
            .set_title("导入 Skill 文件夹")
            .pick_folder()
        {
            self.start_ai_import(crate::ai::AiResourceKind::Skill, vec![path]);
        }
    }

    fn import_ai_skill_files(&mut self) {
        if let Some(paths) = rfd::FileDialog::new()
            .set_title("导入 Skill 文件")
            .add_filter("Skill", &["md", "markdown", "txt"])
            .pick_files()
        {
            self.start_ai_import(crate::ai::AiResourceKind::Skill, paths);
        }
    }

    fn import_ai_reference_files(&mut self) {
        if let Some(paths) = rfd::FileDialog::new()
            .set_title("导入参考资料")
            .add_filter(
                "参考资料",
                &[
                    "pdf", "md", "markdown", "txt", "rst", "c", "h", "cpp", "hpp", "json", "csv",
                    "yaml", "yml", "toml", "log",
                ],
            )
            .pick_files()
        {
            self.start_ai_import(crate::ai::AiResourceKind::Reference, paths);
        }
    }

    fn import_ai_reference_folder(&mut self) {
        if let Some(path) = rfd::FileDialog::new()
            .set_title("导入参考资料文件夹")
            .pick_folder()
        {
            self.start_ai_import(crate::ai::AiResourceKind::Reference, vec![path]);
        }
    }

    fn open_ai_resource_dir(&self, kind: crate::ai::AiResourceKind) {
        let directory = kind.directory();
        if let Err(err) = std::fs::create_dir_all(&directory) {
            eprintln!("Could not create {}: {err}", directory.display());
            return;
        }
        util::open_in_file_manager(&directory);
    }

    fn record_ai_orb_click(&mut self) {
        let now = Instant::now();
        if self
            .ai
            .orb_clicks
            .first()
            .is_some_and(|first| now.duration_since(*first) > Duration::from_secs(3))
        {
            self.ai.orb_clicks.clear();
        }
        self.ai.orb_clicks.push(now);
        if self.ai.orb_clicks.len() >= 5 {
            self.ai.orb_clicks.clear();
            self.export_ai_skill();
        }
    }

    fn export_ai_skill(&mut self) {
        let Some(root) = self.project.as_ref().map(|project| project.root.clone()) else {
            self.set_toast("请先打开工程再导出 Skill", Severity::Warning);
            return;
        };
        let Some(parent) = rfd::FileDialog::new()
            .set_title("导出进化后的 Skill")
            .pick_folder()
        else {
            return;
        };
        match crate::ai::export_skill_bundle(&root, &parent) {
            Ok(path) => {
                self.set_toast(
                    format!("Skill 已导出到 {}", util::display_path(&path)),
                    Severity::Success,
                );
                util::open_in_file_manager(&path);
            }
            Err(err) => self.set_toast(format!("导出 Skill 失败：{err}"), Severity::Error),
        }
    }

    fn refresh_ai_framework(&mut self, force: bool) {
        let Some(root) = self.project.as_ref().map(|project| project.root.clone()) else {
            self.ai.framework = None;
            self.ai.framework_path = None;
            self.ai.framework_error = None;
            self.ai.framework_mtime = None;
            self.ai.framework_missing = false;
            self.ai.framework_checked_at = None;
            return;
        };
        let checked_at = Instant::now();
        self.ai.framework_checked_at = Some(checked_at);
        let existing = if force {
            None
        } else {
            self.ai
                .framework_path
                .as_ref()
                .filter(|path| path.is_file())
                .cloned()
        };
        let Some(path) = existing.or_else(|| crate::framework::find(&root)) else {
            self.ai.framework = None;
            self.ai.framework_path = None;
            self.ai.framework_error = None;
            self.ai.framework_mtime = None;
            self.ai.framework_missing = true;
            return;
        };
        let mtime = std::fs::metadata(&path)
            .and_then(|metadata| metadata.modified())
            .ok();
        let same = self.ai.framework_path.as_ref() == Some(&path)
            && self.ai.framework_mtime == mtime
            && (self.ai.framework.is_some() || self.ai.framework_error.is_some());
        if !force && same {
            self.ai.framework_missing = false;
            return;
        }
        match crate::framework::load(&path) {
            Ok(doc) => {
                self.ai.framework = Some(doc);
                self.ai.framework_path = Some(path);
                self.ai.framework_error = None;
                self.ai.framework_mtime = mtime;
                self.ai.framework_missing = false;
            }
            Err(err) => {
                self.ai.framework = None;
                self.ai.framework_path = Some(path);
                self.ai.framework_error = Some(err);
                self.ai.framework_mtime = mtime;
                self.ai.framework_missing = false;
            }
        }
    }

    fn poll_ai_framework(&mut self, ctx: &egui::Context) {
        let due = self
            .ai
            .framework_checked_at
            .is_none_or(|checked| checked.elapsed() >= Duration::from_millis(800));
        if due {
            self.refresh_ai_framework(false);
        }
        ctx.request_repaint_after(Duration::from_millis(800));
    }

    fn request_ai_framework(&mut self) {
        self.ai.input = "请使用 stm32-embedded-dev skill 的 S3 架构确认和 framework-template.md，分析当前工程并生成/补全工程根目录的 framework.md。必须包含三章：1 分层架构；2 软件架构（2.1 执行范式、2.2 调度模式、2.3 设计模式）；3 运行视图（3.1 启动调用链、3.2 主循环调度时间线 Mermaid gantt、3.3 中断来源与抢占关系表、3.4 关键事件上下文链）。如果 framework.md 已存在但不完整，只补全缺失章节，保留已有正确内容，不要整份重写；不要改其他代码。输出较长时优先使用 se-replace 按小节补全。".to_owned();
        self.ai.focus_input = true;
        self.send_ai_message();
    }

    fn open_ai_framework(&mut self) {
        let Some(path) = self.ai.framework_path.clone() else {
            return;
        };
        self.exit_ai_mode();
        self.open_file(path);
        self.focus_editor = true;
    }

    fn open_ai_settings(&mut self) {
        if self.has_modal() && self.ai_settings_draft.is_none() {
            return;
        }
        self.refresh_ai_skill();
        self.refresh_ai_resources();
        self.ai_settings_draft = Some(AiSettingsDraft {
            api_key: self.settings.ai_api_key.clone(),
            base_url: self.settings.ai_base_url.clone(),
            model: self.settings.ai_model.clone(),
            show_key: false,
            error: None,
        });
    }

    fn clear_ai_chat(&mut self) {
        self.ai.messages.clear();
        self.ai.input.clear();
        self.ai.sending = false;
        self.ai.receiver = None;
    }

    fn send_ai_message(&mut self) {
        if self.ai.sending {
            return;
        }
        let text = self.ai.input.trim().to_owned();
        if text.is_empty() {
            return;
        }
        if !self.ai_config().is_ready() {
            self.set_toast("请先在 AI 设置中填写 API Key 和模型名称", Severity::Info);
            self.open_ai_settings();
            return;
        }
        let Some(root) = self.project.as_ref().map(|project| project.root.clone()) else {
            self.set_toast("请先打开工程", Severity::Warning);
            return;
        };
        // The prompt and the file-edit base hashes are built from disk. Save
        // dirty tabs first so the model sees the same content the user does.
        if let Err(err) = self.save_all_dirty() {
            self.set_toast(format!("发送前保存失败：{err}"), Severity::Error);
            return;
        }
        // Prefer the skill checked out in the project (or a global skills
        // folder), then fall back to the copy compiled into the executable.
        let skill = crate::ai::find_skill(&root)
            .and_then(|path| crate::ai::load_skill(&path).ok())
            .unwrap_or_else(crate::ai::embedded_skill);
        let project_name = self
            .project
            .as_ref()
            .map(|project| project.name.clone())
            .unwrap_or_default();
        let extra_skills = crate::ai::load_extra_skills();
        let references = crate::ai::load_references();
        let evolution = crate::ai::load_user_evolution();
        let system_prompt = crate::ai::build_system_prompt(&crate::ai::Context {
            root: &root,
            project_name: &project_name,
            meta: &self.project_meta,
            build: &self.build_config,
            debug: &self.project_debug,
            memory_layout: self.memory_layout.as_ref(),
            active_file: self.ai.context_file.as_deref(),
            include_dirs: self.effective_includes(),
            library_dirs: self.effective_library_dirs(),
            defines: self.effective_defines(),
            skill: &skill,
            extra_skills: &extra_skills,
            references: &references,
            evolution: &evolution,
        });
        self.ai
            .messages
            .push(AiChatMessage::text("user", text, false));
        // Keep the recent conversation, but do not let a few long code
        // answers grow every follow-up request without bound.
        let mut history = Vec::new();
        let mut history_chars = 0usize;
        for message in self
            .ai
            .messages
            .iter()
            .rev()
            .filter(|message| !message.error)
            .take(12)
        {
            let content = crate::ai::truncate_chars(&message.content, 12_000);
            let chars = content.chars().count();
            if !history.is_empty() && history_chars + chars > 48_000 {
                break;
            }
            history_chars += chars;
            history.push(crate::ai::ChatMessage {
                role: message.role.clone(),
                content,
            });
        }
        history.reverse();
        let config = self.ai_config();
        let (sender, receiver) = mpsc::channel();
        self.ai.receiver = Some(receiver);
        self.ai.sending = true;
        self.ai.input.clear();
        self.ai.focus_input = true;
        self.ai.messages.push(AiChatMessage {
            role: "assistant".to_owned(),
            content: String::new(),
            reasoning: String::new(),
            streaming: true,
            error: false,
            edits: Vec::new(),
            edit_status: AiEditStatus::Pending,
            apply_error: None,
            backup_dir: None,
            evolutions: Vec::new(),
            evolution_status: AiEditStatus::Pending,
            evolution_error: None,
        });
        let spawned = std::thread::Builder::new()
            .name("simple-embedded-ai".to_owned())
            .spawn(move || {
                crate::ai::chat_stream(&config, &system_prompt, &history, sender);
            });
        if let Err(err) = spawned {
            self.ai.sending = false;
            self.ai.receiver = None;
            if let Some(message) = self.ai.messages.iter_mut().rev().find(|m| m.streaming) {
                message.streaming = false;
                message.error = true;
                message.content = format!("无法启动 AI 请求：{err}");
            }
        }
    }

    fn ai_config(&self) -> crate::ai::AiConfig {
        crate::ai::AiConfig {
            api_key: self.settings.ai_api_key.trim().to_owned(),
            base_url: self.settings.ai_base_url.trim().to_owned(),
            model: self.settings.ai_model.trim().to_owned(),
        }
    }

    fn append_ai_reasoning(message: &mut AiChatMessage, delta: &str) {
        const MAX_REASONING_BYTES: usize = 120_000;
        if message.reasoning.len() >= MAX_REASONING_BYTES {
            return;
        }
        message.reasoning.push_str(delta);
        if message.reasoning.len() > MAX_REASONING_BYTES {
            let mut end = MAX_REASONING_BYTES;
            while end > 0 && !message.reasoning.is_char_boundary(end) {
                end -= 1;
            }
            message.reasoning.truncate(end);
            message.reasoning.push_str("\n\n[思考过程过长，已截断]");
        }
    }

    fn finish_ai_stream(&mut self, interrupted: bool, truncated: bool) {
        let Some(index) = self
            .ai
            .messages
            .iter()
            .rposition(|message| message.streaming)
        else {
            return;
        };
        let raw = self.ai.messages[index].content.clone();
        let reasoning = self.ai.messages[index].reasoning.clone();
        if interrupted || truncated || crate::ai_edit::incomplete_edit_block(&raw) {
            let reason = if truncated {
                "[AI 输出达到长度上限，已停止；未应用任何文件修改或技能规则]"
            } else if interrupted {
                "[AI 输出中断；未应用任何文件修改或技能规则]"
            } else {
                "[文件修改块不完整；为避免写入截断内容，未应用任何修改]"
            };
            let message = &mut self.ai.messages[index];
            message.streaming = false;
            message.error = true;
            message.content = if raw.trim().is_empty() {
                format!("AI 输出不完整，未收到可用内容。{reason}")
            } else {
                format!("{raw}\n\n{reason}")
            };
            return;
        }

        let parsed = crate::ai_edit::parse_edits(&raw);
        let text = parsed.text.trim().to_owned();
        let mut edits = crate::ai_edit::merge_edits(parsed.edits);
        let evolutions = parsed.evolutions;
        if let Some(root) = self.project.as_ref().map(|project| project.root.clone()) {
            for edit in &mut edits {
                edit.prepare(&root);
            }
        }
        let content = if text.is_empty() && !edits.is_empty() {
            format!(
                "已准备 {} 个文件修改，请查看下方差异并点击“应用全部”。",
                edits.len()
            )
        } else if text.is_empty() && !evolutions.is_empty() {
            format!(
                "已准备 {} 条技能进化规则，请确认后保存到用户 evolution.md。",
                evolutions.len()
            )
        } else if text.is_empty() && !reasoning.trim().is_empty() {
            "（模型只返回了思考过程，没有正文）".to_owned()
        } else {
            text
        };
        let message = &mut self.ai.messages[index];
        message.content = content;
        message.edits = edits;
        message.edit_status = AiEditStatus::Pending;
        message.apply_error = None;
        message.backup_dir = None;
        message.evolutions = evolutions;
        message.evolution_status = AiEditStatus::Pending;
        message.evolution_error = None;
        message.streaming = false;
        if message.content.trim().is_empty() {
            message.error = true;
            message.content = "模型返回了空内容".to_owned();
        }
    }

    fn poll_ai(&mut self, ctx: &egui::Context) {
        self.poll_ai_import(ctx);
        let mut events = Vec::new();
        let mut disconnected = false;
        if let Some(receiver) = &self.ai.receiver {
            for _ in 0..256 {
                match receiver.try_recv() {
                    Ok(event) => events.push(event),
                    Err(std::sync::mpsc::TryRecvError::Empty) => break,
                    Err(std::sync::mpsc::TryRecvError::Disconnected) => {
                        disconnected = true;
                        break;
                    }
                }
            }
        }
        if events.is_empty() && !disconnected {
            if self.ai.receiver.is_some() {
                ctx.request_repaint_after(Duration::from_millis(30));
            }
            return;
        }

        let mut done_truncated = None;
        let mut errored = false;
        for event in events {
            match event {
                crate::ai::AiStreamEvent::Reasoning(delta) => {
                    if let Some(message) = self
                        .ai
                        .messages
                        .iter_mut()
                        .rev()
                        .find(|message| message.streaming)
                    {
                        Self::append_ai_reasoning(message, &delta);
                    }
                }
                crate::ai::AiStreamEvent::Content(delta) => {
                    if let Some(message) = self
                        .ai
                        .messages
                        .iter_mut()
                        .rev()
                        .find(|message| message.streaming)
                    {
                        message.content.push_str(&delta);
                    }
                }
                crate::ai::AiStreamEvent::Done { truncated } => {
                    done_truncated = Some(truncated);
                    break;
                }
                crate::ai::AiStreamEvent::Error(err) => {
                    errored = true;
                    if let Some(message) = self
                        .ai
                        .messages
                        .iter_mut()
                        .rev()
                        .find(|message| message.streaming)
                    {
                        message.streaming = false;
                        message.error = true;
                        if message.content.trim().is_empty() {
                            message.content = err;
                        } else {
                            message
                                .content
                                .push_str(&format!("\n\n[AI 输出中断：{err}]"));
                        }
                    }
                    break;
                }
            }
        }
        if let Some(truncated) = done_truncated {
            self.ai.receiver = None;
            self.ai.sending = false;
            self.finish_ai_stream(false, truncated);
        } else if errored {
            self.ai.receiver = None;
            self.ai.sending = false;
        } else if disconnected {
            self.ai.receiver = None;
            self.ai.sending = false;
            self.finish_ai_stream(true, false);
        } else if self.ai.receiver.is_some() {
            ctx.request_repaint_after(Duration::from_millis(30));
        }
        ctx.request_repaint();
    }

    fn cancel_ai_edits(&mut self, index: usize) {
        if let Some(message) = self.ai.messages.get_mut(index) {
            if message.edit_status == AiEditStatus::Pending {
                message.edit_status = AiEditStatus::Cancelled;
                message.apply_error = None;
            }
        }
    }

    fn apply_ai_evolutions(&mut self, index: usize) {
        let Some(message) = self.ai.messages.get(index) else {
            return;
        };
        if message.evolution_status != AiEditStatus::Pending || message.evolutions.is_empty() {
            return;
        }
        let rules = message.evolutions.clone();
        match crate::ai::append_user_evolution(&rules) {
            Ok((path, added)) => {
                if let Some(message) = self.ai.messages.get_mut(index) {
                    message.evolution_status = AiEditStatus::Applied;
                    message.evolution_error = None;
                }
                self.set_toast(
                    if added > 0 {
                        format!(
                            "已保存 {added} 条技能进化规则：{}",
                            util::display_path(&path)
                        )
                    } else {
                        "这些规则已存在，未重复写入".to_owned()
                    },
                    Severity::Success,
                );
            }
            Err(err) => {
                if let Some(message) = self.ai.messages.get_mut(index) {
                    message.evolution_error = Some(err);
                }
            }
        }
    }

    fn cancel_ai_evolutions(&mut self, index: usize) {
        if let Some(message) = self.ai.messages.get_mut(index) {
            if message.evolution_status == AiEditStatus::Pending {
                message.evolution_status = AiEditStatus::Cancelled;
                message.evolution_error = None;
            }
        }
    }

    /// Validate and apply the file edits proposed in an assistant message.
    /// Everything is checked before the first write; modified files are copied
    /// to `%APPDATA%\SimpleEmbedded\backups\...` first.
    fn apply_ai_edits(&mut self, index: usize) {
        let Some(message) = self.ai.messages.get(index) else {
            return;
        };
        if message.edit_status != AiEditStatus::Pending || message.edits.is_empty() {
            return;
        }
        let edits = message.edits.clone();
        let Some(root) = self.project.as_ref().map(|project| project.root.clone()) else {
            if let Some(message) = self.ai.messages.get_mut(index) {
                message.apply_error = Some("请先打开工程".to_owned());
            }
            return;
        };

        struct Plan {
            edit: crate::ai_edit::AiEdit,
            absolute: PathBuf,
            loaded: Option<util::LoadedText>,
            new_text: String,
            open_index: Option<usize>,
        }

        macro_rules! fail {
            ($err:expr) => {{
                if let Some(message) = self.ai.messages.get_mut(index) {
                    message.apply_error = Some($err);
                }
                return;
            }};
        }

        let mut plans = Vec::new();
        for edit in edits {
            if let Some(err) = edit.error.clone() {
                fail!(err);
            }
            let Some(new_text) = edit.resolved.clone() else {
                fail!(format!("{} 没有可应用的内容", edit.path));
            };
            let absolute = root.join(&edit.path);
            let loaded = if absolute.is_file() {
                match util::read_text_file(&absolute) {
                    Ok(loaded) => Some(loaded),
                    Err(err) => fail!(format!("读取 {} 失败：{err}", edit.path)),
                }
            } else {
                None
            };
            let actual_hash = loaded
                .as_ref()
                .map(|loaded| crate::ai_edit::hash_text(&loaded.text));
            match (edit.base_hash, actual_hash) {
                (Some(expected), Some(actual)) if expected == actual => {}
                (None, None) => {}
                _ => fail!(format!(
                    "{} 在 AI 生成后被修改或创建，请重新生成或取消本次修改",
                    edit.path
                )),
            }
            let open_index = self
                .tabs
                .iter()
                .position(|doc| same_path(&doc.path, &absolute));
            if let Some(doc_index) = open_index {
                let doc = &self.tabs[doc_index];
                if doc.is_dirty() {
                    fail!(format!(
                        "{} 有未保存修改，请先保存（Ctrl+S）后再应用",
                        edit.path
                    ));
                }
                if doc.read_only || doc.binary.is_some() {
                    fail!(format!("{} 是只读或二进制文件，不能由 AI 修改", edit.path));
                }
            }
            if plans
                .iter()
                .any(|plan: &Plan| plan.edit.path.eq_ignore_ascii_case(&edit.path))
            {
                fail!(format!("同一文件出现了多次修改：{}", edit.path));
            }
            plans.push(Plan {
                edit,
                absolute,
                loaded,
                new_text,
                open_index,
            });
        }

        let stamp = SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|duration| duration.as_millis())
            .unwrap_or_default();
        let project_name = root
            .file_name()
            .map(|name| name.to_string_lossy().into_owned())
            .unwrap_or_else(|| "project".to_owned());
        let key = crate::ai_edit::hash_text(&project_key(&root));
        let backup_root = crate::config::settings_path()
            .parent()
            .map(|path| path.join("backups"))
            .unwrap_or_else(|| std::env::temp_dir().join("SimpleEmbedded-backups"));
        let backup_dir = backup_root
            .join(format!("{project_name}-{key:08x}"))
            .join(format!("ai-{stamp}"));
        if let Err(err) = std::fs::create_dir_all(&backup_dir) {
            fail!(format!("无法创建 AI 修改备份目录：{err}"));
        }
        for plan in &plans {
            if plan.loaded.is_none() {
                continue;
            }
            let destination = backup_dir.join(&plan.edit.path);
            if let Some(parent) = destination.parent() {
                if let Err(err) = std::fs::create_dir_all(parent) {
                    fail!(format!("无法创建备份目录：{err}"));
                }
            }
            if let Err(err) = std::fs::copy(&plan.absolute, &destination) {
                fail!(format!(
                    "备份 {} 失败：{err}",
                    util::display_path(&plan.absolute)
                ));
            }
        }

        for plan in &plans {
            if let Some(doc_index) = plan.open_index {
                let doc = &mut self.tabs[doc_index];
                doc.buffer.set_text(plan.new_text.clone());
                if let Err(err) = doc.save() {
                    fail!(format!("写入 {} 失败：{err}", plan.edit.path));
                }
                doc.macro_issue_revision = u64::MAX;
            } else {
                if let Some(parent) = plan.absolute.parent() {
                    if let Err(err) = std::fs::create_dir_all(parent) {
                        fail!(format!("无法创建目录：{err}"));
                    }
                }
                let (line_ending, bom, encoding) = plan
                    .loaded
                    .as_ref()
                    .map(|loaded| (loaded.line_ending, loaded.bom, loaded.encoding))
                    .unwrap_or((util::LineEnding::Lf, false, util::TextEncoding::Utf8));
                if let Err(err) = util::write_text_file(
                    &plan.absolute,
                    &plan.new_text,
                    line_ending,
                    bom,
                    encoding,
                ) {
                    fail!(format!("写入 {} 失败：{err}", plan.edit.path));
                }
            }
        }

        let count = plans.len();
        if let Some(message) = self.ai.messages.get_mut(index) {
            message.edit_status = AiEditStatus::Applied;
            message.apply_error = None;
            message.backup_dir = Some(backup_dir.clone());
        }
        if let Some(tree) = &mut self.tree {
            tree.refresh();
        }
        self.symbols.scanned = false;
        self.relations = RelationState::default();
        self.start_relations_scan();
        self.refresh_ai_framework(true);
        self.find.dirty = true;
        self.set_toast(
            format!(
                "AI 已应用 {count} 个文件修改，备份：{}",
                util::display_path(&backup_dir)
            ),
            Severity::Success,
        );
    }

    fn poll_resource_report(&mut self, ctx: &egui::Context) {
        let received = self
            .report
            .receiver
            .as_ref()
            .map(|receiver| receiver.try_recv());
        match received {
            Some(Ok(Ok(path))) => {
                self.report.receiver = None;
                if let Some(tree) = &mut self.tree {
                    tree.refresh();
                }
                self.set_toast(
                    format!("已生成资源报告：{}", util::display_path(&path)),
                    Severity::Success,
                );
                ctx.request_repaint();
            }
            Some(Ok(Err(err))) => {
                self.report.receiver = None;
                self.set_toast(err, Severity::Error);
                ctx.request_repaint();
            }
            Some(Err(std::sync::mpsc::TryRecvError::Disconnected)) => {
                self.report.receiver = None;
                self.set_toast("资源分析任务已中断", Severity::Error);
                ctx.request_repaint();
            }
            Some(Err(std::sync::mpsc::TryRecvError::Empty)) => {
                ctx.request_repaint_after(Duration::from_millis(200));
            }
            None => {}
        }
    }

    fn refresh_git_status(&mut self) {
        let Some(root) = self.project.as_ref().map(|project| project.root.clone()) else {
            self.git = GitStatus::default();
            self.git_line_marks.clear();
            self.git_line_key = None;
            return;
        };
        self.git = GitStatus::load(&root);
        if let Some(tree) = &mut self.tree {
            tree.set_git_status(&self.git);
        }
        self.git_line_key = None;
        self.refresh_git_line_marks();
    }

    fn refresh_git_line_marks(&mut self) {
        let Some(root) = self.project.as_ref().map(|project| project.root.clone()) else {
            self.git_line_marks.clear();
            self.git_line_key = None;
            return;
        };
        let Some(active) = self.active else {
            self.git_line_marks.clear();
            self.git_line_key = None;
            return;
        };
        let Some(doc) = self.tabs.get(active) else {
            return;
        };
        let key = (doc.path.clone(), doc.saved_revision);
        if self.git_line_key.as_ref() == Some(&key) {
            return;
        }
        self.git_line_key = Some(key);
        if self.git.is_repo {
            self.git_line_marks = git::changed_lines(&root, &doc.path);
        } else {
            self.git_line_marks.clear();
        }
    }

    fn save_active(&mut self) -> bool {
        let Some(index) = self.active else {
            return true;
        };
        let Some(doc) = self.tabs.get(index) else {
            return true;
        };
        if !doc.is_dirty() {
            return true;
        }
        if doc.read_only {
            self.set_toast("只读文件不能保存", Severity::Warning);
            return false;
        }
        let affects_relations = crate::formatting::is_c_family(&doc.path);
        let configured = self.settings.clang_format_path.clone();
        let format_on_save = self.settings.format_on_save;
        let Some(doc) = self.tabs.get_mut(index) else {
            return true;
        };
        let mut format_error = None;
        if format_on_save && formatting::is_c_family(&doc.path) {
            if let Err(err) = Self::format_document(doc, &configured) {
                format_error = Some(err);
            }
        }
        let name = doc.name.clone();
        let saved = match doc.save() {
            Ok(()) => {
                if let Some(err) = format_error {
                    self.set_toast(
                        format!("已保存 {name}，但格式化失败：{err}"),
                        Severity::Warning,
                    );
                } else {
                    self.set_toast(format!("已保存 {name}"), Severity::Success);
                }
                true
            }
            Err(err) => {
                self.set_toast(format!("保存 {name} 失败：{err}"), Severity::Error);
                false
            }
        };
        if saved {
            self.member_index = None;
            self.git_line_key = None;
            self.refresh_git_status();
            if affects_relations {
                self.mark_relations_dirty();
            }
        }
        saved
    }

    fn save_tab_as(&mut self, index: usize, ctx: &egui::Context) {
        let Some(doc) = self.tabs.get(index) else {
            return;
        };
        if doc.read_only {
            self.set_toast("只读文件不能另存为", Severity::Warning);
            return;
        }
        let old_path = doc.path.clone();
        let name = doc.name.clone();
        let text = doc.buffer.as_string().to_owned();
        let line_ending = doc.line_ending;
        let bom = doc.bom;
        let encoding = doc.encoding;
        let mut dialog = rfd::FileDialog::new()
            .set_title("另存为")
            .set_file_name(&name);
        if let Some(dir) = old_path.parent() {
            dialog = dialog.set_directory(dir);
        }
        let Some(path) = dialog.save_file() else {
            return;
        };
        if same_path(&path, &old_path) {
            self.active = Some(index);
            let _ = self.save_active();
            return;
        }
        if let Err(err) = util::write_text_file(&path, &text, line_ending, bom, encoding) {
            self.set_toast(format!("另存为失败：{err}"), Severity::Error);
            return;
        }
        if crate::formatting::is_c_family(&path) {
            self.mark_relations_dirty();
        }
        let highlighter = Arc::clone(&self.highlighter);
        let project_root = self.project.as_ref().map(|project| project.root.clone());
        if let Some(doc) = self.tabs.get_mut(index) {
            doc.path = path.clone();
            doc.name = util::file_name(&path);
            doc.rel_path = project_root
                .as_ref()
                .map(|root| util::relative_display(root, &path))
                .unwrap_or_else(|| util::display_path(&path));
            let (syntax, language) = highlighter.detect(&path);
            doc.syntax_name = syntax;
            doc.language = language;
            doc.saved_revision = doc.buffer.revision();
            doc.layout_cache = highlight::new_cache();
            doc.macro_issue_revision = u64::MAX;
        }
        if self
            .split_primary
            .as_ref()
            .is_some_and(|primary| same_path(primary, &old_path))
        {
            self.split_primary = Some(path.clone());
        }
        if self
            .split_path
            .as_ref()
            .is_some_and(|secondary| same_path(secondary, &old_path))
        {
            self.split_path = Some(path.clone());
        }
        if let Some(tree) = &mut self.tree {
            tree.reveal(&path);
        }
        self.member_index = None;
        self.git_line_key = None;
        self.refresh_git_status();
        self.find.dirty = true;
        self.set_toast(
            format!("已另存为 {}", util::display_path(&path)),
            Severity::Success,
        );
        ctx.request_repaint();
    }

    fn save_all_dirty(&mut self) -> Result<usize, String> {
        let configured = self.settings.clang_format_path.clone();
        let format_on_save = self.settings.format_on_save;
        let mut saved = 0usize;
        let mut affects_relations = false;
        for doc in self
            .tabs
            .iter_mut()
            .filter(|doc| doc.is_dirty() && !doc.read_only)
        {
            if formatting::is_c_family(&doc.path) {
                affects_relations = true;
            }
            if format_on_save && formatting::is_c_family(&doc.path) {
                let _ = Self::format_document(doc, &configured);
            }
            doc.save()
                .map_err(|err| format!("保存 {} 失败：{err}", doc.name))?;
            saved += 1;
        }
        if saved > 0 {
            self.member_index = None;
            self.refresh_git_status();
            if affects_relations {
                self.mark_relations_dirty();
            }
        }
        Ok(saved)
    }

    fn tick_autosave(&mut self, ctx: &egui::Context) {
        let mut hasher = std::collections::hash_map::DefaultHasher::new();
        let mut dirty_count = 0usize;
        for doc in self.tabs.iter().filter(|doc| doc.is_dirty()) {
            doc.path.hash(&mut hasher);
            doc.buffer.revision().hash(&mut hasher);
            dirty_count += 1;
        }
        let signature = if dirty_count == 0 { 0 } else { hasher.finish() };
        if signature != self.dirty_signature {
            self.dirty_signature = signature;
            self.autosave_since = (signature != 0).then(Instant::now);
        }
        if !self.settings.autosave {
            self.autosave_since = None;
            return;
        }
        let delay = Duration::from_millis(self.settings.autosave_delay_ms);
        let Some(since) = self.autosave_since else {
            return;
        };
        let elapsed = since.elapsed();
        if elapsed < delay {
            ctx.request_repaint_after(delay - elapsed);
            return;
        }
        self.autosave_since = None;
        match self.save_all_dirty() {
            Ok(0) => {}
            Ok(count) => self.set_toast(format!("已自动保存 {count} 个文件"), Severity::Success),
            Err(err) => self.set_toast(format!("自动保存失败：{err}"), Severity::Error),
        }
    }

    fn save_all_with_toast(&mut self) {
        match self.save_all_dirty() {
            Ok(0) => {}
            Ok(count) => self.set_toast(format!("已保存 {count} 个文件"), Severity::Success),
            Err(err) => self.set_toast(err, Severity::Error),
        }
    }

    fn toggle_bookmark(&mut self) {
        let Some(index) = self.active else {
            return;
        };
        let Some(doc) = self.tabs.get_mut(index) else {
            return;
        };
        let line = doc.cursor_line;
        let added = doc.bookmarks.insert(line);
        if !added {
            doc.bookmarks.remove(&line);
        }
        let message = if added {
            format!("已添加书签：第 {line} 行")
        } else {
            format!("已移除书签：第 {line} 行")
        };
        self.set_toast(message, Severity::Info);
    }

    fn goto_bookmark(&mut self, ctx: &egui::Context, forward: bool) {
        let Some(index) = self.active else {
            return;
        };
        let Some(doc) = self.tabs.get_mut(index) else {
            return;
        };
        if doc.bookmarks.is_empty() {
            self.set_toast("当前文件没有书签", Severity::Warning);
            return;
        }
        let current = doc.cursor_line;
        let target = if forward {
            doc.bookmarks
                .iter()
                .copied()
                .find(|line| *line > current)
                .or_else(|| doc.bookmarks.iter().next().copied())
        } else {
            doc.bookmarks
                .iter()
                .rev()
                .copied()
                .find(|line| *line < current)
                .or_else(|| doc.bookmarks.iter().next_back().copied())
        };
        let Some(target) = target else {
            return;
        };
        let index = crate::util::char_index_at_line(doc.buffer.as_string(), target);
        doc.update_cursor(index);
        let editor_id = egui::Id::new(("simple_embedded_code_editor", &doc.path));
        let mut state = egui::text_edit::TextEditState::load(ctx, editor_id).unwrap_or_default();
        state
            .cursor
            .set_char_range(Some(egui::text::CCursorRange::one(
                egui::text::CCursor::new(index),
            )));
        state.store(ctx, editor_id);
        self.focus_editor = true;
        self.set_toast(format!("跳转到书签：第 {target} 行"), Severity::Info);
    }

    fn jump_to_matching_bracket(&mut self, ctx: &egui::Context) {
        let Some(index) = self.active else {
            return;
        };
        let Some(doc) = self.tabs.get_mut(index) else {
            return;
        };
        let Some((_, target)) =
            crate::brackets::matching_bracket(doc.buffer.as_string(), doc.cursor_index)
        else {
            self.set_toast("光标不在可匹配的括号上", Severity::Info);
            return;
        };
        doc.update_cursor(target);
        let editor_id = egui::Id::new(("simple_embedded_code_editor", &doc.path));
        let mut state = egui::text_edit::TextEditState::load(ctx, editor_id).unwrap_or_default();
        state
            .cursor
            .set_char_range(Some(egui::text::CCursorRange::one(
                egui::text::CCursor::new(target),
            )));
        state.store(ctx, editor_id);
        self.focus_editor = true;
    }

    fn run_project_search(&mut self) {
        let Some(root) = self.project.as_ref().map(|project| project.root.clone()) else {
            self.set_toast("请先打开一个工程文件夹", Severity::Warning);
            return;
        };
        let ignore = IgnoreRules::load(
            &root,
            self.settings.respect_gitignore,
            &self.settings.ignore_patterns,
        );
        let options = SearchOptions {
            query: self.project_search.query.clone(),
            case_sensitive: self.project_search.case_sensitive,
            use_regex: self.project_search.use_regex,
        };
        self.project_search.searched = true;
        match search_project(&root, &ignore, &options) {
            Ok(outcome) => {
                self.project_search.results = outcome.hits;
                self.project_search.scanned_files = outcome.scanned_files;
                self.project_search.truncated = outcome.truncated;
                self.project_search.error = None;
            }
            Err(err) => {
                self.project_search.results.clear();
                self.project_search.error = Some(err);
            }
        }
    }

    fn capture_undo(&mut self) {
        let limit = self.settings.undo_limit.clamp(1, 64);
        let mut open_paths = std::collections::BTreeSet::new();
        for doc in &self.tabs {
            open_paths.insert(doc.path.clone());
            let revision = doc.buffer.revision();
            let text = doc.buffer.as_string();
            let cursor = doc.cursor_index;
            if let Some(snapshot) = self.undo.snapshots.get_mut(&doc.path) {
                if snapshot.revision == revision {
                    snapshot.cursor = cursor;
                    continue;
                }
                if !self.undo.applying && snapshot.text != text {
                    self.undo.undo.push(UndoEntry {
                        path: doc.path.clone(),
                        before_text: snapshot.text.clone(),
                        after_text: text.to_owned(),
                        before_cursor: snapshot.cursor,
                        after_cursor: cursor,
                    });
                    if self.undo.undo.len() > limit {
                        let excess = self.undo.undo.len() - limit;
                        self.undo.undo.drain(0..excess);
                    }
                    self.undo.redo.clear();
                }
                snapshot.revision = revision;
                snapshot.text = text.to_owned();
                snapshot.cursor = cursor;
            } else {
                self.undo.snapshots.insert(
                    doc.path.clone(),
                    UndoSnapshot {
                        revision,
                        text: text.to_owned(),
                        cursor,
                    },
                );
            }
        }
        self.undo
            .snapshots
            .retain(|path, _| open_paths.contains(path));
        self.undo.applying = false;
    }

    fn fold_current(&mut self, ctx: &egui::Context) {
        let Some(active) = self.active else {
            return;
        };
        let Some(doc) = self.tabs.get_mut(active) else {
            return;
        };
        let ranges = crate::fold::ranges(doc.buffer.as_string());
        let line = doc.cursor_line.saturating_sub(1);
        let Some(start) = crate::fold::fold_start_for_line(line, &ranges) else {
            self.set_toast("当前行没有可折叠的代码块", Severity::Info);
            return;
        };
        if !doc.folds.remove(&start) {
            doc.folds.insert(start);
        }
        self.pending_scroll = Some(self.last_editor_scroll);
        self.find.dirty = true;
        self.focus_editor = true;
        ctx.request_repaint();
    }

    fn unfold_active(&mut self, ctx: &egui::Context) {
        let Some(active) = self.active else {
            return;
        };
        let Some(doc) = self.tabs.get_mut(active) else {
            return;
        };
        if !doc.folds.is_empty() {
            doc.folds.clear();
            self.pending_scroll = Some(self.last_editor_scroll);
            self.find.dirty = true;
            self.focus_editor = true;
            ctx.request_repaint();
        }
    }

    fn ensure_undo_document(&mut self, path: &Path) -> Option<usize> {
        if let Some(index) = self.tabs.iter().position(|doc| same_path(&doc.path, path)) {
            return Some(index);
        }
        self.open_file(path.to_path_buf());
        self.tabs.iter().position(|doc| same_path(&doc.path, path))
    }

    fn apply_undo(&mut self, ctx: &egui::Context, entry: UndoEntry, undo: bool) {
        let Some(index) = self.ensure_undo_document(&entry.path) else {
            self.set_toast(
                format!("无法打开 {}", util::display_path(&entry.path)),
                Severity::Warning,
            );
            return;
        };
        self.undo.applying = true;
        self.active = Some(index);
        self.focus_editor = true;
        self.scroll_to_cursor = true;
        self.column_selection = None;
        self.multi_cursor = None;
        {
            let doc = &mut self.tabs[index];
            let (text, cursor) = if undo {
                (entry.before_text.clone(), entry.before_cursor)
            } else {
                (entry.after_text.clone(), entry.after_cursor)
            };
            doc.buffer.set_text(text);
            doc.update_cursor(cursor);
            doc.macro_issue_revision = u64::MAX;
            doc.folds.clear();
            set_editor_cursor(ctx, &doc.path, cursor);
        }
        self.find.dirty = true;
        let limit = self.settings.undo_limit.clamp(1, 64);
        if undo {
            self.undo.redo.push(entry);
            if self.undo.redo.len() > limit {
                let excess = self.undo.redo.len() - limit;
                self.undo.redo.drain(0..excess);
            }
        } else {
            self.undo.undo.push(entry);
            if self.undo.undo.len() > limit {
                let excess = self.undo.undo.len() - limit;
                self.undo.undo.drain(0..excess);
            }
        }
        ctx.request_repaint();
    }

    fn undo_last(&mut self, ctx: &egui::Context) {
        let Some(entry) = self.undo.undo.pop() else {
            self.set_toast("没有可撤销的操作", Severity::Info);
            return;
        };
        self.apply_undo(ctx, entry, true);
    }

    fn redo_last(&mut self, ctx: &egui::Context) {
        let Some(entry) = self.undo.redo.pop() else {
            self.set_toast("没有可重做的操作", Severity::Info);
            return;
        };
        self.apply_undo(ctx, entry, false);
    }

    fn find_next(&mut self, ctx: &egui::Context, forward: bool) {
        let Some(active) = self.active else {
            return;
        };
        let Some(doc) = self.tabs.get_mut(active) else {
            return;
        };
        let revision = doc.buffer.revision();
        self.find.ensure_matches(doc.buffer.as_string(), revision);
        if self.find.error.is_some() {
            self.set_toast("查找正则表达式有误", Severity::Warning);
            return;
        }
        if self.find.matches.is_empty() {
            self.set_toast("未找到匹配项", Severity::Info);
            return;
        }
        let cursor = doc.cursor_index;
        let mut from = cursor;
        if forward
            && self
                .find
                .current()
                .is_some_and(|matched| matched.start_char == cursor)
        {
            from = cursor.saturating_add(1);
        }
        if self.find.select_next(from, forward) {
            let target = self
                .find
                .current()
                .map(|matched| matched.start_char)
                .unwrap_or(0);
            doc.folds.clear();
            doc.update_cursor(target);
            set_editor_cursor(ctx, &doc.path, target);
            self.scroll_to_cursor = true;
            self.multi_cursor = None;
            self.column_selection = None;
            ctx.request_repaint();
        }
    }

    fn replace_current_match(&mut self, ctx: &egui::Context) {
        let Some(active) = self.active else {
            return;
        };
        let Some(doc) = self.tabs.get_mut(active) else {
            return;
        };
        if doc.read_only {
            self.set_toast("只读文件不能替换", Severity::Warning);
            return;
        }
        let revision = doc.buffer.revision();
        self.find.ensure_matches(doc.buffer.as_string(), revision);
        let Some(matched) = self.find.current().cloned() else {
            self.set_toast("没有可替换的匹配项", Severity::Info);
            return;
        };
        let text = doc.buffer.as_string().to_owned();
        let mut replacement = if self.find.use_regex {
            self.find
                .compiled
                .as_ref()
                .map(|regex| {
                    regex
                        .replace(
                            &text[matched.start_byte..matched.end_byte],
                            self.find.replacement.as_str(),
                        )
                        .into_owned()
                })
                .unwrap_or_else(|| self.find.replacement.clone())
        } else {
            self.find.replacement.clone()
        };
        if self.find.preserve_case {
            replacement = preserve_case_replacement(
                &text[matched.start_byte..matched.end_byte],
                &replacement,
            );
        }
        let mut new_text = String::with_capacity(text.len() + replacement.len());
        new_text.push_str(&text[..matched.start_byte]);
        new_text.push_str(&replacement);
        new_text.push_str(&text[matched.end_byte..]);
        let new_cursor = matched.start_char + replacement.chars().count();
        doc.buffer.set_text(new_text);
        doc.update_cursor(new_cursor);
        set_editor_cursor(ctx, &doc.path, new_cursor);
        self.find.dirty = true;
        self.find
            .ensure_matches(doc.buffer.as_string(), doc.buffer.revision());
        self.find.select_next(new_cursor, true);
        self.set_toast("已替换 1 处", Severity::Success);
    }

    fn replace_all_matches(&mut self, ctx: &egui::Context) {
        let Some(active) = self.active else {
            return;
        };
        let Some(doc) = self.tabs.get_mut(active) else {
            return;
        };
        if doc.read_only {
            self.set_toast("只读文件不能替换", Severity::Warning);
            return;
        }
        let revision = doc.buffer.revision();
        self.find.ensure_matches(doc.buffer.as_string(), revision);
        let count = self.find.matches.len();
        if count == 0 {
            self.set_toast("没有可替换的匹配项", Severity::Info);
            return;
        }
        let text = doc.buffer.as_string().to_owned();
        let new_text = if self.find.preserve_case {
            let mut result = String::with_capacity(text.len());
            let mut cursor = 0usize;
            for matched in &self.find.matches {
                if matched.start_byte < cursor || matched.end_byte > text.len() {
                    continue;
                }
                result.push_str(&text[cursor..matched.start_byte]);
                let original = &text[matched.start_byte..matched.end_byte];
                let expanded = if self.find.use_regex {
                    self.find
                        .compiled
                        .as_ref()
                        .map(|regex| {
                            regex
                                .replace(original, self.find.replacement.as_str())
                                .into_owned()
                        })
                        .unwrap_or_else(|| self.find.replacement.clone())
                } else {
                    self.find.replacement.clone()
                };
                result.push_str(&preserve_case_replacement(original, &expanded));
                cursor = matched.end_byte;
            }
            result.push_str(&text[cursor..]);
            result
        } else if let Some(regex) = &self.find.compiled {
            if self.find.use_regex {
                regex
                    .replace_all(&text, self.find.replacement.as_str())
                    .into_owned()
            } else {
                regex
                    .replace_all(&text, regex::NoExpand(&self.find.replacement))
                    .into_owned()
            }
        } else {
            text
        };
        doc.buffer.set_text(new_text);
        doc.update_cursor(0);
        set_editor_cursor(ctx, &doc.path, 0);
        self.find.dirty = true;
        self.find
            .ensure_matches(doc.buffer.as_string(), doc.buffer.revision());
        self.find.active = 0;
        self.set_toast(format!("已替换 {count} 处"), Severity::Success);
    }

    fn request_close_tab(&mut self, index: usize) {
        let Some(doc) = self.tabs.get(index) else {
            return;
        };
        if doc.is_dirty() {
            self.pending = Some(PendingDialog {
                action: PendingAction::CloseTab(index),
                error: None,
            });
        } else {
            self.close_tab_now(index);
        }
    }

    fn request_close_paths(&mut self, paths: Vec<PathBuf>, focus: Option<PathBuf>) {
        if paths.is_empty() {
            return;
        }
        let has_dirty = self
            .tabs
            .iter()
            .any(|doc| doc.is_dirty() && paths.iter().any(|path| same_path(path, &doc.path)));
        if has_dirty {
            self.pending = Some(PendingDialog {
                action: PendingAction::CloseTabs { paths, focus },
                error: None,
            });
        } else {
            self.close_tab_paths_now(&paths, focus.as_deref());
        }
    }

    fn request_close_others(&mut self, index: usize) {
        let Some(keep) = self.tabs.get(index).map(|doc| doc.path.clone()) else {
            return;
        };
        let paths = self
            .tabs
            .iter()
            .filter(|doc| !same_path(&doc.path, &keep))
            .map(|doc| doc.path.clone())
            .collect::<Vec<_>>();
        self.request_close_paths(paths, Some(keep));
    }

    fn request_close_to_right(&mut self, index: usize) {
        let Some(focus) = self.tabs.get(index).map(|doc| doc.path.clone()) else {
            return;
        };
        let paths = self
            .tabs
            .iter()
            .skip(index + 1)
            .map(|doc| doc.path.clone())
            .collect::<Vec<_>>();
        self.request_close_paths(paths, Some(focus));
    }

    fn request_close_all(&mut self) {
        let paths = self.tabs.iter().map(|doc| doc.path.clone()).collect();
        self.request_close_paths(paths, None);
    }

    fn close_saved_tabs(&mut self) {
        let paths = self
            .tabs
            .iter()
            .filter(|doc| !doc.is_dirty())
            .map(|doc| doc.path.clone())
            .collect::<Vec<_>>();
        self.close_tab_paths_now(&paths, None);
    }

    fn close_tab_paths_now(&mut self, paths: &[PathBuf], focus: Option<&Path>) {
        if paths.is_empty() {
            return;
        }
        let current = self.active_document().map(|doc| doc.path.clone());
        let focus = focus
            .filter(|path| self.tabs.iter().any(|doc| same_path(&doc.path, path)))
            .map(Path::to_path_buf);
        self.tabs
            .retain(|doc| !paths.iter().any(|path| same_path(path, &doc.path)));
        if self
            .split_path
            .as_ref()
            .is_some_and(|path| paths.iter().any(|removed| same_path(path, removed)))
            || self
                .split_primary
                .as_ref()
                .is_some_and(|path| paths.iter().any(|removed| same_path(path, removed)))
        {
            self.split_path = None;
            self.split_primary = None;
            self.split_focus = 0;
        }
        self.active = focus
            .and_then(|path| self.tabs.iter().position(|doc| same_path(&doc.path, &path)))
            .or_else(|| {
                current
                    .and_then(|path| self.tabs.iter().position(|doc| same_path(&doc.path, &path)))
            })
            .or_else(|| (!self.tabs.is_empty()).then_some(0));
        self.focus_editor = self.active.is_some();
    }

    fn close_tab_now(&mut self, index: usize) {
        if index >= self.tabs.len() {
            return;
        }
        let closed_path = self.tabs[index].path.clone();
        if self
            .split_path
            .as_ref()
            .is_some_and(|path| same_path(path, &closed_path))
            || self
                .split_primary
                .as_ref()
                .is_some_and(|path| same_path(path, &closed_path))
        {
            self.split_path = None;
            self.split_primary = None;
            self.split_focus = 0;
        }
        self.tabs.remove(index);
        match self.active {
            Some(active) if active == index => {
                self.active = if self.tabs.is_empty() {
                    None
                } else {
                    Some(index.min(self.tabs.len() - 1))
                };
                self.focus_editor = self.active.is_some();
            }
            Some(active) if active > index => {
                self.active = Some(active - 1);
            }
            _ => {}
        }
    }

    fn build_config_uses_micropython(&self) -> bool {
        self.build_config.toolchain.is_micropython()
    }

    fn start_micropython(&mut self, action: MicroPythonAction) {
        if self.build.is_running() {
            self.set_toast(
                "构建任务正在进行，暂不能操作 MicroPython",
                Severity::Warning,
            );
            return;
        }
        if self.micropython_busy {
            self.set_toast("MicroPython 操作正在进行", Severity::Warning);
            return;
        }
        if self.build_config_uses_micropython() && !self.build_config.serial.is_ready() {
            self.set_toast(
                "请先在配置 → 构建中设置 MicroPython 串口和波特率",
                Severity::Warning,
            );
            self.open_settings();
            return;
        }
        let serial = self.build_config.serial.clone();
        let port = serial.port.trim().to_owned();
        let baud = serial.baud;
        let (file_name, code) = if action == MicroPythonAction::Reset {
            ("main.py".to_owned(), String::new())
        } else {
            let Some(doc) = self.active_document() else {
                self.set_toast("请先打开一个 MicroPython 文件", Severity::Warning);
                return;
            };
            if doc.binary.is_some() {
                self.set_toast("二进制文件不能作为 MicroPython 代码运行", Severity::Warning);
                return;
            }
            if !doc.name.to_ascii_lowercase().ends_with(".py") {
                self.set_toast("请打开 .py 文件后再运行或上传", Severity::Warning);
                return;
            }
            let name = if action == MicroPythonAction::Run {
                "main.py".to_owned()
            } else {
                self.board_remote_paths
                    .get(&doc.path)
                    .cloned()
                    .unwrap_or_else(|| doc.name.clone())
            };
            (name, doc.buffer.as_string().to_owned())
        };

        let command = match action {
            MicroPythonAction::Run => {
                format!("MicroPython · 运行 → {port} @ {baud}（上传为 main.py 并软复位）")
            }
            MicroPythonAction::Upload => {
                format!("MicroPython · 上传 {file_name} → {port} @ {baud}")
            }
            MicroPythonAction::Reset => format!("MicroPython · 软复位 {port} @ {baud}"),
        };
        self.build.log.clear_diagnostics();
        self.build.log.push_blank();
        self.build.log.push_message(command, Severity::Command);
        self.output_view = OutputView::Build;
        self.settings.output_open = true;
        if self.settings.output_height < 100.0 {
            self.settings.output_height = 214.0;
        }
        self.settings.save();

        let (sender, receiver) = mpsc::channel();
        self.micropython_busy = true;
        self.micropython_action = Some(action.label().to_owned());
        self.micropython_rx = Some(receiver);
        self.board_file_error = None;
        self.set_toast(
            format!("MicroPython 正在{}…", action.label()),
            Severity::Info,
        );

        std::thread::spawn(move || {
            let result = match action {
                MicroPythonAction::Run => crate::micropython::run(&port, baud, &code),
                MicroPythonAction::Upload => {
                    crate::micropython::upload(&port, baud, &file_name, &code)
                }
                MicroPythonAction::Reset => crate::micropython::soft_reset(&port, baud),
            };
            let _ = sender.send(MicroPythonJobResult::Report(result));
        });
    }

    fn start_firmware_flash(&mut self) -> bool {
        if self.build.is_running() || self.micropython_busy {
            self.set_toast("已有任务正在进行", Severity::Warning);
            return false;
        }
        if !self.build_config_uses_micropython() {
            self.set_toast("请先选择 MicroPython 工具链", Severity::Warning);
            return false;
        }
        let serial = self.build_config.serial.clone();
        if !serial.is_ready() {
            self.set_toast(
                "请先在配置 → 构建中设置 MicroPython 串口和波特率",
                Severity::Warning,
            );
            return false;
        }
        let config = self.build_config.firmware.clone();
        if !config.is_ready() {
            self.set_toast("请先选择固件文件并配置 esptool", Severity::Warning);
            return false;
        }
        if !Path::new(config.file.trim()).is_file() {
            self.set_toast(format!("固件文件不存在：{}", config.file), Severity::Error);
            return false;
        }
        let port = serial.port.trim().to_owned();
        let baud = serial.baud;
        let command = format!(
            "MicroPython · 烧录固件 {} → {} @ {}（{} @ {baud}）",
            config.file, config.chip, config.address, port
        );
        self.build.log.clear_diagnostics();
        self.build.log.push_blank();
        self.build.log.push_message(command, Severity::Command);
        self.output_view = OutputView::Build;
        self.settings.output_open = true;
        if self.settings.output_height < 100.0 {
            self.settings.output_height = 214.0;
        }
        self.settings.save();
        let (sender, receiver) = mpsc::channel();
        self.micropython_busy = true;
        self.micropython_action = Some("烧录固件".to_owned());
        self.micropython_rx = Some(receiver);
        self.board_file_error = None;
        self.set_toast("正在烧录 MicroPython 固件…", Severity::Info);
        std::thread::spawn(move || {
            let result = crate::micropython::flash_firmware(&port, baud, &config);
            let _ = sender.send(MicroPythonJobResult::Report(result));
        });
        true
    }

    fn start_board_files(&mut self) {
        if self.build.is_running() || self.micropython_busy {
            return;
        }
        if !self.build_config.serial.is_ready() {
            self.set_toast(
                "请先在配置 → 构建中设置 MicroPython 串口和波特率",
                Severity::Warning,
            );
            return;
        }
        let serial = self.build_config.serial.clone();
        let port = serial.port.trim().to_owned();
        let baud = serial.baud;
        let (sender, receiver) = mpsc::channel();
        self.micropython_busy = true;
        self.micropython_action = Some("读取文件".to_owned());
        self.micropython_rx = Some(receiver);
        self.board_error = None;
        self.board_file_error = None;
        self.build.log.push_message(
            format!("MicroPython · 读取芯片文件 {port} @ {baud}"),
            Severity::Command,
        );
        std::thread::spawn(move || {
            let result = crate::micropython::list_files(&port, baud);
            let _ = sender.send(MicroPythonJobResult::Files(result));
        });
    }

    fn start_board_file(&mut self, file: crate::micropython::BoardFile) {
        if file.is_dir {
            return;
        }
        if self.build.is_running() || self.micropython_busy {
            self.set_toast("MicroPython 操作正在进行", Severity::Warning);
            return;
        }
        if !self.build_config.serial.is_ready() {
            self.set_toast(
                "请先在配置 → 构建中设置 MicroPython 串口和波特率",
                Severity::Warning,
            );
            return;
        }
        if file.size as usize > crate::micropython::MAX_READ_BYTES {
            let message = format!(
                "{} 超过 {} KB，暂不支持读取",
                file.path,
                crate::micropython::MAX_READ_BYTES / 1024
            );
            self.board_file_error = Some(message.clone());
            self.set_toast(message, Severity::Warning);
            return;
        }
        let Some(root) = self.project.as_ref().map(|project| project.root.clone()) else {
            self.set_toast("请先打开一个工程文件夹", Severity::Warning);
            return;
        };
        let local_path = match board_download_path(&root, &file) {
            Ok(path) => path,
            Err(error) => {
                self.board_file_error = Some(error.clone());
                self.set_toast(error, Severity::Error);
                return;
            }
        };
        let serial = self.build_config.serial.clone();
        let port = serial.port.trim().to_owned();
        let baud = serial.baud;
        let remote_path = file.path.clone();
        let (sender, receiver) = mpsc::channel();
        self.micropython_busy = true;
        self.micropython_action = Some("读取文件".to_owned());
        self.micropython_rx = Some(receiver);
        self.board_file_error = None;
        self.build.log.push_message(
            format!(
                "MicroPython · 读取 {} → {}",
                file.path,
                util::display_path(&local_path)
            ),
            Severity::Command,
        );
        std::thread::spawn(move || {
            let result = crate::micropython::read_file(&port, baud, &remote_path);
            let _ = sender.send(MicroPythonJobResult::File {
                result,
                remote_path,
                local_path,
            });
        });
    }

    fn poll_micropython(&mut self) {
        let Some(receiver) = self.micropython_rx.take() else {
            return;
        };
        match receiver.try_recv() {
            Ok(MicroPythonJobResult::Report(result)) => {
                self.micropython_busy = false;
                let action = self
                    .micropython_action
                    .take()
                    .unwrap_or_else(|| "操作".to_owned());
                match result {
                    Ok(report) => {
                        for line in report.output.lines() {
                            if !line.trim().is_empty() {
                                self.build.log.push_raw(line);
                            }
                        }
                        for line in report.error.lines() {
                            if !line.trim().is_empty() {
                                self.build.log.push_message(line, Severity::Error);
                            }
                        }
                        if report.success {
                            self.board_file_error = None;
                            if action == "烧录固件" {
                                self.board_files = None;
                                self.board_error = None;
                                self.board_remote_paths.clear();
                            }
                            let message = match action.as_str() {
                                "上传" => "MicroPython 上传完成".to_owned(),
                                "复位" => "MicroPython 已软复位".to_owned(),
                                "烧录固件" => "MicroPython 固件烧录完成".to_owned(),
                                _ => "MicroPython 已启动 main.py（设备继续运行）".to_owned(),
                            };
                            self.build
                                .log
                                .push_message(message.clone(), Severity::Success);
                            self.set_toast(message, Severity::Success);
                        } else {
                            let detail = report
                                .error
                                .lines()
                                .find(|line| !line.trim().is_empty())
                                .unwrap_or("MicroPython 返回错误");
                            let message = format!("MicroPython {}失败：{detail}", action);
                            self.board_file_error = Some(message.clone());
                            self.build
                                .log
                                .push_message(message.clone(), Severity::Error);
                            self.set_toast(message, Severity::Error);
                        }
                    }
                    Err(error) => {
                        let message = format!("MicroPython {action}失败：{error}");
                        self.board_file_error = Some(message.clone());
                        self.build
                            .log
                            .push_message(message.clone(), Severity::Error);
                        self.set_toast(message, Severity::Error);
                    }
                }
            }
            Ok(MicroPythonJobResult::Files(result)) => {
                self.micropython_busy = false;
                self.micropython_action = None;
                match result {
                    Ok(files) => {
                        let count = files.len();
                        self.board_files = Some(files);
                        self.board_error = None;
                        self.board_file_error = None;
                        self.build
                            .log
                            .push_message(format!("芯片文件：{count} 项"), Severity::Info);
                        self.set_toast(format!("芯片文件：{count} 项"), Severity::Success);
                    }
                    Err(error) => {
                        self.board_files = None;
                        self.board_error = Some(error.clone());
                        self.build.log.push_message(error.clone(), Severity::Error);
                        self.set_toast(error, Severity::Error);
                    }
                }
            }
            Ok(MicroPythonJobResult::File {
                result,
                remote_path,
                local_path,
            }) => {
                self.micropython_busy = false;
                self.micropython_action = None;
                match result {
                    Ok(content) => match std::fs::write(&local_path, content) {
                        Ok(()) => {
                            let canonical = std::fs::canonicalize(&local_path)
                                .unwrap_or_else(|_| local_path.clone());
                            self.board_remote_paths
                                .insert(canonical, remote_path.clone());
                            self.board_file_error = None;
                            if let Some(tree) = self.tree.as_mut() {
                                tree.refresh();
                            }
                            self.build.log.push_message(
                                format!(
                                    "已读取 {} → {}",
                                    remote_path,
                                    util::display_path(&local_path)
                                ),
                                Severity::Success,
                            );
                            let message = format!("已读取到 {}", util::display_path(&local_path));
                            self.set_toast(message, Severity::Success);
                            self.open_file(local_path);
                        }
                        Err(err) => {
                            let message = format!("保存芯片文件失败：{err}");
                            self.board_file_error = Some(message.clone());
                            self.build
                                .log
                                .push_message(message.clone(), Severity::Error);
                            self.set_toast(message, Severity::Error);
                        }
                    },
                    Err(error) => {
                        self.board_file_error = Some(error.clone());
                        self.build.log.push_message(error.clone(), Severity::Error);
                        self.set_toast(error, Severity::Error);
                    }
                }
            }
            Err(std::sync::mpsc::TryRecvError::Empty) => {
                self.micropython_rx = Some(receiver);
            }
            Err(std::sync::mpsc::TryRecvError::Disconnected) => {
                self.micropython_busy = false;
                self.micropython_action = None;
                self.board_file_error = Some("MicroPython 操作线程已退出".to_owned());
                self.set_toast("MicroPython 操作线程已退出", Severity::Error);
            }
        }
    }

    fn start_task(&mut self, kind: TaskKind) {
        if self.build.is_running() {
            self.set_toast("已有任务正在运行", Severity::Warning);
            return;
        }
        let Some(project_root) = self.project.as_ref().map(|project| project.root.clone()) else {
            self.set_toast("请先打开一个工程文件夹", Severity::Warning);
            return;
        };
        if self.build_config_uses_micropython() {
            match kind {
                TaskKind::Build => self.start_micropython(MicroPythonAction::Run),
                TaskKind::Flash => self.start_micropython(MicroPythonAction::Upload),
                TaskKind::Clean | TaskKind::Rebuild => {
                    self.set_toast("MicroPython 模式不执行清理或重构", Severity::Warning);
                }
            }
            return;
        }
        if !self.build_config.has_command(kind) {
            let message = if self.build_config.toolchain.is_micropython() {
                format!(
                    "MicroPython 模式下不执行{}，请先在配置 → 构建中设置串口",
                    kind.label()
                )
            } else {
                format!("{} 尚未配置", kind.label())
            };
            self.set_toast(message, Severity::Warning);
            if kind == TaskKind::Flash {
                self.open_flash_config();
            } else {
                self.open_settings();
            }
            return;
        }
        self.build.log.clear_diagnostics();
        self.output_view = OutputView::Build;

        match self.save_all_dirty() {
            Ok(saved) if saved > 0 => {
                self.set_toast(format!("已自动保存 {saved} 个文件"), Severity::Info);
                self.build
                    .log
                    .push_message(format!("已自动保存 {saved} 个文件"), Severity::Info);
            }
            Ok(_) => {}
            Err(err) => {
                self.set_toast(err, Severity::Error);
                return;
            }
        }

        let mut spec = match self.build_config.make_spec(kind, &project_root) {
            Ok(spec) => spec,
            Err(err) => {
                self.set_toast(err, Severity::Error);
                return;
            }
        };
        if self.build_config.target == BuildTarget::Local || kind == TaskKind::Flash {
            for dir in crate::toolchain::build_env_dirs(&self.settings)
                .into_iter()
                .rev()
            {
                spec.env_paths.insert(0, dir);
            }
        }

        self.settings.output_open = true;
        self.settings.save();
        match self.build.start(spec, kind) {
            Ok(()) => {}
            Err(err) => self.set_toast(err, Severity::Error),
        }
    }

    fn code_theme(&self) -> &'static crate::codetheme::CodeTheme {
        crate::codetheme::by_name(&self.settings.editor_theme)
            .unwrap_or(crate::codetheme::default_theme())
    }

    fn open_theme_settings(&mut self) {
        if self.has_modal() {
            return;
        }
        self.theme_settings = Some(ThemeSettingsDraft {
            selected: self.settings.editor_theme.clone(),
            original: self.settings.editor_theme.clone(),
            filter: String::new(),
        });
    }

    fn set_editor_theme(&mut self, ctx: &egui::Context, name: &str) {
        let Some(code_theme) = crate::codetheme::by_name(name) else {
            return;
        };
        self.settings.editor_theme = code_theme.name.to_owned();
        self.highlighter.set_code_theme(code_theme);
        crate::theme::apply(ctx, code_theme);
        self.native_theme_requested = false;
        self.native_titlebar_styled = false;
        for document in &mut self.tabs {
            document.layout_cache = highlight::new_cache();
            document.minimap_cache = None;
        }
        ctx.request_repaint();
    }

    fn apply_editor_theme(&mut self, ctx: &egui::Context, name: &str) {
        self.set_editor_theme(ctx, name);
        self.settings.save();
        let code_theme = self.code_theme();
        self.set_toast(
            format!("已应用主题：{}", code_theme.name),
            Severity::Success,
        );
    }

    fn cancel_theme_settings(&mut self, ctx: &egui::Context) {
        let Some(draft) = self.theme_settings.take() else {
            return;
        };
        self.set_editor_theme(ctx, &draft.original);
    }

    fn open_settings(&mut self) {
        let (debug_program, gdb_path) = if self.project.is_some() {
            (
                self.project_debug.program.clone(),
                self.project_debug.gdb_path.clone(),
            )
        } else {
            (
                self.settings.debug_program.clone(),
                self.settings.gdb_path.clone(),
            )
        };
        let mut extra_paths = self.build_config.extra_paths.clone();
        if let Some(project) = &self.project {
            for path in crate::config::autodetect(&project.root).extra_paths {
                if !extra_paths
                    .iter()
                    .any(|existing| existing.eq_ignore_ascii_case(&path))
                {
                    extra_paths.push(path);
                }
            }
        }
        let mut draft = SettingsDraft {
            tab: SettingsTab::Build,
            extra_paths_text: extra_paths.join("\n"),
            config: self.build_config.clone(),
            font_size: self.settings.font_size,
            word_wrap: self.settings.word_wrap,
            relative_line_numbers: self.settings.relative_line_numbers,
            show_whitespace: self.settings.show_whitespace,
            check_duplicate_macro_values: self.settings.check_duplicate_macro_values,
            ruler_column: self.settings.ruler_column,
            use_spaces: self.settings.use_spaces,
            indent_width: self.settings.indent_width,
            autosave: self.settings.autosave,
            autosave_delay_ms: self.settings.autosave_delay_ms,
            undo_limit: self.settings.undo_limit,
            format_on_save: self.settings.format_on_save,
            clang_format_path: self.settings.clang_format_path.clone(),
            clangd_enabled: self.settings.clangd_enabled,
            clangd_path: self.settings.clangd_path.clone(),
            debug_program,
            gdb_path,
            auto_scroll_output: self.settings.auto_scroll_output,
            show_minimap: self.settings.show_minimap,
            show_indent_guides: self.settings.show_indent_guides,
            show_hover_info: self.settings.show_hover_info,
            show_hidden: self.settings.show_hidden,
            respect_gitignore: self.settings.respect_gitignore,
            ignore_patterns: self.settings.ignore_patterns.clone(),
            serial_ports: Vec::new(),
            serial_note: None,
            note: None,
            error: None,
        };
        if draft.config.toolchain.is_micropython() {
            draft.refresh_serial_ports(false);
        }
        self.settings_draft = Some(draft);
    }

    fn effective_includes(&self) -> &[String] {
        self.build_config
            .meta_includes
            .as_deref()
            .unwrap_or(&self.project_meta.include_dirs)
    }

    fn effective_library_dirs(&self) -> &[String] {
        self.build_config
            .meta_library_dirs
            .as_deref()
            .unwrap_or(&self.project_meta.library_dirs)
    }

    fn effective_defines(&self) -> &[String] {
        self.build_config
            .meta_defines
            .as_deref()
            .unwrap_or(&self.project_meta.defines)
    }

    fn meta_values(&self, field: MetaListField) -> Vec<String> {
        match field {
            MetaListField::Includes => self.effective_includes().to_vec(),
            MetaListField::LibraryDirs => self.effective_library_dirs().to_vec(),
            MetaListField::Defines => self.effective_defines().to_vec(),
        }
    }

    fn set_meta_values(&mut self, field: MetaListField, values: Vec<String>) {
        match field {
            MetaListField::Includes => self.build_config.meta_includes = Some(values),
            MetaListField::LibraryDirs => self.build_config.meta_library_dirs = Some(values),
            MetaListField::Defines => self.build_config.meta_defines = Some(values),
        }
    }

    fn open_meta_editor(&mut self) {
        if self.project.is_none() {
            return;
        }
        self.meta_draft = Some(MetaDraft {
            includes: self.effective_includes().join("\n"),
            library_dirs: self.effective_library_dirs().join("\n"),
            defines: self.effective_defines().join("\n"),
            note: None,
        });
    }

    fn open_meta_add(&mut self, field: MetaListField) {
        if self.project.is_none() {
            return;
        }
        self.meta_add = Some(MetaAddDraft {
            field,
            value: String::new(),
        });
    }

    fn add_meta_value(&mut self, field: MetaListField, value: &str) {
        let value = value.trim();
        if value.is_empty() || self.project.is_none() {
            return;
        }
        let mut values = self.meta_values(field);
        if !values.iter().any(|existing| existing == value) {
            values.push(value.to_owned());
        }
        self.set_meta_values(field, values);
        let result = self.persist_meta_overrides();
        self.notify_sync(
            &format!("已添加{}", field.label()),
            result,
            Severity::Success,
        );
    }

    fn save_meta_overrides(
        &mut self,
        includes: Vec<String>,
        library_dirs: Vec<String>,
        defines: Vec<String>,
    ) {
        self.build_config.meta_includes = Some(includes);
        self.build_config.meta_library_dirs = Some(library_dirs);
        self.build_config.meta_defines = Some(defines);
        let result = self.persist_meta_overrides();
        self.notify_sync("已保存 C/C++ 属性", result, Severity::Success);
    }

    fn clear_meta_overrides(&mut self) {
        self.build_config.meta_includes = None;
        self.build_config.meta_library_dirs = None;
        self.build_config.meta_defines = None;
        let result = self.persist_meta_overrides();
        self.notify_sync("已恢复自动检测的 C/C++ 属性", result, Severity::Info);
    }

    /// Scan the project for C/C++ headers and append every parent directory to
    /// the include list. Make projects then sync the result to `C_INCLUDES`.
    fn detect_include_dirs(&mut self) {
        let Some(root) = self.project.as_ref().map(|project| project.root.clone()) else {
            self.set_toast("请先打开工程", Severity::Info);
            return;
        };
        let system = self.build_config.effective_build_system();
        let base = if system == BuildSystem::Keil {
            crate::project_meta::find_uvprojx(&root)
                .and_then(|path| path.parent().map(Path::to_path_buf))
                .unwrap_or_else(|| root.clone())
        } else {
            root.clone()
        };
        let ignore = IgnoreRules::load(
            &root,
            self.settings.respect_gitignore,
            &self.settings.ignore_patterns,
        );
        let detected = crate::project_meta::detect_include_dirs(&root, &base, &ignore);
        let existing = self.effective_includes().to_vec();
        let (pruned, removed) = crate::project_meta::prune_source_only_includes(&existing, &base);
        let (merged, added) = crate::project_meta::merge_include_dirs(&pruned, &detected, &base);
        if added > 0 || removed > 0 {
            self.build_config.meta_includes = Some(merged.clone());
            let result = self.persist_meta_overrides();
            let removed_note = if removed > 0 {
                format!("，移除 {removed} 个仅含源文件的目录")
            } else {
                String::new()
            };
            self.notify_sync(
                &format!(
                    "已添加 {} 个头文件包含目录（共 {} 个）{removed_note}",
                    added,
                    merged.len()
                ),
                result,
                Severity::Success,
            );
        } else if detected.is_empty() {
            self.set_toast(
                "未找到 C/C++ 头文件；请确认工程中包含 .h / .hpp 文件",
                Severity::Info,
            );
        } else {
            self.set_toast(
                format!(
                    "已检测到 {} 个头文件目录，全部已在包含目录中",
                    detected.len()
                ),
                Severity::Info,
            );
        }
        // One click also keeps the Makefile source list in sync, so a BSP or
        // application `.c` file that was never added to `C_SOURCES` is picked
        // up without a second action.
        if self.build_config.effective_build_system() == BuildSystem::Make {
            self.sync_makefile_sources();
        }
    }

    fn sync_makefile_sources(&mut self) {
        let Some(root) = self.project.as_ref().map(|project| project.root.clone()) else {
            self.set_toast("请先打开工程", Severity::Info);
            return;
        };
        if self.build_config.effective_build_system() != BuildSystem::Make {
            self.set_toast("自动同步 C_SOURCES 仅支持 Makefile 工程", Severity::Info);
            return;
        }
        let Some(makefile_path) = makefile::find_makefile(&root) else {
            self.set_toast("未找到 Makefile", Severity::Warning);
            return;
        };
        if let Some(doc) = self
            .tabs
            .iter()
            .find(|doc| same_path(&doc.path, &makefile_path))
        {
            if doc.is_dirty() {
                self.set_toast("请先保存 Makefile，再同步源文件", Severity::Warning);
                return;
            }
        }
        let ignore = IgnoreRules::load(
            &root,
            self.settings.respect_gitignore,
            &self.settings.ignore_patterns,
        );
        let sources = crate::project_meta::detect_source_files(&root, &ignore);
        match crate::makefile::sync_sources(&root, &sources) {
            Ok((changed, added)) => {
                if let Some(changed) = changed {
                    if let Some(doc) = self
                        .tabs
                        .iter_mut()
                        .find(|doc| same_path(&doc.path, &changed))
                    {
                        if let Ok(loaded) = crate::util::read_text_file(&changed) {
                            doc.buffer.set_text(loaded.text);
                            doc.saved_revision = doc.buffer.revision();
                            doc.update_cursor(0);
                        }
                    }
                    self.set_toast(
                        format!("已向 Makefile 的 C_SOURCES 添加 {added} 个源文件（修改前已备份）"),
                        Severity::Success,
                    );
                } else if added == 0 {
                    self.set_toast("C_SOURCES 已包含工程内所有可同步的源文件", Severity::Info);
                }
            }
            Err(err) => self.set_toast(err, Severity::Warning),
        }
    }

    fn persist_meta_overrides(&mut self) -> Result<bool, String> {
        let Some(root) = self.project.as_ref().map(|project| project.root.clone()) else {
            return Ok(false);
        };
        let includes = self.effective_includes().to_vec();
        let library_dirs = self.effective_library_dirs().to_vec();
        let defines = self.effective_defines().to_vec();

        self.persist_build_config();
        if self.build_config.effective_build_system() != BuildSystem::Make {
            return Ok(false);
        }

        let Some(makefile_path) = makefile::find_makefile(&root) else {
            return Ok(false);
        };
        let open_tab = self
            .tabs
            .iter()
            .position(|document| same_path(&document.path, &makefile_path));
        if let Some(index) = open_tab {
            if self.tabs[index].is_dirty() {
                return Err("Makefile 有未保存修改，请先保存后再同步".to_owned());
            }
        }
        let synced = makefile::sync_lists(&root, &includes, &library_dirs, &defines)?.is_some();
        if synced {
            self.refresh_project_meta();
            if let Some(index) = open_tab {
                if let Ok(loaded) = crate::util::read_text_file(&makefile_path) {
                    let doc = &mut self.tabs[index];
                    let cursor = doc.cursor_index.min(loaded.text.chars().count());
                    doc.buffer.set_text(loaded.text);
                    doc.update_cursor(cursor);
                    doc.saved_revision = doc.buffer.revision();
                    doc.folds.clear();
                }
            }
        }
        Ok(synced)
    }

    fn effective_toolchain(&self) -> ToolchainKind {
        self.build_config.effective_toolchain(&self.project_meta)
    }

    fn refresh_project_meta(&mut self) {
        let Some(root) = self.project.as_ref().map(|project| project.root.clone()) else {
            return;
        };
        let system = self.build_config.effective_build_system();
        self.project_meta = ProjectMeta::detect_for(&root, system);
    }

    fn persist_build_config(&mut self) {
        let Some(root) = self.project.as_ref().map(|project| project.root.clone()) else {
            return;
        };
        match crate::config::ProjectSettings::save(&root, &self.build_config, &self.project_debug) {
            Ok(_) => {
                // The project file is authoritative once it can be written.
                self.settings.build_configs.remove(&project_key(&root));
            }
            Err(err) => {
                // Fall back to the user directory for read-only projects.
                self.settings.set_build_config(&root, &self.build_config);
                self.set_toast(format!("工程配置保存失败：{err}"), Severity::Error);
            }
        }
        self.settings.save();
        self.refresh_memory_layout();
    }

    fn report_link_problem(&mut self) {
        if !matches!(
            self.build.last().map(|last| &last.outcome),
            Some(BuildOutcome::Failed(_))
        ) {
            return;
        }
        let len = self.build.log.len();
        let start = len.saturating_sub(400);
        let mut symbols = Vec::new();
        for index in start..len {
            if let Some(line) = self.build.log.line_at(index) {
                if let Some(symbol) = crate::log::undefined_reference(&line.plain) {
                    if !symbols.contains(&symbol) {
                        symbols.push(symbol);
                    }
                }
            }
        }
        if symbols.is_empty() {
            return;
        }
        let names = symbols
            .iter()
            .take(3)
            .cloned()
            .collect::<Vec<_>>()
            .join(" / ");
        let hint = format!(
            "链接失败：找不到 {names} 的实现。请检查对应的 .c 是否加入 Makefile 的 C_SOURCES（或在 CubeMX 中勾选源文件），并确认函数已实现且没有加 static。"
        );
        self.build.log.push_message(hint.clone(), Severity::Warning);
        self.set_toast(hint, Severity::Error);
    }

    fn lsp_config_fingerprint(&self, root: &Path) -> u64 {
        use std::hash::{Hash, Hasher};
        let mut hasher = std::collections::hash_map::DefaultHasher::new();
        root.hash(&mut hasher);
        for include in self.effective_includes() {
            include.hash(&mut hasher);
        }
        for define in self.effective_defines() {
            define.hash(&mut hasher);
        }
        hasher.finish()
    }

    fn start_lsp(&mut self) {
        self.stop_lsp();
        if !self.settings.clangd_enabled {
            return;
        }
        let Some(root) = self.project.as_ref().map(|project| project.root.clone()) else {
            return;
        };
        self.lsp_config_key = self.lsp_config_fingerprint(&root);
        let Some(clangd) = crate::lsp::detect_clangd(&self.settings.clangd_path) else {
            self.set_toast(
                "未找到 clangd；请安装 LLVM/clangd，或在 配置 → 其他 → 语言服务 指定路径",
                Severity::Info,
            );
            return;
        };
        let compiler = crate::lsp::detect_arm_gcc(&self.build_config.extra_paths);
        let config = crate::lsp::WorkspaceConfig {
            root: root.clone(),
            includes: self.effective_includes().to_vec(),
            defines: self.effective_defines().to_vec(),
            cpu_flags: crate::lsp::makefile_cpu_flags(&root),
            compiler,
        };
        let ignore = crate::ignore::IgnoreRules::load(
            &root,
            self.settings.respect_gitignore,
            &self.settings.ignore_patterns,
        );
        match crate::lsp::Session::start(&clangd, &config, &ignore) {
            Ok(session) => {
                self.lsp = Some(session);
            }
            Err(err) => {
                self.lsp = None;
                self.set_toast(format!("clangd 启动失败：{err}"), Severity::Warning);
            }
        }
    }

    fn stop_lsp(&mut self) {
        if let Some(mut session) = self.lsp.take() {
            session.stop();
        }
        self.lsp_refs = None;
        self.lsp_hover = None;
        self.lsp_rename = None;
    }

    fn sync_lsp(&mut self) {
        let Some(root) = self.project.as_ref().map(|project| project.root.clone()) else {
            return;
        };
        if self.settings.clangd_enabled
            && self.lsp_config_key != 0
            && self.lsp_config_key != self.lsp_config_fingerprint(&root)
        {
            self.start_lsp();
            return;
        }
        let Some(session) = self.lsp.as_mut() else {
            return;
        };
        let docs = self
            .tabs
            .iter()
            .map(|doc| (doc.path.clone(), doc.buffer.revision()))
            .collect::<Vec<_>>();
        let needed = session.sync(&docs);
        if needed.is_empty() {
            return;
        }
        let mut feeds = Vec::new();
        for path in needed {
            if let Some(doc) = self.tabs.iter().find(|doc| same_path(&doc.path, &path)) {
                feeds.push((
                    path,
                    doc.buffer.as_string().to_owned(),
                    doc.buffer.revision(),
                ));
            }
        }
        for (path, text, revision) in feeds {
            session.feed(&path, &text, revision);
        }
    }

    fn poll_lsp(&mut self, ctx: &egui::Context) {
        let events = match self.lsp.as_mut() {
            Some(session) => session.poll(),
            None => return,
        };
        for event in events {
            match event {
                crate::lsp::Event::Ready => {
                    self.set_toast("clangd 已就绪", Severity::Success);
                    self.sync_lsp();
                }
                crate::lsp::Event::Failed(err) | crate::lsp::Event::Error(err) => {
                    self.set_toast(err, Severity::Warning);
                }
                crate::lsp::Event::Definition(items) => {
                    if items.is_empty() {
                        self.definition_from_index(ctx);
                    } else if items.len() == 1 {
                        let item = &items[0];
                        let path = item.path.to_string_lossy().to_string();
                        self.open_at(ctx, &path, item.line, item.column);
                    } else {
                        self.lsp_refs = Some(crate::lsp::RefsView {
                            title: "定义".to_owned(),
                            query: self.current_word().unwrap_or_default(),
                            items,
                        });
                    }
                }
                crate::lsp::Event::References(view) => {
                    if view.items.is_empty() {
                        self.set_toast("clangd 没有找到引用", Severity::Info);
                    } else {
                        self.lsp_refs = Some(view);
                    }
                }
                crate::lsp::Event::Hover(view) => {
                    self.lsp_hover = Some(view);
                    ctx.request_repaint();
                }
                crate::lsp::Event::Completion(view) => {
                    let matches = self.active_document().is_some_and(|doc| {
                        same_path(&doc.path, &view.path)
                            && doc.cursor_index == view.cursor
                            && doc.buffer.revision() == view.revision
                    });
                    if matches && !view.items.is_empty() {
                        self.completion.items = view
                            .items
                            .into_iter()
                            .map(|item| crate::completion::CompletionItem {
                                name: item.label,
                                kind: completion_kind(item.kind),
                                detail: item.detail,
                            })
                            .collect();
                        self.completion.open = true;
                        self.completion.selected = 0;
                        self.completion.prefix_start = view.prefix_start;
                        self.completion.cursor = view.cursor;
                        ctx.request_repaint();
                    }
                }
                crate::lsp::Event::Rename(files) => {
                    self.apply_lsp_edits(ctx, files);
                }
            }
        }
    }

    fn lsp_hover_text(&self, path: &Path, line: usize, word: &str) -> Option<String> {
        self.lsp_hover
            .as_ref()
            .filter(|hover| {
                same_path(&hover.path, path) && hover.line == line && hover.word == word
            })
            .map(|hover| hover.text.clone())
    }

    fn request_lsp_hover(&mut self, pane_index: usize, line: usize, column: usize) {
        let Some(doc) = self.tabs.get(pane_index) else {
            return;
        };
        let path = doc.path.clone();
        if !crate::lsp::supports_lsp(&path) {
            return;
        }
        let text = doc.buffer.as_string().to_owned();
        let cursor = crate::util::char_index_at_line(&text, line) + column.saturating_sub(1);
        if let Some(session) = self.lsp.as_mut() {
            let _ = session.hover(&path, &text, cursor);
        }
    }

    fn request_lsp_completion(&mut self) {
        let Some((path, text, revision)) = self.active_document().map(|doc| {
            (
                doc.path.clone(),
                doc.buffer.as_string().to_owned(),
                doc.buffer.revision(),
            )
        }) else {
            return;
        };
        let cursor = self.completion.cursor;
        let prefix_start = self.completion.prefix_start;
        if let Some(session) = self.lsp.as_mut() {
            let _ = session.completion(&path, &text, cursor, revision, prefix_start);
        }
    }

    fn open_lsp_rename(&mut self) {
        if !self.lsp.as_ref().is_some_and(|session| session.is_ready()) {
            self.set_toast("clangd 尚未就绪，无法重命名", Severity::Warning);
            return;
        }
        let Some(doc) = self.active_document() else {
            return;
        };
        if doc.read_only {
            self.set_toast("只读文件不能重命名", Severity::Warning);
            return;
        }
        if !crate::lsp::supports_lsp(&doc.path) {
            self.set_toast("clangd 只支持 C/C++ 文件", Severity::Info);
            return;
        }
        let Some(word) = self.current_word() else {
            self.set_toast("光标不在标识符上", Severity::Info);
            return;
        };
        let path = doc.path.clone();
        let cursor = doc.cursor_index;
        self.lsp_rename = Some(RenameDialog {
            path,
            cursor,
            original: word.clone(),
            name: word,
            error: None,
        });
    }

    fn confirm_lsp_rename(&mut self, ctx: &egui::Context) {
        let Some(dialog) = self.lsp_rename.clone() else {
            return;
        };
        let name = dialog.name.trim().to_owned();
        if name.is_empty() || name == dialog.original {
            self.set_toast("请输入新的符号名", Severity::Info);
            return;
        }
        let Some(doc) = self
            .tabs
            .iter()
            .find(|doc| same_path(&doc.path, &dialog.path))
        else {
            self.lsp_rename = None;
            return;
        };
        let text = doc.buffer.as_string().to_owned();
        let Some(session) = self.lsp.as_mut() else {
            self.lsp_rename = None;
            return;
        };
        match session.rename(&dialog.path, &text, dialog.cursor, &name) {
            Ok(()) => {
                self.lsp_rename = None;
            }
            Err(err) => {
                if let Some(dialog) = &mut self.lsp_rename {
                    dialog.error = Some(err);
                }
            }
        }
        ctx.request_repaint();
    }

    fn apply_lsp_edits(&mut self, ctx: &egui::Context, files: Vec<crate::lsp::FileEdits>) {
        let mut changed_docs = 0usize;
        let mut changed_files = 0usize;
        let mut failures = Vec::new();
        for file in files {
            let open_index = self
                .tabs
                .iter()
                .position(|doc| same_path(&doc.path, &file.path));
            let (mut text, line_ending, bom, encoding) = if let Some(index) = open_index {
                let doc = &self.tabs[index];
                (
                    doc.buffer.as_string().to_owned(),
                    doc.line_ending,
                    doc.bom,
                    doc.encoding,
                )
            } else {
                match crate::util::read_text_file(&file.path) {
                    Ok(loaded) => (loaded.text, loaded.line_ending, loaded.bom, loaded.encoding),
                    Err(err) => {
                        failures.push(format!("{}：{err}", crate::util::display_path(&file.path)));
                        continue;
                    }
                }
            };
            let mut edits = file.edits;
            edits.sort_by(|a, b| {
                (b.start.line, b.start.character).cmp(&(a.start.line, a.start.character))
            });
            for edit in edits {
                let start = crate::lsp::position_to_char(&text, &edit.start);
                let end = crate::lsp::position_to_char(&text, &edit.end);
                let (updated, _) =
                    crate::navigation::replace_range_chars(&text, start, end, &edit.new_text);
                text = updated;
            }
            if let Some(index) = open_index {
                let doc = &mut self.tabs[index];
                doc.buffer.set_text(text);
                doc.folds.clear();
                doc.macro_issue_revision = u64::MAX;
                changed_docs += 1;
            } else if let Err(err) =
                crate::util::write_text_file(&file.path, &text, line_ending, bom, encoding)
            {
                failures.push(format!("{}：{err}", crate::util::display_path(&file.path)));
            } else {
                changed_files += 1;
            }
        }
        self.find.dirty = true;
        self.sync_lsp();
        if failures.is_empty() {
            self.set_toast(
                format!("已重命名：修改 {changed_docs} 个打开文件、{changed_files} 个磁盘文件"),
                Severity::Success,
            );
        } else {
            self.set_toast(
                format!("重命名部分完成：{}", failures.join("；")),
                Severity::Warning,
            );
        }
        ctx.request_repaint();
    }

    fn definition_from_index(&mut self, ctx: &egui::Context) {
        let Some(word) = self.current_word() else {
            self.set_toast("光标不在标识符上", Severity::Info);
            return;
        };
        self.ensure_symbols();
        let current_path = self.active_document().map(|doc| doc.path.clone());
        let mut candidates = self
            .symbols
            .items
            .iter()
            .filter(|symbol| symbol.name == word)
            .cloned()
            .collect::<Vec<_>>();
        if candidates.is_empty() {
            self.open_references_for_word(&word);
            self.set_toast(
                format!("没有找到 {word} 的定义，已列出相关匹配"),
                Severity::Info,
            );
            return;
        }
        if let Some(path) = current_path {
            candidates.sort_by_key(|symbol| !same_path(&symbol.path, &path));
        }
        let symbol = candidates[0].clone();
        self.open_at(
            ctx,
            &symbol.path.to_string_lossy(),
            symbol.line,
            symbol.column,
        );
    }

    fn apply_openocd_defaults(&self, openocd: &mut OpenOcdConfig) {
        if openocd.executable.trim().is_empty() {
            openocd.executable = "openocd".to_owned();
        }
        if openocd.program_file.trim().is_empty() {
            openocd.program_file = "${ExecutableName}.hex".to_owned();
        }
        if openocd.target_config.trim().is_empty() {
            if let Some(target) = flash::infer_target_config(self.project_meta.device.as_deref()) {
                openocd.target_config = target;
            }
        }
        // Keep the interface empty unless the project already saved one. The
        // flash dialog offers a probe dropdown and an auto-detect button;
        // silently defaulting to ST-Link breaks DAPLink/J-Link users.
    }

    fn apply_pyocd_defaults(&self, pyocd: &mut PyOcdConfig) {
        if pyocd.executable.trim().is_empty() {
            pyocd.executable = "pyocd".to_owned();
        }
        if pyocd.program_file.trim().is_empty() {
            pyocd.program_file = "${ExecutableName}.hex".to_owned();
        }
        if pyocd.target.trim().is_empty() {
            if let Some(target) = flash::infer_pyocd_target(self.project_meta.device.as_deref()) {
                pyocd.target = target;
            }
        }
        if pyocd.pack.trim().is_empty() {
            if let Some(pack) = self.support_packs.iter().find(|pack| {
                pack.path
                    .extension()
                    .is_some_and(|extension| extension.eq_ignore_ascii_case("pack"))
            }) {
                pyocd.pack = pack.file_name.clone();
            }
        }
    }

    fn open_flash_config(&mut self) {
        let mut draft = FlashDraft::from_config(&self.build_config);
        match draft.tool {
            FlashTool::OpenOcd => self.apply_openocd_defaults(&mut draft.openocd),
            FlashTool::PyOcd => self.apply_pyocd_defaults(&mut draft.pyocd),
            FlashTool::Custom => {}
        }
        self.flash_draft = Some(draft);
    }

    fn set_flash_tool(&mut self, tool: FlashTool) {
        self.build_config.flash_tool = tool;
        match tool {
            FlashTool::OpenOcd => {
                let mut openocd = self.build_config.openocd.clone();
                self.apply_openocd_defaults(&mut openocd);
                self.build_config.openocd = openocd;
            }
            FlashTool::PyOcd => {
                let mut pyocd = self.build_config.pyocd.clone();
                self.apply_pyocd_defaults(&mut pyocd);
                self.build_config.pyocd = pyocd;
            }
            FlashTool::Custom => {}
        }
        self.persist_build_config();
    }

    fn set_openocd_path(&mut self, path: String) {
        self.build_config.openocd.executable = path;
        self.persist_build_config();
    }

    fn browse_openocd(&mut self) {
        let mut dialog = rfd::FileDialog::new().set_title("选择 OpenOCD 可执行文件");
        if cfg!(target_os = "windows") {
            dialog = dialog.add_filter("OpenOCD", &["exe"]);
        }
        let current = self.build_config.openocd.executable.trim();
        if let Some(parent) = Path::new(current)
            .parent()
            .filter(|parent| !parent.as_os_str().is_empty() && parent.is_dir())
        {
            dialog = dialog.set_directory(parent);
        }
        if let Some(path) = dialog.pick_file() {
            self.set_openocd_path(util::display_path(&path));
        }
    }

    fn set_pyocd_path(&mut self, path: String) {
        self.build_config.pyocd.executable = path;
        self.persist_build_config();
    }

    fn browse_pyocd(&mut self) {
        let mut dialog = rfd::FileDialog::new().set_title("选择 pyOCD 可执行文件");
        if cfg!(target_os = "windows") {
            dialog = dialog.add_filter("pyOCD", &["exe"]);
        }
        let current = self.build_config.pyocd.executable.trim();
        if let Some(parent) = Path::new(current)
            .parent()
            .filter(|parent| !parent.as_os_str().is_empty() && parent.is_dir())
        {
            dialog = dialog.set_directory(parent);
        }
        if let Some(path) = dialog.pick_file() {
            self.set_pyocd_path(util::display_path(&path));
        }
    }

    fn save_flash_config(&mut self, draft: FlashDraft) {
        self.build_config.flash_tool = draft.tool;
        self.build_config.flash_command = draft.command.trim().to_owned();
        self.build_config.openocd = draft.openocd.normalized();
        self.build_config.pyocd = draft.pyocd.normalized();
        self.persist_build_config();
    }

    fn refresh_memory_layout(&mut self) {
        let Some(root) = self.project.as_ref().map(|project| project.root.clone()) else {
            self.memory_layout = None;
            return;
        };
        self.memory_layout = crate::memory::resolve(
            &root,
            &self.project_meta,
            &self.support_packs,
            &self.build_config.memory,
        );
    }

    fn open_memory_layout(&mut self) {
        self.refresh_memory_layout();
        let auto = self.project.as_ref().map(|project| {
            crate::memory::resolve(
                &project.root,
                &self.project_meta,
                &self.support_packs,
                &crate::config::MemoryConfig::default(),
            )
        });
        let auto = auto.flatten();
        let current = &self.build_config.memory;
        let mut regions = if current.regions.is_empty() {
            crate::memory::default_regions(auto.as_ref().map(|memory| memory.layout))
        } else {
            crate::memory::normalize_regions(current.regions.clone())
        };
        if current.regions.is_empty() {
            if let Some(start) = crate::memory::parse_number(&current.ram_start) {
                if let Some(region) = regions.iter_mut().find(|region| region.name == "IRAM1") {
                    region.start = crate::memory::format_hex(start);
                }
            }
            if let Some(size) = crate::memory::parse_number(&current.ram_size) {
                if let Some(region) = regions.iter_mut().find(|region| region.name == "IRAM1") {
                    region.size = crate::memory::format_capacity(size);
                }
            }
            if let Some(start) = crate::memory::parse_number(&current.flash_start) {
                if let Some(region) = regions.iter_mut().find(|region| region.name == "IROM1") {
                    region.start = crate::memory::format_hex(start);
                }
            }
            if let Some(size) = crate::memory::parse_number(&current.flash_size) {
                if let Some(region) = regions.iter_mut().find(|region| region.name == "IROM1") {
                    region.size = crate::memory::format_capacity(size);
                }
            }
        }
        let flash_start = crate::memory::primary_start(&regions, crate::config::MemoryKind::Rom)
            .map(crate::memory::format_hex)
            .unwrap_or_default();
        let download_address = if current.download_address.trim().is_empty() {
            flash_start
        } else {
            current.download_address.trim().to_owned()
        };
        self.memory_draft = Some(MemoryDraft {
            regions,
            download_address,
            auto,
            restored: false,
            error: None,
        });
    }

    fn save_memory_layout(&mut self, mut draft: MemoryDraft) {
        let mut error = None;
        for region in &mut draft.regions {
            if !region.enabled {
                continue;
            }
            let Some(start) = crate::memory::parse_number(&region.start) else {
                error = Some(format!("{} 起始地址无效：{}", region.name, region.start));
                break;
            };
            let Some(size) = crate::memory::parse_number(&region.size) else {
                error = Some(format!("{} 大小无效：{}", region.name, region.size));
                break;
            };
            if size == 0 {
                error = Some(format!("{} 大小必须大于 0", region.name));
                break;
            }
            region.start = crate::memory::format_hex(start);
            region.size = crate::memory::format_capacity(size);
        }
        if error.is_none() {
            let ram_total =
                crate::memory::regions_total(&draft.regions, crate::config::MemoryKind::Ram);
            let rom_total =
                crate::memory::regions_total(&draft.regions, crate::config::MemoryKind::Rom);
            if ram_total == 0 {
                error = Some("至少需要启用一个 RAM 区域".to_owned());
            } else if rom_total == 0 {
                error = Some("至少需要启用一个 ROM / Flash 区域".to_owned());
            }
        }
        if error.is_none() {
            let mut has_startup = draft
                .regions
                .iter()
                .any(|region| region.enabled && region.startup);
            if !has_startup {
                if let Some(region) = draft
                    .regions
                    .iter_mut()
                    .find(|region| region.enabled && region.kind == crate::config::MemoryKind::Rom)
                {
                    region.startup = true;
                    has_startup = true;
                }
            }
            if !has_startup {
                error = Some("请选择一个 ROM 启动区域".to_owned());
            }
        }
        if let Some(error) = error {
            draft.error = Some(error);
            self.memory_draft = Some(draft);
            return;
        }

        let download = crate::memory::parse_number(&draft.download_address);
        if !draft.download_address.trim().is_empty() && download.is_none() {
            draft.error = Some("下载起始地址无效".to_owned());
            self.memory_draft = Some(draft);
            return;
        }

        let ram_total =
            crate::memory::regions_total(&draft.regions, crate::config::MemoryKind::Ram);
        let rom_total =
            crate::memory::regions_total(&draft.regions, crate::config::MemoryKind::Rom);
        let ram_start =
            crate::memory::primary_start(&draft.regions, crate::config::MemoryKind::Ram)
                .map(crate::memory::format_hex)
                .unwrap_or_default();
        let flash_start =
            crate::memory::primary_start(&draft.regions, crate::config::MemoryKind::Rom)
                .map(crate::memory::format_hex)
                .unwrap_or_default();
        let config = crate::config::MemoryConfig {
            regions: draft.regions.clone(),
            ram_size: crate::memory::format_capacity(ram_total),
            flash_size: crate::memory::format_capacity(rom_total),
            ram_start,
            flash_start,
            download_address: download.map(crate::memory::format_hex).unwrap_or_default(),
        };
        self.build_config.memory = config;
        self.persist_build_config();
        self.refresh_memory_layout();
        self.memory_draft = None;
        self.set_toast("内存布局已保存", Severity::Success);
    }

    fn ensure_firmware_analysis(&mut self, force: bool) {
        if self.effective_toolchain().is_micropython() {
            self.firmware_view.path = None;
            self.firmware_view.modified = None;
            self.firmware_view.analysis = None;
            self.firmware_view.error = Some("MicroPython 串口模式不使用 ELF 内存分析".to_owned());
            self.firmware_view.checked_at = Some(Instant::now());
            return;
        }
        if !force
            && self
                .firmware_view
                .checked_at
                .is_some_and(|checked| checked.elapsed() < Duration::from_millis(800))
        {
            return;
        }
        self.firmware_view.checked_at = Some(Instant::now());
        let Some(target) = self.debug_target() else {
            self.firmware_view.path = None;
            self.firmware_view.modified = None;
            self.firmware_view.analysis = None;
            self.firmware_view.error = Some("未找到固件文件；请先构建工程".to_owned());
            return;
        };
        let modified = std::fs::metadata(&target)
            .ok()
            .and_then(|metadata| metadata.modified().ok());
        if !force
            && self.firmware_view.path.as_ref() == Some(&target)
            && self.firmware_view.modified == modified
            && self.firmware_view.analysis.is_some()
        {
            return;
        }
        self.firmware_view.path = Some(target.clone());
        self.firmware_view.modified = modified;
        if !target.extension().is_some_and(|extension| {
            extension.eq_ignore_ascii_case("elf") || extension.eq_ignore_ascii_case("axf")
        }) {
            self.firmware_view.analysis = None;
            self.firmware_view.error = Some("内存可视化需要 ELF/AXF 文件".to_owned());
            return;
        }
        match crate::memory::analyze_elf(&target) {
            Some(analysis) => {
                self.firmware_view.analysis = Some(analysis);
                self.firmware_view.error = None;
            }
            None => {
                self.firmware_view.analysis = None;
                self.firmware_view.error = Some("无法解析 ELF 符号表".to_owned());
            }
        }
    }

    fn build_memory_usage(&mut self) -> Option<crate::memory::MemoryUsage> {
        if self.build.is_running() {
            return None;
        }
        let len = self.build.log.len();
        let start = len.saturating_sub(200);
        let mut text = String::new();
        for index in start..len {
            if let Some(line) = self.build.log.line_at(index) {
                text.push_str(&line.plain);
                text.push('\n');
            }
        }
        if let Some(usage) = crate::memory::parse_size_output(&text) {
            return Some(usage);
        }

        let target = self.debug_target()?;
        let candidates = [
            target.clone(),
            target.with_extension("elf"),
            target.with_extension("axf"),
        ];
        for candidate in candidates {
            let Ok(metadata) = std::fs::metadata(&candidate) else {
                continue;
            };
            let Ok(modified) = metadata.modified() else {
                continue;
            };
            if let Some((path, time, usage)) = &self.memory_usage_cache {
                if same_path(path, &candidate) && *time == modified {
                    return *usage;
                }
            }
            let usage = crate::memory::usage_from_elf(&candidate);
            if usage.is_some() {
                self.memory_usage_cache = Some((candidate, modified, usage));
                return usage;
            }
        }
        None
    }

    fn memory_usage_summary(&mut self) -> Option<String> {
        let usage = self.build_memory_usage()?;
        Some(crate::memory::format_usage(
            usage,
            self.memory_layout.as_ref().map(|memory| memory.layout),
        ))
    }

    fn memory_layout_summary(&self) -> Option<String> {
        let resolved = self.memory_layout.as_ref()?;
        let layout = resolved.layout;
        let mut parts = Vec::new();
        if layout.flash.is_valid() {
            parts.push(format!(
                "Flash {} @ {}",
                crate::memory::format_size(layout.flash.size),
                crate::memory::format_hex(layout.flash.start)
            ));
        }
        if layout.ram.is_valid() {
            parts.push(format!(
                "RAM {} @ {}",
                crate::memory::format_size(layout.ram.size),
                crate::memory::format_hex(layout.ram.start)
            ));
        }
        (!parts.is_empty()).then(|| format!("{}（{}）", parts.join(" · "), resolved.source))
    }

    fn open_build_options(&mut self) {
        let Some(root) = self.project.as_ref().map(|project| project.root.clone()) else {
            return;
        };
        let mut options = self.build_config.build_options.clone();
        if let Some(makefile) = makefile::find_makefile(&root) {
            if let Some(parsed) = makefile::read_options(&makefile, &options) {
                options = parsed;
            }
        }
        self.build_config.build_options = options.clone();
        self.build_options_draft = Some(options);
    }

    fn save_build_options(&mut self, options: BuildOptions) {
        let Some(root) = self.project.as_ref().map(|project| project.root.clone()) else {
            return;
        };
        let fallback = self.build_config.build_options.clone();
        let make_project = self.build_config.effective_build_system() == BuildSystem::Make
            && self.effective_toolchain().is_make_gcc_like();
        let before = if make_project {
            makefile::find_makefile(&root)
                .and_then(|path| makefile::read_options(&path, &fallback))
                .unwrap_or_else(|| fallback.clone())
        } else {
            fallback.clone()
        };
        self.build_config.build_options = options.clone();
        self.persist_build_config();

        let result = if make_project {
            self.sync_makefile_options(&root, &before, &options)
        } else {
            Ok(false)
        };
        self.notify_sync("已保存构建选项", result, Severity::Success);
        self.refresh_project_meta();
    }

    fn sync_makefile_options(
        &self,
        root: &Path,
        before: &BuildOptions,
        after: &BuildOptions,
    ) -> Result<bool, String> {
        let Some(makefile) = makefile::find_makefile(root) else {
            return Ok(false);
        };
        if self
            .tabs
            .iter()
            .any(|document| same_path(&document.path, &makefile))
        {
            return Err("Makefile 已在编辑器中打开，请先关闭后再同步".to_owned());
        }
        Ok(makefile::sync_options(root, before, after)?.is_some())
    }

    fn notify_sync(&mut self, message: &str, result: Result<bool, String>, severity: Severity) {
        match result {
            Ok(true) => self.set_toast(format!("{message}，已同步到 Makefile"), severity),
            Ok(false) => self.set_toast(message.to_owned(), severity),
            Err(err) => self.set_toast(
                format!("{message}，但同步 Makefile 失败：{err}"),
                Severity::Warning,
            ),
        }
    }

    fn set_toast(&mut self, message: impl Into<String>, severity: Severity) {
        self.toast = Some(Toast {
            message: message.into(),
            severity,
            born: Instant::now(),
        });
    }

    /// Apply the OS-level fullscreen/topmost state that goes with the
    /// immersive mode. Borderless fullscreen removes both the title bar and the
    /// taskbar; the topmost level keeps the taskbar from drawing over it.
    fn apply_immersive_viewport(&self, ctx: &egui::Context) {
        ctx.send_viewport_cmd(ViewportCommand::Fullscreen(self.immersive));
        ctx.send_viewport_cmd(ViewportCommand::WindowLevel(if self.immersive {
            egui::WindowLevel::AlwaysOnTop
        } else {
            egui::WindowLevel::Normal
        }));
    }

    fn handle_shortcuts(&mut self, ctx: &egui::Context) {
        let modal_open = self.has_modal();
        if modal_open {
            if ctx.input(|input| input.key_pressed(egui::Key::Escape)) {
                self.settings_draft = None;
                self.cancel_theme_settings(ctx);
                self.firmware_confirm = false;
                self.pending = None;
                self.meta_draft = None;
                self.meta_add = None;
                self.flash_draft = None;
                self.build_options_draft = None;
                self.create_entry = None;
                self.rename_entry = None;
                self.delete_entry = None;
                self.memory_draft = None;
                self.ai_settings_draft = None;
            }
            return;
        }

        let ai_toggle = ctx.input_mut(|input| {
            input.consume_key(egui::Modifiers::CTRL | egui::Modifiers::ALT, egui::Key::A)
        });
        if ai_toggle {
            if self.ai.enabled {
                self.exit_ai_mode();
            } else {
                self.enter_ai_mode();
            }
            return;
        }
        if self.ai.enabled {
            return;
        }

        let editor_focused = self.active_document().is_some_and(|doc| {
            let editor_id = egui::Id::new(("simple_embedded_code_editor", &doc.path));
            let folded_id = egui::Id::new(("simple_embedded_folded_editor", &doc.path));
            ctx.memory(|memory| memory.has_focus(editor_id) || memory.has_focus(folded_id))
        });
        let undo_pressed = editor_focused
            && ctx.input_mut(|input| input.consume_key(egui::Modifiers::COMMAND, egui::Key::Z));
        let redo_pressed = editor_focused
            && ctx.input_mut(|input| {
                input.consume_key(
                    egui::Modifiers::COMMAND | egui::Modifiers::SHIFT,
                    egui::Key::Z,
                ) || input.consume_key(egui::Modifiers::COMMAND, egui::Key::Y)
            });
        if undo_pressed {
            self.undo_last(ctx);
            return;
        }
        if redo_pressed {
            self.redo_last(ctx);
            return;
        }

        let fold_pressed = editor_focused
            && ctx.input_mut(|input| {
                input.consume_key(
                    egui::Modifiers::COMMAND | egui::Modifiers::SHIFT,
                    egui::Key::OpenBracket,
                ) || input.consume_key(
                    egui::Modifiers::COMMAND | egui::Modifiers::SHIFT,
                    egui::Key::OpenCurlyBracket,
                )
            });
        let unfold_pressed = editor_focused
            && ctx.input_mut(|input| {
                input.consume_key(
                    egui::Modifiers::COMMAND | egui::Modifiers::SHIFT,
                    egui::Key::CloseBracket,
                ) || input.consume_key(
                    egui::Modifiers::COMMAND | egui::Modifiers::SHIFT,
                    egui::Key::CloseCurlyBracket,
                )
            });
        if fold_pressed {
            self.fold_current(ctx);
            return;
        }
        if unfold_pressed {
            self.unfold_active(ctx);
            return;
        }

        let toggle_immersive = ctx.input_mut(|input| {
            input.consume_key(
                egui::Modifiers::COMMAND | egui::Modifiers::SHIFT,
                egui::Key::Enter,
            )
        });
        if toggle_immersive {
            self.immersive = !self.immersive;
            self.focus_editor = true;
            self.find.open = false;
            self.completion.open = false;
            self.goto_line.open = false;
            self.apply_immersive_viewport(ctx);
            ctx.request_repaint();
        }

        if self
            .chord_k_at
            .is_some_and(|at| at.elapsed() > Duration::from_millis(1500))
        {
            self.chord_k_at = None;
        }
        let chord_active = self.chord_k_at.is_some();
        let (
            open,
            save,
            save_all,
            save_as,
            copy_path,
            copy_relative,
            reveal_file_manager,
            build,
            rebuild,
            close_tab,
            open_settings,
            zoom_in,
            zoom_out,
            bookmark_toggle,
            bookmark_next,
            bookmark_prev,
            lsp_rename,
            bracket_jump,
            find_open,
            find_replace,
            find_next,
            find_prev,
            project_search_open,
            format_document,
            git_open,
            todo_open,
            symbols_open,
            hex_open,
            array_open,
            registers_open,
            debug_continue,
            debug_stop,
            debug_next,
            debug_step,
            debug_finish,
            breakpoint_toggle,
            definition_jump,
            references_jump,
            completion_open,
            close_tab_f4,
            split_right,
            chord_k,
            chord_close_saved,
            chord_close_all,
            chord_split_up,
            split_in_group,
            goto_line_open,
            memory_layout_open,
        ) = ctx.input(|input| {
            let command = input.modifiers.command;
            (
                command && !input.modifiers.shift && input.key_pressed(egui::Key::O),
                command && !input.modifiers.shift && input.key_pressed(egui::Key::S),
                chord_active && input.key_pressed(egui::Key::S),
                command && input.modifiers.shift && input.key_pressed(egui::Key::S),
                input.modifiers.shift && input.modifiers.alt && input.key_pressed(egui::Key::C),
                chord_active && command && input.modifiers.shift && input.key_pressed(egui::Key::C),
                input.modifiers.shift && input.modifiers.alt && input.key_pressed(egui::Key::R),
                command && !input.modifiers.shift && input.key_pressed(egui::Key::B),
                command && input.modifiers.shift && input.key_pressed(egui::Key::B),
                command && input.key_pressed(egui::Key::W),
                command && input.key_pressed(egui::Key::Comma),
                command
                    && (input.key_pressed(egui::Key::Plus) || input.key_pressed(egui::Key::Equals)),
                command && input.key_pressed(egui::Key::Minus),
                command && !input.modifiers.shift && input.key_pressed(egui::Key::F2),
                !command && !input.modifiers.shift && input.key_pressed(egui::Key::F2),
                !command && input.modifiers.shift && input.key_pressed(egui::Key::F2),
                command && input.modifiers.shift && input.key_pressed(egui::Key::F2),
                command && input.key_pressed(egui::Key::CloseBracket),
                command && !input.modifiers.shift && input.key_pressed(egui::Key::F),
                command && !input.modifiers.shift && input.key_pressed(egui::Key::H),
                input.key_pressed(egui::Key::F3) && !input.modifiers.shift,
                input.key_pressed(egui::Key::F3) && input.modifiers.shift,
                command && input.modifiers.shift && input.key_pressed(egui::Key::F),
                command && input.modifiers.shift && input.key_pressed(egui::Key::I),
                command && input.modifiers.shift && input.key_pressed(egui::Key::G),
                command && input.modifiers.shift && input.key_pressed(egui::Key::T),
                command && input.modifiers.shift && input.key_pressed(egui::Key::O),
                command && input.modifiers.shift && input.key_pressed(egui::Key::H),
                command && input.modifiers.shift && input.key_pressed(egui::Key::A),
                command && input.modifiers.shift && input.key_pressed(egui::Key::R),
                input.key_pressed(egui::Key::F5) && !input.modifiers.shift,
                input.key_pressed(egui::Key::F5) && input.modifiers.shift,
                input.key_pressed(egui::Key::F10),
                input.key_pressed(egui::Key::F11) && !input.modifiers.shift,
                input.key_pressed(egui::Key::F11) && input.modifiers.shift,
                input.key_pressed(egui::Key::F9),
                input.key_pressed(egui::Key::F12) && !input.modifiers.shift,
                input.key_pressed(egui::Key::F12) && input.modifiers.shift,
                command && input.key_pressed(egui::Key::Space),
                command && input.key_pressed(egui::Key::F4),
                command && input.key_pressed(egui::Key::Backslash),
                command && input.key_pressed(egui::Key::K),
                chord_active && input.key_pressed(egui::Key::U),
                chord_active && input.key_pressed(egui::Key::W),
                chord_active
                    && command
                    && !input.modifiers.shift
                    && input.key_pressed(egui::Key::Backslash),
                chord_active
                    && command
                    && input.modifiers.shift
                    && input.key_pressed(egui::Key::Backslash),
                command && !input.modifiers.shift && input.key_pressed(egui::Key::G),
                command && input.modifiers.shift && input.key_pressed(egui::Key::M),
            )
        });

        if open {
            self.pick_project_folder();
        }
        if save_as {
            if let Some(index) = self.active {
                self.save_tab_as(index, ctx);
            }
        }
        if copy_path {
            let text = self
                .active_document()
                .map(|doc| util::display_path(&doc.path));
            if let Some(text) = text {
                ctx.copy_text(text);
                self.set_toast("已复制完整路径", Severity::Info);
            }
        }
        if copy_relative {
            let text = self
                .active_document()
                .map(|doc| doc.rel_path.replace('/', std::path::MAIN_SEPARATOR_STR));
            if let Some(text) = text {
                ctx.copy_text(text);
                self.set_toast("已复制相对路径", Severity::Info);
            }
        }
        if reveal_file_manager {
            let path = self.active_document().map(|doc| doc.path.clone());
            if let Some(path) = path {
                util::open_in_file_manager(&path);
            }
        }
        if save_all {
            self.save_all_with_toast();
        } else if save {
            self.save_active();
        }
        if rebuild {
            self.start_task(TaskKind::Rebuild);
        } else if build {
            self.start_task(TaskKind::Build);
        }
        if close_tab {
            if let Some(index) = self.active {
                self.request_close_tab(index);
            }
        }
        if open_settings {
            if self.ai.enabled {
                self.open_ai_settings();
            } else {
                self.open_settings();
            }
        }
        if bookmark_toggle {
            self.toggle_bookmark();
        }
        if bookmark_next {
            self.goto_bookmark(ctx, true);
        } else if bookmark_prev {
            self.goto_bookmark(ctx, false);
        }
        if lsp_rename {
            self.open_lsp_rename();
        }
        if bracket_jump {
            self.jump_to_matching_bracket(ctx);
        }
        if find_open {
            self.find.open = true;
            self.find.replace_open = false;
            self.find.focus_query = true;
        }
        if find_replace {
            ctx.input_mut(|input| {
                input.consume_key(egui::Modifiers::COMMAND, egui::Key::H);
            });
            self.find.open = true;
            self.find.replace_open = true;
            self.find.focus_query = true;
        }
        if find_next {
            self.find_next(ctx, true);
        } else if find_prev {
            self.find_next(ctx, false);
        }
        if project_search_open {
            self.project_search.open = true;
            self.project_search.focus_query = true;
            if self.project_search.query.is_empty() && !self.find.query.is_empty() {
                self.project_search.query = self.find.query.clone();
                self.project_search.case_sensitive = self.find.case_sensitive;
                self.project_search.use_regex = self.find.use_regex;
            }
        }
        if git_open {
            self.git_open = true;
            self.refresh_git_status();
        }
        if todo_open {
            self.todo.open = true;
            self.refresh_todos();
        }
        if symbols_open {
            self.symbols.open = true;
            self.refresh_symbols();
        }
        if hex_open {
            if let Some(doc) = self.active_document() {
                let path = doc.path.clone();
                self.open_hex(&path);
            }
        }
        if array_open {
            self.open_array_generator(ctx);
        }
        if registers_open {
            self.registers.open = true;
            self.ensure_registers_loaded();
        }
        if debug_continue {
            self.debug_continue();
        } else if debug_stop {
            self.debug_stop();
        } else if debug_next {
            self.debug_step_command("next");
        } else if debug_step {
            self.debug_step_command("step");
        } else if debug_finish {
            self.debug_step_command("finish");
        }
        if breakpoint_toggle {
            self.toggle_breakpoint();
        }
        if definition_jump {
            self.definition_at_cursor(ctx);
        } else if references_jump {
            self.references_at_cursor();
        }
        if completion_open {
            self.open_completion();
        }
        if goto_line_open {
            if self.active_document().is_some() {
                self.goto_line.open = true;
                self.goto_line.input.clear();
                self.goto_line.error = None;
                self.goto_line.focus_input = true;
                self.find.open = false;
            } else {
                self.set_toast("请先打开一个文件", Severity::Info);
            }
        }
        if memory_layout_open {
            self.open_memory_layout();
        }
        if chord_k {
            self.chord_k_at = Some(Instant::now());
        }
        if chord_close_saved {
            self.chord_k_at = None;
            self.close_saved_tabs();
        } else if chord_close_all {
            self.chord_k_at = None;
            self.request_close_all();
        } else if chord_split_up {
            self.chord_k_at = None;
            self.split_active(SplitDirection::Up);
        } else if split_in_group {
            self.chord_k_at = None;
            self.split_active(SplitDirection::Right);
        }
        if close_tab_f4 {
            if let Some(index) = self.active {
                self.request_close_tab(index);
            }
        }
        if split_right && !chord_split_up && !split_in_group {
            self.split_active(SplitDirection::Right);
        }
        if format_document {
            self.format_current_document();
        }
        if ctx.input(|input| input.key_pressed(egui::Key::Escape)) {
            if let Some(active) = self.active {
                if let Some(doc) = self.tabs.get_mut(active) {
                    doc.folds.clear();
                }
            }
        }
        if self.find.open && ctx.input(|input| input.key_pressed(egui::Key::Escape)) {
            self.find.open = false;
            self.focus_editor = true;
        }
        if self.project_search.open && ctx.input(|input| input.key_pressed(egui::Key::Escape)) {
            self.project_search.open = false;
            self.focus_editor = true;
        }
        if self.git_open && ctx.input(|input| input.key_pressed(egui::Key::Escape)) {
            self.git_open = false;
            self.focus_editor = true;
        }
        if self.todo.open && ctx.input(|input| input.key_pressed(egui::Key::Escape)) {
            self.todo.open = false;
            self.focus_editor = true;
        }
        if self.symbols.open && ctx.input(|input| input.key_pressed(egui::Key::Escape)) {
            self.symbols.open = false;
            self.focus_editor = true;
        }
        if self.hex.open && ctx.input(|input| input.key_pressed(egui::Key::Escape)) {
            self.hex.open = false;
            self.focus_editor = true;
        }
        if self.registers.open && ctx.input(|input| input.key_pressed(egui::Key::Escape)) {
            self.registers.open = false;
            self.focus_editor = true;
        }
        if self.memory_check.open && ctx.input(|input| input.key_pressed(egui::Key::Escape)) {
            self.memory_check.open = false;
            self.focus_editor = true;
        }
        if self.array.open && ctx.input(|input| input.key_pressed(egui::Key::Escape)) {
            self.array.open = false;
            self.focus_editor = true;
        }
        if self.debug.open && ctx.input(|input| input.key_pressed(egui::Key::Escape)) {
            self.debug.open = false;
        }
        if self.goto_line.open && ctx.input(|input| input.key_pressed(egui::Key::Escape)) {
            self.goto_line.open = false;
            self.focus_editor = true;
        }
        if (self.column_selection.is_some() || self.multi_cursor.is_some())
            && ctx.input(|input| input.key_pressed(egui::Key::Escape))
        {
            self.column_selection = None;
            self.multi_cursor = None;
        }
        if zoom_in || zoom_out {
            let delta = if zoom_in { 1.0 } else { -1.0 };
            self.settings.font_size = (self.settings.font_size + delta).clamp(10.0, 28.0);
            self.settings.save();
        }
    }

    fn handle_close_request(&mut self, ctx: &egui::Context) {
        if !ctx.input(|input| input.viewport().close_requested()) || self.allow_close {
            return;
        }
        if self.has_unsaved() {
            ctx.send_viewport_cmd(ViewportCommand::CancelClose);
            if self.pending.is_none() {
                self.pending = Some(PendingDialog {
                    action: PendingAction::CloseWindow,
                    error: None,
                });
            }
        }
    }

    fn handle_dropped_files(&mut self, ctx: &egui::Context) {
        if self.settings_draft.is_some() || self.pending.is_some() {
            return;
        }
        let dropped: Vec<PathBuf> = ctx.input(|input| {
            input
                .raw
                .dropped_files
                .iter()
                .filter_map(|file| file.path.clone())
                .collect()
        });
        for path in dropped {
            if path.is_dir() || (path.is_file() && crate::config::is_workspace_file(&path)) {
                self.request_open_project(path);
                break;
            }
            if path.is_file() {
                if self.project.is_none() {
                    if let Some(parent) = path.parent() {
                        if !self.load_project(parent, true) {
                            continue;
                        }
                    }
                }
                self.open_file(path);
                break;
            }
        }
    }

    fn pick_project_folder(&mut self) {
        let mut dialog = rfd::FileDialog::new().set_title("打开工程文件夹");
        if let Some(project) = &self.project {
            dialog = dialog.set_directory(&project.root);
        } else if let Some(recent) = self.settings.recent_projects.first() {
            dialog = dialog.set_directory(recent);
        }
        if let Some(path) = dialog.pick_folder() {
            self.request_open_project(path);
        }
    }

    fn pick_workspace_file(&mut self) {
        let mut dialog = rfd::FileDialog::new()
            .set_title("打开工作区文件")
            .add_filter(
                "Simple embedded 工作区",
                &["se-workspace", "code-workspace"],
            );
        if let Some(project) = &self.project {
            dialog = dialog.set_directory(&project.root);
        } else if let Some(recent) = self.settings.recent_projects.first() {
            dialog = dialog.set_directory(recent);
        }
        if let Some(path) = dialog.pick_file() {
            self.request_open_project(path);
        }
    }

    fn create_workspace(&mut self) {
        let Some(root) = self.project.as_ref().map(|project| project.root.clone()) else {
            self.set_toast("请先打开工程", Severity::Warning);
            return;
        };
        if let Err(err) =
            crate::config::ProjectSettings::save(&root, &self.build_config, &self.project_debug)
        {
            self.set_toast(format!("工程配置保存失败：{err}"), Severity::Error);
            return;
        }
        match crate::config::write_workspace_file(&root) {
            Ok(path) => {
                let display = util::display_path(&path);
                if let Some(tree) = &mut self.tree {
                    tree.refresh();
                }
                match crate::config::register_workspace_association() {
                    Ok(_) => self.set_toast(
                        format!("已生成 {display}，双击即可打开工程"),
                        Severity::Success,
                    ),
                    Err(err) => self.set_toast(
                        format!("已生成 {display}，但关联双击失败：{err}"),
                        Severity::Warning,
                    ),
                }
            }
            Err(err) => self.set_toast(err, Severity::Error),
        }
    }

    fn open_project_config(&mut self) {
        let Some(root) = self.project.as_ref().map(|project| project.root.clone()) else {
            self.set_toast("请先打开工程", Severity::Warning);
            return;
        };
        let path = crate::config::ProjectSettings::path(&root);
        if !crate::config::ProjectSettings::exists(&root) {
            if let Err(err) =
                crate::config::ProjectSettings::save(&root, &self.build_config, &self.project_debug)
            {
                self.set_toast(format!("工程配置保存失败：{err}"), Severity::Error);
                return;
            }
        }
        self.open_file(path);
    }

    fn open_create_entry(&mut self, dir: PathBuf, kind: CreateKind) {
        self.create_entry = Some(CreateEntryDraft {
            kind,
            dir,
            name: String::new(),
            error: None,
            focus: true,
        });
    }

    fn open_to_side(&mut self, path: PathBuf) {
        let other = self
            .active_document()
            .map(|doc| doc.path.clone())
            .filter(|current| !same_path(current, &path));
        self.open_file(path.clone());
        let Some(other) = other else {
            self.set_toast(
                format!("已打开 {}；再打开一个文件即可分屏", util::file_name(&path)),
                Severity::Info,
            );
            return;
        };
        if self.split_path.is_some() {
            self.split_primary = Some(other);
            self.split_path = Some(path.clone());
            self.split_direction = SplitDirection::Right;
            self.split_focus = 1;
            self.active = self.tabs.iter().position(|doc| same_path(&doc.path, &path));
            self.set_toast(
                format!("已在侧边打开 {}", util::file_name(&path)),
                Severity::Info,
            );
        } else {
            self.start_split(other, path, SplitDirection::Right, 1);
        }
    }

    fn set_file_clipboard(&mut self, path: PathBuf, cut: bool) {
        if !path.exists() {
            self.set_toast("文件已不存在", Severity::Warning);
            return;
        }
        self.file_clipboard = Some(FileClipboard { path, cut });
        self.set_toast(
            if cut {
                "已剪切，选择目标文件夹后粘贴"
            } else {
                "已复制，选择目标文件夹后粘贴"
            },
            Severity::Info,
        );
    }

    fn paste_file(&mut self, target: PathBuf) {
        let Some(clipboard) = self.file_clipboard.clone() else {
            self.set_toast("剪贴板为空", Severity::Info);
            return;
        };
        if !clipboard.path.exists() {
            self.file_clipboard = None;
            self.set_toast("剪贴板中的文件已不存在", Severity::Warning);
            return;
        }
        let directory = if target.is_dir() {
            target
        } else {
            target
                .parent()
                .map(Path::to_path_buf)
                .unwrap_or_else(|| clipboard.path.clone())
        };
        let source_name = util::file_name(&clipboard.path);
        if clipboard.path.is_dir()
            && (same_path(&directory, &clipboard.path) || directory.starts_with(&clipboard.path))
        {
            self.set_toast("不能把文件夹粘贴到它自己或子文件夹中", Severity::Warning);
            return;
        }
        let mut destination = directory.join(&source_name);
        if same_path(&destination, &clipboard.path) {
            self.set_toast("文件已在目标文件夹中", Severity::Info);
            return;
        }
        if destination.exists() {
            if clipboard.cut {
                self.set_toast(
                    format!("目标位置已存在 {source_name}，请先重命名或删除"),
                    Severity::Warning,
                );
                return;
            }
            destination = unique_copy_path(&directory, &clipboard.path);
        }
        let result = if clipboard.cut {
            match std::fs::rename(&clipboard.path, &destination) {
                Ok(()) => Ok(()),
                Err(_) => copy_recursive(&clipboard.path, &destination)
                    .and_then(|()| remove_path(&clipboard.path)),
            }
        } else {
            copy_recursive(&clipboard.path, &destination)
        };
        if let Err(err) = result {
            self.set_toast(
                format!(
                    "无法{} {}：{err}",
                    if clipboard.cut { "移动" } else { "复制" },
                    util::display_path(&clipboard.path)
                ),
                Severity::Error,
            );
            return;
        }
        if clipboard.cut {
            self.file_clipboard = None;
            self.rewrite_paths(&clipboard.path, &destination);
        }
        if let Some(tree) = &mut self.tree {
            tree.refresh();
            tree.reveal(&destination);
        }
        if destination.is_dir() {
            self.member_index = None;
        }
        if clipboard.cut || destination.is_dir() || crate::formatting::is_c_family(&destination) {
            self.mark_relations_dirty();
        }
        self.refresh_support_packs();
        self.refresh_git_status();
        self.set_toast(
            format!(
                "已{}到 {}",
                if clipboard.cut { "移动" } else { "复制" },
                util::display_path(&directory)
            ),
            Severity::Success,
        );
    }

    fn start_rename(&mut self, path: PathBuf) {
        let name = util::file_name(&path);
        self.rename_entry = Some(RenameEntryDraft {
            path,
            name,
            error: None,
            focus: true,
        });
    }

    fn commit_rename(&mut self, mut draft: RenameEntryDraft) {
        let name = draft.name.trim();
        if name.is_empty() {
            draft.error = Some("名称不能为空".to_owned());
            self.rename_entry = Some(draft);
            return;
        }
        if name == "."
            || name == ".."
            || name.contains('/')
            || name.contains('\\')
            || name.contains(':')
        {
            draft.error = Some("名称不能包含路径分隔符".to_owned());
            self.rename_entry = Some(draft);
            return;
        }
        let Some(parent) = draft.path.parent().map(Path::to_path_buf) else {
            draft.error = Some("无法确定上级目录".to_owned());
            self.rename_entry = Some(draft);
            return;
        };
        let destination = parent.join(name);
        if same_path(&destination, &draft.path) {
            self.rename_entry = None;
            return;
        }
        if destination.exists() {
            draft.error = Some(format!("{} 已存在", util::display_path(&destination)));
            self.rename_entry = Some(draft);
            return;
        }
        if let Err(err) = std::fs::rename(&draft.path, &destination) {
            draft.error = Some(format!("重命名失败：{err}"));
            self.rename_entry = Some(draft);
            return;
        }
        self.rewrite_paths(&draft.path, &destination);
        if let Some(tree) = &mut self.tree {
            tree.refresh();
            tree.reveal(&destination);
        }
        self.member_index = None;
        if destination.is_dir() || crate::formatting::is_c_family(&destination) {
            self.mark_relations_dirty();
        }
        self.refresh_support_packs();
        self.refresh_git_status();
        self.rename_entry = None;
        self.set_toast(format!("已重命名为 {name}"), Severity::Success);
    }

    fn start_delete(&mut self, path: PathBuf) {
        let is_dir = path.is_dir();
        self.delete_entry = Some(DeleteEntryDraft { path, is_dir });
    }

    /// The confirmation modal takes the draft out of `delete_entry`, so the
    /// draft is passed back explicitly instead of being read from `self`.
    fn commit_delete(&mut self, draft: DeleteEntryDraft) {
        if path_has_dirty_tab(&self.tabs, &draft.path) {
            self.set_toast("请先保存或关闭该文件，再删除", Severity::Warning);
            return;
        }
        let is_dir = draft.path.is_dir();
        if let Err(err) = remove_path(&draft.path) {
            self.set_toast(
                format!("无法删除 {}：{err}", util::display_path(&draft.path)),
                Severity::Error,
            );
            return;
        }
        let mut sync_note = String::new();
        if let Some(root) = self.project.as_ref().map(|project| project.root.clone()) {
            let makefile_path = makefile::find_makefile(&root);
            let makefile_dirty = makefile_path.as_ref().is_some_and(|path| {
                self.tabs
                    .iter()
                    .any(|doc| same_path(&doc.path, path) && doc.is_dirty())
            });
            if makefile_dirty {
                sync_note = "；Makefile 有未保存修改，未同步 C_SOURCES".to_owned();
            } else {
                match makefile::remove_sources(&root, &draft.path, is_dir) {
                    Ok(Some(changed)) => {
                        if let Some(doc) = self
                            .tabs
                            .iter_mut()
                            .find(|doc| same_path(&doc.path, &changed))
                        {
                            if let Ok(loaded) = crate::util::read_text_file(&changed) {
                                let cursor = doc.cursor_index.min(loaded.text.chars().count());
                                doc.buffer.set_text(loaded.text);
                                doc.update_cursor(cursor);
                                doc.saved_revision = doc.buffer.revision();
                                doc.folds.clear();
                            }
                        }
                        sync_note = "；已从 Makefile C_SOURCES 移除".to_owned();
                    }
                    Ok(None) => {}
                    Err(err) => sync_note = format!("；更新 C_SOURCES 失败：{err}"),
                }
            }
        }
        self.remove_path_references(&draft.path);
        if let Some(tree) = &mut self.tree {
            tree.refresh();
        }
        self.member_index = None;
        if is_dir || crate::formatting::is_c_family(&draft.path) {
            self.mark_relations_dirty();
        }
        self.refresh_support_packs();
        self.refresh_git_status();
        self.set_toast(
            format!("已删除 {}{sync_note}", util::file_name(&draft.path)),
            Severity::Success,
        );
    }

    /// Keep open tabs, split panes, breakpoints and the debug marker pointing
    /// at the new location after a rename or cut/paste.
    fn rewrite_paths(&mut self, old: &Path, new: &Path) {
        let root = self.project.as_ref().map(|project| project.root.clone());
        for doc in &mut self.tabs {
            if !same_path(&doc.path, old) && !doc.path.starts_with(old) {
                continue;
            }
            let path = remap_path(&doc.path, old, new);
            doc.path = path.clone();
            doc.name = util::file_name(&path);
            doc.rel_path = root
                .as_deref()
                .map(|root| util::relative_display(root, &path))
                .unwrap_or_else(|| util::display_path(&path));
            doc.macro_issue_revision = u64::MAX;
        }
        let breakpoints = std::mem::take(&mut self.debug.breakpoints);
        for (path, line) in breakpoints {
            self.debug
                .breakpoints
                .insert((remap_path(&path, old, new), line));
        }
        if let Some((path, line)) = self.debug.current_source.clone() {
            self.debug.current_source = Some((remap_path(&path, old, new), line));
        }
        if let Some(path) = self.split_path.clone() {
            self.split_path = Some(remap_path(&path, old, new));
        }
        if let Some(path) = self.split_primary.clone() {
            self.split_primary = Some(remap_path(&path, old, new));
        }
        if let Some(preview) = &mut self.image_preview {
            if same_path(&preview.path, old) {
                preview.path = new.to_path_buf();
            }
        }
    }

    fn remove_path_references(&mut self, removed: &Path) {
        let active_path = self.active_document().map(|doc| doc.path.clone());
        self.tabs
            .retain(|doc| !same_path(&doc.path, removed) && !doc.path.starts_with(removed));
        self.active = active_path
            .and_then(|path| self.tabs.iter().position(|doc| same_path(&doc.path, &path)))
            .or_else(|| (!self.tabs.is_empty()).then_some(self.tabs.len().saturating_sub(1)));
        if self
            .split_path
            .as_ref()
            .is_some_and(|path| same_path(path, removed) || path.starts_with(removed))
        {
            self.split_path = None;
        }
        if self
            .split_primary
            .as_ref()
            .is_some_and(|path| same_path(path, removed) || path.starts_with(removed))
        {
            self.split_primary = None;
        }
        if self.split_path.is_none() || self.split_primary.is_none() {
            self.split_focus = 0;
        }
        self.debug
            .breakpoints
            .retain(|(path, _)| !same_path(path, removed) && !path.starts_with(removed));
        if self
            .debug
            .current_source
            .as_ref()
            .is_some_and(|(path, _)| same_path(path, removed) || path.starts_with(removed))
        {
            self.debug.current_source = None;
        }
        if self.image_preview.as_ref().is_some_and(|preview| {
            same_path(&preview.path, removed) || preview.path.starts_with(removed)
        }) {
            self.image_preview = None;
        }
    }

    fn commit_create_entry(&mut self, mut draft: CreateEntryDraft) {
        let name = draft.name.trim();
        if name.is_empty() {
            draft.error = Some("名称不能为空".to_owned());
            self.create_entry = Some(draft);
            return;
        }
        if name == "."
            || name == ".."
            || name.contains('/')
            || name.contains('\\')
            || name.contains(':')
        {
            draft.error = Some("名称不能包含路径分隔符".to_owned());
            self.create_entry = Some(draft);
            return;
        }
        let path = draft.dir.join(name);
        if path.exists() {
            draft.error = Some(format!("{} 已存在", util::display_path(&path)));
            self.create_entry = Some(draft);
            return;
        }
        let result = match draft.kind {
            CreateKind::File => std::fs::OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(&path)
                .map(|_| ()),
            CreateKind::Folder => std::fs::create_dir(&path),
        };
        if let Err(err) = result {
            draft.error = Some(format!("无法创建 {}：{err}", util::display_path(&path)));
            self.create_entry = Some(draft);
            return;
        }
        if let Some(tree) = &mut self.tree {
            tree.refresh();
            tree.reveal(&path);
        }
        self.member_index = None;
        self.refresh_git_status();
        self.create_entry = None;
        match draft.kind {
            CreateKind::File => {
                self.open_file(path);
                self.set_toast(format!("已创建文件 {name}"), Severity::Success);
            }
            CreateKind::Folder => {
                self.set_toast(format!("已创建文件夹 {name}"), Severity::Success);
            }
        }
    }

    fn open_image_preview(&mut self, ctx: &egui::Context, path: PathBuf) {
        let metadata = match std::fs::metadata(&path) {
            Ok(metadata) => metadata,
            Err(err) => {
                self.set_toast(
                    format!("无法读取 {}：{err}", util::display_path(&path)),
                    Severity::Error,
                );
                return;
            }
        };
        if metadata.len() > 32 * 1024 * 1024 {
            self.set_toast("图片超过 32 MB，已取消预览", Severity::Warning);
            return;
        }
        let bytes = match std::fs::read(&path) {
            Ok(bytes) => bytes,
            Err(err) => {
                self.set_toast(
                    format!("无法读取 {}：{err}", util::display_path(&path)),
                    Severity::Error,
                );
                return;
            }
        };
        let image = match image::load_from_memory(&bytes) {
            Ok(image) => image,
            Err(err) => {
                self.set_toast(format!("无法解码图片：{err}"), Severity::Error);
                return;
            }
        };
        let image = if image.width() > 8192 || image.height() > 8192 {
            image.thumbnail(4096, 4096)
        } else {
            image
        };
        let rgba = image.to_rgba8();
        let size = [rgba.width() as usize, rgba.height() as usize];
        let color = egui::ColorImage::from_rgba_unmultiplied(size, rgba.as_raw());
        let texture = ctx.load_texture(
            format!("simple_embedded_image_{}", path.display()),
            color,
            egui::TextureOptions::LINEAR,
        );
        self.image_preview = Some(ImagePreviewState {
            path,
            texture: Some(texture),
            size,
            error: None,
        });
    }

    fn refresh_support_packs(&mut self) {
        let Some(root) = self.project.as_ref().map(|project| project.root.clone()) else {
            self.support_packs.clear();
            return;
        };
        let mut packs = crate::support_pack::scan(&root);
        if packs.is_empty() {
            packs = crate::support_pack::scan_keil_installed(self.project_meta.device.as_deref());
        }
        self.support_packs = packs;
        self.refresh_memory_layout();
    }

    fn add_support_pack(&mut self) {
        let Some(root) = self.project.as_ref().map(|project| project.root.clone()) else {
            self.set_toast("请先打开工程", Severity::Warning);
            return;
        };
        let start_dir = self
            .support_packs
            .first()
            .and_then(|pack| pack.path.parent())
            .filter(|parent| parent.is_dir())
            .unwrap_or(&root);
        let Some(paths) = rfd::FileDialog::new()
            .set_title("添加芯片支持包")
            .add_filter("Keil/CMSIS 支持包", &["pack", "pdsc"])
            .set_directory(start_dir)
            .pick_files()
        else {
            return;
        };
        self.import_support_pack_files(paths);
    }

    fn import_support_pack_files(&mut self, paths: Vec<PathBuf>) {
        let Some(root) = self.project.as_ref().map(|project| project.root.clone()) else {
            self.set_toast("请先打开工程", Severity::Warning);
            return;
        };
        match crate::support_pack::import(&root, &paths) {
            Ok(copied) => {
                self.refresh_support_packs();
                if let Some(tree) = &mut self.tree {
                    tree.refresh();
                }
                self.set_toast(
                    format!("已添加 {} 个芯片支持包", copied.len()),
                    Severity::Success,
                );
            }
            Err(err) => self.set_toast(err, Severity::Error),
        }
    }

    fn apply_actions(&mut self, ctx: &egui::Context, actions: Vec<Action>) {
        for action in actions {
            match action {
                Action::OpenPicker => self.pick_project_folder(),
                Action::OpenWorkspacePicker => self.pick_workspace_file(),
                Action::CreateWorkspace => self.create_workspace(),
                Action::OpenProjectConfig => self.open_project_config(),
                Action::OpenFile(path) => self.open_file(path),
                Action::OpenAt(path, line, column) => self.open_at(ctx, &path, line, column),
                Action::SetEncoding(encoding) => self.set_encoding(encoding),
                Action::ToggleReadOnly => self.toggle_read_only(),
                Action::FormatDocument => self.format_current_document(),
                Action::OpenGitStatus => {
                    self.git_open = true;
                    self.refresh_git_status();
                }
                Action::OpenTodos => {
                    self.todo.open = true;
                    self.refresh_todos();
                }
                Action::OpenSymbols => {
                    self.symbols.open = true;
                    self.refresh_symbols();
                }
                Action::LspRename => {
                    self.open_lsp_rename();
                }
                Action::OpenRegisters => {
                    self.registers.open = true;
                    self.ensure_registers_loaded();
                }
                Action::OpenHex(path) => self.open_hex(&path),
                Action::OpenArrayGenerator => self.open_array_generator(ctx),
                Action::OpenDebug => {
                    if self.build_config_uses_micropython() {
                        self.start_micropython(MicroPythonAction::Reset);
                    } else {
                        self.debug.open = true;
                        self.focus_debug_output();
                    }
                }
                Action::DebugContinue => self.debug_continue(),
                Action::DebugPause => self.debug_pause(),
                Action::DebugJumpToCurrent => self.debug_jump_to_current(ctx),
                Action::DebugStepOver => self.debug_step_command("next"),
                Action::DebugStepInto => self.debug_step_command("step"),
                Action::DebugFinish => self.debug_step_command("finish"),
                Action::DebugStop => self.debug_stop(),
                Action::DebugSend(command) => self.debug_command(&command),
                Action::ToggleBreakpointAt(path, line) => self.toggle_breakpoint_at(&path, line),
                Action::SelectTab(index) => {
                    if index < self.tabs.len() {
                        let path = self.tabs[index].path.clone();
                        if let (Some(primary), Some(secondary)) =
                            (self.split_primary.clone(), self.split_path.clone())
                        {
                            if same_path(&path, &primary) {
                                self.split_focus = 0;
                            } else if same_path(&path, &secondary) {
                                self.split_focus = 1;
                            } else if self.split_focus == 0 {
                                self.split_primary = Some(path);
                            } else {
                                self.split_path = Some(path);
                            }
                        }
                        self.active = Some(index);
                        self.focus_editor = true;
                    }
                }
                Action::CloseTab(index) => self.request_close_tab(index),
                Action::CloseOtherTabs(index) => self.request_close_others(index),
                Action::CloseTabsToRight(index) => self.request_close_to_right(index),
                Action::CloseSavedTabs => self.close_saved_tabs(),
                Action::CloseAllTabs => self.request_close_all(),
                Action::SplitTab(path, direction) => self.split_tab(path, direction),
                Action::SplitGroup => self.split_active(SplitDirection::Right),
                Action::MoveSplit(direction) => self.move_split(direction),
                Action::CloseSplit => {
                    self.split_path = None;
                    self.split_primary = None;
                    self.split_focus = 0;
                }
                Action::SaveActive => {
                    self.save_active();
                }
                Action::SaveAs(index) => self.save_tab_as(index, ctx),
                Action::CopyText(text) => {
                    ctx.copy_text(text);
                    self.set_toast("已复制到剪贴板", Severity::Info);
                }
                Action::RevealInTree(path) => {
                    self.resource_open = true;
                    self.settings.resource_open = true;
                    self.settings.save();
                    if let Some(tree) = &mut self.tree {
                        tree.reveal(&path);
                    }
                    self.set_toast("已在资源管理器中定位", Severity::Info);
                }
                Action::Run(kind) => self.start_task(kind),
                Action::StopTask => {
                    self.build.stop();
                    self.settings.output_open = true;
                    self.settings.save();
                }
                Action::RefreshTree => {
                    if let Some(tree) = &mut self.tree {
                        tree.refresh();
                        self.set_toast("文件树已刷新", Severity::Info);
                    }
                    self.refresh_project_meta();
                    self.member_index = None;
                    self.refresh_support_packs();
                    self.refresh_git_status();
                }
                Action::ToggleHidden => {
                    self.settings.show_hidden = !self.settings.show_hidden;
                    if let Some(tree) = &mut self.tree {
                        tree.set_show_hidden(self.settings.show_hidden);
                    }
                    self.settings.save();
                }
                Action::OpenSettings => self.open_settings(),
                Action::OpenThemeSettings => self.open_theme_settings(),
                Action::EnterAiMode => self.enter_ai_mode(),
                Action::OpenFirmwareConfirm => {
                    if self.build_config_uses_micropython() {
                        self.firmware_confirm = true;
                    }
                }
                Action::RefreshBoardFiles => self.start_board_files(),
                Action::ReadBoardFile(file) => self.start_board_file(file),
                Action::EditMeta => self.open_meta_editor(),
                Action::DetectIncludeDirs => self.detect_include_dirs(),
                Action::SyncMakefileSources => self.sync_makefile_sources(),
                Action::AddMetaItem(field) => self.open_meta_add(field),
                Action::EditBuildOptions => self.open_build_options(),
                Action::AddSupportPack => self.add_support_pack(),
                Action::ImportSupportPack(path) => self.import_support_pack_files(vec![path]),
                Action::EditFlashConfig => self.open_flash_config(),
                Action::EditMemoryLayout => self.open_memory_layout(),
                Action::GenerateResourceReport => self.start_resource_report(),
                Action::RunMemoryCheck => self.open_memory_check(),
                Action::SetFlashTool(tool) => self.set_flash_tool(tool),
                Action::SetOpenOcdPath(path) => self.set_openocd_path(path),
                Action::BrowseOpenOcd => self.browse_openocd(),
                Action::SetPyOcdPath(path) => self.set_pyocd_path(path),
                Action::BrowsePyOcd => self.browse_pyocd(),
                Action::ClearLog => {
                    self.build.log.clear();
                }
                Action::CopyLog => {
                    if self.build.log.is_empty() {
                        self.set_toast("构建输出为空", Severity::Info);
                    } else {
                        ctx.copy_text(self.build.log.all_text());
                        self.set_toast("构建输出已复制到剪贴板", Severity::Info);
                    }
                }
                Action::ClearDebugLog => {
                    if let Some(session) = &mut self.debug.session {
                        session.log.clear();
                    } else {
                        self.debug.log.clear();
                    }
                }
                Action::CopyDebugLog => {
                    let text = if let Some(session) = &self.debug.session {
                        session.log.join("\n")
                    } else {
                        self.debug.log.join("\n")
                    };
                    if text.is_empty() {
                        self.set_toast("调试输出为空", Severity::Info);
                    } else {
                        ctx.copy_text(text);
                        self.set_toast("调试输出已复制到剪贴板", Severity::Info);
                    }
                }
                Action::Reveal(path) => util::open_in_file_manager(&path),
                Action::CopyPath(path) => {
                    ctx.copy_text(util::display_path(&path));
                    self.set_toast("路径已复制", Severity::Info);
                }
                Action::CopyRelativePath(path) => {
                    let relative = self
                        .project
                        .as_ref()
                        .map(|project| util::relative_display(&project.root, &path))
                        .filter(|value| !value.is_empty())
                        .unwrap_or_else(|| ".".to_owned());
                    ctx.copy_text(relative);
                    self.set_toast("相对路径已复制", Severity::Info);
                }
                Action::NewFile(dir) => self.open_create_entry(dir, CreateKind::File),
                Action::NewFolder(dir) => self.open_create_entry(dir, CreateKind::Folder),
                Action::OpenToSide(path) => self.open_to_side(path),
                Action::CutFile(path) => self.set_file_clipboard(path, true),
                Action::CopyFile(path) => self.set_file_clipboard(path, false),
                Action::PasteFile(path) => self.paste_file(path),
                Action::RenameFile(path) => self.start_rename(path),
                Action::DeleteFile(path) => self.start_delete(path),
                Action::PreviewImage(path) => self.open_image_preview(ctx, path),
                Action::OpenVscode(path) => util::open_in_vscode(&path),
                Action::ToggleWrap => {
                    self.settings.word_wrap = !self.settings.word_wrap;
                    self.settings.save();
                }
            }
        }
    }

    fn style_native_window(&mut self, ctx: &egui::Context, frame: &eframe::Frame) {
        if !self.native_theme_requested {
            ctx.send_viewport_cmd(ViewportCommand::SetTheme(if theme::is_dark() {
                egui::SystemTheme::Dark
            } else {
                egui::SystemTheme::Light
            }));
            self.native_theme_requested = true;
        }

        #[cfg(target_os = "windows")]
        if !self.native_titlebar_styled {
            if let Ok(handle) = frame.window_handle() {
                if let RawWindowHandle::Win32(win32) = handle.as_raw() {
                    style_windows_titlebar(win32.hwnd.get());
                    self.native_titlebar_styled = true;
                    ctx.request_repaint();
                }
            }
        }
        #[cfg(not(target_os = "windows"))]
        {
            let _ = frame;
            self.native_titlebar_styled = true;
        }
    }

    /// Fade the whole native window while AI mode is dissolving, so the
    /// desktop is visible between the old editor and the new AI surface.
    /// Layered windows keep the current OpenGL renderer; the alpha is applied
    /// by DWM when the window is composited.
    fn apply_ai_window_opacity(&mut self, frame: &eframe::Frame) {
        #[cfg(target_os = "windows")]
        {
            let target = if self.ai.enabled {
                self.ai
                    .transition
                    .as_ref()
                    .map(|transition| {
                        crate::app::ai_ui::ai_window_alpha(
                            transition.started.elapsed().as_secs_f32(),
                        )
                    })
                    .unwrap_or(255)
            } else {
                255
            };
            let Some(hwnd) = window_hwnd(frame) else {
                return;
            };
            if self.ai.enabled && self.ai.transition.is_some() {
                if target < 255 || self.ai.window_layered {
                    set_window_alpha(hwnd, target, false, &mut self.ai.window_layered);
                }
            } else if self.ai.window_layered {
                set_window_alpha(hwnd, 255, true, &mut self.ai.window_layered);
            }
        }
        #[cfg(not(target_os = "windows"))]
        {
            let _ = frame;
        }
    }

    fn sync_title(&mut self, ctx: &egui::Context) {
        let base = if self.ai.enabled {
            format!("AI 模式 · {APP_NAME}")
        } else if let Some(doc) = self.active_document() {
            let dirty = if doc.is_dirty() { " ●" } else { "" };
            format!("{}{dirty} — {APP_NAME}", doc.name)
        } else {
            APP_NAME.to_owned()
        };
        let title = format!("{base} · 作者 {APP_AUTHOR}");
        if title != self.last_title {
            self.last_title = title.clone();
            ctx.send_viewport_cmd(ViewportCommand::Title(title));
        }
    }

    fn build_status(&self) -> (Color32, String) {
        if self.micropython_busy {
            return (
                theme::accent(),
                format!(
                    "MicroPython {}中…",
                    self.micropython_action.as_deref().unwrap_or("运行")
                ),
            );
        }
        if let Some(kind) = self.build.running_kind() {
            let elapsed = self.build.elapsed().unwrap_or_default();
            return (
                theme::accent(),
                format!("{}中… {}", kind.verb(), format_duration(elapsed)),
            );
        }
        if let Some(last) = self.build.last() {
            return build_result_status(last);
        }
        (theme::text_faint(), "就绪".to_owned())
    }
}

fn build_result_status(last: &LastBuild) -> (Color32, String) {
    let duration = format_duration(last.duration);
    match &last.outcome {
        BuildOutcome::Success => (
            theme::success(),
            format!("{}完成 · {duration}", last.kind.verb()),
        ),
        BuildOutcome::Failed(Some(code)) => (
            theme::error(),
            format!("{}失败 · 退出码 {code}", last.kind.verb()),
        ),
        BuildOutcome::Failed(None) => (
            theme::error(),
            format!("{}失败 · 进程终止", last.kind.verb()),
        ),
        BuildOutcome::Cancelled => (theme::warning(), format!("{}已停止", last.kind.verb())),
        BuildOutcome::Error(_) => (theme::error(), format!("{}无法执行", last.kind.verb())),
    }
}

fn preserve_case_replacement(original: &str, replacement: &str) -> String {
    if replacement.is_empty() {
        return String::new();
    }
    let letters = original
        .chars()
        .filter(|ch| ch.is_alphabetic())
        .collect::<Vec<_>>();
    if letters.is_empty() {
        return replacement.to_owned();
    }
    if letters.iter().all(|ch| ch.is_uppercase()) {
        return replacement.to_uppercase();
    }
    if letters.iter().all(|ch| ch.is_lowercase()) {
        return replacement.to_lowercase();
    }
    if letters[0].is_uppercase() && letters.iter().skip(1).all(|ch| ch.is_lowercase()) {
        let mut chars = replacement.chars();
        if let Some(first) = chars.next() {
            return format!("{}{}", first.to_uppercase(), chars.as_str().to_lowercase());
        }
    }
    replacement.to_owned()
}

fn strip_gdb_prompts(mut value: &str) -> &str {
    loop {
        if let Some(rest) = value.strip_prefix("(gdb) ") {
            value = rest.trim_start();
        } else if value == "(gdb)" {
            return "";
        } else {
            return value;
        }
    }
}

fn watch_marker_index(line: &str, prefix: &str, id: u64) -> Option<usize> {
    let rest = line.strip_prefix(prefix)?;
    let rest = rest.strip_suffix("__")?;
    let (marker_id, index) = rest.rsplit_once('_')?;
    if marker_id.parse::<u64>().ok()? != id {
        return None;
    }
    index.parse().ok()
}

fn clean_watch_value(lines: &[String]) -> String {
    let mut text = String::new();
    for line in lines {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        let line = if let Some(rest) = line.strip_prefix('$') {
            rest.split_once(" = ").map_or(line, |(_, value)| value)
        } else {
            line
        };
        if !text.is_empty() {
            text.push(' ');
        }
        text.push_str(line);
    }
    text
}

fn clean_watch_type(lines: &[String]) -> String {
    let mut text = String::new();
    for line in lines {
        let line = line.trim();
        let line = line.strip_prefix("type = ").unwrap_or(line);
        if line.is_empty() {
            continue;
        }
        if !text.is_empty() {
            text.push(' ');
        }
        text.push_str(line);
    }
    text
}

fn parse_register_line(line: &str) -> Option<RegisterValue> {
    let mut parts = line.split_whitespace();
    let name = parts.next()?;
    let value = parts.next()?;
    if name.starts_with("__") || name.eq_ignore_ascii_case("all") {
        return None;
    }
    if !(value.starts_with("0x")
        || value.starts_with("0X")
        || value.chars().all(|character| character.is_ascii_digit()))
    {
        return None;
    }
    Some(RegisterValue {
        name: name.to_owned(),
        value: value.to_owned(),
        raw: line.to_owned(),
    })
}

fn parse_memory_word(line: &str) -> Option<String> {
    let token = line.split_whitespace().last()?;
    let hex = token
        .strip_prefix("0x")
        .or_else(|| token.strip_prefix("0X"))?;
    if hex.is_empty() || !hex.chars().all(|character| character.is_ascii_hexdigit()) {
        return None;
    }
    Some(token.to_owned())
}

fn gdb_source_path(path: &Path) -> String {
    util::display_path(path).replace('\\', "/")
}

/// Find the newest user source file that is newer than the debug program.
fn newest_newer_source(target: &Path, root: &Path) -> Option<PathBuf> {
    let target_time = std::fs::metadata(target)
        .and_then(|metadata| metadata.modified())
        .ok()?;
    let mut newest: Option<(SystemTime, PathBuf)> = None;
    let mut visited = 0usize;
    walk_sources(root, target, 0, &mut visited, &mut newest);
    newest
        .filter(|(time, _)| *time > target_time)
        .map(|(_, path)| path)
}

fn walk_sources(
    dir: &Path,
    target: &Path,
    depth: usize,
    visited: &mut usize,
    newest: &mut Option<(SystemTime, PathBuf)>,
) {
    if depth > 8 || *visited > 5000 {
        return;
    }
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        *visited += 1;
        if *visited > 5000 {
            return;
        }
        let path = entry.path();
        let Ok(file_type) = entry.file_type() else {
            continue;
        };
        if file_type.is_dir() {
            let name = entry.file_name().to_string_lossy().to_ascii_lowercase();
            if matches!(
                name.as_str(),
                ".git"
                    | ".svn"
                    | ".opencode"
                    | ".simple-embedded"
                    | "build"
                    | "target"
                    | "dist"
                    | "out"
                    | "debug"
                    | "release"
                    | ".vscode"
            ) {
                continue;
            }
            walk_sources(&path, target, depth + 1, visited, newest);
            continue;
        }
        if !file_type.is_file() || path == target {
            continue;
        }
        let extension = path
            .extension()
            .and_then(|value| value.to_str())
            .unwrap_or_default()
            .to_ascii_lowercase();
        if !matches!(
            extension.as_str(),
            "c" | "h" | "cpp" | "hpp" | "cc" | "hh" | "s" | "asm" | "ld"
        ) {
            continue;
        }
        let Ok(modified) = entry.metadata().and_then(|metadata| metadata.modified()) else {
            continue;
        };
        if newest.as_ref().is_none_or(|(time, _)| modified > *time) {
            *newest = Some((modified, path));
        }
    }
}

/// Find the newest build output when the project does not pin a debug program.
fn detect_debug_program(root: &Path) -> Option<PathBuf> {
    let mut candidates = Vec::new();
    for dir in [
        root.join("build"),
        root.join("Debug"),
        root.join("Release"),
        root.join("out"),
        root.to_path_buf(),
    ] {
        let Ok(read_dir) = std::fs::read_dir(&dir) else {
            continue;
        };
        for entry in read_dir.flatten() {
            let path = entry.path();
            if !path.is_file() {
                continue;
            }
            let extension = path
                .extension()
                .map(|value| value.to_string_lossy().to_lowercase())
                .unwrap_or_default();
            if !matches!(extension.as_str(), "elf" | "axf" | "exe" | "out") {
                continue;
            }
            let modified = entry
                .metadata()
                .ok()
                .and_then(|metadata| metadata.modified().ok());
            candidates.push((modified, path));
        }
    }
    candidates.sort_by_key(|candidate| std::cmp::Reverse(candidate.0));
    candidates.into_iter().next().map(|(_, path)| path)
}

fn remap_path(path: &Path, old: &Path, new: &Path) -> PathBuf {
    if same_path(path, old) {
        return new.to_path_buf();
    }
    path.strip_prefix(old)
        .map(|suffix| new.join(suffix))
        .unwrap_or_else(|_| path.to_path_buf())
}

fn path_has_dirty_tab(tabs: &[Document], path: &Path) -> bool {
    tabs.iter()
        .any(|doc| (same_path(&doc.path, path) || doc.path.starts_with(path)) && doc.is_dirty())
}

fn remove_path(path: &Path) -> std::io::Result<()> {
    if path.is_dir() {
        std::fs::remove_dir_all(path)
    } else {
        std::fs::remove_file(path)
    }
}

fn copy_recursive(source: &Path, destination: &Path) -> std::io::Result<()> {
    if source.is_dir() {
        std::fs::create_dir_all(destination)?;
        for entry in std::fs::read_dir(source)? {
            let entry = entry?;
            copy_recursive(&entry.path(), &destination.join(entry.file_name()))?;
        }
        Ok(())
    } else {
        if let Some(parent) = destination.parent() {
            std::fs::create_dir_all(parent)?;
        }
        std::fs::copy(source, destination).map(|_| ())
    }
}

fn board_download_path(
    root: &Path,
    file: &crate::micropython::BoardFile,
) -> Result<PathBuf, String> {
    let relative = Path::new(&file.path);
    let safe = !relative.is_absolute()
        && !relative.as_os_str().is_empty()
        && relative
            .components()
            .all(|component| matches!(component, std::path::Component::Normal(_)));
    let target = if safe {
        root.join(relative)
    } else {
        root.join(&file.name)
    };
    if let Some(parent) = target.parent() {
        std::fs::create_dir_all(parent).map_err(|error| format!("创建下载目录失败：{error}"))?;
    }
    Ok(if target.exists() {
        unique_copy_path(target.parent().unwrap_or(root), &target)
    } else {
        target
    })
}

fn unique_copy_path(directory: &Path, source: &Path) -> PathBuf {
    let stem = source
        .file_stem()
        .map(|value| value.to_string_lossy().into_owned())
        .unwrap_or_else(|| "file".to_owned());
    let extension = source
        .extension()
        .map(|value| format!(".{}", value.to_string_lossy()))
        .unwrap_or_default();
    for index in 1..10_000usize {
        let name = if index == 1 {
            format!("{stem} - 副本{extension}")
        } else {
            format!("{stem} - 副本 ({index}){extension}")
        };
        let candidate = directory.join(name);
        if !candidate.exists() {
            return candidate;
        }
    }
    directory.join(format!("{stem} - 副本{extension}"))
}

fn selected_char_range(ctx: &egui::Context, path: &Path) -> Option<(usize, usize)> {
    let editor_id = egui::Id::new(("simple_embedded_code_editor", path));
    let state = egui::text_edit::TextEditState::load(ctx, editor_id)?;
    let range = state.cursor.char_range()?;
    let start = range.primary.index.min(range.secondary.index);
    let end = range.primary.index.max(range.secondary.index);
    (end > start).then_some((start, end))
}

fn set_editor_cursor(ctx: &egui::Context, path: &Path, char_index: usize) {
    let editor_id = egui::Id::new(("simple_embedded_code_editor", path));
    let mut state = egui::text_edit::TextEditState::load(ctx, editor_id).unwrap_or_default();
    state
        .cursor
        .set_char_range(Some(egui::text::CCursorRange::one(
            egui::text::CCursor::new(char_index),
        )));
    state.store(ctx, editor_id);
}

fn set_editor_selection(ctx: &egui::Context, path: &Path, start: usize, end: usize) {
    let editor_id = egui::Id::new(("simple_embedded_code_editor", path));
    let mut state = egui::text_edit::TextEditState::load(ctx, editor_id).unwrap_or_default();
    state
        .cursor
        .set_char_range(Some(egui::text::CCursorRange::two(
            egui::text::CCursor::new(start),
            egui::text::CCursor::new(end),
        )));
    state.store(ctx, editor_id);
}

fn completion_kind(kind: u32) -> crate::completion::CompletionKind {
    match kind {
        2 | 3 => crate::completion::CompletionKind::Function,
        5 | 10 => crate::completion::CompletionKind::Field,
        7 | 8 | 9 | 22 | 23 | 25 => crate::completion::CompletionKind::Type,
        14 => crate::completion::CompletionKind::Macro,
        _ => crate::completion::CompletionKind::Label,
    }
}

fn same_path(a: &Path, b: &Path) -> bool {
    #[cfg(target_os = "windows")]
    {
        a.to_string_lossy()
            .eq_ignore_ascii_case(&b.to_string_lossy())
    }
    #[cfg(not(target_os = "windows"))]
    {
        a == b
    }
}

impl eframe::App for App {
    fn ui(&mut self, ui: &mut egui::Ui, frame: &mut eframe::Frame) {
        crate::apply_pending_icons();
        let ctx = ui.ctx().clone();
        if !self.ai.enabled {
            if let Some(previous) = self.ai_zoom_before.take() {
                ctx.set_zoom_factor(previous);
            }
        }
        if self.build.poll() {
            self.report_link_problem();
        }
        self.poll_lsp(&ctx);
        self.poll_relations(&ctx);
        self.poll_install_system(&ctx);
        self.poll_resource_report(&ctx);
        self.poll_ai(&ctx);
        self.sync_lsp();
        self.poll_micropython();
        self.poll_debug_start();
        if let Some(session) = &mut self.debug.session {
            session.poll();
            let watch_visible = self.debug.auto_watch
                && self.settings.output_open
                && self.output_view == OutputView::Debug;
            if self.debug.open || watch_visible {
                ctx.request_repaint_after(Duration::from_millis(120));
            }
        }
        self.poll_debug_location(&ctx);
        self.poll_watchpoints();
        self.poll_debug_watch();
        self.tick_debug_watch();
        self.poll_debug_peripheral();
        self.tick_debug_peripheral();
        if self.debug.starting {
            ctx.request_repaint_after(Duration::from_millis(100));
        }
        self.refresh_diagnostics();
        self.handle_close_request(&ctx);
        self.handle_shortcuts(&ctx);
        self.handle_completion_keys(&ctx);
        self.handle_dropped_files(&ctx);
        self.style_native_window(&ctx, frame);

        let mut actions = Vec::new();
        let ai_elapsed = if self.ai.enabled {
            self.ai
                .transition
                .as_ref()
                .map(|transition| transition.started.elapsed().as_secs_f32())
        } else {
            None
        };
        let early_ai_fog =
            ai_elapsed.is_some_and(|elapsed| elapsed < crate::app::ai_ui::AI_EARLY_FOG_SECS);

        if !self.immersive && (!self.ai.enabled || early_ai_fog) {
            self.show_status_bar(ui, &mut actions);
        }
        let old_spacing = ui.spacing().item_spacing;
        let old_resize = ui.style().interaction.resize_grab_radius_side;
        ui.spacing_mut().item_spacing.x = 0.0;
        ui.style_mut().interaction.resize_grab_radius_side = 0.0;
        if self.ai.enabled && !early_ai_fog {
            self.show_ai_mode(ui);
        } else if self.immersive {
            self.show_center(ui, &mut actions);
        } else {
            self.show_action_rail(ui, &mut actions);
            self.show_side_panel(ui, &mut actions);
            self.show_output_panel(ui, &mut actions);
            self.show_center(ui, &mut actions);
        }
        ui.spacing_mut().item_spacing = old_spacing;
        ui.style_mut().interaction.resize_grab_radius_side = old_resize;
        if early_ai_fog {
            if let Some(elapsed) = ai_elapsed {
                self.show_ai_early_fog(ui, elapsed);
            }
        }
        if self.immersive && !self.ai.enabled {
            self.show_immersive_explorer(&ctx, &mut actions);
        }
        self.apply_ai_window_opacity(frame);
        self.update_completion(&ctx);
        if self
            .lsp
            .as_ref()
            .is_some_and(|session| session.has_pending())
        {
            ctx.request_repaint_after(Duration::from_millis(40));
        }

        if self.settings_draft.is_some() {
            self.show_settings_modal(&ctx);
            actions.clear();
        }
        if self.ai_settings_draft.is_some() {
            self.show_ai_settings_modal(&ctx);
            actions.clear();
        }
        if self.theme_settings.is_some() {
            self.show_theme_settings_modal(&ctx);
            actions.clear();
        }
        if self.firmware_confirm {
            self.show_firmware_confirm(&ctx);
            actions.clear();
        }
        if self.pending.is_some() {
            self.show_pending_modal(&ctx);
            actions.clear();
        }
        if self.meta_draft.is_some() {
            self.show_meta_modal(&ctx);
            actions.clear();
        }
        if self.meta_add.is_some() {
            self.show_meta_add_modal(&ctx);
            actions.clear();
        }
        if self.flash_draft.is_some() {
            self.show_flash_modal(&ctx);
            actions.clear();
        }
        if self.build_options_draft.is_some() {
            self.show_build_options_modal(&ctx);
            actions.clear();
        }
        if self.create_entry.is_some() {
            self.show_create_entry_modal(&ctx);
            actions.clear();
        }
        if self.rename_entry.is_some() {
            self.show_rename_entry_modal(&ctx);
            actions.clear();
        }
        if self.delete_entry.is_some() {
            self.show_delete_entry_modal(&ctx);
            actions.clear();
        }
        if self.memory_draft.is_some() {
            self.show_memory_layout_modal(&ctx);
            actions.clear();
        }
        if self.lsp_rename.is_some() {
            self.show_lsp_rename(&ctx);
            actions.clear();
        }
        if self.debug.peripheral_edit.is_some() {
            self.show_peripheral_edit(&ctx);
            actions.clear();
        }
        if !self.ai.enabled {
            let modal_open = self.has_modal();
            if self.project_search.open && !modal_open {
                self.show_project_search(&ctx, &mut actions);
            }
            if self.git_open && !modal_open {
                self.show_git_status(&ctx, &mut actions);
            }
            if self.todo.open && !modal_open {
                self.show_todos(&ctx, &mut actions);
            }
            if self.symbols.open && !modal_open {
                self.show_symbols(&ctx, &mut actions);
            }
            if self.lsp_refs.is_some() && !modal_open {
                self.show_lsp_references(&ctx, &mut actions);
            }
            if self.hex.open && !modal_open {
                self.show_hex_viewer(&ctx);
            }
            if self.registers.open && !modal_open {
                self.show_registers(&ctx, &mut actions);
            }
            if self.memory_check.open && !modal_open {
                self.show_memory_check(&ctx, &mut actions);
            }
            if self.array.open && !modal_open {
                self.show_array_generator(&ctx);
            }
            if self.debug.open && !self.immersive && !modal_open {
                self.show_debug_overlay(&ctx, &mut actions);
            }
            if self.image_preview.is_some() {
                self.show_image_preview(&ctx);
            }
            if self.completion.open && !modal_open {
                self.show_completion(&ctx);
            }
            if self.column_edit.open && !modal_open {
                self.show_column_edit(&ctx);
            }
        }

        if self.settings_draft.is_none()
            && self.pending.is_none()
            && self.meta_draft.is_none()
            && self.meta_add.is_none()
            && self.flash_draft.is_none()
            && self.build_options_draft.is_none()
            && self.create_entry.is_none()
            && self.rename_entry.is_none()
            && self.delete_entry.is_none()
            && self.memory_draft.is_none()
            && self.ai_settings_draft.is_none()
        {
            self.apply_actions(&ctx, actions);
        }

        self.capture_undo();
        self.refresh_git_line_marks();
        self.tick_autosave(&ctx);
        self.show_toast(&ctx);
        self.sync_title(&ctx);

        if self.build.is_running() {
            ctx.request_repaint_after(Duration::from_millis(80));
        }
        if self.micropython_busy {
            ctx.request_repaint_after(Duration::from_millis(100));
        }
        if let Some(toast) = &self.toast {
            let life = toast_life(toast.severity);
            let elapsed = toast.born.elapsed();
            if elapsed >= life {
                self.toast = None;
            } else {
                ctx.request_repaint_after(life - elapsed);
            }
        }
    }
}

fn toast_life(severity: Severity) -> Duration {
    if matches!(severity, Severity::Error) {
        Duration::from_secs(7)
    } else {
        Duration::from_secs(4)
    }
}

#[cfg(target_os = "windows")]
fn style_windows_titlebar(hwnd: isize) {
    use std::ffi::c_void;

    const DWMWA_USE_IMMERSIVE_DARK_MODE: u32 = 20;
    const DWMWA_USE_IMMERSIVE_DARK_MODE_OLD: u32 = 19;
    const DWMWA_BORDER_COLOR: u32 = 34;
    const DWMWA_CAPTION_COLOR: u32 = 35;
    const DWMWA_TEXT_COLOR: u32 = 36;

    #[link(name = "dwmapi")]
    extern "system" {
        fn DwmSetWindowAttribute(
            hwnd: isize,
            attribute: u32,
            value: *const c_void,
            value_size: u32,
        ) -> i32;
    }

    fn colorref(color: Color32) -> u32 {
        u32::from(color.r()) | (u32::from(color.g()) << 8) | (u32::from(color.b()) << 16)
    }

    let dark = i32::from(theme::is_dark());
    unsafe {
        for attribute in [
            DWMWA_USE_IMMERSIVE_DARK_MODE,
            DWMWA_USE_IMMERSIVE_DARK_MODE_OLD,
        ] {
            DwmSetWindowAttribute(
                hwnd,
                attribute,
                (&dark as *const i32).cast(),
                std::mem::size_of::<i32>() as u32,
            );
        }

        let caption = colorref(theme::panel());
        DwmSetWindowAttribute(
            hwnd,
            DWMWA_CAPTION_COLOR,
            (&caption as *const u32).cast(),
            std::mem::size_of::<u32>() as u32,
        );
        let border = colorref(theme::border_soft());
        DwmSetWindowAttribute(
            hwnd,
            DWMWA_BORDER_COLOR,
            (&border as *const u32).cast(),
            std::mem::size_of::<u32>() as u32,
        );
        let text = colorref(theme::text());
        DwmSetWindowAttribute(
            hwnd,
            DWMWA_TEXT_COLOR,
            (&text as *const u32).cast(),
            std::mem::size_of::<u32>() as u32,
        );
    }
}

#[cfg(target_os = "windows")]
fn window_hwnd(frame: &eframe::Frame) -> Option<isize> {
    if let Ok(handle) = frame.window_handle() {
        if let RawWindowHandle::Win32(win32) = handle.as_raw() {
            return Some(win32.hwnd.get());
        }
    }
    None
}

#[cfg(target_os = "windows")]
fn set_window_alpha(hwnd: isize, alpha: u8, remove: bool, layered: &mut bool) {
    const GWL_EXSTYLE: i32 = -20;
    const WS_EX_LAYERED: u32 = 0x0008_0000;
    const LWA_ALPHA: u32 = 0x0000_0002;
    const SWP_NOSIZE: u32 = 0x0001;
    const SWP_NOMOVE: u32 = 0x0002;
    const SWP_NOZORDER: u32 = 0x0004;
    const SWP_NOACTIVATE: u32 = 0x0010;
    const SWP_FRAMECHANGED: u32 = 0x0020;

    #[link(name = "user32")]
    extern "system" {
        fn GetWindowLongW(hwnd: isize, index: i32) -> i32;
        fn SetWindowLongW(hwnd: isize, index: i32, value: i32) -> i32;
        fn SetLayeredWindowAttributes(hwnd: isize, key: u32, alpha: u8, flags: u32) -> i32;
        fn SetWindowPos(
            hwnd: isize,
            after: isize,
            x: i32,
            y: i32,
            cx: i32,
            cy: i32,
            flags: u32,
        ) -> i32;
    }

    unsafe {
        let style = GetWindowLongW(hwnd, GWL_EXSTYLE) as u32;
        if remove {
            if *layered || style & WS_EX_LAYERED != 0 {
                SetLayeredWindowAttributes(hwnd, 0, 255, LWA_ALPHA);
                if style & WS_EX_LAYERED != 0 {
                    SetWindowLongW(hwnd, GWL_EXSTYLE, (style & !WS_EX_LAYERED) as i32);
                    SetWindowPos(
                        hwnd,
                        0,
                        0,
                        0,
                        0,
                        0,
                        SWP_NOSIZE | SWP_NOMOVE | SWP_NOZORDER | SWP_NOACTIVATE | SWP_FRAMECHANGED,
                    );
                }
            }
            *layered = false;
            return;
        }
        if style & WS_EX_LAYERED == 0 {
            SetWindowLongW(hwnd, GWL_EXSTYLE, (style | WS_EX_LAYERED) as i32);
            SetWindowPos(
                hwnd,
                0,
                0,
                0,
                0,
                0,
                SWP_NOSIZE | SWP_NOMOVE | SWP_NOZORDER | SWP_NOACTIVATE | SWP_FRAMECHANGED,
            );
        }
        SetLayeredWindowAttributes(hwnd, 0, alpha, LWA_ALPHA);
        *layered = true;
    }
}

impl App {
    fn show_action_rail(&mut self, ui: &mut egui::Ui, actions: &mut Vec<Action>) {
        let running = self.build.is_running();
        let has_project = self.project.is_some();
        let micropython = self.effective_toolchain().is_micropython();
        let micro_ready = micropython
            && self.build_config.serial.is_ready()
            && !self.micropython_busy
            && !self.build.is_running();
        let can_build = if micropython {
            micro_ready
        } else {
            has_project && !running && self.build_config.has_command(TaskKind::Build)
        };
        let can_clean = !micropython
            && has_project
            && !running
            && self.build_config.has_command(TaskKind::Clean);
        let can_rebuild = !micropython
            && has_project
            && !running
            && self.build_config.has_command(TaskKind::Rebuild);
        let can_flash = if micropython {
            micro_ready
        } else {
            has_project && !running && self.build_config.has_command(TaskKind::Flash)
        };
        let can_debug = if micropython {
            micro_ready
        } else {
            has_project
        };

        egui::Panel::right("simple_embedded_action_rail")
            .show_separator_line(false)
            .exact_size(58.0)
            .frame(
                Frame::new()
                    .fill(theme::gutter())
                    .inner_margin(Margin::same(3)),
            )
            .show_inside(ui, |ui| {
                ui.spacing_mut().item_spacing.x = 8.0;
                ui.painter().rect(
                    ui.max_rect(),
                    CornerRadius::same(8),
                    theme::panel(),
                    Stroke::NONE,
                    egui::StrokeKind::Inside,
                );
                ui.add_space(6.0);
                ui.spacing_mut().interact_size.y = 30.0;
                ui.spacing_mut().button_padding = egui::vec2(4.0, 5.0);
                ui.vertical_centered(|ui| {
                    let width = ui.available_width();

                    if running {
                        let stop = egui::Button::new(
                            RichText::new("停止")
                                .size(12.5)
                                .strong()
                                .color(theme::error()),
                        )
                        .fill(theme::error().gamma_multiply(0.13))
                        .stroke(Stroke::new(1.0_f32, theme::error().gamma_multiply(0.55)))
                        .corner_radius(6)
                        .min_size(egui::vec2(width, 34.0));
                        if ui.add(stop).on_hover_text("终止构建进程树").clicked() {
                            actions.push(Action::StopTask);
                        }
                        ui.add_space(8.0);
                        ui.spinner();
                    } else {
                        let build_label = if micropython { "运行" } else { "构建" };
                        let build_tip = if micropython && !self.build_config.serial.is_ready() {
                            "请先在配置 → 构建中选择 MicroPython 串口和波特率"
                        } else if micropython && self.micropython_busy {
                            "MicroPython 操作正在进行…"
                        } else if micropython {
                            "运行当前 .py：上传为 main.py 并软复位 ESP32"
                        } else if can_build {
                            "构建工程 (Ctrl+B)"
                        } else if has_project {
                            "请先在左上角工程卡片中配置构建环境"
                        } else {
                            "请先导入工程并配置构建环境"
                        };
                        let button = if can_build {
                            theme::primary_button(build_label).min_size(egui::vec2(width, 34.0))
                        } else {
                            egui::Button::new(
                                RichText::new(build_label)
                                    .color(theme::text_faint())
                                    .size(12.5),
                            )
                            .fill(theme::surface())
                            .stroke(Stroke::new(1.0_f32, theme::border_soft()))
                            .corner_radius(6)
                            .min_size(egui::vec2(width, 34.0))
                        };
                        let response = ui.add_enabled(can_build, button);
                        let response = if can_build {
                            response.on_hover_text(build_tip)
                        } else {
                            response.on_disabled_hover_text(build_tip)
                        };
                        if response.clicked() {
                            actions.push(Action::Run(TaskKind::Build));
                        }

                        if !micropython {
                            ui.add_space(6.0);
                            let clean_tip = if can_clean {
                                "执行 Clean 命令"
                            } else if has_project {
                                "Clean 命令尚未配置"
                            } else {
                                "请先导入工程并配置构建环境"
                            };
                            let response = ui
                                .add_enabled_ui(can_clean, |ui| {
                                    ui.add_sized([width, 30.0], theme::secondary_button("清理"))
                                })
                                .inner;
                            let response = if can_clean {
                                response.on_hover_text(clean_tip)
                            } else {
                                response.on_disabled_hover_text(clean_tip)
                            };
                            if response.clicked() {
                                actions.push(Action::Run(TaskKind::Clean));
                            }

                            ui.add_space(6.0);
                            let rebuild_tip = if can_rebuild {
                                "清理后重新构建 (Ctrl+Shift+B)"
                            } else if has_project {
                                "Rebuild 命令尚未配置"
                            } else {
                                "请先导入工程并配置构建环境"
                            };
                            let response = ui
                                .add_enabled_ui(can_rebuild, |ui| {
                                    ui.add_sized([width, 30.0], theme::secondary_button("重构"))
                                })
                                .inner;
                            let response = if can_rebuild {
                                response.on_hover_text(rebuild_tip)
                            } else {
                                response.on_disabled_hover_text(rebuild_tip)
                            };
                            if response.clicked() {
                                actions.push(Action::Run(TaskKind::Rebuild));
                            }
                        }

                        ui.add_space(6.0);
                        let flash_label = if micropython { "上传" } else { "烧录" };
                        let flash_tip = if micropython {
                            if !self.build_config.serial.is_ready() {
                                "请先在配置 → 构建中选择 MicroPython 串口和波特率"
                            } else if self.micropython_busy {
                                "MicroPython 操作正在进行…"
                            } else {
                                "把当前 .py 文件上传到 ESP32 文件系统"
                            }
                        } else if can_flash {
                            match self.build_config.flash_tool {
                                FlashTool::OpenOcd => "使用 OpenOCD 烧录（本地工程目录）",
                                FlashTool::PyOcd => "使用 pyOCD 烧录（本地工程目录）",
                                FlashTool::Custom => "调用配置的烧录命令（本地工程目录）",
                            }
                        } else if has_project {
                            "尚未配置烧录，请在左侧“烧录配置”中设置"
                        } else {
                            "请先导入工程并配置构建环境"
                        };
                        let response = ui
                            .add_enabled_ui(can_flash, |ui| {
                                ui.add_sized([width, 30.0], theme::secondary_button(flash_label))
                            })
                            .inner;
                        let response = if can_flash {
                            response.on_hover_text(flash_tip)
                        } else {
                            response.on_disabled_hover_text(flash_tip)
                        };
                        if response.clicked() {
                            actions.push(Action::Run(TaskKind::Flash));
                        }

                        if micropython {
                            ui.add_space(6.0);
                            let firmware_tip = if !self.build_config.serial.is_ready() {
                                "请先在配置 → 构建中选择 MicroPython 串口和波特率"
                            } else if self.micropython_busy {
                                "MicroPython 操作正在进行…"
                            } else if !self.build_config.firmware.is_ready() {
                                "烧录 MicroPython 固件（请先在配置中填写固件和 esptool）"
                            } else {
                                "烧录 MicroPython 固件（会覆盖开发板固件）"
                            };
                            let response = ui
                                .add_enabled_ui(micro_ready, |ui| {
                                    ui.add_sized([width, 30.0], theme::secondary_button("固件"))
                                })
                                .inner;
                            let response = if micro_ready {
                                response.on_hover_text(firmware_tip)
                            } else {
                                response.on_disabled_hover_text(firmware_tip)
                            };
                            if response.clicked() {
                                actions.push(Action::OpenFirmwareConfirm);
                            }
                        }

                        ui.add_space(6.0);
                        let debug_label = if micropython { "复位" } else { "调试" };
                        let debug_tip = if micropython {
                            if !self.build_config.serial.is_ready() {
                                "请先在配置 → 构建中选择 MicroPython 串口和波特率"
                            } else if self.micropython_busy {
                                "MicroPython 操作正在进行…"
                            } else {
                                "软复位 ESP32（Ctrl-D），运行板上的 main.py"
                            }
                        } else if self.debug.session.is_some() {
                            "打开调试控制 (F5 继续)"
                        } else {
                            "启动 gdb 调试（需在设置中配置调试程序）"
                        };
                        let response = ui
                            .add_enabled_ui(can_debug, |ui| {
                                ui.add_sized([width, 30.0], theme::secondary_button(debug_label))
                            })
                            .inner;
                        let response = if can_debug {
                            response.on_hover_text(debug_tip)
                        } else {
                            response.on_disabled_hover_text(debug_tip)
                        };
                        if response.clicked() {
                            actions.push(Action::OpenDebug);
                        }

                        if !micropython {
                            ui.add_space(6.0);
                            let can_check = has_project;
                            let check_tip = if !has_project {
                                "请先打开一个工程"
                            } else if self.debug.session.is_some() {
                                "打开动态监测：硬件 watchpoint（调试已连接）"
                            } else {
                                "静态扫描数组越界、危险字符串操作、重复释放等内存问题（无需调试）"
                            };
                            let response = ui
                                .add_enabled_ui(can_check, |ui| {
                                    ui.add_sized([width, 30.0], theme::secondary_button("检测"))
                                })
                                .inner;
                            let response = if can_check {
                                response.on_hover_text(check_tip)
                            } else {
                                response.on_disabled_hover_text(check_tip)
                            };
                            if response.clicked() {
                                actions.push(Action::RunMemoryCheck);
                            }
                        }
                    }
                    if self.micropython_busy {
                        ui.add_space(8.0);
                        ui.spinner();
                    }
                    let remaining = ui.available_height();
                    if remaining > 74.0 {
                        ui.add_space(remaining - 74.0);
                    }
                    if ui
                        .add_sized([width, 30.0], theme::primary_button("AI模式"))
                        .on_hover_text(
                            "AI 模式：加载 stm32-embedded-dev skill，按工程上下文生成代码 (Ctrl+Alt+A)",
                        )
                        .clicked()
                    {
                        actions.push(Action::EnterAiMode);
                    }
                    ui.add_space(6.0);
                    if ui
                        .add_sized([width, 30.0], theme::secondary_button("设置"))
                        .on_hover_text("打开设置")
                        .clicked()
                    {
                        actions.push(Action::OpenThemeSettings);
                    }
                });
            });
    }

    fn show_output_panel(&mut self, ui: &mut egui::Ui, actions: &mut Vec<Action>) {
        let open = self.settings.output_open;
        let build_running = self.build.is_running();
        let debug_view = self.output_view == OutputView::Debug;
        let debug_log = self
            .debug
            .session
            .as_ref()
            .map(|session| session.log.clone())
            .unwrap_or_else(|| self.debug.log.clone());
        let debug_count = debug_log.len();
        let log_count = if debug_view {
            debug_count
        } else {
            self.build.log.len()
        };
        let problem_count = self.build.log.problem_count();
        let debug_running = self
            .debug
            .session
            .as_mut()
            .map(|session| session.is_running())
            .unwrap_or(false);
        let debug_has_session = self.debug.session.is_some();
        let debug_starting = self.debug.starting;
        let memory_summary = self.memory_usage_summary();
        let mut auto_scroll = self.settings.auto_scroll_output;
        let mut toggle_output = false;
        let (status_color, status_text) = if debug_view {
            if self.debug.error.is_some() {
                (theme::error(), "错误".to_owned())
            } else if debug_starting {
                (theme::accent(), "启动中".to_owned())
            } else if debug_has_session {
                if debug_running {
                    (theme::success(), "调试器已连接".to_owned())
                } else {
                    (theme::text_faint(), "调试器已停止".to_owned())
                }
            } else {
                (theme::text_faint(), "未启动".to_owned())
            }
        } else {
            self.build_status()
        };
        let full_height = ui.available_height().max(100.0);
        let output_panel_id = egui::Id::new("simple_embedded_build_output");
        let height = if open {
            self.settings.output_height.clamp(40.0, full_height)
        } else {
            40.0
        };
        let mut output_open = open;
        let mut drag_height: Option<f32> = None;
        let mut drag_stopped = false;

        let panel = egui::Panel::bottom(output_panel_id)
            .show_separator_line(false)
            .resizable(false)
            .exact_size(height);

        panel
            .frame(
                Frame::new()
                    .fill(theme::gutter())
                    .inner_margin(Margin::same(4)),
            )
            .show_inside(ui, |ui| {
                ui.spacing_mut().item_spacing.x = 8.0;
                ui.painter().rect(
                    ui.max_rect(),
                    CornerRadius::same(8),
                    theme::panel(),
                    Stroke::NONE,
                    egui::StrokeKind::Inside,
                );
                ui.spacing_mut().item_spacing.y = 0.0;
                let drag_rect = egui::Rect::from_min_size(
                    ui.cursor().min,
                    egui::vec2(ui.available_width(), 4.0),
                );
                let handle = ui
                    .interact(
                        drag_rect,
                        egui::Id::new("simple_embedded_output_drag"),
                        Sense::drag(),
                    )
                    .on_hover_cursor(egui::CursorIcon::ResizeVertical);
                if handle.dragged() {
                    let new_height = (height - handle.drag_delta().y).clamp(40.0, full_height);
                    drag_height = Some(new_height);
                    output_open = new_height > 44.0;
                }
                if handle.drag_stopped() {
                    drag_stopped = true;
                }

                let header = Frame::new()
                    .fill(theme::panel_raised())
                    .corner_radius(CornerRadius {
                        nw: 8,
                        ne: 8,
                        sw: 6,
                        se: 6,
                    })
                    .inner_margin(Margin::symmetric(10, 3))
                    .show(ui, |ui| {
                        ui.spacing_mut().interact_size.y = 22.0;
                        ui.allocate_ui_with_layout(
                            egui::vec2(ui.available_width(), 22.0),
                            Layout::left_to_right(Align::Center),
                            |ui| {
                                let toggle_label = if open { "收起" } else { "展开" };
                                if ui
                                    .add(
                                        Button::new(
                                            RichText::new(toggle_label)
                                                .size(12.0)
                                                .color(theme::text_faint()),
                                        )
                                        .frame(false),
                                    )
                                    .on_hover_text(if open {
                                        "收起输出面板"
                                    } else {
                                        "展开输出面板"
                                    })
                                    .clicked()
                                {
                                    toggle_output = true;
                                }

                                let (bar_rect, _) =
                                    ui.allocate_exact_size(egui::vec2(3.0, 13.0), Sense::hover());
                                ui.painter().rect_filled(
                                    bar_rect,
                                    CornerRadius::same(1),
                                    theme::accent(),
                                );
                                let build_view_label =
                                    if self.build_config.toolchain.is_micropython() {
                                        "MicroPython 输出"
                                    } else {
                                        "构建输出"
                                    };
                                if output_view_tab(ui, build_view_label, !debug_view)
                                    .on_hover_text("查看构建 / MicroPython 输出")
                                    .clicked()
                                {
                                    self.output_view = OutputView::Build;
                                }
                                if output_view_tab(ui, "调试输出", debug_view)
                                    .on_hover_text("查看调试输出")
                                    .clicked()
                                {
                                    self.output_view = OutputView::Debug;
                                }

                                colored_dot(ui, status_color, 6.0);
                                ui.label(RichText::new(status_text).size(12.0).color(status_color));

                                if debug_view {
                                    ui.add_space(6.0);
                                    ui.label(
                                        RichText::new(format!("{debug_count} 行"))
                                            .size(11.0)
                                            .color(theme::text_faint()),
                                    );
                                } else if problem_count > 0 {
                                    ui.label(
                                        RichText::new(format!("{problem_count} 条问题"))
                                            .size(11.0)
                                            .color(theme::error()),
                                    );
                                }
                                if !debug_view {
                                    if let Some(summary) = &memory_summary {
                                        ui.add_space(8.0);
                                        ui.label(
                                            RichText::new(summary)
                                                .size(11.0)
                                                .color(theme::success()),
                                        )
                                        .on_hover_text(
                                            "Flash = text + data，RAM = data + bss；总量来自链接脚本或芯片支持包",
                                        );
                                    }
                                }

                                if !debug_view {
                                    ui.add_space(6.0);
                                    for (filter, label) in [
                                        (LogFilter::All, "全部"),
                                        (LogFilter::Errors, "错误"),
                                        (LogFilter::Warnings, "警告"),
                                    ] {
                                        let active = self.log_filter == filter;
                                        let color = if active {
                                            theme::accent()
                                        } else {
                                            theme::text_faint()
                                        };
                                        if ui
                                            .add_sized(
                                                [38.0, 22.0],
                                                Button::new(
                                                    RichText::new(label).size(11.5).color(color),
                                                )
                                                .fill(if active {
                                                    theme::accent_soft()
                                                } else {
                                                    Color32::TRANSPARENT
                                                })
                                                .frame(active)
                                                .corner_radius(3),
                                            )
                                            .clicked()
                                        {
                                            self.log_filter = filter;
                                        }
                                    }
                                }

                                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                                    let auto = ui.checkbox(
                                        &mut auto_scroll,
                                        RichText::new("自动滚动").size(12.0).color(theme::text_dim()),
                                    );
                                    auto.on_hover_text("输出增加时自动滚动到底部");

                                    let copy_enabled = log_count > 0;
                                    let copy = ui.add_enabled(
                                        copy_enabled,
                                        small_flat_button("复制", theme::text_dim()),
                                    );
                                    if copy.clicked() {
                                        if debug_view {
                                            actions.push(Action::CopyDebugLog);
                                        } else {
                                            actions.push(Action::CopyLog);
                                        }
                                    }

                                    let clear = ui.add_enabled(
                                        log_count > 0,
                                        small_flat_button("清空", theme::text_dim()),
                                    );
                                    if clear.clicked() {
                                        if debug_view {
                                            actions.push(Action::ClearDebugLog);
                                        } else {
                                            actions.push(Action::ClearLog);
                                        }
                                    }

                                    if build_running && !debug_view {
                                        let stop = ui.add(
                                            Button::new(
                                                RichText::new("停止")
                                                    .size(12.0)
                                                    .color(theme::error()),
                                            )
                                            .fill(theme::error().gamma_multiply(0.12))
                                            .stroke(Stroke::new(
                                                1.0_f32,
                                                theme::error().gamma_multiply(0.45),
                                            ))
                                            .corner_radius(4)
                                            .min_size(egui::vec2(0.0, 21.0)),
                                        );
                                        if stop.clicked() {
                                            actions.push(Action::StopTask);
                                        }
                                    }
                                });
                            },
                        );
                    });
                let header_rect = header.response.rect;
                let handle_color = if handle.hovered() || handle.dragged() {
                    theme::accent()
                } else {
                    theme::border_soft()
                };
                ui.painter().rect_filled(
                    egui::Rect::from_center_size(
                        egui::pos2(header_rect.center().x, header_rect.top() + 2.0),
                        egui::vec2(46.0, 1.0),
                    ),
                    CornerRadius::ZERO,
                    handle_color,
                );

                if open {
                    Frame::new()
                        .fill(theme::editor_bg())
                        .corner_radius(CornerRadius {
                            nw: 0,
                            ne: 0,
                            sw: 8,
                            se: 8,
                        })
                        .inner_margin(Margin::symmetric(10, 5))
                        .show(ui, |ui| {
                            ui.set_min_width(ui.available_width());
                            ui.set_min_height(ui.available_height());
                            if debug_view {
                                self.show_debug_output(ui, actions, &debug_log, auto_scroll);
                            } else {
                                let show_firmware =
                                    !self.build_config.toolchain.is_micropython();
                                if show_firmware {
                                    self.ensure_firmware_analysis(false);
                                }
                                let total_width = ui.available_width();
                                let body_height = ui.available_height();
                                let mut side_width = self.firmware_view.side_width;
                                if side_width <= 0.0 {
                                    side_width = 380.0;
                                }
                                let max_side = (total_width * 0.55).max(280.0);
                                let side_width = if show_firmware {
                                    side_width.clamp(280.0, max_side)
                                } else {
                                    0.0
                                };
                                let handle_width = if show_firmware { 6.0 } else { 0.0 };
                                let log_width = if show_firmware {
                                    (total_width - side_width - handle_width).max(180.0)
                                } else {
                                    total_width
                                };
                                let mut new_side = side_width;
                                ui.horizontal(|ui| {
                                    ui.allocate_ui_with_layout(
                                        egui::vec2(log_width, body_height),
                                        Layout::top_down(Align::Min),
                                        |ui| {
                                            ui.set_min_size(egui::vec2(log_width, body_height));
                                            ui.spacing_mut().item_spacing.y = 0.0;
                                            if self.build.log.is_empty() {
                                                let empty_label =
                                                    if self.build_config.toolchain.is_micropython() {
                                                        "MicroPython 串口输出会显示在这里"
                                                    } else {
                                                        "构建输出会显示在这里"
                                                    };
                                                ui.label(
                                                    RichText::new(empty_label)
                                                        .size(11.0)
                                                        .color(theme::text_faint()),
                                                );
                                            } else {
                                                let row_height = 18.0;
                                                let filter = self.log_filter;
                                                let visible = if filter == LogFilter::All {
                                                    None
                                                } else {
                                                    Some(
                                                        (0..self.build.log.len())
                                                            .filter(|index| {
                                                                self.build
                                                                    .log
                                                                    .line_at(*index)
                                                                    .is_some_and(|line| {
                                                                        filter
                                                                            .matches(line.severity)
                                                                    })
                                                            })
                                                            .collect::<Vec<_>>(),
                                                    )
                                                };
                                                let row_count = visible
                                                    .as_ref()
                                                    .map_or(self.build.log.len(), |indices| {
                                                        indices.len()
                                                    });
                                                if row_count == 0 {
                                                    ui.label(
                                                        RichText::new("没有符合过滤条件的日志")
                                                            .size(11.0)
                                                            .color(theme::text_faint()),
                                                    );
                                                } else {
                                    let mut scroll = ScrollArea::vertical()
                                        .id_salt("simple_embedded_build_log")
                                        .auto_shrink([false, false]);
                                    if auto_scroll {
                                        scroll = scroll.stick_to_bottom(true);
                                    }
                                    scroll.show_rows(ui, row_height, row_count, |ui, range| {
                                        for index in range {
                                            let real_index = visible
                                                .as_ref()
                                                .map_or(index, |indices| indices[index]);
                                            let Some(line) = self.build.log.line_at(real_index)
                                            else {
                                                continue;
                                            };
                                            let severity_color = line.severity.color();
                                            let force_severity =
                                                !matches!(line.severity, Severity::Normal);
                                            let mut job = egui::text::LayoutJob::default();
                                            job.wrap.max_width = f32::INFINITY;
                                            job.break_on_newline = false;
                                            let font = FontId::monospace(11.5);
                                            if line.spans.is_empty() {
                                                job.append(
                                                    " ",
                                                    0.0,
                                                    egui::TextFormat {
                                                        font_id: font,
                                                        color: theme::text(),
                                                        ..Default::default()
                                                    },
                                                );
                                            } else {
                                                for span in &line.spans {
                                                    let color = if force_severity {
                                                        severity_color
                                                    } else {
                                                        span.color.unwrap_or(theme::text())
                                                    };
                                                    job.append(
                                                        &span.text,
                                                        0.0,
                                                        egui::TextFormat {
                                                            font_id: font.clone(),
                                                            color,
                                                            ..Default::default()
                                                        },
                                                    );
                                                }
                                            }
                                            let galley =
                                                ui.fonts_mut(|fonts| fonts.layout_job(job));
                                            let location = line.location.clone();
                                            let sense = if location.is_some() {
                                                Sense::click()
                                            } else {
                                                Sense::hover()
                                            };
                                            let (rect, response) = ui.allocate_exact_size(
                                                egui::vec2(ui.available_width(), row_height),
                                                sense,
                                            );
                                            let response = if location.is_some() {
                                                response
                                                    .on_hover_cursor(egui::CursorIcon::PointingHand)
                                            } else {
                                                response
                                            };
                                            if let Some(location) = location {
                                                if response.clicked() {
                                                    actions.push(Action::OpenAt(
                                                        location.path.clone(),
                                                        location.line,
                                                        location.column,
                                                    ));
                                                }
                                            }
                                            let painter = ui.painter_at(rect);
                                            match line.severity {
                                                Severity::Error => {
                                                    painter.rect_filled(
                                                        rect,
                                                        CornerRadius::ZERO,
                                                        theme::with_alpha(theme::error(), 18),
                                                    );
                                                    painter.rect_filled(
                                                        egui::Rect::from_min_size(
                                                            rect.min,
                                                            egui::vec2(2.0, rect.height()),
                                                        ),
                                                        CornerRadius::ZERO,
                                                        theme::error(),
                                                    );
                                                }
                                                Severity::Warning => {
                                                    painter.rect_filled(
                                                        rect,
                                                        CornerRadius::ZERO,
                                                        theme::with_alpha(theme::warning(), 14),
                                                    );
                                                    painter.rect_filled(
                                                        egui::Rect::from_min_size(
                                                            rect.min,
                                                            egui::vec2(2.0, rect.height()),
                                                        ),
                                                        CornerRadius::ZERO,
                                                        theme::warning(),
                                                    );
                                                }
                                                _ => {}
                                            }
                                            let y = rect.top()
                                                + ((row_height - galley.size().y) * 0.5).max(0.0);
                                            painter.galley(
                                                egui::pos2(rect.left() + 7.0, y),
                                                galley,
                                                severity_color,
                                            );
                                            if line.plain.chars().count() > 170 {
                                                response.on_hover_text(&line.plain);
                                            }
                                        }
                                    });
                                                }
                                            }
                                        },
                                    );
                                    if show_firmware {
                                    let (rect, response) = ui.allocate_exact_size(
                                        egui::vec2(handle_width, body_height),
                                        Sense::drag(),
                                    );
                                    let response = response
                                        .on_hover_cursor(egui::CursorIcon::ResizeHorizontal);
                                    let color = if response.hovered() || response.dragged() {
                                        theme::accent()
                                    } else {
                                        theme::border_soft()
                                    };
                                    ui.painter().rect_filled(
                                        egui::Rect::from_center_size(
                                            rect.center(),
                                            egui::vec2(1.0, rect.height()),
                                        ),
                                        CornerRadius::ZERO,
                                        color,
                                    );
                                    if response.dragged() {
                                        new_side = (new_side - response.drag_delta().x)
                                            .clamp(280.0, max_side);
                                    }
                                    ui.allocate_ui_with_layout(
                                        egui::vec2(new_side, body_height),
                                        Layout::top_down(Align::Min),
                                        |ui| {
                                            ui.set_min_size(egui::vec2(new_side, body_height));
                                            self.show_firmware_view(ui, actions);
                                        },
                                    );
                                    }
                                });
                                if show_firmware {
                                    self.firmware_view.side_width = new_side;
                                }
                            }
                        });
                }
            });

        if toggle_output {
            let next_open = !open;
            self.settings.output_open = next_open;
            if next_open && self.settings.output_height < 100.0 {
                self.settings.output_height = 214.0;
            }
            self.settings.save();
        } else {
            if let Some(new_height) = drag_height {
                self.settings.output_height = new_height;
                self.settings.output_open = output_open;
            }
            if drag_stopped {
                self.settings.save();
            }
        }
        if auto_scroll != self.settings.auto_scroll_output {
            self.settings.auto_scroll_output = auto_scroll;
            self.build.auto_scroll = auto_scroll;
            self.settings.save();
        }
    }

    fn current_stack_pointer(&self) -> Option<u64> {
        if self.debug.session.is_none() || self.debug.running {
            return None;
        }
        for name in ["sp", "msp", "psp"] {
            if let Some(register) = self
                .debug
                .registers
                .iter()
                .find(|register| register.name.eq_ignore_ascii_case(name))
            {
                if let Some(value) = crate::memory::parse_number(&register.value) {
                    return Some(value);
                }
            }
        }
        None
    }

    fn show_firmware_view(&mut self, ui: &mut egui::Ui, _actions: &mut Vec<Action>) {
        if self.debug.session.is_some()
            && !self.debug.running
            && self.debug.watch_refresh.is_none()
            && self
                .debug
                .last_watch_refresh
                .is_none_or(|last| last.elapsed() >= Duration::from_secs(1))
        {
            self.refresh_debug_watch();
        }
        let mut refresh = false;
        Frame::new()
            .fill(theme::panel())
            .stroke(Stroke::new(1.0_f32, theme::border_soft()))
            .corner_radius(6)
            .inner_margin(Margin::symmetric(9, 7))
            .show(ui, |ui| {
                ui.set_min_size(ui.available_size());
                ui.horizontal(|ui| {
                    ui.label(
                        RichText::new("内存可视化")
                            .size(12.5)
                            .strong()
                            .color(theme::text()),
                    );
                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        if ui
                            .add(small_flat_button("刷新", theme::text_faint()))
                            .on_hover_text("重新读取 ELF 符号")
                            .clicked()
                        {
                            refresh = true;
                        }
                    });
                });
                if let Some(path) = self.firmware_view.path.as_ref() {
                    ui.add(
                        Label::new(
                            RichText::new(util::file_name(path))
                                .size(9.5)
                                .color(theme::text_faint()),
                        )
                        .truncate(),
                    )
                    .on_hover_text(util::display_path(path));
                }
                if refresh {
                    self.ensure_firmware_analysis(true);
                }
                if let Some(error) = self.firmware_view.error.as_ref() {
                    ui.label(RichText::new(error).size(10.0).color(theme::warning()));
                }
                let Some(analysis) = self.firmware_view.analysis.as_ref() else {
                    ui.add_space(8.0);
                    ui.label(
                        RichText::new(
                            "构建生成 ELF 后，这里显示全局变量分布、栈使用预估和 FreeRTOS 堆。",
                        )
                        .size(10.0)
                        .color(theme::text_faint()),
                    );
                    return;
                };
                let ram = self
                    .memory_layout
                    .as_ref()
                    .map(|layout| layout.layout.ram)
                    .filter(|region| region.is_valid());
                let sp = self.current_stack_pointer();
                ui.add_space(4.0);
                ScrollArea::vertical()
                    .id_salt("simple_embedded_firmware_view_scroll")
                    .auto_shrink([false, false])
                    .show(ui, |ui| {
                        Self::show_global_distribution(ui, analysis);
                        ui.add_space(8.0);
                        Self::show_stack_estimate(ui, analysis, ram, sp);
                        ui.add_space(8.0);
                        Self::show_heap_usage(ui, analysis);
                        ui.add_space(4.0);
                        ui.label(
                            RichText::new("数据来自 ELF 符号；栈溢出为静态或暂停点预估")
                                .size(9.0)
                                .color(theme::text_faint()),
                        );
                    });
            });
    }

    fn show_global_distribution(ui: &mut egui::Ui, analysis: &crate::memory::FirmwareAnalysis) {
        let palette: [Color32; 7] = [
            theme::accent(),
            theme::success(),
            theme::warning(),
            theme::info(),
            theme::dir(),
            theme::error(),
            theme::accent_dark(),
        ];
        ui.label(
            RichText::new("全局变量分布")
                .size(11.5)
                .strong()
                .color(theme::text()),
        );
        ui.label(
            RichText::new(format!(
                ".data {} · .bss {}",
                crate::memory::format_size(analysis.data_size),
                crate::memory::format_size(analysis.bss_size)
            ))
            .size(9.5)
            .color(theme::text_faint()),
        );
        if !analysis.has_symbols {
            ui.label(
                RichText::new("ELF 未包含符号表，无法列出全局变量")
                    .size(10.0)
                    .color(theme::warning()),
            );
            return;
        }
        if analysis.globals.is_empty() {
            ui.label(
                RichText::new("未在 .data / .bss 中找到带大小的全局变量")
                    .size(10.0)
                    .color(theme::text_faint()),
            );
            return;
        }
        let total = analysis
            .globals
            .iter()
            .map(|symbol| symbol.size)
            .sum::<u64>()
            .max(1);
        let (rect, response) =
            ui.allocate_exact_size(egui::vec2(ui.available_width(), 14.0), Sense::hover());
        let painter = ui.painter_at(rect);
        painter.rect_filled(rect, CornerRadius::same(2), theme::surface());
        let mut left = rect.left();
        for (index, symbol) in analysis.globals.iter().take(12).enumerate() {
            let width = rect.width() * symbol.size as f32 / total as f32;
            let right = (left + width.max(1.0)).min(rect.right());
            if right <= left {
                break;
            }
            painter.rect_filled(
                egui::Rect::from_min_max(
                    egui::pos2(left, rect.top()),
                    egui::pos2(right, rect.bottom()),
                ),
                CornerRadius::same(1),
                palette[index % palette.len()],
            );
            left = right;
            if left >= rect.right() {
                break;
            }
        }
        response.on_hover_text(format!(
            "{} 个 RAM 全局变量，共 {}",
            analysis.globals.len(),
            crate::memory::format_size(total)
        ));
        ui.add_space(3.0);
        for (index, symbol) in analysis.globals.iter().take(14).enumerate() {
            ui.horizontal(|ui| {
                colored_dot(ui, palette[index % palette.len()], 5.0);
                let name_width = (ui.available_width() - 102.0).max(60.0);
                ui.allocate_ui_with_layout(
                    egui::vec2(name_width, 14.0),
                    Layout::left_to_right(Align::Center),
                    |ui| {
                        ui.add(
                            Label::new(
                                RichText::new(&symbol.name)
                                    .monospace()
                                    .size(10.0)
                                    .color(theme::text()),
                            )
                            .truncate(),
                        )
                        .on_hover_text(&symbol.name);
                    },
                );
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    ui.label(
                        RichText::new(crate::memory::format_size(symbol.size))
                            .monospace()
                            .size(9.5)
                            .color(theme::success()),
                    );
                    ui.label(
                        RichText::new(crate::memory::format_hex(symbol.address))
                            .monospace()
                            .size(9.0)
                            .color(theme::text_faint()),
                    );
                    ui.label(
                        RichText::new(&symbol.section)
                            .size(9.0)
                            .color(theme::text_faint()),
                    );
                });
            });
        }
        if analysis.globals.len() > 14 {
            ui.label(
                RichText::new(format!("还有 {} 个变量", analysis.globals.len() - 14))
                    .size(9.0)
                    .color(theme::text_faint()),
            );
        }
    }

    fn show_stack_estimate(
        ui: &mut egui::Ui,
        analysis: &crate::memory::FirmwareAnalysis,
        ram: Option<crate::memory::Region>,
        sp: Option<u64>,
    ) {
        ui.label(
            RichText::new("栈使用预估")
                .size(11.5)
                .strong()
                .color(theme::text()),
        );
        let (Some(top), Some(limit), Some(size)) = (
            analysis.stack_top,
            analysis.stack_limit,
            analysis.stack_size,
        ) else {
            ui.label(
                RichText::new("ELF 中未找到 _estack / _Min_Stack_Size，无法定位栈区")
                    .size(10.0)
                    .color(theme::text_faint()),
            );
            return;
        };
        if size == 0 {
            ui.label(
                RichText::new("栈大小配置为 0，无法计算使用率")
                    .size(10.0)
                    .color(theme::warning()),
            );
            return;
        }
        let used = sp.and_then(|sp| analysis.stack_used(sp));
        let ratio = used.map(|used| (used as f32 / size as f32).clamp(0.0, 1.0));
        let overflow = sp.is_some_and(|sp| sp < limit);
        let risky = ratio.is_some_and(|ratio| ratio > 0.8);
        let color = if overflow {
            theme::error()
        } else if risky {
            theme::warning()
        } else {
            theme::accent()
        };
        let (rect, response) =
            ui.allocate_exact_size(egui::vec2(ui.available_width(), 12.0), Sense::hover());
        let painter = ui.painter_at(rect);
        painter.rect_filled(rect, CornerRadius::same(2), theme::surface());
        if let Some(ratio) = ratio {
            let width = (rect.width() * ratio).max(1.0);
            painter.rect_filled(
                egui::Rect::from_min_size(rect.min, egui::vec2(width, rect.height())),
                CornerRadius::same(2),
                color,
            );
        } else {
            painter.rect_stroke(
                rect,
                CornerRadius::same(2),
                Stroke::new(1.0_f32, theme::accent_dark()),
                egui::StrokeKind::Inside,
            );
        }
        response.on_hover_text(format!(
            "栈顶 {} · 栈底 {} · 预留 {}",
            crate::memory::format_hex(top),
            crate::memory::format_hex(limit),
            crate::memory::format_size(size)
        ));
        if let Some(sp) = sp {
            ui.label(
                RichText::new(format!(
                    "SP {} · 已用 {} / {}",
                    crate::memory::format_hex(sp),
                    crate::memory::format_size(used.unwrap_or(0)),
                    crate::memory::format_size(size)
                ))
                .monospace()
                .size(9.5)
                .color(color),
            );
        } else {
            ui.label(
                RichText::new(format!(
                    "栈顶 {} · 预留 {}",
                    crate::memory::format_hex(top),
                    crate::memory::format_size(size)
                ))
                .monospace()
                .size(9.5)
                .color(theme::text_faint()),
            );
            ui.label(
                RichText::new("目标暂停后读取 SP，显示实时已用量")
                    .size(9.0)
                    .color(theme::text_faint()),
            );
        }
        if let Some(ram) = ram {
            let used_ram = analysis.data_size.saturating_add(analysis.bss_size);
            let remaining = ram.size.saturating_sub(used_ram);
            ui.label(
                RichText::new(format!(
                    "RAM {} · 静态剩余 {}",
                    crate::memory::format_size(ram.size),
                    crate::memory::format_size(remaining)
                ))
                .size(9.0)
                .color(theme::text_faint()),
            );
            if size > remaining {
                ui.label(
                    RichText::new("风险：栈预留超过 RAM 剩余")
                        .size(9.5)
                        .color(theme::error()),
                );
            }
        }
        if overflow {
            ui.label(
                RichText::new("检测到 SP 已越过栈底，可能已经发生栈溢出")
                    .size(9.5)
                    .color(theme::error()),
            );
        } else if risky {
            ui.label(
                RichText::new("警告：当前栈使用已超过 80%")
                    .size(9.5)
                    .color(theme::warning()),
            );
        } else if sp.is_some() {
            ui.label(
                RichText::new("当前 SP 未越过栈底")
                    .size(9.0)
                    .color(theme::success()),
            );
        }
    }

    fn show_heap_usage(ui: &mut egui::Ui, analysis: &crate::memory::FirmwareAnalysis) {
        ui.label(
            RichText::new("堆内存 (FreeRTOS)")
                .size(11.5)
                .strong()
                .color(theme::text()),
        );
        let Some(size) = analysis.heap_size else {
            ui.label(
                RichText::new("未检测到 FreeRTOS 堆（ucHeap / .heap）")
                    .size(10.0)
                    .color(theme::text_faint()),
            );
            return;
        };
        let kind = analysis.heap_kind.as_deref().unwrap_or("FreeRTOS");
        ui.label(
            RichText::new(format!(
                "{kind} · 堆区 {}",
                crate::memory::format_size(size)
            ))
            .size(10.0)
            .color(theme::text_dim()),
        );
        let (rect, response) =
            ui.allocate_exact_size(egui::vec2(ui.available_width(), 12.0), Sense::hover());
        let painter = ui.painter_at(rect);
        painter.rect_filled(rect, CornerRadius::same(2), theme::surface());
        painter.rect_filled(
            egui::Rect::from_min_size(rect.min, egui::vec2(rect.width(), rect.height())),
            CornerRadius::same(2),
            theme::accent_soft(),
        );
        response.on_hover_text("堆区大小来自 ucHeap / .heap 段");
        ui.label(
            RichText::new("运行时空闲和最高水位可在调试输出中查看")
                .size(9.0)
                .color(theme::text_faint()),
        );
    }

    fn show_debug_output(
        &mut self,
        ui: &mut egui::Ui,
        actions: &mut Vec<Action>,
        log: &[String],
        auto_scroll: bool,
    ) {
        let input_height = 28.0;
        let gap = 4.0;
        let body_height = (ui.available_height() - input_height - gap).max(80.0);
        let total_width = ui.available_width();
        let mut side_width = self.debug.side_width;
        if side_width <= 0.0 {
            side_width = 380.0;
        }
        let max_side = (total_width * 0.62).max(260.0);
        let side_width = side_width.clamp(260.0, max_side);
        let handle_width = 6.0;
        let log_width = (total_width - side_width - handle_width).max(180.0);
        let mut new_side = side_width;
        ui.horizontal(|ui| {
            ui.allocate_ui_with_layout(
                egui::vec2(log_width, body_height),
                Layout::top_down(Align::Min),
                |ui| {
                    ui.set_min_size(egui::vec2(log_width, body_height));
                    self.show_debug_log_pane(ui, log, auto_scroll, body_height);
                },
            );
            let (rect, response) =
                ui.allocate_exact_size(egui::vec2(handle_width, body_height), Sense::drag());
            let response = response.on_hover_cursor(egui::CursorIcon::ResizeHorizontal);
            let color = if response.hovered() || response.dragged() {
                theme::accent()
            } else {
                theme::border_soft()
            };
            ui.painter().rect_filled(
                egui::Rect::from_center_size(rect.center(), egui::vec2(1.0, rect.height())),
                CornerRadius::ZERO,
                color,
            );
            if response.dragged() {
                new_side = (new_side - response.drag_delta().x).clamp(260.0, max_side);
            }
            ui.allocate_ui_with_layout(
                egui::vec2(new_side, body_height),
                Layout::top_down(Align::Min),
                |ui| {
                    ui.set_min_size(egui::vec2(new_side, body_height));
                    self.show_debug_side_panel(ui);
                },
            );
        });
        self.debug.side_width = new_side;
        ui.add_space(gap);
        ui.horizontal(|ui| {
            let send_width = 48.0;
            let response = ui.add(
                TextEdit::singleline(&mut self.debug.input)
                    .id(Id::new("simple_embedded_debug_input"))
                    .font(FontId::monospace(11.5))
                    .text_color(theme::text())
                    .background_color(theme::bg())
                    .desired_width((ui.available_width() - send_width - 8.0).max(80.0))
                    .hint_text(
                        RichText::new("gdb 命令，例如 info locals").color(theme::text_faint()),
                    ),
            );
            let submitted =
                response.has_focus() && ui.input(|input| input.key_pressed(egui::Key::Enter));
            if submitted || ui.add(small_flat_button("发送", theme::accent())).clicked() {
                let command = self.debug.input.trim().to_owned();
                if !command.is_empty() {
                    self.debug.input.clear();
                    actions.push(Action::DebugSend(command));
                }
            }
        });
    }

    fn show_debug_log_pane(
        &mut self,
        ui: &mut egui::Ui,
        log: &[String],
        auto_scroll: bool,
        height: f32,
    ) {
        let mut scroll = ScrollArea::vertical()
            .id_salt("simple_embedded_debug_log")
            .max_height(height)
            .auto_shrink([false, false]);
        if auto_scroll {
            scroll = scroll.stick_to_bottom(true);
        }
        scroll.show(ui, |ui| {
            ui.spacing_mut().item_spacing.y = 0.0;
            if let Some(error) = &self.debug.error {
                ui.label(RichText::new(error).size(11.0).color(theme::error()));
            }
            if log.is_empty() {
                let hint = if self.debug.starting {
                    "正在启动 OpenOCD / pyOCD 和 gdb…"
                } else {
                    "启动 gdb 后，这里显示调试输出"
                };
                ui.label(RichText::new(hint).size(11.0).color(theme::text_faint()));
            }
            for line in log {
                let color = if line.contains("Error")
                    || line.contains("error")
                    || line.starts_with("No ")
                {
                    theme::error()
                } else if line.starts_with('[') {
                    theme::info()
                } else {
                    theme::text_dim()
                };
                ui.label(RichText::new(line).monospace().size(11.5).color(color));
            }
        });
    }
}

fn output_view_tab(ui: &mut egui::Ui, label: &str, active: bool) -> egui::Response {
    ui.add(
        Button::new(RichText::new(label).size(12.5).strong().color(if active {
            theme::text()
        } else {
            theme::text_faint()
        }))
        .frame(false),
    )
}

fn small_flat_button(text: &str, color: Color32) -> egui::Button<'static> {
    let is_close = text == "×";
    Button::new(
        RichText::new(text.to_owned())
            .size(if is_close { 16.0 } else { 12.0 })
            .color(color),
    )
    .frame(false)
    .corner_radius(4)
    .min_size(if is_close {
        egui::vec2(30.0, 30.0)
    } else {
        egui::vec2(0.0, 22.0)
    })
}

fn colored_dot(ui: &mut egui::Ui, color: Color32, size: f32) {
    let (rect, _) = ui.allocate_exact_size(egui::vec2(size, size), Sense::hover());
    ui.painter().circle_filled(rect.center(), size * 0.5, color);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_print_values_and_probe_markers() {
        assert_eq!(
            parse_print_value("(gdb) $1 = 0 '\\000'"),
            Some("0 '\\000'".to_owned())
        );
        assert_eq!(parse_print_value("$2 = 41"), Some("41".to_owned()));
        assert_eq!(parse_print_value("Old value = 0"), None);
        assert_eq!(
            parse_print_value("No symbol \"i\" in current context."),
            None
        );
        assert_eq!(
            marker_number("(gdb) __SE_WV_BEGIN_12__", "__SE_WV_BEGIN_"),
            Some(12)
        );
        assert_eq!(marker_number("__SE_WV_END_3__", "__SE_WV_BEGIN_"), None);
    }

    #[test]
    fn find_matches_literal_and_case_insensitive() {
        let mut find = FindState {
            open: true,
            query: "foo".to_owned(),
            ..Default::default()
        };
        find.rebuild("foo Foo foo", 1);
        assert_eq!(find.matches.len(), 3);
        find.case_sensitive = true;
        find.rebuild("foo Foo foo", 1);
        assert_eq!(find.matches.len(), 2);
    }

    #[test]
    fn find_supports_regex_and_reports_errors() {
        let mut find = FindState {
            open: true,
            query: "f.o".to_owned(),
            use_regex: true,
            ..Default::default()
        };
        find.rebuild("foo fxo f.o", 1);
        assert_eq!(find.matches.len(), 3);
        find.query = "[".to_owned();
        find.rebuild("foo", 2);
        assert!(find.error.is_some());
        assert!(find.matches.is_empty());
    }

    #[test]
    fn find_navigation_wraps() {
        let mut find = FindState {
            open: true,
            query: "x".to_owned(),
            ..Default::default()
        };
        find.rebuild("x a x b x", 1);
        assert_eq!(find.matches.len(), 3);
        assert!(find.select_next(0, true));
        assert_eq!(find.active, 0);
        assert!(find.select_next(1, true));
        assert_eq!(find.active, 1);
        assert!(find.select_next(99, true));
        assert_eq!(find.active, 0);
        assert!(find.select_next(0, false));
        assert_eq!(find.active, 2);
    }

    #[test]
    fn find_whole_word_respects_identifier_boundaries() {
        let mut find = FindState {
            open: true,
            query: "foo".to_owned(),
            whole_word: true,
            ..Default::default()
        };
        find.rebuild("foo foobar foo_bar foo", 1);
        assert_eq!(find.matches.len(), 2);
    }

    #[test]
    fn preserve_case_matches_original_style() {
        assert_eq!(preserve_case_replacement("FOO", "bar"), "BAR");
        assert_eq!(preserve_case_replacement("foo", "BAR"), "bar");
        assert_eq!(preserve_case_replacement("Foo", "bar"), "Bar");
        assert_eq!(preserve_case_replacement("fOo", "bar"), "bar");
    }

    #[test]
    fn parses_live_watch_markers_and_values() {
        assert_eq!(
            watch_marker_index("__SE_WATCH_V_7_3__", "__SE_WATCH_V_", 7),
            Some(3)
        );
        assert_eq!(
            watch_marker_index("__SE_WATCH_V_6_3__", "__SE_WATCH_V_", 7),
            None
        );
        assert_eq!(strip_gdb_prompts("(gdb) (gdb) type = int"), "type = int");
        let value = vec!["$2 = {a = 1, b = 2}".to_owned()];
        assert_eq!(clean_watch_value(&value), "{a = 1, b = 2}");
        let type_lines = vec!["type = struct Foo".to_owned()];
        assert_eq!(clean_watch_type(&type_lines), "struct Foo");
    }

    #[test]
    fn parses_cpu_register_rows() {
        let register = parse_register_line("r0             0x20000000          0")
            .expect("register should parse");
        assert_eq!(register.name, "r0");
        assert_eq!(register.value, "0x20000000");
        assert!(parse_register_line("(gdb)").is_none());
        assert!(parse_register_line("r1 invalid").is_none());
    }

    #[test]
    fn unique_copy_path_avoids_existing_names() {
        let dir = tempfile::tempdir().unwrap();
        let source = dir.path().join("main.c");
        std::fs::write(&source, "int main(void) {}").unwrap();
        assert_eq!(
            unique_copy_path(dir.path(), &source),
            dir.path().join("main - 副本.c")
        );
        std::fs::write(dir.path().join("main - 副本.c"), "").unwrap();
        assert_eq!(
            unique_copy_path(dir.path(), &source),
            dir.path().join("main - 副本 (2).c")
        );
    }

    #[test]
    fn board_download_path_preserves_directories_and_avoids_overwrite() {
        let dir = tempfile::tempdir().unwrap();
        let file = crate::micropython::BoardFile {
            path: "lib/helper.py".to_owned(),
            name: "helper.py".to_owned(),
            is_dir: false,
            size: 42,
        };
        let first = board_download_path(dir.path(), &file).unwrap();
        assert_eq!(first, dir.path().join("lib").join("helper.py"));
        std::fs::write(&first, b"local").unwrap();
        let second = board_download_path(dir.path(), &file).unwrap();
        assert_eq!(second, dir.path().join("lib").join("helper - 副本.py"));

        let unsafe_file = crate::micropython::BoardFile {
            path: "../escape.py".to_owned(),
            name: "escape.py".to_owned(),
            is_dir: false,
            size: 0,
        };
        assert_eq!(
            board_download_path(dir.path(), &unsafe_file).unwrap(),
            dir.path().join("escape.py")
        );
    }

    #[test]
    fn copy_recursive_copies_directories() {
        let dir = tempfile::tempdir().unwrap();
        let source = dir.path().join("Core");
        std::fs::create_dir_all(source.join("Src")).unwrap();
        std::fs::write(source.join("Src/main.c"), "int main(void) {}").unwrap();
        let destination = dir.path().join("Core copy");
        copy_recursive(&source, &destination).unwrap();
        assert_eq!(
            std::fs::read_to_string(destination.join("Src/main.c")).unwrap(),
            "int main(void) {}"
        );
    }

    #[test]
    fn remaps_paths_under_a_renamed_directory() {
        let old = Path::new(r"D:\project\Core");
        let new = Path::new(r"D:\project\Firmware");
        assert_eq!(
            remap_path(Path::new(r"D:\project\Core\Src\main.c"), old, new),
            PathBuf::from(r"D:\project\Firmware\Src\main.c")
        );
        assert_eq!(
            remap_path(Path::new(r"D:\project\README.md"), old, new),
            PathBuf::from(r"D:\project\README.md")
        );
    }

    #[test]
    fn detects_build_output_as_debug_program() {
        let dir = tempfile::tempdir().unwrap();
        let build = dir.path().join("build");
        std::fs::create_dir(&build).unwrap();
        let program = build.join("firmware.elf");
        std::fs::write(&program, b"\x7fELF").unwrap();
        assert_eq!(detect_debug_program(dir.path()), Some(program));
    }
}
