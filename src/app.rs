//! MiniEIDE application shell: three-pane layout, tabs, build output and settings.

mod ui;

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{Duration, Instant};

use eframe::egui::{
    self, Align, Button, Color32, CornerRadius, FontId, Frame, Layout, Margin, RichText,
    ScrollArea, Sense, Stroke, ViewportCommand,
};

use crate::build::{BuildController, BuildOutcome, LastBuild, TaskKind};
use crate::config::{autodetect, detected_tool, settings_path, BuildConfig, BuildTarget, Settings};
use crate::editor::Document;
use crate::file_tree::{FileTree, TreeAction};
use crate::highlight::{self, Highlighter};
use crate::log::Severity;
use crate::theme;
use crate::util::{self, format_duration, project_key, truncate_chars};

struct Project {
    root: PathBuf,
    name: String,
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
    font_size: f32,
    word_wrap: bool,
    auto_scroll_output: bool,
    show_hidden: bool,
    note: Option<String>,
    error: Option<String>,
}

enum PendingAction {
    OpenProject(PathBuf),
    CloseTab(usize),
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
    OpenProject(PathBuf),
    OpenFile(PathBuf),
    SelectTab(usize),
    CloseTab(usize),
    SaveActive,
    Run(TaskKind),
    StopTask,
    RefreshTree,
    ToggleHidden,
    OpenSettings,
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
    build: BuildController,
    build_config: BuildConfig,
    settings_draft: Option<SettingsDraft>,
    pending: Option<PendingDialog>,
    toast: Option<Toast>,
    allow_close: bool,
    focus_editor: bool,
    last_title: String,
}

impl App {
    pub fn new(
        _cc: &eframe::CreationContext<'_>,
        initial_project: Option<PathBuf>,
        initial_file: Option<PathBuf>,
    ) -> Self {
        let settings = Settings::load();
        let mut app = Self {
            build: BuildController::default(),
            highlighter: Arc::new(Highlighter::new()),
            project: None,
            tabs: Vec::new(),
            active: None,
            tree: None,
            build_config: BuildConfig::default(),
            settings_draft: None,
            pending: None,
            toast: None,
            allow_close: false,
            focus_editor: false,
            last_title: String::new(),
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

        let config = self.settings.build_config_for(&root);
        let key = project_key(&root);
        if !self.settings.build_configs.contains_key(&key) {
            self.settings.set_build_config(&root, &config);
        }
        let tool_known = detected_tool(&root).is_some();

        let project_name = Project::new(root.clone()).name;
        self.project = Some(Project::new(root.clone()));
        self.build_config = config;
        self.tree = Some(FileTree::new(root.clone(), self.settings.show_hidden));
        self.tabs.clear();
        self.active = None;
        self.focus_editor = false;
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
        let Some(doc) = self.tabs.get_mut(index) else {
            return true;
        };
        let name = doc.name.clone();
        match doc.save() {
            Ok(()) => {
                self.set_toast(format!("已保存 {name}"), Severity::Success);
                true
            }
            Err(err) => {
                self.set_toast(format!("保存 {name} 失败：{err}"), Severity::Error);
                false
            }
        }
    }

    fn save_all_dirty(&mut self) -> Result<usize, String> {
        let mut saved = 0usize;
        for doc in self.tabs.iter_mut().filter(|doc| doc.is_dirty()) {
            doc.save()
                .map_err(|err| format!("保存 {} 失败：{err}", doc.name))?;
            saved += 1;
        }
        Ok(saved)
    }

    fn save_all_with_toast(&mut self) {
        match self.save_all_dirty() {
            Ok(0) => {}
            Ok(count) => self.set_toast(format!("已保存 {count} 个文件"), Severity::Success),
            Err(err) => self.set_toast(err, Severity::Error),
        }
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

    fn close_tab_now(&mut self, index: usize) {
        if index >= self.tabs.len() {
            return;
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
            config: self.build_config.clone(),
            font_size: self.settings.font_size,
            word_wrap: self.settings.word_wrap,
            auto_scroll_output: self.settings.auto_scroll_output,
            show_hidden: self.settings.show_hidden,
            note: None,
            error: None,
        });
    }

    fn set_toast(&mut self, message: impl Into<String>, severity: Severity) {
        self.toast = Some(Toast {
            message: message.into(),
            severity,
            born: Instant::now(),
        });
    }

    fn handle_shortcuts(&mut self, ctx: &egui::Context) {
        let modal_open = self.settings_draft.is_some() || self.pending.is_some();
        if modal_open {
            if ctx.input(|input| input.key_pressed(egui::Key::Escape)) {
                self.settings_draft = None;
                self.pending = None;
            }
            return;
        }

        let (open, save, save_all, build, rebuild, close_tab, open_settings, zoom_in, zoom_out) =
            ctx.input(|input| {
                let command = input.modifiers.command;
                (
                    command && input.key_pressed(egui::Key::O),
                    command && input.key_pressed(egui::Key::S),
                    command && input.modifiers.shift && input.key_pressed(egui::Key::S),
                    command && !input.modifiers.shift && input.key_pressed(egui::Key::B),
                    command && input.modifiers.shift && input.key_pressed(egui::Key::B),
                    command && input.key_pressed(egui::Key::W),
                    command && input.key_pressed(egui::Key::Comma),
                    command
                        && (input.key_pressed(egui::Key::Plus)
                            || input.key_pressed(egui::Key::Equals)),
                    command && input.key_pressed(egui::Key::Minus),
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
                Action::OpenProject(path) => self.request_open_project(path),
                Action::OpenFile(path) => self.open_file(path),
                Action::SelectTab(index) => {
                    if index < self.tabs.len() {
                        self.active = Some(index);
                        self.focus_editor = true;
                    }
                }
                Action::CloseTab(index) => self.request_close_tab(index),
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
                }
                Action::ToggleHidden => {
                    self.settings.show_hidden = !self.settings.show_hidden;
                    if let Some(tree) = &mut self.tree {
                        tree.set_show_hidden(self.settings.show_hidden);
                    }
                    self.settings.save();
                }
                Action::OpenSettings => self.open_settings(),
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

    fn sync_title(&mut self, ctx: &egui::Context) {
        let title = if let Some(project) = &self.project {
            let file = self
                .active_document()
                .map(|doc| format!("{} — ", doc.name))
                .unwrap_or_default();
            let dirty = if self.has_unsaved() { " ●" } else { "" };
            format!("{file}{}{dirty} — MiniEIDE", project.name)
        } else {
            "MiniEIDE — 嵌入式极简工作台".to_owned()
        };
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
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        let ctx = ui.ctx().clone();
        self.build.poll();
        self.handle_close_request(&ctx);
        self.handle_shortcuts(&ctx);
        self.handle_dropped_files(&ctx);

        let mut actions = Vec::new();

        self.show_top_bar(ui, &mut actions);
        self.show_status_bar(ui);

        if self.project.is_some() {
            self.show_output_panel(ui, &mut actions);
            self.show_side_panel(ui, &mut actions);
        }
        self.show_center(ui, &mut actions);

        if self.settings_draft.is_some() {
            self.show_settings_modal(&ctx);
            actions.clear();
        }
        if self.pending.is_some() {
            self.show_pending_modal(&ctx);
            actions.clear();
        }

        if self.settings_draft.is_none() && self.pending.is_none() {
            self.apply_actions(&ctx, actions);
        }

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

impl App {
    fn show_top_bar(&mut self, ui: &mut egui::Ui, actions: &mut Vec<Action>) {
        let running = self.build.is_running();
        let has_project = self.project.is_some();
        let mode = self.build_config.mode_summary();
        let can_build = has_project && !running && self.build_config.has_command(TaskKind::Build);
        let can_clean = has_project && !running && self.build_config.has_command(TaskKind::Clean);
        let can_rebuild =
            has_project && !running && self.build_config.has_command(TaskKind::Rebuild);
        let can_flash = has_project && !running && self.build_config.has_command(TaskKind::Flash);
        let (status_color, status_text) = self.build_status();
        let project_info = self.project.as_ref().map(|project| {
            (
                truncate_chars(&project.name, 24),
                util::display_path(&project.root),
            )
        });
        let dirty = self.dirty_count();

        egui::Panel::top("mini_eide_top_bar")
            .exact_size(58.0)
            .frame(
                Frame::new()
                    .fill(theme::PANEL)
                    .inner_margin(Margin::symmetric(14, 8))
                    .stroke(Stroke::new(1.0_f32, theme::BORDER_SOFT)),
            )
            .show_inside(ui, |ui| {
                ui.horizontal_centered(|ui| {
                    brand(ui);

                    ui.add_space(8.0);
                    if let Some((name, path)) = &project_info {
                        project_chip(ui, name, &mode, path, dirty > 0);
                        ui.add_space(4.0);
                        let response = ui.add(theme::secondary_button("打开工程"));
                        if response.clicked() {
                            actions.push(Action::OpenPicker);
                        }
                        response.on_hover_text("打开另一个工程文件夹 (Ctrl+O)");
                    } else {
                        let response = ui.add(theme::primary_button("打开工程文件夹"));
                        if response.clicked() {
                            actions.push(Action::OpenPicker);
                        }
                        response.on_hover_text("直接选择任意文件夹作为工程 (Ctrl+O)");
                    }

                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        if ui
                            .add(theme::ghost_button("设置"))
                            .on_hover_text("构建命令与编辑器设置 (Ctrl+,)")
                            .clicked()
                        {
                            actions.push(Action::OpenSettings);
                        }

                        ui.add_space(4.0);
                        if has_project && running {
                            let stop = egui::Button::new(
                                RichText::new("停止")
                                    .size(12.0)
                                    .strong()
                                    .color(theme::ERROR),
                            )
                            .fill(theme::ERROR.gamma_multiply(0.13))
                            .stroke(Stroke::new(1.0_f32, theme::ERROR.gamma_multiply(0.55)))
                            .corner_radius(6)
                            .min_size(egui::vec2(56.0, 28.0));
                            if ui.add(stop).on_hover_text("终止构建进程树").clicked() {
                                actions.push(Action::StopTask);
                            }
                            ui.spinner();
                            ui.label(RichText::new(status_text).size(11.5).color(status_color));
                        } else if has_project {
                            let tip = if self.build_config.has_command(TaskKind::Flash) {
                                "调用配置的烧录命令（在本地工程目录执行）"
                            } else {
                                "尚未配置烧录命令，请打开设置填写"
                            };
                            let response =
                                ui.add_enabled(can_flash, theme::secondary_button("烧录"));
                            let response = if can_flash {
                                response.on_hover_text(tip)
                            } else {
                                response.on_disabled_hover_text(tip)
                            };
                            if response.clicked() {
                                actions.push(Action::Run(TaskKind::Flash));
                            }

                            let rebuild_tip = if can_rebuild {
                                "清理后重新构建 (Ctrl+Shift+B)"
                            } else {
                                "Rebuild 命令尚未配置"
                            };
                            let response =
                                ui.add_enabled(can_rebuild, theme::secondary_button("重构建"));
                            let response = if can_rebuild {
                                response.on_hover_text(rebuild_tip)
                            } else {
                                response.on_disabled_hover_text(rebuild_tip)
                            };
                            if response.clicked() {
                                actions.push(Action::Run(TaskKind::Rebuild));
                            }

                            let clean_tip = if can_clean {
                                "执行 Clean 命令"
                            } else {
                                "Clean 命令尚未配置"
                            };
                            let response =
                                ui.add_enabled(can_clean, theme::secondary_button("清理"));
                            let response = if can_clean {
                                response.on_hover_text(clean_tip)
                            } else {
                                response.on_disabled_hover_text(clean_tip)
                            };
                            if response.clicked() {
                                actions.push(Action::Run(TaskKind::Clean));
                            }

                            let build_tip = if can_build {
                                "构建工程 (Ctrl+B)"
                            } else if has_project {
                                "Build 命令尚未配置，点击“设置”填写"
                            } else {
                                "请先打开工程"
                            };
                            let response = ui.add_enabled(can_build, theme::primary_button("构建"));
                            let response = if can_build {
                                response.on_hover_text(build_tip)
                            } else {
                                response.on_disabled_hover_text(build_tip)
                            };
                            if response.clicked() {
                                actions.push(Action::Run(TaskKind::Build));
                            }
                        }
                    });
                });
            });
    }

    fn show_status_bar(&mut self, ui: &mut egui::Ui) {
        let (status_color, status_text) = self.build_status();
        let mode = if self.project.is_some() {
            self.build_config.mode_summary()
        } else {
            "未打开工程".to_owned()
        };
        let active = self.active_document();
        let dirty = self.dirty_count();
        let mut right_parts: Vec<String> = Vec::new();
        if let Some(doc) = active {
            right_parts.push(format!("行 {}, 列 {}", doc.cursor_line, doc.cursor_column));
            right_parts.push("UTF-8".to_owned());
            right_parts.push(doc.newline_label().to_owned());
            right_parts.push(doc.language.clone());
            right_parts.push(format!("{} 行", doc.line_count()));
        } else if self.project.is_some() {
            right_parts.push("从左侧打开文件".to_owned());
        } else {
            right_parts.push("Ctrl+O 打开工程".to_owned());
        }

        egui::Panel::bottom("mini_eide_status_bar")
            .exact_size(24.0)
            .frame(
                Frame::new()
                    .fill(theme::PANEL)
                    .inner_margin(Margin::symmetric(10, 2))
                    .stroke(Stroke::new(1.0_f32, theme::BORDER_SOFT)),
            )
            .show_inside(ui, |ui| {
                ui.spacing_mut().interact_size.y = 18.0;
                ui.horizontal_centered(|ui| {
                    colored_dot(ui, status_color, 6.0);
                    ui.label(RichText::new(status_text).size(11.0).color(status_color));
                    ui.label(RichText::new("·").size(11.0).color(theme::BORDER));
                    ui.label(RichText::new(mode).size(10.5).color(theme::TEXT_FAINT));
                    if dirty > 0 {
                        ui.label(
                            RichText::new(format!("· {dirty} 个文件未保存"))
                                .size(10.5)
                                .color(theme::WARNING),
                        );
                    }

                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        ui.label(
                            RichText::new(right_parts.join("   ·   "))
                                .size(10.5)
                                .color(theme::TEXT_FAINT),
                        );
                    });
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

        let mut panel = egui::Panel::bottom("mini_eide_build_output");
        panel = if open {
            panel
                .resizable(true)
                .default_size(214.0)
                .min_size(100.0)
                .max_size(520.0)
        } else {
            panel.resizable(false).exact_size(36.0)
        };

        panel
            .frame(Frame::new().fill(theme::EDITOR_BG))
            .show_inside(ui, |ui| {
                Frame::new()
                    .fill(theme::PANEL_RAISED)
                    .inner_margin(Margin::symmetric(10, 3))
                    .stroke(Stroke::new(1.0_f32, theme::BORDER_SOFT))
                    .show(ui, |ui| {
                        ui.spacing_mut().interact_size.y = 22.0;
                        ui.horizontal(|ui| {
                            let toggle_label = if open { "收起" } else { "展开" };
                            if ui
                                .add(
                                    Button::new(
                                        RichText::new(toggle_label)
                                            .size(9.5)
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
                                    .size(11.5)
                                    .strong()
                                    .color(theme::TEXT_DIM),
                            );

                            colored_dot(ui, status_color, 6.0);
                            ui.label(RichText::new(status_text).size(10.5).color(status_color));

                            if problem_count > 0 {
                                ui.label(
                                    RichText::new(format!("{problem_count} 条问题"))
                                        .size(10.0)
                                        .color(theme::ERROR),
                                );
                            }

                            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                                let auto = ui.checkbox(&mut auto_scroll, "自动滚动");
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
                                            RichText::new("停止").size(10.5).color(theme::ERROR),
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
                    let body_height = ui.available_height();
                    Frame::new()
                        .fill(theme::EDITOR_BG)
                        .inner_margin(Margin::symmetric(10, 5))
                        .show(ui, |ui| {
                            ui.set_min_height(170.0);
                            if self.build.log.is_empty() {
                                let hint = if self.project.is_none() {
                                    "打开工程后，构建输出会显示在这里。"
                                } else if self.build_config.build_command.trim().is_empty() {
                                    "尚未配置构建命令。点击“设置”填写 Build / Clean / Rebuild。"
                                } else {
                                    "尚无构建输出。点击上方「构建」开始，或按 Ctrl+B。"
                                };
                                ui.vertical_centered(|ui| {
                                    ui.add_space((body_height * 0.10).clamp(8.0, 24.0));
                                    ui.label(
                                        RichText::new("▶").size(20.0).color(theme::TEXT_FAINT),
                                    );
                                    ui.label(
                                        RichText::new(hint).size(11.5).color(theme::TEXT_FAINT),
                                    );
                                    if !self.build_config.build_command.trim().is_empty() {
                                        ui.label(
                                            RichText::new(format!(
                                                "$ {}",
                                                self.build_config.build_command
                                            ))
                                            .monospace()
                                            .size(10.5)
                                            .color(theme::TEXT_FAINT),
                                        );
                                    }
                                });
                            } else {
                                ui.spacing_mut().item_spacing.y = 0.0;
                                let row_height = 18.0;
                                let mut scroll = ScrollArea::vertical()
                                    .id_salt("mini_eide_build_log")
                                    .auto_shrink([false, false]);
                                if auto_scroll {
                                    scroll = scroll.stick_to_bottom(true);
                                }
                                scroll.show_rows(
                                    ui,
                                    row_height,
                                    self.build.log.len(),
                                    |ui, range| {
                                        for index in range {
                                            let Some(line) = self.build.log.line_at(index) else {
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
                                            let (rect, response) = ui.allocate_exact_size(
                                                egui::vec2(ui.available_width(), row_height),
                                                Sense::hover(),
                                            );
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
                                    },
                                );
                            }
                        });
                }
            });

        if toggle_output {
            self.settings.output_open = !self.settings.output_open;
            self.settings.save();
        }
        if auto_scroll != self.settings.auto_scroll_output {
            self.settings.auto_scroll_output = auto_scroll;
            self.build.auto_scroll = auto_scroll;
            self.settings.save();
        }
    }
}

fn brand(ui: &mut egui::Ui) {
    let (rect, _) = ui.allocate_exact_size(egui::vec2(30.0, 30.0), Sense::hover());
    paint_logo(ui.painter(), rect);
    ui.add_space(1.0);
    ui.scope(|ui| {
        ui.spacing_mut().item_spacing.y = 0.0;
        ui.vertical(|ui| {
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing.x = 0.0;
                ui.label(RichText::new("Mini").size(14.5).strong().color(theme::TEXT));
                ui.label(
                    RichText::new("EIDE")
                        .size(14.5)
                        .strong()
                        .color(theme::ACCENT),
                );
            });
            ui.label(
                RichText::new("嵌入式极简工作台")
                    .size(9.5)
                    .color(theme::TEXT_FAINT),
            );
        });
    });
}

fn project_chip(ui: &mut egui::Ui, name: &str, mode: &str, path: &str, dirty: bool) {
    Frame::new()
        .fill(theme::SURFACE)
        .stroke(Stroke::new(1.0_f32, theme::BORDER))
        .corner_radius(6)
        .inner_margin(Margin::symmetric(9, 4))
        .show(ui, |ui| {
            ui.horizontal(|ui| {
                colored_dot(
                    ui,
                    if dirty {
                        theme::WARNING
                    } else {
                        theme::SUCCESS
                    },
                    5.0,
                );
                ui.label(RichText::new(name).size(12.0).strong().color(theme::TEXT));
                ui.label(RichText::new(mode).size(9.5).color(theme::TEXT_FAINT));
            });
        })
        .response
        .on_hover_text(path);
}

fn small_flat_button(text: &str, color: Color32) -> egui::Button<'static> {
    Button::new(RichText::new(text.to_owned()).size(10.5).color(color))
        .frame(false)
        .corner_radius(4)
        .min_size(egui::vec2(0.0, 21.0))
}

fn colored_dot(ui: &mut egui::Ui, color: Color32, size: f32) {
    let (rect, _) = ui.allocate_exact_size(egui::vec2(size, size), Sense::hover());
    ui.painter().circle_filled(rect.center(), size * 0.5, color);
}

fn paint_logo(painter: &egui::Painter, rect: egui::Rect) {
    let radius = (rect.width() * 0.24).round().clamp(3.0, 20.0) as u8;
    painter.rect_filled(rect, CornerRadius::same(radius), theme::BORDER);
    painter.rect_filled(
        rect.shrink(1.0),
        CornerRadius::same(radius.saturating_sub(1)),
        theme::PANEL_RAISED,
    );

    let w = rect.width();
    let h = rect.height();
    let stroke = Stroke::new((w * 0.055).max(1.4), theme::ACCENT);
    let left = rect.left();
    let top = rect.top();
    let cy = rect.center().y;

    painter.line_segment(
        [
            egui::pos2(left + w * 0.45, top + h * 0.36),
            egui::pos2(left + w * 0.34, cy),
        ],
        stroke,
    );
    painter.line_segment(
        [
            egui::pos2(left + w * 0.34, cy),
            egui::pos2(left + w * 0.45, top + h * 0.64),
        ],
        stroke,
    );
    painter.line_segment(
        [
            egui::pos2(left + w * 0.55, top + h * 0.36),
            egui::pos2(left + w * 0.66, cy),
        ],
        stroke,
    );
    painter.line_segment(
        [
            egui::pos2(left + w * 0.66, cy),
            egui::pos2(left + w * 0.55, top + h * 0.64),
        ],
        stroke,
    );
    painter.circle_filled(
        egui::pos2(left + w * 0.50, top + h * 0.50),
        (w * 0.032).max(0.8),
        theme::SUCCESS,
    );
}
