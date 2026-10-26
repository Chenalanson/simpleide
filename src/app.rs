//! Simple embedded application shell: workspace, setup steps, tabs and build output.

mod ui;

use std::collections::{BTreeMap, BTreeSet};
use std::hash::{Hash, Hasher};
use std::path::{Path, PathBuf};
use std::sync::mpsc::Receiver;
use std::sync::Arc;
use std::time::{Duration, Instant, SystemTime};

use eframe::egui::{
    self, Align, Button, Color32, CornerRadius, FontId, Frame, Id, Layout, Margin, RichText,
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
    format_on_save: bool,
    clang_format_path: String,
    debug_program: String,
    gdb_path: String,
    auto_scroll_output: bool,
    show_minimap: bool,
    show_indent_guides: bool,
    show_hover_info: bool,
    show_hidden: bool,
    respect_gitignore: bool,
    ignore_patterns: String,
    note: Option<String>,
    error: Option<String>,
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
    peripheral_initialized: bool,
    peripheral_refresh: Option<u64>,
    peripheral_id: u64,
    peripheral_last_refresh: Option<Instant>,
    /// Last source location reported by gdb, used for the execution marker.
    current_source: Option<(PathBuf, usize)>,
    /// Number of console lines already scanned for stop locations.
    log_scan: usize,
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
struct HwWatchpoint {
    number: Option<usize>,
    expression: String,
    kind: WatchKind,
    hardware: Option<bool>,
    hits: usize,
    old_value: Option<String>,
    new_value: Option<String>,
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
    EditMeta,
    AddMetaItem(MetaListField),
    EditBuildOptions,
    AddSupportPack,
    ImportSupportPack(PathBuf),
    EditFlashConfig,
    EditMemoryLayout,
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
    hex: HexState,
    array: ArrayState,
    debug: DebugState,
    debug_start: Option<Receiver<Result<(crate::debugger::DebugSession, PathBuf), String>>>,
    create_entry: Option<CreateEntryDraft>,
    rename_entry: Option<RenameEntryDraft>,
    delete_entry: Option<DeleteEntryDraft>,
    memory_draft: Option<MemoryDraft>,
    memory_layout: Option<crate::memory::ResolvedMemory>,
    memory_usage_cache: Option<(PathBuf, SystemTime, Option<crate::memory::MemoryUsage>)>,
    memory_check: MemoryCheckState,
    file_clipboard: Option<FileClipboard>,
    image_preview: Option<ImagePreviewState>,
    output_view: OutputView,
    completion: CompletionState,
    member_index: Option<crate::completion::MemberIndex>,
    goto_line: GotoLineState,
    column_edit: ColumnEditState,
    column_selection: Option<ColumnSelectionState>,
    split_path: Option<PathBuf>,
    split_primary: Option<PathBuf>,
    split_direction: SplitDirection,
    split_focus: usize,
    scroll_to_cursor: bool,
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
        _cc: &eframe::CreationContext<'_>,
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
            hex: HexState::default(),
            array: ArrayState::default(),
            debug: DebugState::default(),
            debug_start: None,
            create_entry: None,
            rename_entry: None,
            delete_entry: None,
            memory_draft: None,
            memory_layout: None,
            memory_usage_cache: None,
            memory_check: MemoryCheckState::default(),
            file_clipboard: None,
            image_preview: None,
            output_view: OutputView::default(),
            completion: CompletionState::default(),
            member_index: None,
            goto_line: GotoLineState::default(),
            column_edit: ColumnEditState::default(),
            column_selection: None,
            split_path: None,
            split_primary: None,
            split_direction: SplitDirection::Right,
            split_focus: 0,
            scroll_to_cursor: false,
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
        self.focus_editor = false;
        self.git_line_marks.clear();
        self.git_line_key = None;
        self.refresh_git_status();
        self.settings.remember_project(&root);
        self.settings.save();

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
                    "工程已打开，但没有检测到构建系统，请打开设置填写命令",
                    Severity::Warning,
                );
            }
        }
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
                self.registers.source = index.source;
                self.registers.truncated = index.truncated;
                self.registers.error = None;
            }
            Err(err) => {
                self.registers.items.clear();
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
        let Some(session) = &mut self.debug.session else {
            self.set_toast("请先启动调试会话", Severity::Warning);
            return;
        };
        session.send("set can-use-hw-watchpoints 1");
        session.send(&format!("{} {expression}", kind.command()));
        self.memory_check.watchpoints.push(HwWatchpoint {
            number: None,
            expression: expression.to_owned(),
            kind,
            hardware: None,
            hits: 0,
            old_value: None,
            new_value: None,
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

        let mut hit = None;
        let mut created_watch = false;
        for (index, line) in fresh.iter().enumerate() {
            let trimmed = line.trim();
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
            for value in fresh.iter().skip(index + 1).take(6) {
                let value = value.trim();
                if let Some(old) = value.strip_prefix("Old value = ") {
                    old_value = Some(old.to_owned());
                }
                if let Some(new) = value.strip_prefix("New value = ") {
                    new_value = Some(new.to_owned());
                }
                if old_value.is_some() && new_value.is_some() {
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
            let position = match position {
                Some(position) => position,
                None if old_value.is_none() && new_value.is_none() => {
                    self.memory_check.watchpoints.push(HwWatchpoint {
                        number: Some(notice.number),
                        expression: notice.expression.clone(),
                        kind: WatchKind::Write,
                        hardware: Some(notice.hardware),
                        hits: 0,
                        old_value: None,
                        new_value: None,
                    });
                    self.memory_check.watchpoints.len() - 1
                }
                None => continue,
            };
            let watchpoint = &mut self.memory_check.watchpoints[position];
            watchpoint.number = Some(notice.number);
            watchpoint.hardware = Some(notice.hardware);
            if old_value.is_some() || new_value.is_some() {
                watchpoint.hits = watchpoint.hits.saturating_add(1);
                watchpoint.old_value = old_value.clone();
                watchpoint.new_value = new_value.clone();
                hit = Some((
                    notice.number,
                    watchpoint.expression.clone(),
                    old_value,
                    new_value,
                ));
            } else {
                created_watch = true;
            }
        }
        if created_watch {
            self.memory_check.watch_error = None;
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

    fn references_at_cursor(&mut self) {
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
        let items = self.completion_items(&context);
        if items.is_empty() {
            self.set_toast("没有可用的补全项", Severity::Info);
            self.completion.open = false;
            return;
        }
        self.completion.open = true;
        self.completion.query = context.prefix;
        self.completion.prefix_start = context.prefix_start;
        self.completion.cursor = cursor;
        self.completion.selected = 0;
        self.completion.items = items;
        self.completion.member_chain = context.member_chain;
        self.completion.path = Some(path);
        self.completion.last_revision = revision;
        self.completion.last_cursor = cursor;
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
            || self.pending.is_some()
            || self.meta_draft.is_some()
            || self.meta_add.is_some()
            || self.flash_draft.is_some()
            || self.build_options_draft.is_some()
            || self.create_entry.is_some()
            || self.rename_entry.is_some()
            || self.delete_entry.is_some()
            || self.memory_draft.is_some()
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
                if items.is_empty() {
                    self.completion.open = false;
                } else {
                    let query_changed = self.completion.query != context.prefix
                        || self.completion.member_chain != context.member_chain;
                    self.completion.open = true;
                    self.completion.query = context.prefix;
                    self.completion.prefix_start = context.prefix_start;
                    self.completion.cursor = cursor;
                    self.completion.items = items;
                    self.completion.member_chain = context.member_chain;
                    if query_changed {
                        self.completion.selected = 0;
                    }
                    self.completion.path = Some(path.clone());
                }
            } else {
                self.completion.open = false;
            }
            self.completion.last_revision = revision;
            self.completion.last_cursor = cursor;
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
        if self.registers.items.is_empty() {
            self.refresh_registers();
        }
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
                        "未找到 arm-none-eabi-gdb，请在“设置 → 调试”填写 ARM gdb 路径；MinGW 的 gdb 不能调试 ARM 目标",
                    );
                }
            }
        } else {
            match crate::debugger::detect_gdb(&configured_gdb) {
                Some(gdb) => gdb,
                None => {
                    return self.debug_start_error("未找到 gdb，请在“设置 → 调试”填写 gdb 路径");
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
                    match crate::flash::openocd_server_spec(&self.build_config.openocd, &root, port)
                    {
                        Ok(spec) => Some((spec, port, crate::debugger::ServerKind::OpenOcd)),
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
                        Ok(spec) => Some((spec, port, crate::debugger::ServerKind::PyOcd)),
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
        self.memory_check.watchpoints.clear();
        self.memory_check.watch_error = None;
        self.memory_check.watch_log_scan = 0;
        self.debug.log.clear();
        self.debug.target = Some(target.clone());
        self.debug.watch_refresh = None;
        self.debug.last_watch_refresh = None;
        self.debug.current_source = None;
        self.debug.log_scan = 0;
        self.debug.registers.clear();
        self.debug.peripheral_values.clear();
        self.debug.peripheral_open.clear();
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
            let result = if let Some((spec, port, server_kind)) = server {
                crate::debugger::DebugSession::start_embedded(
                    &target_for_thread,
                    &gdb_for_thread,
                    spec,
                    port,
                    server_kind,
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
                let continue_now = self.debug.pending_continue;
                self.debug.pending_continue = false;
                if continue_now {
                    if let Some(session) = &mut self.debug.session {
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
        session.interrupt();
        self.set_toast("正在暂停目标", Severity::Info);
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
            self.debug.error = None;
            self.debug.log.push("[调试启动已取消]".to_owned());
            self.memory_check.watchpoints.clear();
            self.memory_check.watch_error = None;
            self.memory_check.watch_log_scan = 0;
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
        self.debug.watch_refresh = None;
        self.debug.last_watch_refresh = None;
        self.debug.error = None;
        self.debug.current_source = None;
        self.debug.log_scan = 0;
        self.memory_check.watchpoints.clear();
        self.memory_check.watch_error = None;
        self.memory_check.watch_log_scan = 0;
        self.debug.peripheral_values.clear();
        self.debug.peripheral_open.clear();
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
        self.open_file(path.to_path_buf());
        self.focus_editor = false;
        let Some(index) = self.tabs.iter().position(|doc| same_path(&doc.path, path)) else {
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
            tree.reveal(path);
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
        let filter_active = !filter.is_empty();
        let mut selected = Vec::new();
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
        for doc in self
            .tabs
            .iter_mut()
            .filter(|doc| doc.is_dirty() && !doc.read_only)
        {
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
            doc.update_cursor(target);
            set_editor_cursor(ctx, &doc.path, target);
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

    fn start_task(&mut self, kind: TaskKind) {
        if self.build.is_running() {
            self.set_toast("已有任务正在运行", Severity::Warning);
            return;
        }
        let Some(project_root) = self.project.as_ref().map(|project| project.root.clone()) else {
            self.set_toast("请先打开一个工程文件夹", Severity::Warning);
            return;
        };
        if !self.build_config.has_command(kind) {
            self.set_toast(format!("{} 尚未配置", kind.label()), Severity::Warning);
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
        self.settings_draft = Some(SettingsDraft {
            tab: SettingsTab::Build,
            extra_paths_text: self.build_config.extra_paths.join("\n"),
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
            format_on_save: self.settings.format_on_save,
            clang_format_path: self.settings.clang_format_path.clone(),
            debug_program,
            gdb_path,
            auto_scroll_output: self.settings.auto_scroll_output,
            show_minimap: self.settings.show_minimap,
            show_indent_guides: self.settings.show_indent_guides,
            show_hover_info: self.settings.show_hover_info,
            show_hidden: self.settings.show_hidden,
            respect_gitignore: self.settings.respect_gitignore,
            ignore_patterns: self.settings.ignore_patterns.clone(),
            note: None,
            error: None,
        });
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
        if self
            .tabs
            .iter()
            .any(|document| same_path(&document.path, &makefile_path))
        {
            return Err("Makefile 已在编辑器中打开，请先关闭后再同步".to_owned());
        }
        let synced = makefile::sync_lists(&root, &includes, &library_dirs, &defines)?.is_some();
        if synced {
            self.refresh_project_meta();
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

    fn handle_shortcuts(&mut self, ctx: &egui::Context) {
        let modal_open = self.has_modal();
        if modal_open {
            if ctx.input(|input| input.key_pressed(egui::Key::Escape)) {
                self.settings_draft = None;
                self.pending = None;
                self.meta_draft = None;
                self.meta_add = None;
                self.flash_draft = None;
                self.build_options_draft = None;
                self.create_entry = None;
                self.rename_entry = None;
                self.delete_entry = None;
                self.memory_draft = None;
            }
            return;
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
                command && input.key_pressed(egui::Key::F2),
                !command && !input.modifiers.shift && input.key_pressed(egui::Key::F2),
                !command && input.modifiers.shift && input.key_pressed(egui::Key::F2),
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
            self.open_settings();
        }
        if bookmark_toggle {
            self.toggle_bookmark();
        }
        if bookmark_next {
            self.goto_bookmark(ctx, true);
        } else if bookmark_prev {
            self.goto_bookmark(ctx, false);
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
            self.refresh_registers();
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
        if self.column_selection.is_some()
            && ctx.input(|input| input.key_pressed(egui::Key::Escape))
        {
            self.column_selection = None;
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
        self.refresh_support_packs();
        self.refresh_git_status();
        self.rename_entry = None;
        self.set_toast(format!("已重命名为 {name}"), Severity::Success);
    }

    fn start_delete(&mut self, path: PathBuf) {
        let is_dir = path.is_dir();
        self.delete_entry = Some(DeleteEntryDraft { path, is_dir });
    }

    fn commit_delete(&mut self) {
        let Some(draft) = self.delete_entry.take() else {
            return;
        };
        if path_has_dirty_tab(&self.tabs, &draft.path) {
            self.set_toast("请先保存或关闭该文件，再删除", Severity::Warning);
            return;
        }
        if let Err(err) = remove_path(&draft.path) {
            self.set_toast(
                format!("无法删除 {}：{err}", util::display_path(&draft.path)),
                Severity::Error,
            );
            return;
        }
        self.remove_path_references(&draft.path);
        if let Some(tree) = &mut self.tree {
            tree.refresh();
        }
        self.member_index = None;
        self.refresh_support_packs();
        self.refresh_git_status();
        self.set_toast(
            format!("已删除 {}", util::file_name(&draft.path)),
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
                Action::OpenRegisters => {
                    self.registers.open = true;
                    self.refresh_registers();
                }
                Action::OpenHex(path) => self.open_hex(&path),
                Action::OpenArrayGenerator => self.open_array_generator(ctx),
                Action::OpenDebug => {
                    self.debug.open = true;
                    self.focus_debug_output();
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
                Action::EditMeta => self.open_meta_editor(),
                Action::AddMetaItem(field) => self.open_meta_add(field),
                Action::EditBuildOptions => self.open_build_options(),
                Action::AddSupportPack => self.add_support_pack(),
                Action::ImportSupportPack(path) => self.import_support_pack_files(vec![path]),
                Action::EditFlashConfig => self.open_flash_config(),
                Action::EditMemoryLayout => self.open_memory_layout(),
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
            ctx.send_viewport_cmd(ViewportCommand::SetTheme(egui::SystemTheme::Dark));
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

    fn sync_title(&mut self, ctx: &egui::Context) {
        let base = if let Some(doc) = self.active_document() {
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
        if let Some(kind) = self.build.running_kind() {
            let elapsed = self.build.elapsed().unwrap_or_default();
            return (
                theme::ACCENT,
                format!("{}中… {}", kind.verb(), format_duration(elapsed)),
            );
        }
        if let Some(last) = self.build.last() {
            return build_result_status(last);
        }
        (theme::TEXT_FAINT, "就绪".to_owned())
    }
}

fn build_result_status(last: &LastBuild) -> (Color32, String) {
    let duration = format_duration(last.duration);
    match &last.outcome {
        BuildOutcome::Success => (
            theme::SUCCESS,
            format!("{}完成 · {duration}", last.kind.verb()),
        ),
        BuildOutcome::Failed(Some(code)) => (
            theme::ERROR,
            format!("{}失败 · 退出码 {code}", last.kind.verb()),
        ),
        BuildOutcome::Failed(None) => {
            (theme::ERROR, format!("{}失败 · 进程终止", last.kind.verb()))
        }
        BuildOutcome::Cancelled => (theme::WARNING, format!("{}已停止", last.kind.verb())),
        BuildOutcome::Error(_) => (theme::ERROR, format!("{}无法执行", last.kind.verb())),
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
        self.build.poll();
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

        self.show_status_bar(ui, &mut actions);
        self.show_action_rail(ui, &mut actions);
        self.show_side_panel(ui, &mut actions);
        self.show_output_panel(ui, &mut actions);
        self.show_center(ui, &mut actions);
        self.update_completion(&ctx);

        if self.settings_draft.is_some() {
            self.show_settings_modal(&ctx);
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
        if self.debug.open && !modal_open {
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
        {
            self.apply_actions(&ctx, actions);
        }

        self.refresh_git_line_marks();
        self.tick_autosave(&ctx);
        self.show_toast(&ctx);
        self.sync_title(&ctx);

        if self.build.is_running() {
            ctx.request_repaint_after(Duration::from_millis(80));
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

    let dark = 1_i32;
    unsafe {
        let result = DwmSetWindowAttribute(
            hwnd,
            DWMWA_USE_IMMERSIVE_DARK_MODE,
            (&dark as *const i32).cast(),
            std::mem::size_of::<i32>() as u32,
        );
        if result != 0 {
            DwmSetWindowAttribute(
                hwnd,
                DWMWA_USE_IMMERSIVE_DARK_MODE_OLD,
                (&dark as *const i32).cast(),
                std::mem::size_of::<i32>() as u32,
            );
        }

        let caption = colorref(theme::PANEL);
        DwmSetWindowAttribute(
            hwnd,
            DWMWA_CAPTION_COLOR,
            (&caption as *const u32).cast(),
            std::mem::size_of::<u32>() as u32,
        );
        let border = colorref(theme::BORDER_SOFT);
        DwmSetWindowAttribute(
            hwnd,
            DWMWA_BORDER_COLOR,
            (&border as *const u32).cast(),
            std::mem::size_of::<u32>() as u32,
        );
        let text = colorref(theme::TEXT);
        DwmSetWindowAttribute(
            hwnd,
            DWMWA_TEXT_COLOR,
            (&text as *const u32).cast(),
            std::mem::size_of::<u32>() as u32,
        );
    }
}

impl App {
    fn show_action_rail(&mut self, ui: &mut egui::Ui, actions: &mut Vec<Action>) {
        let running = self.build.is_running();
        let has_project = self.project.is_some();
        let can_build = has_project && !running && self.build_config.has_command(TaskKind::Build);
        let can_clean = has_project && !running && self.build_config.has_command(TaskKind::Clean);
        let can_rebuild =
            has_project && !running && self.build_config.has_command(TaskKind::Rebuild);
        let can_flash = has_project && !running && self.build_config.has_command(TaskKind::Flash);

        egui::Panel::right("simple_embedded_action_rail")
            .show_separator_line(false)
            .exact_size(58.0)
            .frame(
                Frame::new()
                    .fill(theme::GUTTER)
                    .inner_margin(Margin::same(3)),
            )
            .show_inside(ui, |ui| {
                ui.painter().rect(
                    ui.max_rect(),
                    CornerRadius::same(8),
                    theme::PANEL,
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
                                .color(theme::ERROR),
                        )
                        .fill(theme::ERROR.gamma_multiply(0.13))
                        .stroke(Stroke::new(1.0_f32, theme::ERROR.gamma_multiply(0.55)))
                        .corner_radius(6)
                        .min_size(egui::vec2(width, 34.0));
                        if ui.add(stop).on_hover_text("终止构建进程树").clicked() {
                            actions.push(Action::StopTask);
                        }
                        ui.add_space(8.0);
                        ui.spinner();
                    } else {
                        let build_tip = if can_build {
                            "构建工程 (Ctrl+B)"
                        } else if has_project {
                            "请先在左上角工程卡片中配置构建环境"
                        } else {
                            "请先导入工程并配置构建环境"
                        };
                        let button = if can_build {
                            theme::primary_button("构建").min_size(egui::vec2(width, 34.0))
                        } else {
                            egui::Button::new(
                                RichText::new("构建").color(theme::TEXT_FAINT).size(12.5),
                            )
                            .fill(theme::SURFACE)
                            .stroke(Stroke::new(1.0_f32, theme::BORDER_SOFT))
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

                        ui.add_space(6.0);
                        let flash_tip = if can_flash {
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
                                ui.add_sized([width, 30.0], theme::secondary_button("烧录"))
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

                        ui.add_space(6.0);
                        let debug_tip = if self.debug.session.is_some() {
                            "打开调试控制 (F5 继续)"
                        } else {
                            "启动 gdb 调试（需在设置中配置调试程序）"
                        };
                        let response = ui
                            .add_enabled_ui(has_project, |ui| {
                                ui.add_sized([width, 30.0], theme::secondary_button("调试"))
                            })
                            .inner
                            .on_hover_text(debug_tip);
                        if response.clicked() {
                            actions.push(Action::OpenDebug);
                        }

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
                (theme::ERROR, "错误".to_owned())
            } else if debug_starting {
                (theme::ACCENT, "启动中".to_owned())
            } else if debug_has_session {
                if debug_running {
                    (theme::SUCCESS, "调试器已连接".to_owned())
                } else {
                    (theme::TEXT_FAINT, "调试器已停止".to_owned())
                }
            } else {
                (theme::TEXT_FAINT, "未启动".to_owned())
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
                    .fill(theme::GUTTER)
                    .inner_margin(Margin::same(4)),
            )
            .show_inside(ui, |ui| {
                ui.painter().rect(
                    ui.max_rect(),
                    CornerRadius::same(8),
                    theme::PANEL,
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
                    .fill(theme::PANEL_RAISED)
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
                                                .color(theme::TEXT_FAINT),
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
                                    theme::ACCENT,
                                );
                                if output_view_tab(ui, "构建输出", !debug_view)
                                    .on_hover_text("查看构建输出")
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
                                            .color(theme::TEXT_FAINT),
                                    );
                                } else if problem_count > 0 {
                                    ui.label(
                                        RichText::new(format!("{problem_count} 条问题"))
                                            .size(11.0)
                                            .color(theme::ERROR),
                                    );
                                }
                                if !debug_view {
                                    if let Some(summary) = &memory_summary {
                                        ui.add_space(8.0);
                                        ui.label(
                                            RichText::new(summary)
                                                .size(11.0)
                                                .color(theme::SUCCESS),
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
                                            theme::ACCENT
                                        } else {
                                            theme::TEXT_FAINT
                                        };
                                        if ui
                                            .add_sized(
                                                [38.0, 22.0],
                                                Button::new(
                                                    RichText::new(label).size(11.5).color(color),
                                                )
                                                .fill(if active {
                                                    theme::ACCENT_SOFT
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
                                        RichText::new("自动滚动").size(12.0).color(theme::TEXT_DIM),
                                    );
                                    auto.on_hover_text("输出增加时自动滚动到底部");

                                    let copy_enabled = log_count > 0;
                                    let copy = ui.add_enabled(
                                        copy_enabled,
                                        small_flat_button("复制", theme::TEXT_DIM),
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
                                        small_flat_button("清空", theme::TEXT_DIM),
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
                                                    .color(theme::ERROR),
                                            )
                                            .fill(theme::ERROR.gamma_multiply(0.12))
                                            .stroke(Stroke::new(
                                                1.0_f32,
                                                theme::ERROR.gamma_multiply(0.45),
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
                    theme::ACCENT
                } else {
                    theme::BORDER_SOFT
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
                        .fill(theme::EDITOR_BG)
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
                            } else if !self.build.log.is_empty() {
                                ui.spacing_mut().item_spacing.y = 0.0;
                                let row_height = 18.0;
                                let filter = self.log_filter;
                                let visible = if filter == LogFilter::All {
                                    None
                                } else {
                                    Some(
                                        (0..self.build.log.len())
                                            .filter(|index| {
                                                self.build.log.line_at(*index).is_some_and(|line| {
                                                    filter.matches(line.severity)
                                                })
                                            })
                                            .collect::<Vec<_>>(),
                                    )
                                };
                                let row_count = visible
                                    .as_ref()
                                    .map_or(self.build.log.len(), |indices| indices.len());
                                if row_count == 0 {
                                    ui.label(
                                        RichText::new("没有符合过滤条件的日志")
                                            .size(11.0)
                                            .color(theme::TEXT_FAINT),
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
                                                        color: theme::TEXT,
                                                        ..Default::default()
                                                    },
                                                );
                                            } else {
                                                for span in &line.spans {
                                                    let color = if force_severity {
                                                        severity_color
                                                    } else {
                                                        span.color.unwrap_or(theme::TEXT)
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
                                                        Color32::from_rgba_unmultiplied(
                                                            239, 112, 118, 18,
                                                        ),
                                                    );
                                                    painter.rect_filled(
                                                        egui::Rect::from_min_size(
                                                            rect.min,
                                                            egui::vec2(2.0, rect.height()),
                                                        ),
                                                        CornerRadius::ZERO,
                                                        theme::ERROR,
                                                    );
                                                }
                                                Severity::Warning => {
                                                    painter.rect_filled(
                                                        rect,
                                                        CornerRadius::ZERO,
                                                        Color32::from_rgba_unmultiplied(
                                                            233, 181, 100, 14,
                                                        ),
                                                    );
                                                    painter.rect_filled(
                                                        egui::Rect::from_min_size(
                                                            rect.min,
                                                            egui::vec2(2.0, rect.height()),
                                                        ),
                                                        CornerRadius::ZERO,
                                                        theme::WARNING,
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
                theme::ACCENT
            } else {
                theme::BORDER_SOFT
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
                    .text_color(theme::TEXT)
                    .background_color(theme::BG)
                    .desired_width((ui.available_width() - send_width - 8.0).max(80.0))
                    .hint_text(
                        RichText::new("gdb 命令，例如 info locals").color(theme::TEXT_FAINT),
                    ),
            );
            let submitted =
                response.has_focus() && ui.input(|input| input.key_pressed(egui::Key::Enter));
            if submitted || ui.add(small_flat_button("发送", theme::ACCENT)).clicked() {
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
                ui.label(RichText::new(error).size(11.0).color(theme::ERROR));
            }
            if log.is_empty() {
                let hint = if self.debug.starting {
                    "正在启动 OpenOCD / pyOCD 和 gdb…"
                } else {
                    "启动 gdb 后，这里显示调试输出"
                };
                ui.label(RichText::new(hint).size(11.0).color(theme::TEXT_FAINT));
            }
            for line in log {
                let color = if line.contains("Error")
                    || line.contains("error")
                    || line.starts_with("No ")
                {
                    theme::ERROR
                } else if line.starts_with('[') {
                    theme::INFO
                } else {
                    theme::TEXT_DIM
                };
                ui.label(RichText::new(line).monospace().size(11.5).color(color));
            }
        });
    }
}

fn output_view_tab(ui: &mut egui::Ui, label: &str, active: bool) -> egui::Response {
    ui.add(
        Button::new(RichText::new(label).size(12.5).strong().color(if active {
            theme::TEXT
        } else {
            theme::TEXT_FAINT
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
