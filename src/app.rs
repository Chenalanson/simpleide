//! Simple embedded application shell: workspace, setup steps, tabs and build output.

mod ui;

use std::collections::{BTreeMap, BTreeSet};
use std::hash::{Hash, Hasher};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{Duration, Instant};

use eframe::egui::{
    self, Align, Button, Color32, CornerRadius, FontId, Frame, Layout, Margin, RichText,
    ScrollArea, Sense, Stroke, ViewportCommand,
};
#[cfg(target_os = "windows")]
use raw_window_handle::{HasWindowHandle, RawWindowHandle};

use crate::build::{BuildController, BuildOutcome, LastBuild, TaskKind};
use crate::config::{
    detected_tool, settings_path, BuildConfig, BuildOptions, BuildTarget, Settings, APP_AUTHOR,
    APP_NAME,
};
use crate::editor::Document;
use crate::file_tree::{FileTree, TreeAction};
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
    Up,
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

struct SettingsDraft {
    config: BuildConfig,
    extra_paths_text: String,
    font_size: f32,
    word_wrap: bool,
    relative_line_numbers: bool,
    show_whitespace: bool,
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
    items: Vec<String>,
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
    DebugStepOver,
    DebugStepInto,
    DebugFinish,
    DebugStop,
    DebugSend(String),
    SelectTab(usize),
    CloseTab(usize),
    CloseOtherTabs(usize),
    CloseTabsToRight(usize),
    CloseSavedTabs,
    CloseAllTabs,
    SplitTab(PathBuf, SplitDirection),
    CloseSplit,
    SaveActive,
    Run(TaskKind),
    StopTask,
    RefreshTree,
    ToggleHidden,
    OpenSettings,
    EditMeta,
    AddMetaItem(MetaListField),
    EditBuildOptions,
    ClearLog,
    CopyLog,
    Reveal(PathBuf),
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
    build: BuildController,
    build_config: BuildConfig,
    settings_draft: Option<SettingsDraft>,
    meta_draft: Option<MetaDraft>,
    meta_add: Option<MetaAddDraft>,
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
    completion: CompletionState,
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
            build_config: BuildConfig::default(),
            settings_draft: None,
            meta_draft: None,
            meta_add: None,
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
            completion: CompletionState::default(),
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
        let root = match util::canonical_project(path) {
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

        let mut config = self.settings.build_config_for(&root);
        if let Some(makefile) = makefile::find_makefile(&root) {
            let fallback = config.build_options.clone();
            if let Some(options) = makefile::read_options(&makefile, &fallback) {
                config.build_options = options;
            }
        }
        let key = project_key(&root);
        if !self.settings.build_configs.contains_key(&key) {
            self.settings.set_build_config(&root, &config);
        }
        let tool_known = detected_tool(&root).is_some();

        let project_name = Project::new(root.clone()).name;
        self.project_meta = ProjectMeta::detect(&root);
        self.project = Some(Project::new(root.clone()));
        self.build_config = config;
        self.tree = Some(FileTree::new(
            root.clone(),
            self.settings.show_hidden,
            self.settings.respect_gitignore,
            &self.settings.ignore_patterns,
        ));
        self.tabs.clear();
        self.active = None;
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

        if announce {
            if tool_known || !self.build_config.build_command.trim().is_empty() {
                self.set_toast(format!("已打开工程 {project_name}"), Severity::Success);
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
        match crate::registers::scan_project(&root, &ignore) {
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

    fn current_word(&self) -> Option<String> {
        let doc = self.active_document()?;
        crate::navigation::word_at(doc.buffer.as_string(), doc.cursor_index).map(|value| value.0)
    }

    fn ensure_symbols(&mut self) {
        if self.symbols.items.is_empty() {
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

    fn open_completion(&mut self) {
        let Some(doc) = self.active_document() else {
            return;
        };
        let text = doc.buffer.as_string().to_owned();
        let cursor = doc.cursor_index;
        let (prefix_start, prefix) = crate::navigation::word_prefix(&text, cursor);
        self.ensure_symbols();
        let lower = prefix.to_lowercase();
        let mut names = BTreeSet::new();
        for symbol in &self.symbols.items {
            if symbol.name.to_lowercase().starts_with(&lower) {
                names.insert(symbol.name.clone());
            }
            if names.len() >= 200 {
                break;
            }
        }
        if names.is_empty() {
            self.set_toast("没有匹配的工程符号", Severity::Info);
            return;
        }
        self.completion = CompletionState {
            open: true,
            query: prefix,
            prefix_start,
            cursor,
            items: names.into_iter().collect(),
        };
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
        self.focus_editor = true;
        self.find.dirty = true;
    }

    fn debug_target(&self) -> Option<PathBuf> {
        let configured = self.settings.debug_program.trim();
        if !configured.is_empty() {
            let path = PathBuf::from(configured);
            return path.is_file().then_some(path);
        }
        let root = self.project.as_ref()?.root.clone();
        let mut candidates = Vec::new();
        for dir in [
            root.join("build"),
            root.join("Debug"),
            root.join("Release"),
            root.join("out"),
            root.clone(),
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

    fn start_debug_session(&mut self) -> bool {
        let Some(target) = self.debug_target() else {
            self.set_toast(
                "未找到调试程序，请在设置中填写 ELF/EXE 路径",
                Severity::Warning,
            );
            self.debug.open = true;
            return false;
        };
        let Some(gdb) = crate::debugger::detect_gdb(&self.settings.gdb_path) else {
            self.set_toast("未找到 gdb，请在设置中填写 gdb 路径", Severity::Warning);
            self.debug.open = true;
            return false;
        };
        match crate::debugger::DebugSession::start(&target, &gdb) {
            Ok(mut session) => {
                for (path, line) in &self.debug.breakpoints {
                    session.send(&format!("break \"{}\":{}", gdb_source_path(path), line));
                }
                self.debug.session = Some(session);
                self.debug.target = Some(target.clone());
                self.debug.error = None;
                self.debug.log.clear();
                self.debug.open = true;
                self.set_toast(
                    format!("调试会话已启动：{}", util::display_path(&target)),
                    Severity::Success,
                );
                true
            }
            Err(err) => {
                self.debug.error = Some(err.clone());
                self.debug.open = true;
                self.set_toast(err, Severity::Error);
                false
            }
        }
    }

    fn debug_continue(&mut self) {
        if self.debug.session.is_none() && !self.start_debug_session() {
            return;
        }
        if let Some(session) = &mut self.debug.session {
            if !session.started {
                session.send("run");
                session.started = true;
            } else {
                session.send("continue");
            }
        }
    }

    fn debug_command(&mut self, command: &str) {
        let Some(session) = &mut self.debug.session else {
            self.set_toast("请先启动调试会话", Severity::Warning);
            return;
        };
        session.send(command);
    }

    fn debug_stop(&mut self) {
        if let Some(mut session) = self.debug.session.take() {
            session.poll();
            self.debug.log = session.log.clone();
            session.stop();
            self.debug.log.push("[调试已停止]".to_owned());
        }
        self.debug.error = None;
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
            SplitDirection::Up => {
                self.start_split(path, active_path, direction, 0);
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

    fn toggle_breakpoint(&mut self) {
        let Some(doc) = self.active_document() else {
            return;
        };
        let path = doc.path.clone();
        let line = doc.cursor_line;
        let key = (path.clone(), line);
        let added = if self.debug.breakpoints.contains(&key) {
            self.debug.breakpoints.remove(&key);
            if let Some(session) = &mut self.debug.session {
                session.send(&format!("clear \"{}\":{}", gdb_source_path(&path), line));
            }
            false
        } else {
            self.debug.breakpoints.insert(key);
            if let Some(session) = &mut self.debug.session {
                session.send(&format!("break \"{}\":{}", gdb_source_path(&path), line));
            }
            true
        };
        self.set_toast(
            if added {
                format!("已添加断点：第 {line} 行")
            } else {
                format!("已移除断点：第 {line} 行")
            },
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
            }
            Err(err) => {
                self.symbols.items.clear();
                self.symbols.error = Some(err);
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
            self.git_line_key = None;
            self.refresh_git_status();
        }
        saved
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
            self.set_toast(format!("{} 命令尚未配置", kind.label()), Severity::Warning);
            self.open_settings();
            return;
        }
        self.build.log.clear_diagnostics();

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

        let spec = match self.build_config.make_spec(kind, &project_root) {
            Ok(spec) => spec,
            Err(err) => {
                self.set_toast(err, Severity::Error);
                return;
            }
        };

        self.settings.output_open = true;
        self.settings.save();
        match self.build.start(spec, kind) {
            Ok(()) => {}
            Err(err) => self.set_toast(err, Severity::Error),
        }
    }

    fn open_settings(&mut self) {
        self.settings_draft = Some(SettingsDraft {
            extra_paths_text: self.build_config.extra_paths.join("\n"),
            config: self.build_config.clone(),
            font_size: self.settings.font_size,
            word_wrap: self.settings.word_wrap,
            relative_line_numbers: self.settings.relative_line_numbers,
            show_whitespace: self.settings.show_whitespace,
            ruler_column: self.settings.ruler_column,
            use_spaces: self.settings.use_spaces,
            indent_width: self.settings.indent_width,
            autosave: self.settings.autosave,
            autosave_delay_ms: self.settings.autosave_delay_ms,
            format_on_save: self.settings.format_on_save,
            clang_format_path: self.settings.clang_format_path.clone(),
            debug_program: self.settings.debug_program.clone(),
            gdb_path: self.settings.gdb_path.clone(),
            auto_scroll_output: self.settings.auto_scroll_output,
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

        self.settings.set_build_config(&root, &self.build_config);
        self.settings.save();

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
            self.project_meta = ProjectMeta::detect(&root);
        }
        Ok(synced)
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
        let before = makefile::find_makefile(&root)
            .and_then(|path| makefile::read_options(&path, &fallback))
            .unwrap_or_else(|| fallback.clone());
        self.build_config.build_options = options.clone();
        self.settings.set_build_config(&root, &self.build_config);
        self.settings.save();

        let result = self.sync_makefile_options(&root, &before, &options);
        self.notify_sync("已保存构建选项", result, Severity::Success);
        self.project_meta = ProjectMeta::detect(&root);
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
        let modal_open = self.settings_draft.is_some()
            || self.pending.is_some()
            || self.meta_draft.is_some()
            || self.meta_add.is_some()
            || self.build_options_draft.is_some();
        if modal_open {
            if ctx.input(|input| input.key_pressed(egui::Key::Escape)) {
                self.settings_draft = None;
                self.pending = None;
                self.meta_draft = None;
                self.meta_add = None;
                self.build_options_draft = None;
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
            goto_line_open,
        ) = ctx.input(|input| {
            let command = input.modifiers.command;
            (
                command && !input.modifiers.shift && input.key_pressed(egui::Key::O),
                command && !input.modifiers.shift && input.key_pressed(egui::Key::S),
                command && input.modifiers.shift && input.key_pressed(egui::Key::S),
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
                chord_active && command && input.key_pressed(egui::Key::Backslash),
                command && !input.modifiers.shift && input.key_pressed(egui::Key::G),
            )
        });

        if open {
            self.pick_project_folder();
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
            self.debug_command("next");
        } else if debug_step {
            self.debug_command("step");
        } else if debug_finish {
            self.debug_command("finish");
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
        }
        if close_tab_f4 {
            if let Some(index) = self.active {
                self.request_close_tab(index);
            }
        }
        if split_right && !chord_split_up {
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
        if self.array.open && ctx.input(|input| input.key_pressed(egui::Key::Escape)) {
            self.array.open = false;
            self.focus_editor = true;
        }
        if self.debug.open && ctx.input(|input| input.key_pressed(egui::Key::Escape)) {
            self.debug.open = false;
        }
        if self.completion.open && ctx.input(|input| input.key_pressed(egui::Key::Escape)) {
            self.completion.open = false;
            self.focus_editor = true;
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
            if path.is_dir() {
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

    fn apply_actions(&mut self, ctx: &egui::Context, actions: Vec<Action>) {
        for action in actions {
            match action {
                Action::OpenPicker => self.pick_project_folder(),
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
                Action::OpenDebug => self.debug.open = true,
                Action::DebugContinue => self.debug_continue(),
                Action::DebugStepOver => self.debug_command("next"),
                Action::DebugStepInto => self.debug_command("step"),
                Action::DebugFinish => self.debug_command("finish"),
                Action::DebugStop => self.debug_stop(),
                Action::DebugSend(command) => self.debug_command(&command),
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
                Action::CloseSplit => {
                    self.split_path = None;
                    self.split_primary = None;
                    self.split_focus = 0;
                }
                Action::SaveActive => {
                    self.save_active();
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
                    if let Some(root) = self.project.as_ref().map(|project| project.root.clone()) {
                        self.project_meta = ProjectMeta::detect(&root);
                    }
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
                Action::Reveal(path) => util::open_in_file_manager(&path),
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

fn gdb_source_path(path: &Path) -> String {
    util::display_path(path).replace('\\', "/")
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
        let ctx = ui.ctx().clone();
        self.build.poll();
        if let Some(session) = &mut self.debug.session {
            session.poll();
            if self.debug.open {
                ctx.request_repaint_after(Duration::from_millis(120));
            }
        }
        self.refresh_diagnostics();
        self.handle_close_request(&ctx);
        self.handle_shortcuts(&ctx);
        self.handle_dropped_files(&ctx);
        self.style_native_window(&ctx, frame);

        let mut actions = Vec::new();

        self.show_status_bar(ui, &mut actions);
        self.show_action_rail(ui, &mut actions);
        self.show_side_panel(ui, &mut actions);
        self.show_output_panel(ui, &mut actions);
        self.show_center(ui, &mut actions);

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
        if self.build_options_draft.is_some() {
            self.show_build_options_modal(&ctx);
            actions.clear();
        }
        let modal_open = self.settings_draft.is_some()
            || self.pending.is_some()
            || self.meta_draft.is_some()
            || self.meta_add.is_some()
            || self.build_options_draft.is_some();
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
        if self.array.open && !modal_open {
            self.show_array_generator(&ctx);
        }
        if self.debug.open && !modal_open {
            self.show_debug_window(&ctx, &mut actions);
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
            && self.build_options_draft.is_none()
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
            .exact_size(110.0)
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
                                ui.add_sized([width, 30.0], theme::secondary_button("重构建"))
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
                            "调用配置的烧录命令（在本地工程目录执行）"
                        } else if has_project {
                            "尚未配置烧录命令，请打开设置填写"
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
                            "打开调试窗口 (F5 继续)"
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
                    }
                });
            });
    }

    fn show_output_panel(&mut self, ui: &mut egui::Ui, actions: &mut Vec<Action>) {
        let open = self.settings.output_open;
        let running = self.build.is_running();
        let log_count = self.build.log.len();
        let problem_count = self.build.log.problem_count();
        let mut auto_scroll = self.settings.auto_scroll_output;
        let mut toggle_output = false;
        let (status_color, status_text) = self.build_status();
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
                let (handle_rect, handle) =
                    ui.allocate_exact_size(egui::vec2(ui.available_width(), 10.0), Sense::drag());
                let handle = handle.on_hover_cursor(egui::CursorIcon::ResizeVertical);
                let handle_color = if handle.hovered() || handle.dragged() {
                    theme::ACCENT
                } else {
                    theme::BORDER_SOFT
                };
                ui.painter().rect_filled(
                    egui::Rect::from_center_size(handle_rect.center(), egui::vec2(46.0, 1.0)),
                    CornerRadius::ZERO,
                    handle_color,
                );
                if handle.dragged() {
                    let new_height = (height - handle.drag_delta().y).clamp(40.0, full_height);
                    drag_height = Some(new_height);
                    output_open = new_height > 44.0;
                }
                if handle.drag_stopped() {
                    drag_stopped = true;
                }

                Frame::new()
                    .fill(theme::PANEL_RAISED)
                    .corner_radius(CornerRadius::same(6))
                    .inner_margin(Margin::symmetric(10, 3))
                    .show(ui, |ui| {
                        ui.spacing_mut().interact_size.y = 24.0;
                        ui.horizontal(|ui| {
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
                            ui.label(
                                RichText::new("构建输出")
                                    .size(12.5)
                                    .strong()
                                    .color(theme::TEXT_DIM),
                            );

                            colored_dot(ui, status_color, 6.0);
                            ui.label(RichText::new(status_text).size(12.0).color(status_color));

                            if problem_count > 0 {
                                ui.label(
                                    RichText::new(format!("{problem_count} 条问题"))
                                        .size(11.0)
                                        .color(theme::ERROR),
                                );
                            }

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
                                        [38.0, 18.0],
                                        Button::new(RichText::new(label).size(11.0).color(color))
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

                            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                                let auto = ui.checkbox(
                                    &mut auto_scroll,
                                    RichText::new("自动滚动").size(12.0).color(theme::TEXT_DIM),
                                );
                                auto.on_hover_text("构建输出增加时自动滚动到底部");

                                let copy_enabled = log_count > 0;
                                let copy = ui.add_enabled(
                                    copy_enabled,
                                    small_flat_button("复制", theme::TEXT_DIM),
                                );
                                if copy.clicked() {
                                    actions.push(Action::CopyLog);
                                }

                                let clear = ui.add_enabled(
                                    log_count > 0,
                                    small_flat_button("清空", theme::TEXT_DIM),
                                );
                                if clear.clicked() {
                                    actions.push(Action::ClearLog);
                                }

                                if running {
                                    let stop = ui.add(
                                        Button::new(
                                            RichText::new("停止").size(12.0).color(theme::ERROR),
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
                        });
                    });

                if open {
                    Frame::new()
                        .fill(theme::EDITOR_BG)
                        .inner_margin(Margin::symmetric(10, 5))
                        .show(ui, |ui| {
                            if !self.build.log.is_empty() {
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
}
