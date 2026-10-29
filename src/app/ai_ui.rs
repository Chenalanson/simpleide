//! AI mode layout: three large panels, chat rendering and the enter transition.
//!
//! The layout intentionally does not reuse the editor panels. AI mode owns the
//! whole window so the fog/desktop reveal and the two meteors can cover every
//! widget without fighting panel clipping.

use std::path::Path;
use std::time::Duration;

use eframe::egui::{
    self, Align, Align2, Button, Color32, CornerRadius, FontId, Frame, Id, Label, Layout, Margin,
    Rect, RichText, ScrollArea, Sense, Stroke, TextEdit, Ui, UiBuilder,
};

use super::{AiBuildStatus, AiChatMessage, AiCliStatus, AiEditStatus, AiFsStatus, App};
use crate::framework::{FrameworkDoc, GanttChart, GanttKind};
use crate::log::Severity;
use crate::theme;

pub(super) const AI_EARLY_FOG_SECS: f32 = 0.0;
/// AI mode intentionally uses a larger UI scale because its panels contain a
/// lot of dense architecture information; the normal editor keeps the user's
/// own zoom factor.
const AI_UI_ZOOM: f32 = 1.20;
/// Keep the enter transition cheap and smooth: no fog, meteor or layered-window
/// alpha work. The four panels fade/scale in within one second.
const TRANSITION_SECS: f32 = 1.0;
const METEOR_RIGHT_START: f32 = 1.15;
const METEOR_RIGHT_END: f32 = 1.95;
const METEOR_OUTPUT_START: f32 = 1.35;
const METEOR_OUTPUT_END: f32 = 2.25;
const PANEL_START: f32 = 0.0;
const PANEL_STAGGER: f32 = 0.05;
const PANEL_DURATION: f32 = 0.80;
const INPUT_ID: &str = "simple_embedded_ai_input";

#[derive(Default)]
struct WorkbenchActions {
    original_mode: bool,
    open_settings: bool,
    open_cli: bool,
    rebuild_index: bool,
    learn_style: bool,
    open_style: bool,
    refresh_skill: bool,
    orb_clicked: bool,
    orb_double_clicked: bool,
}

#[derive(Default)]
struct AiCardRequests {
    apply_edits: Option<usize>,
    apply_single_edit: Option<(usize, usize)>,
    analyze_edit: Option<(usize, usize)>,
    analyze_errors: Option<usize>,
    cancel_edits: Option<usize>,
    apply_deletes: Option<usize>,
    cancel_deletes: Option<usize>,
    submit_answers: Option<usize>,
    cancel_asks: Option<usize>,
    stop_build: Option<usize>,
    stop_cli: Option<usize>,
    open_cli: bool,
    apply_evolutions: Option<usize>,
    cancel_evolutions: Option<usize>,
}

#[derive(Clone, Copy)]
struct MeteorSpec {
    target: egui::Pos2,
    start: f32,
    end: f32,
    seed: f32,
    scale: f32,
}

impl App {
    pub(super) fn show_ai_mode(&mut self, ui: &mut egui::Ui) {
        if self.ai_zoom_before.is_none() {
            let previous = ui.ctx().zoom_factor();
            self.ai_zoom_before = Some(previous);
            ui.ctx()
                .set_zoom_factor((previous * AI_UI_ZOOM).clamp(0.8, 1.6));
            ui.ctx().request_repaint();
        }
        ui.spacing_mut().item_spacing = egui::vec2(8.0, 4.0);
        let view = ui.available_rect_before_wrap();
        ui.painter()
            .rect_filled(view, CornerRadius::ZERO, theme::gutter());

        let transition = self
            .ai
            .transition
            .as_ref()
            .map(|transition| (transition.started, transition.from));
        let elapsed = transition
            .map(|(started, _)| started.elapsed().as_secs_f32())
            .unwrap_or(TRANSITION_SECS);
        let progress = (elapsed / TRANSITION_SECS).clamp(0.0, 1.0);

        let margin = 12.0_f32;
        let gap = 10.0_f32;
        let area = view.shrink(margin);
        // D (workbench) is intentionally narrower than the old layout so the
        // new project-architecture column gets a readable share of the width.
        let right_width = (area.width() * 0.20).clamp(208.0, 288.0);
        let arch_space = (area.width() - right_width - gap * 2.0 - 240.0).max(160.0);
        let arch_width = (area.width() * 0.29).clamp(260.0, 420.0).min(arch_space);
        let right = Rect::from_min_size(
            egui::pos2(area.right() - right_width, area.top()),
            egui::vec2(right_width, area.height()),
        );
        let architecture = Rect::from_min_size(
            egui::pos2(
                (right.left() - gap - arch_width).max(area.left()),
                area.top(),
            ),
            egui::vec2(arch_width, area.height()),
        );
        let left_width = (architecture.left() - gap - area.left()).max(240.0);
        let left = Rect::from_min_size(area.min, egui::vec2(left_width, area.height()));
        let available_height = (left.height() - gap).max(1.0);
        let max_output = (available_height - 110.0).max(110.0);
        let output_height =
            (available_height * self.ai_output_ratio.clamp(0.20, 0.85)).clamp(110.0, max_output);
        let output = Rect::from_min_size(left.min, egui::vec2(left.width(), output_height));
        let input = Rect::from_min_max(egui::pos2(left.left(), output.bottom() + gap), left.max);

        // Drag the bar between AI output and input to give long prompts more
        // room. The ratio is kept on the app so it survives project switches
        // and re-entering AI mode.
        let split_rect = Rect::from_center_size(
            egui::pos2(left.center().x, output.bottom() + gap * 0.5),
            egui::vec2(left.width(), 8.0),
        );
        let split = ui.interact(
            split_rect,
            egui::Id::new("simple_embedded_ai_input_split"),
            egui::Sense::drag(),
        );
        if split.drag_started() {
            if let Some(position) = split.interact_pointer_pos() {
                self.ai_split_drag = Some((position.y, self.ai_output_ratio));
            }
        }
        if split.dragged() {
            if let (Some((start_y, start_ratio)), Some(position)) =
                (self.ai_split_drag, split.interact_pointer_pos())
            {
                self.ai_output_ratio =
                    (start_ratio + (position.y - start_y) / available_height).clamp(0.20, 0.85);
                ui.ctx().request_repaint();
            }
        }
        if split.drag_stopped() {
            self.ai_split_drag = None;
        }
        let split_color = if split.hovered() || split.dragged() {
            theme::accent()
        } else {
            theme::border()
        };
        ui.painter().rect_filled(
            Rect::from_center_size(split_rect.center(), egui::vec2(84.0, 3.0)),
            2,
            split_color,
        );
        if split.hovered() {
            ui.ctx().set_cursor_icon(egui::CursorIcon::ResizeVertical);
        }

        let reveal_right = reveal_at(elapsed, 0);
        let reveal_output = reveal_at(elapsed, 1);
        let reveal_arch = reveal_at(elapsed, 2);
        let reveal_input = reveal_at(elapsed, 3);
        let right_orb = egui::pos2(right.center().x, right.top() + 66.0);
        let output_orb = egui::pos2(output.center().x, output.top() + 99.0);
        let arch_origin = egui::pos2(architecture.center().x, architecture.top() + 100.0);

        let mut send = false;
        let mut open_cli = false;
        let mut clear = false;
        let mut quick_prompt = None;
        let mut workbench = WorkbenchActions::default();
        let mut framework_refresh = false;
        let mut framework_generate = false;
        let mut framework_open = false;

        self.poll_ai_framework(ui.ctx());
        self.ai_output_panel(
            ui,
            output,
            reveal_output,
            output_orb,
            &mut clear,
            &mut quick_prompt,
        );
        self.ai_input_panel(ui, input, reveal_input, &mut send, &mut open_cli);
        ai_architecture_panel(
            ui,
            architecture,
            reveal_arch,
            arch_origin,
            self.ai.framework.as_ref(),
            self.ai.framework_path.as_deref(),
            self.ai.framework_error.as_deref(),
            self.ai.framework_missing,
            &mut framework_refresh,
            &mut framework_generate,
            &mut framework_open,
        );
        self.ai_workbench_panel(ui, right, reveal_right, right_orb, &mut workbench);

        if framework_refresh {
            self.refresh_ai_framework(true);
        }
        if framework_generate {
            self.request_ai_framework();
        }
        if framework_open {
            self.open_ai_framework();
            ui.ctx().request_repaint();
            return;
        }
        if clear {
            self.clear_ai_chat();
        }
        if open_cli {
            self.open_cli();
        }
        if let Some(prompt) = quick_prompt {
            self.ai.input = prompt.to_owned();
            self.ai.focus_input = true;
        }
        if workbench.orb_double_clicked {
            self.export_ai_evolution();
        } else if workbench.orb_clicked {
            self.record_ai_orb_click();
        }
        if workbench.refresh_skill {
            self.refresh_ai_skill();
        }
        if workbench.open_settings {
            self.open_ai_settings();
        }
        if workbench.original_mode {
            self.exit_ai_mode();
        }
        if workbench.open_cli {
            self.open_cli();
        }
        if workbench.rebuild_index {
            self.start_context_index_scan();
        }
        if workbench.learn_style {
            self.request_code_style_learning();
        }
        if workbench.open_style {
            self.open_code_style();
        }
        if send {
            self.send_ai_message();
        }

        if progress < 1.0 {
            ui.ctx().request_repaint();
            let from = transition
                .map(|(_, from)| from)
                .filter(|from| from.width() > 1.0 && from.height() > 1.0);
            paint_transition(
                ui,
                from,
                view,
                elapsed,
                self.ai.fog_texture.as_ref(),
                right_orb,
                output_orb,
            );
        } else if self.ai.transition.is_some() {
            self.ai.transition = None;
        }

        // Keep the orb rotation smooth instead of stepping at the old 180 ms
        // idle rate; 30 fps is enough for the slow rings, 60 fps while waiting.
        let repaint = if self.ai.sending { 16 } else { 33 };
        ui.ctx()
            .request_repaint_after(Duration::from_millis(repaint));
    }

    /// The first ~0.3 s still render the editor (under an increasing fog) so
    /// the dissolve is visible; then `show_ai_mode` takes over while the fog is
    /// already opaque.
    pub(super) fn show_ai_early_fog(&mut self, ui: &mut egui::Ui, elapsed: f32) {
        if self.ai.fog_texture.is_none() {
            self.ai.fog_texture = Some(ui.ctx().load_texture(
                "simple_embedded_ai_fog",
                fog_image(256),
                egui::TextureOptions::LINEAR_REPEAT,
            ));
        }
        let strength = smoothstep((elapsed / AI_EARLY_FOG_SECS).clamp(0.0, 1.0));
        if strength <= 0.001 {
            return;
        }
        let view = ui.max_rect();
        if let Some(fog) = &self.ai.fog_texture {
            paint_fog(
                ui.painter(),
                view,
                self.last_editor_rect,
                fog,
                elapsed,
                strength,
            );
        }
    }

    fn ai_output_panel(
        &mut self,
        ui: &mut egui::Ui,
        rect: Rect,
        reveal: f32,
        origin: egui::Pos2,
        clear: &mut bool,
        quick_prompt: &mut Option<&'static str>,
    ) {
        let mut requests = AiCardRequests::default();
        let sending = self.ai.sending;
        let messages_empty = self.ai.messages.is_empty();
        let message_count = self.ai.messages.len();
        ai_panel(ui, rect, reveal, origin, |ui| {
            ui.horizontal(|ui| {
                ui.label(
                    RichText::new("AI 输出")
                        .size(13.0)
                        .strong()
                        .color(theme::text()),
                );
                let pending = self.ai.messages.iter().any(|message| {
                    (message.edit_status == AiEditStatus::Pending && !message.edits.is_empty())
                        || (message.delete_status == AiEditStatus::Pending
                            && !message.deletes.is_empty())
                        || (message.ask_status == AiEditStatus::Pending && !message.asks.is_empty())
                        || (message.evolution_status == AiEditStatus::Pending
                            && !message.evolutions.is_empty())
                });
                let building = self.ai.build_check.is_some()
                    && self.ai.messages.iter().any(|message| {
                        message.build_requested
                            && matches!(
                                message.build_status,
                                AiBuildStatus::Waiting | AiBuildStatus::Running
                            )
                    });
                let (status, color) = if self.ai.sending {
                    ("生成中", theme::accent())
                } else if building {
                    ("构建验证", theme::accent())
                } else if pending {
                    ("待确认", theme::warning())
                } else if self.ai.messages.is_empty() {
                    ("待命", theme::text_faint())
                } else {
                    ("已完成", theme::success())
                };
                paint_dot(ui, color);
                ui.label(RichText::new(status).size(10.0).color(color));
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    if !self.ai.messages.is_empty() && ai_ghost_button(ui, "清空") {
                        *clear = true;
                    }
                    ui.label(
                        RichText::new(format!("{message_count} 条消息"))
                            .size(9.5)
                            .color(theme::text_faint()),
                    );
                });
            });
            ai_rule(ui);
            ui.add_space(6.0);

            let height = ui.available_height().max(40.0);
            ScrollArea::vertical()
                .id_salt("simple_embedded_ai_output_scroll")
                .auto_shrink([false, false])
                .stick_to_bottom(true)
                .max_height(height)
                .show(ui, |ui| {
                    ui.set_width(ui.available_width());
                    if messages_empty {
                        ai_empty_state(ui, quick_prompt);
                    } else {
                        for (index, message) in self.ai.messages.iter_mut().enumerate() {
                            ai_message(ui, message, index, sending, &mut requests);
                            ui.add_space(8.0);
                        }
                    }
                    ui.add_space(4.0);
                });
        });
        if let Some(index) = requests.apply_edits {
            self.apply_ai_edits(index);
            ui.ctx().request_repaint();
        }
        if let Some((index, edit_index)) = requests.apply_single_edit {
            self.apply_ai_single_edit(index, edit_index);
            ui.ctx().request_repaint();
        }
        if let Some((index, edit_index)) = requests.analyze_edit {
            self.analyze_ai_edit(index, edit_index);
            ui.ctx().request_repaint();
        }
        if let Some(index) = requests.analyze_errors {
            self.analyze_ai_edits(index);
            ui.ctx().request_repaint();
        }
        if let Some(index) = requests.cancel_edits {
            self.cancel_ai_edits(index);
            ui.ctx().request_repaint();
        }
        if let Some(index) = requests.apply_deletes {
            self.apply_ai_deletes(index);
            ui.ctx().request_repaint();
        }
        if let Some(index) = requests.cancel_deletes {
            self.cancel_ai_deletes(index);
            ui.ctx().request_repaint();
        }
        if let Some(index) = requests.submit_answers {
            self.submit_ai_answers(index);
            ui.ctx().request_repaint();
        }
        if let Some(index) = requests.cancel_asks {
            self.cancel_ai_asks(index);
            ui.ctx().request_repaint();
        }
        if let Some(index) = requests.stop_build {
            self.stop_ai_build(index);
            ui.ctx().request_repaint();
        }
        if let Some(index) = requests.stop_cli {
            self.stop_ai_cli(index);
            ui.ctx().request_repaint();
        }
        if requests.open_cli {
            self.open_cli();
        }
        if let Some(index) = requests.apply_evolutions {
            self.apply_ai_evolutions(index);
            ui.ctx().request_repaint();
        }
        if let Some(index) = requests.cancel_evolutions {
            self.cancel_ai_evolutions(index);
            ui.ctx().request_repaint();
        }
    }

    fn ai_input_panel(
        &mut self,
        ui: &mut egui::Ui,
        rect: Rect,
        reveal: f32,
        send: &mut bool,
        open_cli: &mut bool,
    ) {
        let input_id = Id::new(INPUT_ID);
        let enter_pressed = ui.memory(|memory| memory.has_focus(input_id))
            && ui.input_mut(|input| input.consume_key(egui::Modifiers::CTRL, egui::Key::Enter))
            && !self.ai.sending
            && !self.ai.input.trim().is_empty();
        let mut remove_context = false;

        ai_panel(ui, rect, reveal, rect.right_bottom(), |ui| {
            ui.horizontal(|ui| {
                ui.label(
                    RichText::new("输入")
                        .size(13.0)
                        .strong()
                        .color(theme::text()),
                );
                ui.label(
                    RichText::new("Ctrl+Enter 发送")
                        .size(9.5)
                        .color(theme::text_faint()),
                );
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    if let Some(path) = &self.ai.context_file {
                        let name = path
                            .file_name()
                            .map(|name| name.to_string_lossy().into_owned())
                            .unwrap_or_else(|| crate::util::display_path(path));
                        if ai_ghost_button(ui, "移除") {
                            remove_context = true;
                        }
                        ui.label(
                            RichText::new(format!("附加 · {name}"))
                                .size(9.5)
                                .color(theme::accent()),
                        )
                        .on_hover_text(format!(
                            "额外附加的文件：{}\nframework.md、hardware.dts、其中引用的文件和工程源文件会始终自动加入。",
                            crate::util::display_path(path)
                        ));
                    }
                    let auto = if self.context_index.index.is_some() {
                        "自动 · framework.md + 检索片段 + 按需读取"
                    } else if self.ai.framework_path.is_some() {
                        "自动 · framework.md + 引用文件"
                    } else {
                        "自动 · framework.md / 工程源文件"
                    };
                    ui.label(RichText::new(auto).size(8.8).color(theme::text_faint()))
                        .on_hover_text(
                            "每次发送会自动附带 framework.md、hardware.dts、framework.md 中引用的工程文件和工程源文件；本地上下文索引会按你的需求只检索相关代码片段，减少 token。缺少某个文件时，AI 可以用 se-list / se-read 主动检索工程目录并读取真实正文，IDE 会读取后自动继续。右侧的附加文件只是额外补充。",
                        );
                });
            });
            ai_rule(ui);
            ui.add_space(4.0);

            let footer = 44.0;
            let editor_height = (ui.available_height() - footer).max(40.0);
            let rows = ((editor_height - 14.0) / 18.0).max(1.0) as usize;
            Frame::new()
                .fill(theme::editor_bg())
                .stroke(Stroke::new(1.0_f32, theme::border_soft()))
                .corner_radius(9)
                .inner_margin(Margin::symmetric(8, 6))
                .show(ui, |ui| {
                    ui.set_width(ui.available_width());
                    let mut response = None;
                    ScrollArea::vertical()
                        .id_salt("simple_embedded_ai_input_scroll")
                        .auto_shrink([false, false])
                        .max_height((editor_height - 14.0).max(28.0))
                        .show(ui, |ui| {
                            ui.set_width(ui.available_width());
                            response = Some(
                                ui.add(
                                    TextEdit::multiline(&mut self.ai.input)
                                        .id(input_id)
                                        .frame(Frame::NONE)
                                        .margin(Margin::symmetric(0, 0))
                                        .font(FontId::proportional(13.0))
                                        .text_color(theme::text())
                                        .hint_text(
                                            "描述要新增的外设、要修改的逻辑，或贴上报错信息…",
                                        )
                                        .desired_width(f32::INFINITY)
                                        .desired_rows(rows),
                                ),
                            );
                        });
                    if self.ai.focus_input {
                        if let Some(response) = response {
                            response.request_focus();
                        }
                        self.ai.focus_input = false;
                    }
                });

            ui.add_space(5.0);
            ui.horizontal(|ui| {
                ui.label(
                    RichText::new(
                        "AI 可直接修改工程文件：读取已保存内容，修改前显示差异，确认后写入",
                    )
                    .size(9.0)
                    .color(theme::text_faint()),
                );
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    let label = if self.ai.sending {
                        "生成中"
                    } else {
                        "发送"
                    };
                    let can_send = !self.ai.input.trim().is_empty() && !self.ai.sending;
                    if ui
                        .add_enabled(can_send, theme::primary_button(label))
                        .clicked()
                    {
                        *send = true;
                    }
                    if ui.add(theme::secondary_button("CLI")).clicked() {
                        *open_cli = true;
                    }
                    if self.ai.sending {
                        ui.spinner();
                    }
                });
            });
        });

        if remove_context {
            self.ai.context_file = None;
        }
        if enter_pressed {
            *send = true;
        }
    }

    fn ai_workbench_panel(
        &mut self,
        ui: &mut egui::Ui,
        rect: Rect,
        reveal: f32,
        origin: egui::Pos2,
        actions: &mut WorkbenchActions,
    ) {
        ai_panel(ui, rect, reveal, origin, |ui| {
            let footer = 42.0;
            let content_height = (ui.available_height() - footer).max(60.0);
            ScrollArea::vertical()
                .id_salt("simple_embedded_ai_workbench_scroll")
                .auto_shrink([false, false])
                .max_height(content_height)
                .show(ui, |ui| {
                    ui.set_width(ui.available_width());
                    let (orb_rect, orb_response) = ui.allocate_exact_size(
                        egui::vec2(ui.available_width(), 116.0),
                        Sense::click(),
                    );
                    let orb_center = orb_rect.center() - egui::vec2(0.0, 4.0);
                    let inside_orb = orb_response
                        .interact_pointer_pos()
                        .is_some_and(|pos| pos.distance(orb_center) <= 48.0);
                    if orb_response.double_clicked() && inside_orb {
                        actions.orb_double_clicked = true;
                    } else if orb_response.clicked() && inside_orb {
                        actions.orb_clicked = true;
                    }
                    paint_orb(
                        ui,
                        orb_center,
                        40.0,
                        ui.input(|input| input.time),
                        self.ai.sending,
                    );
                    ui.vertical_centered(|ui| {
                        ui.label(
                            RichText::new("Simple AI")
                                .size(16.0)
                                .strong()
                                .color(theme::text()),
                        );
                        let (label, color) = if self.ai.sending {
                            ("正在生成", theme::accent())
                        } else {
                            ("就绪", theme::success())
                        };
                        ui.label(RichText::new(label).size(10.0).color(color));
                    });

                    ui.add_space(12.0);
                    ai_section(ui, "工程上下文");
                    if let Some(project) = &self.project {
                        ai_kv(ui, "工程", &project.name);
                        ai_kv(ui, "路径", &crate::util::display_path(&project.root));
                    }
                    if let Some(device) = &self.project_meta.device {
                        ai_kv(ui, "芯片", device);
                    }
                    ai_kv(
                        ui,
                        "工具链",
                        self.build_config
                            .effective_toolchain(&self.project_meta)
                            .label(),
                    );
                    if let Some(file) = &self.ai.context_file {
                        let name = file
                            .file_name()
                            .map(|name| name.to_string_lossy().into_owned())
                            .unwrap_or_else(|| crate::util::display_path(file));
                        ai_kv(ui, "附加文件", &name);
                    }
                    ai_kv(
                        ui,
                        "自动上下文",
                        if self.context_index.index.is_some() {
                            "framework.md + 检索片段"
                        } else if self.ai.framework_path.is_some() {
                            "framework.md + 引用文件"
                        } else {
                            "framework.md / 工程源文件"
                        },
                    );

                    ui.add_space(10.0);
                    ai_section(ui, "上下文索引");
                    if self.context_index.building {
                        ui.horizontal(|ui| {
                            ui.spinner();
                            ui.label(
                                RichText::new("构建中…")
                                    .size(9.0)
                                    .color(theme::text_faint()),
                            );
                        });
                    } else if let Some(index) = &self.context_index.index {
                        let stats = index.stats();
                        ai_kv(
                            ui,
                            "规模",
                            &format!("{} 文件 / {} 片段", stats.files, stats.chunks),
                        );
                        ai_kv(ui, "大小", &format!("{} KB", stats.bytes / 1024));
                        if let Some(context) = &self.ai.context_stats {
                            ai_kv(
                                ui,
                                "上次检索",
                                &format!(
                                    "{} 项 / {} KB",
                                    context.selected_chunks,
                                    context.selected_bytes / 1024
                                ),
                            );
                        }
                        if let Some(built_at) = self.context_index.built_at {
                            ai_kv(
                                ui,
                                "更新",
                                &format!("{} 秒前", built_at.elapsed().as_secs()),
                            );
                        }
                    } else {
                        ui.label(
                            RichText::new("尚未建立索引，发送时会使用完整源文件回退")
                                .size(8.8)
                                .color(theme::text_faint()),
                        );
                    }
                    if let Some(error) = &self.context_index.error {
                        ui.add(
                            Label::new(RichText::new(error).size(8.5).color(theme::error()))
                                .wrap(),
                        );
                    }
                    if ui.add(theme::secondary_button("重建索引")).clicked() {
                        actions.rebuild_index = true;
                    }
                    ui.label(
                        RichText::new("只把相关片段发给模型；framework.md 和关键文件仍完整附带。")
                            .size(8.2)
                            .color(theme::text_faint()),
                    );

                    ui.add_space(10.0);
                    ai_section(ui, "代码风格");
                    if let Some(project) = &self.project {
                        let style_path = project.root.join("code_style.md");
                        ai_kv(
                            ui,
                            "文档",
                            if style_path.is_file() {
                                "code_style.md"
                            } else {
                                "未生成"
                            },
                        );
                        let samples = project.root.join("code_style_samples");
                        ai_kv(
                            ui,
                            "样例",
                            if samples.is_dir() {
                                "code_style_samples/"
                            } else {
                                "建议放 code_style_samples/"
                            },
                        );
                        if ui.add(theme::secondary_button("学习代码风格")).clicked() {
                            actions.learn_style = true;
                        }
                        if style_path.is_file() && ai_ghost_button(ui, "打开") {
                            actions.open_style = true;
                        }
                    }
                    ui.label(
                        RichText::new("AI 编码优先遵守 code_style.md；样例目录只用于学习，不会自动复制。")
                            .size(8.2)
                            .color(theme::text_faint()),
                    );

                    ui.add_space(10.0);
                    ai_section(ui, "串口 CLI");
                    let cli_port = if self.project_cli.port.trim().is_empty() {
                        "未选择".to_owned()
                    } else {
                        self.project_cli.summary()
                    };
                    ai_kv(ui, "端口", &cli_port);
                    ai_kv(
                        ui,
                        "状态",
                        if self.cli.session.is_some() {
                            "已连接"
                        } else {
                            "未连接"
                        },
                    );
                    if ui.add(theme::secondary_button("打开 CLI")).clicked() {
                        actions.open_cli = true;
                    }

                    ui.add_space(10.0);
                    ai_section(ui, "已加载 Skill");
                    ai_skill_card(
                        ui,
                        &self.ai.skill_source,
                        self.ai.skill_files,
                        self.ai.skill_builtin,
                        self.ai.skill_error.as_deref(),
                        &mut actions.refresh_skill,
                    );
                    ui.label(
                        RichText::new(
                            "双击 AI 图标：导出并编辑本项目 evolution-user.md",
                        )
                        .size(8.3)
                        .color(theme::text_faint()),
                    );

                    ui.add_space(10.0);
                    ai_section(ui, "对话记录");
                    let chat_path = self.ai.chat_path.clone().or_else(|| {
                        self.project.as_ref().map(|project| {
                            crate::ai_chat::chat_path(&project.root, &project.name)
                        })
                    });
                    if let Some(path) = chat_path {
                        ui.add(
                            Label::new(
                                RichText::new(crate::util::display_path(&path))
                                    .size(9.5)
                                    .color(theme::text_dim()),
                            )
                            .wrap(),
                        );
                        ui.add(
                            Label::new(
                                RichText::new(
                                    "进入 AI 模式时自动加载；清空聊天会重写该文件。",
                                )
                                .size(8.8)
                                .color(theme::text_faint()),
                            )
                            .wrap(),
                        );
                    } else {
                        ui.label(
                            RichText::new("打开工程并进入 AI 模式后自动创建。")
                                .size(9.0)
                                .color(theme::text_faint()),
                        );
                    }
                    if let Some(error) = &self.ai.chat_error {
                        ui.add(
                            Label::new(RichText::new(error).size(9.0).color(theme::error()))
                                .wrap(),
                        );
                    }

                    ui.add_space(10.0);
                    ai_section(ui, "问题记录");
                    if let Some(project) = &self.project {
                        let path = crate::ai_issue::issue_path(&project.root, &project.name);
                        ui.add(
                            Label::new(
                                RichText::new(crate::util::display_path(&path))
                                    .size(9.5)
                                    .color(theme::text_dim()),
                            )
                            .wrap(),
                        );
                        ui.add(
                            Label::new(
                                RichText::new(
                                    "AI 遇到并解决编译/运行问题时自动补充；同标题会更新原因和解决方案。",
                                )
                                .size(8.8)
                                .color(theme::text_faint()),
                            )
                            .wrap(),
                        );
                    } else {
                        ui.label(
                            RichText::new("打开工程并进入 AI 模式后自动创建。")
                                .size(9.0)
                                .color(theme::text_faint()),
                        );
                    }

                    ui.add_space(10.0);
                    ai_section(ui, "模型");
                    let model = if self.settings.ai_model.trim().is_empty() {
                        "未配置"
                    } else {
                        self.settings.ai_model.trim()
                    };
                    ai_kv(ui, "模型", model);
                    let key_state = if self.settings.ai_api_key.trim().is_empty() {
                        "未填写 API Key"
                    } else {
                        "API Key 已配置"
                    };
                    ai_kv(ui, "凭据", key_state);
                    ai_kv(
                        ui,
                        "接口",
                        if self.settings.ai_base_url.trim().is_empty() {
                            "未配置"
                        } else {
                            self.settings.ai_base_url.trim()
                        },
                    );

                    ui.add_space(12.0);
                    Frame::new()
                        .fill(theme::with_alpha(theme::surface(), 120))
                        .stroke(Stroke::new(1.0_f32, theme::border_soft()))
                        .corner_radius(8)
                        .inner_margin(Margin::symmetric(9, 7))
                        .show(ui, |ui| {
                            ui.set_width(ui.available_width());
                            ui.label(
                                RichText::new("API Key 只保存在用户设置中，不会写入工程文件。")
                                    .size(9.0)
                                    .color(theme::text_faint()),
                            );
                            ui.label(
                                RichText::new(format!(
                                    "目录 skill 优先；未找到时使用内置副本（{} 个文件）。",
                                    crate::ai::embedded_skill().files.len()
                                ))
                                .size(9.0)
                                .color(theme::text_faint()),
                            );
                            ui.label(
                                RichText::new(
                                    "AI 的文件修改会先显示差异；点击“应用全部”后写入，写入前备份到用户目录。",
                                )
                                .size(9.0)
                                .color(theme::text_faint()),
                            );
                        });
                    ui.add_space(6.0);
                });

            ui.add_space(6.0);
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                if ui
                    .add(theme::primary_button("设置"))
                    .on_hover_text("填写 API Key、接口地址和模型名称")
                    .clicked()
                {
                    actions.open_settings = true;
                }
                ui.add_space(4.0);
                if ui
                    .add(theme::secondary_button("原始模式"))
                    .on_hover_text("返回编辑器（Ctrl+Alt+A）")
                    .clicked()
                {
                    actions.original_mode = true;
                }
            });
        });
    }

    pub(super) fn show_cli_window(&mut self, ctx: &egui::Context) {
        if !self.cli.open {
            return;
        }
        let mut open = self.cli.open;
        let mut changed = false;
        let connected = self.cli.session.is_some();
        egui::Window::new("串口 CLI")
            .id(Id::new("simple_embedded_cli_window"))
            .open(&mut open)
            .default_pos(egui::pos2(140.0, 110.0))
            .default_width(680.0)
            .default_height(480.0)
            .resizable(true)
            .collapsible(false)
            .frame(
                Frame::window(&ctx.global_style())
                    .fill(theme::panel())
                    .stroke(Stroke::new(1.0_f32, theme::border()))
                    .corner_radius(10)
                    .inner_margin(Margin::symmetric(14, 12)),
            )
            .show(ctx, |ui| {
                ui.horizontal(|ui| {
                    ui.label(
                        RichText::new("用于 AI 串口调试和手动命令")
                            .size(9.5)
                            .color(theme::text_faint()),
                    );
                });
                ui.add_space(4.0);
                ui.horizontal(|ui| {
                    ui.label(RichText::new("端口").size(9.5).color(theme::text_dim()));
                    let port = ui.add_enabled(
                        !connected,
                        TextEdit::singleline(&mut self.project_cli.port)
                            .desired_width(110.0)
                            .hint_text("COM3"),
                    );
                    if port.changed() {
                        changed = true;
                    }
                    if ui.add(theme::secondary_button("自动检测")).clicked() {
                        self.scan_cli_ports();
                    }
                    ui.label(RichText::new("波特率").size(9.5).color(theme::text_dim()));
                    egui::ComboBox::from_id_salt("simple_embedded_cli_baud")
                        .selected_text(self.project_cli.baud.to_string())
                        .width(86.0)
                        .show_ui(ui, |ui| {
                            for baud in [
                                9600, 19200, 38400, 57600, 115_200, 230_400, 460_800, 921_600,
                                1_500_000, 2_000_000,
                            ] {
                                if ui
                                    .selectable_value(
                                        &mut self.project_cli.baud,
                                        baud,
                                        baud.to_string(),
                                    )
                                    .clicked()
                                {
                                    changed = true;
                                }
                            }
                        });
                    if ui
                        .add(if connected {
                            theme::secondary_button("断开")
                        } else {
                            theme::primary_button("连接")
                        })
                        .clicked()
                    {
                        self.toggle_cli_connection();
                    }
                });
                ui.horizontal(|ui| {
                    ui.label(RichText::new("行尾").size(9.5).color(theme::text_dim()));
                    egui::ComboBox::from_id_salt("simple_embedded_cli_line_ending")
                        .selected_text(self.project_cli.line_ending.clone())
                        .width(74.0)
                        .show_ui(ui, |ui| {
                            for ending in ["CRLF", "LF", "CR"] {
                                if ui
                                    .selectable_value(
                                        &mut self.project_cli.line_ending,
                                        ending.to_owned(),
                                        ending,
                                    )
                                    .clicked()
                                {
                                    changed = true;
                                }
                            }
                        });
                    ui.label(RichText::new("等待").size(9.5).color(theme::text_dim()));
                    if ui
                        .add(
                            egui::DragValue::new(&mut self.project_cli.timeout_ms)
                                .range(100..=10_000)
                                .speed(20.0)
                                .suffix(" ms"),
                        )
                        .changed()
                    {
                        changed = true;
                    }
                    ui.label(
                        RichText::new(if connected {
                            "已连接；端口/波特率修改后需重新连接"
                        } else {
                            "先连接串口，再发送命令"
                        })
                        .size(9.0)
                        .color(theme::text_faint()),
                    );
                });
                ui.add_space(4.0);
                ui.separator();
                ui.horizontal(|ui| {
                    ui.label(
                        RichText::new("输出")
                            .size(9.5)
                            .strong()
                            .color(theme::text_dim()),
                    );
                    if ai_ghost_button(ui, "清空") {
                        self.cli.output.clear();
                    }
                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        ui.label(
                            RichText::new(&self.cli.status)
                                .size(9.0)
                                .color(if connected {
                                    theme::success()
                                } else {
                                    theme::text_faint()
                                }),
                        );
                    });
                });
                Frame::new()
                    .fill(theme::editor_bg())
                    .stroke(Stroke::new(1.0_f32, theme::border_soft()))
                    .corner_radius(7)
                    .inner_margin(Margin::symmetric(7, 5))
                    .show(ui, |ui| {
                        ScrollArea::vertical()
                            .id_salt("simple_embedded_cli_output")
                            .auto_shrink([false, false])
                            .max_height(250.0)
                            .stick_to_bottom(self.cli.auto_scroll)
                            .show(ui, |ui| {
                                ui.set_width(ui.available_width());
                                if self.cli.output.is_empty() {
                                    ui.label(
                                        RichText::new("连接串口后，命令输出会显示在这里。")
                                            .size(9.5)
                                            .color(theme::text_faint()),
                                    );
                                } else {
                                    ui.add(
                                        Label::new(
                                            RichText::new(&self.cli.output)
                                                .monospace()
                                                .size(9.2)
                                                .color(theme::text()),
                                        )
                                        .wrap(),
                                    );
                                }
                            });
                    });
                ui.add_space(4.0);
                ui.horizontal(|ui| {
                    let response = ui.add_sized(
                        [ui.available_width() - 80.0, 28.0],
                        TextEdit::singleline(&mut self.cli.input)
                            .frame(Frame::new().fill(theme::editor_bg()).corner_radius(6))
                            .font(FontId::monospace(10.5))
                            .text_color(theme::text())
                            .hint_text("输入 CLI 命令，例如 help / info / adc read"),
                    );
                    let enter = response.lost_focus()
                        && ui.input(|input| input.key_pressed(egui::Key::Enter));
                    if enter || ui.add(theme::primary_button("发送")).clicked() {
                        self.send_cli_command();
                    }
                });
            });
        self.cli.open = open;
        if !open {
            if self.cli.session.is_some() {
                self.cli.session = None;
                self.cli.status = "已断开".to_owned();
            }
            if changed {
                self.persist_build_config();
            }
        }
    }

    pub(super) fn show_ai_settings_modal(&mut self, ctx: &egui::Context) {
        let Some(mut draft) = self.ai_settings_draft.take() else {
            return;
        };
        let mut close = false;
        let mut save = false;
        let mut reload_skill = false;
        let mut import_skill_folder = false;
        let mut import_skill_files = false;
        let mut import_reference_files = false;
        let mut import_reference_folder = false;
        let mut open_skill_dir = false;
        let mut open_reference_dir = false;

        let modal = egui::Modal::new(Id::new("simple_embedded_ai_settings_modal"))
            .frame(
                Frame::new()
                    .fill(theme::panel())
                    .stroke(Stroke::new(1.0_f32, theme::border()))
                    .corner_radius(10)
                    .inner_margin(Margin::symmetric(20, 16)),
            )
            .backdrop_color(Color32::from_black_alpha(170));
        modal.show(ctx, |ui| {
            ui.set_width(620.0);
            ui.horizontal(|ui| {
                ui.label(
                    RichText::new("AI 设置")
                        .size(18.0)
                        .strong()
                        .color(theme::text()),
                );
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    if ai_close_button(ui) {
                        close = true;
                    }
                });
            });
            ui.add_space(4.0);
            ui.label(
                RichText::new(
                    "使用 OpenAI 兼容接口。设置、额外 Skill 和参考资料都保存在用户目录，不写入当前工程；进入 AI 模式后会自动加载内置 stm32-embedded-dev skill。",
                )
                .size(10.5)
                .color(theme::text_dim()),
            );
            ui.label(
                RichText::new(
                    "技能自我进化：AI 会把长期偏好整理成规则，确认后写入用户目录；双击 AI 图标可导出并编辑本项目 evolution-user.md。",
                )
                .size(10.0)
                .color(theme::text_faint()),
            );
            ui.add_space(12.0);

            ScrollArea::vertical()
                .id_salt("simple_embedded_ai_settings_scroll")
                .max_height(540.0)
                .auto_shrink([false, true])
                .show(ui, |ui| {
                    ui.set_width(ui.available_width());
                    ui.label(
                        RichText::new("API Key")
                            .size(10.5)
                            .color(theme::text_dim()),
                    );
            ui.horizontal(|ui| {
                let width = (ui.available_width() - 64.0).max(200.0);
                ui.add_sized(
                    [width, 30.0],
                    TextEdit::singleline(&mut draft.api_key)
                        .password(!draft.show_key)
                        .hint_text("sk-...")
                        .font(FontId::monospace(12.0))
                        .text_color(theme::text())
                        .background_color(theme::bg())
                        .margin(Margin::symmetric(8, 6)),
                );
                if ai_ghost_button(ui, if draft.show_key { "隐藏" } else { "显示" }) {
                    draft.show_key = !draft.show_key;
                }
            });
            ui.add_space(9.0);

            ai_form_line(
                ui,
                "API 地址",
                &mut draft.base_url,
                "https://api.deepseek.com/v1",
            );
            ui.add_space(9.0);
            ai_form_line(
                ui,
                "模型名称",
                &mut draft.model,
                "deepseek-chat / gpt-4o / qwen-plus",
            );
            ui.add_space(12.0);

                    ai_section(ui, "内置 / 目录 Skill");
                    ai_skill_card(
                        ui,
                        &self.ai.skill_source,
                        self.ai.skill_files,
                        self.ai.skill_builtin,
                        self.ai.skill_error.as_deref(),
                        &mut reload_skill,
                    );
                    ui.add(
                        Label::new(
                            RichText::new(
                                "stm32-embedded-dev 是内置的嵌入式专用 Skill；额外 Skill 会追加到系统提示词，用于补充其他领域知识。",
                            )
                            .size(8.8)
                            .color(theme::text_faint()),
                        )
                        .wrap(),
                    );
                    ui.add_space(12.0);

                    ai_section(ui, "导入其他 Skill");
                    if self.ai.extra_skills.is_empty() {
                        ui.label(
                            RichText::new("尚未导入额外 Skill。")
                                .size(9.0)
                                .color(theme::text_faint()),
                        );
                    } else {
                        ai_resource_list(ui, &self.ai.extra_skills);
                    }
                    ui.horizontal(|ui| {
                        let importing = self.ai.import_receiver.is_some();
                        if ui
                            .add_enabled(
                                !importing,
                                theme::secondary_button("导入 Skill 文件夹"),
                            )
                            .on_hover_text("选择包含 SKILL.md 或 Markdown 的文件夹")
                            .clicked()
                        {
                            import_skill_folder = true;
                        }
                        if ui
                            .add_enabled(!importing, theme::secondary_button("导入文件"))
                            .clicked()
                        {
                            import_skill_files = true;
                        }
                        if ai_ghost_button(ui, "打开目录") {
                            open_skill_dir = true;
                        }
                    });
                    ui.add_space(12.0);

                    ai_section(ui, "参考资料");
                    ui.add(
                        Label::new(
                            RichText::new(
                                "可导入芯片手册、数据手册、HAL 使用说明、例程等。支持 PDF（自动提取文本）、Markdown、TXT、C/H、JSON、CSV、YAML 等；扫描版 PDF 可能无法提取。",
                            )
                            .size(8.8)
                            .color(theme::text_faint()),
                        )
                        .wrap(),
                    );
                    ui.add_space(4.0);
                    if self.ai.extra_references.is_empty() {
                        ui.label(
                            RichText::new("尚未导入参考资料。")
                                .size(9.0)
                                .color(theme::text_faint()),
                        );
                    } else {
                        ai_resource_list(ui, &self.ai.extra_references);
                    }
                    ui.horizontal(|ui| {
                        let importing = self.ai.import_receiver.is_some();
                        if ui
                            .add_enabled(!importing, theme::secondary_button("导入文件"))
                            .on_hover_text("可多选 PDF / Markdown / 文本 / HAL 例程")
                            .clicked()
                        {
                            import_reference_files = true;
                        }
                        if ui
                            .add_enabled(!importing, theme::secondary_button("导入文件夹"))
                            .clicked()
                        {
                            import_reference_folder = true;
                        }
                        if ai_ghost_button(ui, "打开目录") {
                            open_reference_dir = true;
                        }
                    });
                    if self.ai.import_receiver.is_some() {
                        ui.add_space(5.0);
                        ui.horizontal(|ui| {
                            ui.spinner();
                            if let Some(status) = &self.ai.import_status {
                                ui.label(
                                    RichText::new(status).size(9.0).color(theme::text_dim()),
                                );
                            }
                        });
                    } else if let Some(status) = &self.ai.import_status {
                        ui.add_space(5.0);
                        ui.add(
                            Label::new(
                                RichText::new(status).size(9.0).color(theme::success()),
                            )
                            .wrap(),
                        );
                    }
                    if let Some(error) = &self.ai.import_error {
                        ui.add_space(4.0);
                        ui.add(
                            Label::new(
                                RichText::new(error).size(9.0).color(theme::error()),
                            )
                            .wrap(),
                        );
                    }
                });

            if let Some(error) = &draft.error {
                ui.add_space(8.0);
                ui.label(RichText::new(error).size(10.5).color(theme::error()));
            }

            ui.add_space(14.0);
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                if ui.add(theme::primary_button("保存")).clicked() {
                    save = true;
                }
                ui.add_space(4.0);
                if ui.add(theme::secondary_button("取消")).clicked() {
                    close = true;
                }
            });
        });

        if close {
            self.ai_settings_draft = None;
            return;
        }
        if reload_skill {
            self.refresh_ai_skill();
        }
        if import_skill_folder {
            self.import_ai_skill_folder();
        }
        if import_skill_files {
            self.import_ai_skill_files();
        }
        if import_reference_files {
            self.import_ai_reference_files();
        }
        if import_reference_folder {
            self.import_ai_reference_folder();
        }
        if open_skill_dir {
            self.open_ai_resource_dir(crate::ai::AiResourceKind::Skill);
        }
        if open_reference_dir {
            self.open_ai_resource_dir(crate::ai::AiResourceKind::Reference);
        }
        if save {
            let api_key = draft.api_key.trim().to_owned();
            let base_url = draft.base_url.trim().trim_end_matches('/').to_owned();
            let model = draft.model.trim().to_owned();
            if api_key.is_empty() {
                draft.error = Some("请填写 API Key".to_owned());
                self.ai_settings_draft = Some(draft);
                return;
            }
            if base_url.is_empty() {
                draft.error = Some("请填写 API 地址，例如 https://api.deepseek.com/v1".to_owned());
                self.ai_settings_draft = Some(draft);
                return;
            }
            if model.is_empty() {
                draft.error = Some("请填写模型名称".to_owned());
                self.ai_settings_draft = Some(draft);
                return;
            }
            self.settings.ai_api_key = api_key;
            self.settings.ai_base_url = base_url;
            self.settings.ai_model = model;
            self.settings.save();
            self.ai_settings_draft = None;
            self.refresh_ai_skill();
            self.set_toast("AI 设置已保存", Severity::Success);
            return;
        }
        if !close {
            self.ai_settings_draft = Some(draft);
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn ai_architecture_panel(
    ui: &mut Ui,
    rect: Rect,
    reveal: f32,
    origin: egui::Pos2,
    doc: Option<&FrameworkDoc>,
    path: Option<&Path>,
    error: Option<&str>,
    missing: bool,
    refresh: &mut bool,
    generate: &mut bool,
    open: &mut bool,
) {
    ai_panel(ui, rect, reveal, origin, |ui| {
        let incomplete = doc.is_some_and(|doc| !doc.missing_sections().is_empty());
        ui.horizontal(|ui| {
            ui.label(
                RichText::new("项目架构")
                    .size(13.0)
                    .strong()
                    .color(theme::text()),
            );
            if error.is_some() {
                ui.label(RichText::new("读取失败").size(9.0).color(theme::error()));
            } else if incomplete {
                paint_dot(ui, theme::warning());
                ui.label(
                    RichText::new("文档不完整")
                        .size(9.0)
                        .color(theme::warning()),
                );
            } else if doc.is_some() {
                paint_dot(ui, theme::success());
                ui.label(
                    RichText::new("framework.md")
                        .size(9.0)
                        .color(theme::success()),
                );
            } else if missing {
                paint_dot(ui, theme::warning());
                ui.label(RichText::new("等待文档").size(9.0).color(theme::warning()));
            } else {
                paint_dot(ui, theme::text_faint());
                ui.label(RichText::new("未生成").size(9.0).color(theme::text_faint()));
            }
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                if ai_ghost_button(ui, "刷新") {
                    *refresh = true;
                }
                if doc.is_some() && ai_ghost_button(ui, "打开") {
                    *open = true;
                }
            });
        });
        ui.label(
            RichText::new("内容来自 framework.md（stm32-embedded-dev T8 / S9）")
                .size(9.0)
                .color(theme::text_faint()),
        );
        ai_rule(ui);
        if let Some(error) = error {
            ui.add(Label::new(RichText::new(error).size(9.5).color(theme::error())).wrap());
            ui.add_space(4.0);
        }
        if let Some(path) = path {
            ui.add(
                Label::new(
                    RichText::new(crate::util::display_path(path))
                        .size(8.5)
                        .color(theme::text_faint()),
                )
                .wrap(),
            );
            ui.add_space(4.0);
        }

        let Some(doc) = doc else {
            Frame::new()
                .fill(theme::with_alpha(theme::warning(), 11))
                .stroke(Stroke::new(
                    1.0_f32,
                    theme::with_alpha(theme::warning(), 55),
                ))
                .corner_radius(8)
                .inner_margin(Margin::symmetric(10, 9))
                .show(ui, |ui| {
                    ui.set_width(ui.available_width());
                    ui.label(
                        RichText::new(if error.is_some() {
                            "无法读取 framework.md"
                        } else {
                            "等待 framework.md"
                        })
                        .size(13.0)
                        .strong()
                        .color(theme::warning()),
                    );
                    ui.add(
                        Label::new(
                            RichText::new(
                                "工程根目录或 docs/ 下还没有 framework.md。该窗口不再使用启发式架构分析；请先生成文档，生成后会自动加载。",
                            )
                            .size(9.5)
                            .color(theme::text_dim()),
                        )
                        .wrap(),
                    );
                    if missing {
                        ui.add_space(7.0);
                        ui.label(
                            RichText::new(
                                "可以点击下面的按钮让 AI 按 stm32-embedded-dev skill 的 T8 / S9 流程先感知工程，再决定是否生成或替换 framework.md。",
                            )
                            .size(8.8)
                            .color(theme::text_faint()),
                        );
                        ui.add_space(6.0);
                        ui.horizontal(|ui| {
                            if ui
                                .add(theme::primary_button("让 AI 生成 framework.md"))
                                .clicked()
                            {
                                *generate = true;
                            }
                            if ai_ghost_button(ui, "刷新") {
                                *refresh = true;
                            }
                        });
                    }
                });
            return;
        };

        let height = ui.available_height().max(60.0);
        ScrollArea::vertical()
            .id_salt("simple_embedded_ai_framework_scroll")
            .auto_shrink([false, false])
            .max_height(height)
            .show(ui, |ui| {
                ui.set_width(ui.available_width());
                ui.label(
                    RichText::new(&doc.title)
                        .size(14.0)
                        .strong()
                        .color(theme::text()),
                );
                let missing_sections = doc.missing_sections();
                if !missing_sections.is_empty() {
                    Frame::new()
                        .fill(theme::with_alpha(theme::warning(), 11))
                        .stroke(Stroke::new(
                            1.0_f32,
                            theme::with_alpha(theme::warning(), 55),
                        ))
                        .corner_radius(6)
                        .inner_margin(Margin::symmetric(8, 6))
                        .show(ui, |ui| {
                            ui.set_width(ui.available_width());
                            ui.label(
                                RichText::new("framework.md 不完整")
                                    .size(10.0)
                                    .strong()
                                    .color(theme::warning()),
                            );
                            ui.add(
                                Label::new(
                                    RichText::new(format!(
                                        "缺少：{}。可能是上一次生成被输出长度截断；补全前不会用启发式结果替代。",
                                        missing_sections.join("、")
                                    ))
                                    .size(8.8)
                                    .color(theme::text_dim()),
                                )
                                .wrap(),
                            );
                            ui.add_space(4.0);
                            if ui
                                .add(theme::primary_button("让 AI 补全 framework.md"))
                                .clicked()
                            {
                                *generate = true;
                            }
                        });
                    ui.add_space(7.0);
                }
                ui.add_space(7.0);

                framework_section(
                    ui,
                    "00",
                    "分层架构",
                    doc.section_any(&["层级表", "分层架构"]),
                    1,
                );
                framework_section(ui, "01", "执行范式", doc.section("执行范式"), 2);
                framework_section(ui, "02", "调度模式", doc.section("调度模式"), 3);
                framework_section(
                    ui,
                    "03",
                    "设计模式",
                    doc.section_any(&["设计模式清单", "设计模式"]),
                    4,
                );

                ai_section(ui, "函数调度图");
                framework_part(ui, "调用方向图", doc.section("调用方向图"), 5);
                framework_part(ui, "关键事件上下文链", doc.section("关键事件上下文链"), 6);
                ui.add_space(9.0);

                ai_section(ui, "调度入口");
                framework_part(ui, "启动调用链", doc.section("启动调用链"), 7);
                ui.add_space(9.0);

                ai_section(ui, "中断与任务");
                if let Some(section) = doc.section("中断来源与抢占关系") {
                    render_framework_body(ui, &section.body, 8);
                } else {
                    framework_missing_section(ui);
                }
                ui.label(
                    RichText::new("任务与时间变量表见 02 调度模式。")
                        .size(8.5)
                        .color(theme::text_faint()),
                );
                ui.add_space(9.0);

                arch_section(ui, "04", "调度甘特图");
                if let Some(gantt) = &doc.gantt {
                    if !gantt.title.is_empty() {
                        ui.label(
                            RichText::new(&gantt.title)
                                .size(9.0)
                                .color(theme::text_dim()),
                        );
                    }
                    arch_gantt(ui, gantt);
                    ui.label(
                        RichText::new(&gantt.note)
                            .size(8.5)
                            .color(theme::text_faint()),
                    );
                } else if let Some(section) = doc.section("调度时间线") {
                    render_framework_body(ui, &section.body, 9);
                } else {
                    framework_missing_section(ui);
                }
                ui.add_space(8.0);

                ui.label(
                    RichText::new("点击右上角“打开”查看完整 framework.md 和 Mermaid 图源。")
                        .size(8.5)
                        .color(theme::text_faint()),
                );
                ui.add_space(6.0);
            });
    });
}

fn framework_section(
    ui: &mut Ui,
    number: &str,
    title: &str,
    section: Option<&crate::framework::FrameworkSection>,
    seed: usize,
) {
    arch_section(ui, number, title);
    match section {
        Some(section) => render_framework_body(ui, &section.body, seed),
        None => framework_missing_section(ui),
    }
    ui.add_space(9.0);
}

fn framework_part(
    ui: &mut Ui,
    title: &str,
    section: Option<&crate::framework::FrameworkSection>,
    seed: usize,
) {
    ui.label(
        RichText::new(title)
            .size(9.5)
            .strong()
            .color(theme::text_dim()),
    );
    match section {
        Some(section) => render_framework_body(ui, &section.body, seed),
        None => framework_missing_section(ui),
    }
    ui.add_space(6.0);
}

fn framework_missing_section(ui: &mut Ui) {
    ui.label(
        RichText::new("framework.md 未包含该小节。")
            .size(8.8)
            .color(theme::text_faint()),
    );
}

fn render_framework_body(ui: &mut Ui, body: &str, seed: usize) {
    if body.trim().is_empty() {
        framework_missing_section(ui);
        return;
    }
    let lines = body.lines().collect::<Vec<_>>();
    let mut prose = String::new();
    let mut index = 0usize;
    let mut prose_index = 0usize;
    let mut table_index = 0usize;
    let mut in_fence = false;
    while index < lines.len() {
        let line = lines[index];
        let trimmed = line.trim_start();
        if trimmed.starts_with("```") {
            in_fence = !in_fence;
            prose.push_str(line);
            prose.push('\n');
            index += 1;
            continue;
        }
        if !in_fence && is_framework_table_start(&lines, index) {
            if !prose.trim().is_empty() {
                render_markdown(ui, &prose, seed * 100 + prose_index, false);
                prose_index += 1;
                prose.clear();
            }
            let start = index;
            index += 1;
            while index < lines.len() && lines[index].trim_start().starts_with('|') {
                index += 1;
            }
            render_framework_table(ui, &lines[start..index].join("\n"), seed, table_index);
            table_index += 1;
            ui.add_space(3.0);
            continue;
        }
        prose.push_str(line);
        prose.push('\n');
        index += 1;
    }
    if !prose.trim().is_empty() {
        render_markdown(ui, &prose, seed * 100 + prose_index, false);
    }
}

fn is_framework_table_start(lines: &[&str], index: usize) -> bool {
    let Some(line) = lines.get(index) else {
        return false;
    };
    if !line.trim_start().starts_with('|') {
        return false;
    }
    let Some(separator) = lines.get(index + 1) else {
        return false;
    };
    let separator = separator.trim();
    separator.starts_with('|')
        && separator.contains("---")
        && separator
            .chars()
            .all(|character| matches!(character, '|' | '-' | ':' | ' ' | '\t'))
}

fn render_framework_table(ui: &mut Ui, text: &str, seed: usize, table_index: usize) {
    Frame::new()
        .fill(theme::with_alpha(theme::surface(), 110))
        .stroke(Stroke::new(1.0_f32, theme::border_soft()))
        .corner_radius(5)
        .inner_margin(Margin::symmetric(5, 4))
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            ScrollArea::horizontal()
                .id_salt(("simple_embedded_framework_table", seed, table_index))
                .auto_shrink([false, true])
                .show(ui, |ui| {
                    ui.add(
                        Label::new(
                            RichText::new(text)
                                .font(FontId::monospace(9.2))
                                .color(theme::text()),
                        )
                        .wrap_mode(egui::TextWrapMode::Extend),
                    );
                });
        });
}
fn arch_section(ui: &mut Ui, number: &str, title: &str) {
    ui.horizontal(|ui| {
        Frame::new()
            .fill(theme::with_alpha(theme::accent(), 24))
            .stroke(Stroke::new(1.0_f32, theme::with_alpha(theme::accent(), 75)))
            .corner_radius(4)
            .inner_margin(Margin::symmetric(5, 1))
            .show(ui, |ui| {
                ui.label(
                    RichText::new(number)
                        .size(8.5)
                        .strong()
                        .color(theme::accent()),
                );
            });
        ui.label(
            RichText::new(title)
                .size(11.0)
                .strong()
                .color(theme::text()),
        );
    });
    ui.add_space(5.0);
}

#[allow(clippy::too_many_arguments)]
fn arch_gantt(ui: &mut Ui, chart: &GanttChart) {
    let width = ui.available_width();
    let label_width = 74.0;
    let track_x = label_width + 8.0;
    let track_width = (width - track_x - 4.0).max(80.0);
    let row_height = 15.0;
    let row_gap = 4.0;
    let header_height = 13.0;
    let height = header_height + chart.rows.len() as f32 * (row_height + row_gap);
    let (rect, response) = ui.allocate_exact_size(egui::vec2(width, height), Sense::hover());
    let painter = ui.painter();
    let label_font = FontId::proportional(8.0);
    let tick_font = FontId::proportional(7.0);
    let details = chart
        .rows
        .iter()
        .map(|row| {
            let start = row
                .segments
                .first()
                .map(|segment| segment.start * 100.0)
                .unwrap_or(0.0);
            let end = row
                .segments
                .last()
                .map(|segment| segment.end * 100.0)
                .unwrap_or(0.0);
            if row.section.is_empty() {
                format!("{}：{start:.0}%–{end:.0}%", row.label)
            } else {
                format!("{} / {}：{start:.0}%–{end:.0}%", row.section, row.label)
            }
        })
        .collect::<Vec<_>>()
        .join("\n");
    if !details.is_empty() {
        response.on_hover_text(details);
    }

    for index in 0..=4 {
        let x = rect.left() + track_x + track_width * index as f32 / 4.0;
        painter.line_segment(
            [
                egui::pos2(x, rect.top() + header_height - 2.0),
                egui::pos2(x, rect.bottom()),
            ],
            Stroke::new(0.5_f32, theme::border_soft()),
        );
        painter.text(
            egui::pos2(x, rect.top()),
            Align2::CENTER_TOP,
            format!("{index}"),
            tick_font.clone(),
            theme::text_faint(),
        );
    }

    for (index, row) in chart.rows.iter().enumerate() {
        let y = rect.top() + header_height + index as f32 * (row_height + row_gap);
        painter.text(
            egui::pos2(rect.left(), y + row_height / 2.0),
            Align2::LEFT_CENTER,
            truncate_chars(&row.label, 10),
            label_font.clone(),
            theme::text_dim(),
        );
        let track = Rect::from_min_size(
            egui::pos2(rect.left() + track_x, y + 2.0),
            egui::vec2(track_width, row_height - 4.0),
        );
        painter.rect_filled(
            track,
            CornerRadius::same(3),
            theme::with_alpha(theme::surface(), 120),
        );
        let color = match row.kind {
            GanttKind::Main => theme::accent(),
            GanttKind::Tick => theme::success(),
            GanttKind::Interrupt => theme::warning(),
            GanttKind::Task => theme::info(),
            GanttKind::Event => theme::text_dim(),
        };
        for segment in &row.segments {
            let start = segment.start.clamp(0.0, 1.0);
            let end = segment.end.clamp(start, 1.0);
            let bar = Rect::from_min_max(
                egui::pos2(track.left() + track.width() * start, track.top() + 2.0),
                egui::pos2(track.left() + track.width() * end, track.bottom() - 2.0),
            );
            if bar.width() < 1.0 {
                continue;
            }
            painter.rect_filled(bar, CornerRadius::same(2), color);
        }
    }

    ui.add_space(4.0);
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing = egui::vec2(8.0, 2.0);
        for (kind, label) in [
            (GanttKind::Main, "主循环"),
            (GanttKind::Tick, "节拍"),
            (GanttKind::Interrupt, "中断"),
            (GanttKind::Task, "任务"),
            (GanttKind::Event, "事件"),
        ] {
            if chart.rows.iter().any(|row| row.kind == kind) {
                let color = match kind {
                    GanttKind::Main => theme::accent(),
                    GanttKind::Tick => theme::success(),
                    GanttKind::Interrupt => theme::warning(),
                    GanttKind::Task => theme::info(),
                    GanttKind::Event => theme::text_dim(),
                };
                paint_dot(ui, color);
                ui.label(RichText::new(label).size(8.0).color(theme::text_faint()));
            }
        }
    });
}

fn reveal_at(elapsed: f32, index: usize) -> f32 {
    let start = PANEL_START + index as f32 * PANEL_STAGGER;
    smoothstep(((elapsed - start) / PANEL_DURATION).clamp(0.0, 1.0))
}

/// Native window opacity is kept opaque; the enter transition is a fast panel
/// fade so we avoid layered-window/DWM calls every frame.
pub(super) fn ai_window_alpha(_elapsed: f32) -> u8 {
    255
}

fn fog_strength(_elapsed: f32) -> f32 {
    0.0
}

fn smoothstep(t: f32) -> f32 {
    let t = t.clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

fn ease_out_cubic(t: f32) -> f32 {
    let t = t.clamp(0.0, 1.0);
    1.0 - (1.0 - t).powi(3)
}

fn ease_in_out(t: f32) -> f32 {
    smoothstep(t)
}

fn ai_panel(ui: &mut Ui, rect: Rect, reveal: f32, origin: egui::Pos2, add: impl FnOnce(&mut Ui)) {
    let p = reveal.clamp(0.0, 1.0);
    if p <= 0.0 {
        return;
    }
    let painter = ui.painter().clone();
    painter.rect_filled(
        rect,
        CornerRadius::same(14),
        theme::with_alpha(theme::panel(), (235.0 * p) as u8),
    );
    painter.rect_stroke(
        rect,
        CornerRadius::same(14),
        Stroke::new(
            1.0_f32,
            theme::with_alpha(theme::border_soft(), (255.0 * p) as u8),
        ),
        egui::StrokeKind::Inside,
    );

    let content_rect = rect.shrink2(egui::vec2(14.0, 12.0));
    let origin = egui::pos2(
        origin.x.clamp(rect.left(), rect.right()),
        origin.y.clamp(rect.top(), rect.bottom()),
    );
    let eased = ease_out_cubic(p);
    let left = (origin.x - rect.left()).max(0.0);
    let right = (rect.right() - origin.x).max(0.0);
    let top = (origin.y - rect.top()).max(0.0);
    let bottom = (rect.bottom() - origin.y).max(0.0);
    let revealed = Rect::from_min_max(
        egui::pos2(origin.x - left * eased, origin.y - top * eased),
        egui::pos2(origin.x + right * eased, origin.y + bottom * eased),
    );
    let clip = revealed.intersect(content_rect);
    if p < 0.999 {
        let glow = theme::with_alpha(theme::accent(), ((1.0 - p) * 150.0) as u8);
        painter.rect_stroke(
            clip,
            CornerRadius::same(8),
            Stroke::new(1.3_f32, glow),
            egui::StrokeKind::Inside,
        );
    }

    let mut child = ui.new_child(
        UiBuilder::new()
            .max_rect(content_rect)
            .layout(Layout::top_down(Align::Min)),
    );
    child.set_clip_rect(clip.intersect(ui.clip_rect()));
    child.set_opacity(p);
    if p < 0.995 {
        child.disable();
    }
    add(&mut child);
}

fn ai_rule(ui: &mut Ui) {
    let rect = ui.available_rect_before_wrap();
    let y = rect.top() + 1.0;
    ui.painter().hline(
        rect.x_range(),
        y,
        Stroke::new(1.0_f32, theme::border_soft()),
    );
    ui.add_space(5.0);
}

fn ai_section(ui: &mut Ui, title: &str) {
    ui.label(
        RichText::new(title)
            .size(10.0)
            .strong()
            .color(theme::text_dim()),
    );
    ui.add_space(2.0);
    let rect = ui.available_rect_before_wrap();
    ui.painter().hline(
        rect.x_range(),
        rect.top(),
        Stroke::new(1.0_f32, theme::border_soft()),
    );
    ui.add_space(5.0);
}

fn ai_kv(ui: &mut Ui, label: &str, value: &str) {
    ui.horizontal(|ui| {
        ui.add_sized(
            [48.0, 16.0],
            Label::new(RichText::new(label).size(9.5).color(theme::text_faint())),
        );
        ui.add(Label::new(RichText::new(value).size(10.5).color(theme::text())).wrap());
    });
    ui.add_space(2.0);
}

fn ai_skill_card(
    ui: &mut Ui,
    source: &Option<String>,
    files: usize,
    builtin: bool,
    error: Option<&str>,
    refresh: &mut bool,
) {
    let (line, color) = if let Some(error) = error {
        (error.to_owned(), theme::error())
    } else if builtin {
        (format!("使用内置副本 · {files} 个文件"), theme::info())
    } else {
        (format!("已加载 · {files} 个文件"), theme::success())
    };
    Frame::new()
        .fill(theme::with_alpha(color, 16))
        .stroke(Stroke::new(1.0_f32, theme::with_alpha(color, 90)))
        .corner_radius(8)
        .inner_margin(Margin::symmetric(9, 7))
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.horizontal(|ui| {
                paint_dot(ui, color);
                ui.label(
                    RichText::new("stm32-embedded-dev")
                        .size(11.0)
                        .strong()
                        .color(theme::text()),
                );
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    if ai_ghost_button(ui, "重新加载") {
                        *refresh = true;
                    }
                });
            });
            if let Some(source) = source {
                ui.add(
                    Label::new(RichText::new(source).size(9.0).color(theme::text_faint())).wrap(),
                );
            }
            ui.label(RichText::new(line).size(9.5).color(color));
        });
}

fn ai_ghost_button(ui: &mut Ui, label: &str) -> bool {
    ui.add(Button::new(RichText::new(label).size(9.5).color(theme::text_dim())).frame(false))
        .clicked()
}

fn ai_form_line(ui: &mut Ui, label: &str, value: &mut String, hint: &str) {
    ui.label(RichText::new(label).size(10.5).color(theme::text_dim()));
    ui.add_sized(
        [ui.available_width(), 30.0],
        TextEdit::singleline(value)
            .hint_text(hint)
            .font(FontId::monospace(12.0))
            .text_color(theme::text())
            .background_color(theme::bg())
            .margin(Margin::symmetric(8, 6)),
    );
}

fn ai_close_button(ui: &mut Ui) -> bool {
    let (rect, response) = ui.allocate_exact_size(egui::vec2(22.0, 22.0), Sense::click());
    let color = if response.hovered() {
        theme::text()
    } else {
        theme::text_dim()
    };
    let painter = ui.painter();
    let inset = rect.shrink(6.0);
    painter.line_segment(
        [inset.left_top(), inset.right_bottom()],
        Stroke::new(1.3_f32, color),
    );
    painter.line_segment(
        [inset.right_top(), inset.left_bottom()],
        Stroke::new(1.3_f32, color),
    );
    if response.hovered() {
        painter.rect_filled(
            rect,
            CornerRadius::same(5),
            theme::with_alpha(theme::hover(), 120),
        );
    }
    response
        .on_hover_cursor(egui::CursorIcon::PointingHand)
        .clicked()
}

fn paint_dot(ui: &mut Ui, color: Color32) {
    let (rect, _) = ui.allocate_exact_size(egui::vec2(8.0, 8.0), Sense::hover());
    ui.painter().circle_filled(rect.center(), 3.0, color);
}

fn ai_message(
    ui: &mut Ui,
    message: &mut AiChatMessage,
    index: usize,
    sending: bool,
    requests: &mut AiCardRequests,
) {
    let user = message.role == "user";
    let build_report = user && message.content.starts_with("【AI 构建验证结果】");
    let (fill, border) = if message.error {
        (
            theme::with_alpha(theme::error(), 22),
            theme::with_alpha(theme::error(), 110),
        )
    } else if build_report {
        (
            theme::with_alpha(theme::warning(), 16),
            theme::with_alpha(theme::warning(), 70),
        )
    } else if user {
        (
            theme::with_alpha(theme::accent(), 24),
            theme::with_alpha(theme::accent(), 80),
        )
    } else {
        (theme::panel_raised(), theme::border_soft())
    };
    Frame::new()
        .fill(fill)
        .stroke(Stroke::new(1.0_f32, border))
        .corner_radius(10)
        .inner_margin(Margin::symmetric(12, 9))
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.horizontal(|ui| {
                let (role, color) = if message.error {
                    ("错误", theme::error())
                } else if build_report {
                    ("构建", theme::warning())
                } else if user {
                    ("你", theme::accent())
                } else {
                    ("AI", theme::success())
                };
                ui.label(RichText::new(role).size(10.0).strong().color(color));
                if message.streaming {
                    ui.label(RichText::new("生成中").size(9.0).color(theme::accent()));
                }
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    if ai_ghost_button(ui, "复制") {
                        ui.ctx().copy_text(message.content.clone());
                    }
                });
            });
            ui.add_space(3.0);
            if !message.reasoning.is_empty() || (message.streaming && message.content.is_empty()) {
                ai_reasoning_block(ui, &*message, index);
                ui.add_space(6.0);
            }
            if message.streaming {
                if !message.content.is_empty() {
                    let live_tail = tail_preview(&message.content, 12_000);
                    let live_text = live_tail.as_deref().unwrap_or(&message.content);
                    Frame::new()
                        .fill(theme::with_alpha(theme::editor_bg(), 120))
                        .corner_radius(6)
                        .inner_margin(Margin::symmetric(7, 5))
                        .show(ui, |ui| {
                            ui.add(
                                Label::new(
                                    RichText::new(live_text).size(11.0).color(theme::text()),
                                )
                                .wrap(),
                            );
                        });
                    ui.add_space(5.0);
                }
                ui.horizontal(|ui| {
                    ui.spinner();
                    ui.label(
                        RichText::new(if message.reasoning.trim().is_empty() {
                            "正在等待模型输出…"
                        } else if message.content.trim().is_empty() {
                            "正在思考…"
                        } else {
                            "正在生成回答…"
                        })
                        .size(10.0)
                        .color(theme::text_dim()),
                    );
                });
            } else {
                render_markdown(ui, &message.content, index, message.error);
                if !message.asks.is_empty() {
                    ui.add_space(7.0);
                    ai_asks_card(
                        ui,
                        message,
                        index,
                        sending,
                        &mut requests.submit_answers,
                        &mut requests.cancel_asks,
                    );
                }
                if !message.edits.is_empty() {
                    ui.add_space(7.0);
                    ai_edits_card(ui, &*message, index, &mut *requests);
                }
                if !message.deletes.is_empty() {
                    ui.add_space(7.0);
                    ai_deletes_card(
                        ui,
                        &*message,
                        index,
                        &mut requests.apply_deletes,
                        &mut requests.cancel_deletes,
                    );
                }
                if message.fs_requested {
                    ui.add_space(7.0);
                    ai_fs_card(ui, &*message, index);
                }
                if message.build_requested {
                    ui.add_space(7.0);
                    ai_build_card(ui, &*message, index, &mut requests.stop_build);
                }
                if message.cli_requested {
                    ui.add_space(7.0);
                    ai_cli_card(
                        ui,
                        &*message,
                        index,
                        &mut requests.stop_cli,
                        &mut requests.open_cli,
                    );
                }
                if !message.issues.is_empty() {
                    ui.add_space(7.0);
                    ai_issues_card(ui, &*message);
                }
                if !message.evolutions.is_empty() {
                    ui.add_space(7.0);
                    ai_evolution_card(
                        ui,
                        &*message,
                        index,
                        &mut requests.apply_evolutions,
                        &mut requests.cancel_evolutions,
                    );
                }
            }
        });
}

fn ai_fs_card(ui: &mut Ui, message: &AiChatMessage, index: usize) {
    let (status, color) = match message.fs_status {
        AiFsStatus::Waiting => ("读取中", theme::warning()),
        AiFsStatus::Done => ("已读取", theme::success()),
        AiFsStatus::Limited => ("已保留结果", theme::warning()),
        AiFsStatus::Error => ("读取失败", theme::error()),
    };
    Frame::new()
        .fill(theme::with_alpha(theme::info(), 8))
        .stroke(Stroke::new(1.0_f32, theme::with_alpha(theme::info(), 65)))
        .corner_radius(8)
        .inner_margin(Margin::symmetric(10, 8))
        .show(ui, |ui| {
            ui.horizontal(|ui| {
                ui.label(
                    RichText::new("AI 文件检索")
                        .size(10.0)
                        .strong()
                        .color(theme::info()),
                );
                ui.label(
                    RichText::new(format!("{} 项", message.fs_requests.len()))
                        .size(9.5)
                        .color(theme::text_faint()),
                );
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    ui.label(RichText::new(status).size(9.2).color(color));
                });
            });
            ui.add_space(3.0);
            ui.add(
                Label::new(
                    RichText::new(
                        "IDE 从工程磁盘读取真实内容并加入下一轮上下文；不需要手动附加文件。",
                    )
                    .size(8.6)
                    .color(theme::text_dim()),
                )
                .wrap(),
            );
            for request in &message.fs_requests {
                ui.add_space(3.0);
                let detail = request
                    .error
                    .as_ref()
                    .map(|error| format!("失败：{error}"))
                    .unwrap_or_else(|| request.label());
                ui.add(
                    Label::new(RichText::new(format!("• {detail}")).size(9.0).color(
                        if request.error.is_some() {
                            theme::error()
                        } else {
                            theme::text()
                        },
                    ))
                    .wrap(),
                );
            }
            if let Some(summary) = &message.fs_summary {
                ui.add_space(3.0);
                ui.add(
                    Label::new(RichText::new(summary).size(8.6).color(theme::text_faint())).wrap(),
                );
            }
            if let Some(error) = &message.fs_error {
                ui.add_space(3.0);
                ui.add(Label::new(RichText::new(error).size(8.6).color(theme::error())).wrap());
            }
            if let Some(output) = &message.fs_output {
                ui.add_space(4.0);
                egui::CollapsingHeader::new("查看读取内容")
                    .id_salt(("simple_embedded_ai_fs_output", index))
                    .show(ui, |ui| {
                        render_code_block(ui, "检索结果", output, index, 10_000);
                    });
            }
        });
}

fn ai_issues_card(ui: &mut Ui, message: &AiChatMessage) {
    Frame::new()
        .fill(theme::with_alpha(theme::info(), 9))
        .stroke(Stroke::new(1.0_f32, theme::with_alpha(theme::info(), 70)))
        .corner_radius(8)
        .inner_margin(Margin::symmetric(10, 8))
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.horizontal(|ui| {
                paint_dot(ui, theme::info());
                ui.label(
                    RichText::new("AI 问题记录")
                        .size(10.5)
                        .strong()
                        .color(theme::text()),
                );
                ui.label(
                    RichText::new(format!("{} 条", message.issues.len()))
                        .size(9.5)
                        .color(theme::text_faint()),
                );
            });
            for issue in &message.issues {
                ui.add_space(4.0);
                ui.horizontal(|ui| {
                    let color = if issue.resolved {
                        theme::success()
                    } else {
                        theme::warning()
                    };
                    ui.label(
                        RichText::new(if issue.resolved {
                            "已解决"
                        } else {
                            "未解决"
                        })
                        .size(8.8)
                        .strong()
                        .color(color),
                    );
                    ui.add(
                        Label::new(RichText::new(&issue.title).size(9.2).color(theme::text()))
                            .wrap(),
                    );
                });
                if !issue.solution.is_empty() {
                    ui.add(
                        Label::new(
                            RichText::new(format!("解决：{}", issue.solution.replace('\n', " ")))
                                .size(8.6)
                                .color(theme::text_dim()),
                        )
                        .wrap(),
                    );
                }
            }
            ui.add_space(4.0);
            ui.add(
                Label::new(
                    RichText::new("已写入工程根目录 .ai-issues.md；同标题问题会更新解决方案。")
                        .size(8.5)
                        .color(theme::text_faint()),
                )
                .wrap(),
            );
        });
}

fn ai_cli_card(
    ui: &mut Ui,
    message: &AiChatMessage,
    message_index: usize,
    stop: &mut Option<usize>,
    open_cli: &mut bool,
) {
    let color = match message.cli_status {
        AiCliStatus::Waiting => theme::warning(),
        AiCliStatus::Running => theme::accent(),
        AiCliStatus::Done => theme::success(),
        AiCliStatus::Error => theme::error(),
        AiCliStatus::Cancelled => theme::text_faint(),
    };
    Frame::new()
        .fill(theme::with_alpha(color, 9))
        .stroke(Stroke::new(1.0_f32, theme::with_alpha(color, 85)))
        .corner_radius(8)
        .inner_margin(Margin::symmetric(10, 8))
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.horizontal(|ui| {
                paint_dot(ui, color);
                ui.label(
                    RichText::new("AI 串口 Debug")
                        .size(10.5)
                        .strong()
                        .color(theme::text()),
                );
                ui.label(
                    RichText::new(message.cli_status.label())
                        .size(9.5)
                        .color(color),
                );
                if matches!(
                    message.cli_status,
                    AiCliStatus::Running | AiCliStatus::Waiting
                ) {
                    ui.spinner();
                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        if ui.add(theme::secondary_button("停止")).clicked() {
                            *stop = Some(message_index);
                        }
                    });
                }
            });
            if let Some(summary) = &message.cli_summary {
                ui.add_space(2.0);
                ui.add(
                    Label::new(RichText::new(summary).size(9.0).color(theme::text_dim())).wrap(),
                );
            }
            if !message.cli_commands.is_empty() {
                ui.add_space(3.0);
                for command in &message.cli_commands {
                    ui.add(
                        Label::new(
                            RichText::new(format!("$ {command}"))
                                .monospace()
                                .size(8.8)
                                .color(theme::text_dim()),
                        )
                        .wrap(),
                    );
                }
            }
            if let Some(output) = &message.cli_output {
                let id = ui.make_persistent_id(("simple_embedded_ai_cli_output", message_index));
                let mut open = ui
                    .data_mut(|data| data.get_persisted::<bool>(id))
                    .unwrap_or(false);
                ui.add_space(4.0);
                ui.horizontal(|ui| {
                    if ai_ghost_button(ui, if open { "收起输出" } else { "串口输出" }) {
                        open = !open;
                        ui.data_mut(|data| data.insert_persisted(id, open));
                    }
                    if ai_ghost_button(ui, "打开 CLI") {
                        *open_cli = true;
                    }
                });
                if open {
                    Frame::new()
                        .fill(theme::editor_bg())
                        .stroke(Stroke::new(1.0_f32, theme::border_soft()))
                        .corner_radius(6)
                        .inner_margin(Margin::symmetric(7, 5))
                        .show(ui, |ui| {
                            ScrollArea::vertical()
                                .id_salt(("simple_embedded_ai_cli_output_scroll", message_index))
                                .auto_shrink([false, true])
                                .max_height(220.0)
                                .show(ui, |ui| {
                                    ui.set_width(ui.available_width());
                                    ui.add(
                                        Label::new(
                                            RichText::new(output)
                                                .monospace()
                                                .size(8.8)
                                                .color(theme::text_dim()),
                                        )
                                        .wrap(),
                                    );
                                });
                        });
                }
            }
        });
}

fn ai_build_card(
    ui: &mut Ui,
    message: &AiChatMessage,
    message_index: usize,
    stop: &mut Option<usize>,
) {
    let color = match message.build_status {
        AiBuildStatus::Waiting => theme::warning(),
        AiBuildStatus::Running => theme::accent(),
        AiBuildStatus::Success => theme::success(),
        AiBuildStatus::Failed | AiBuildStatus::Error => theme::error(),
        AiBuildStatus::Cancelled => theme::text_faint(),
    };
    Frame::new()
        .fill(theme::with_alpha(color, 9))
        .stroke(Stroke::new(1.0_f32, theme::with_alpha(color, 85)))
        .corner_radius(8)
        .inner_margin(Margin::symmetric(10, 8))
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.horizontal(|ui| {
                paint_dot(ui, color);
                ui.label(
                    RichText::new("AI 构建验证")
                        .size(10.5)
                        .strong()
                        .color(theme::text()),
                );
                ui.label(
                    RichText::new(message.build_status.label())
                        .size(9.5)
                        .color(color),
                );
                if message.build_status == AiBuildStatus::Running {
                    ui.spinner();
                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        if ui.add(theme::secondary_button("停止")).clicked() {
                            *stop = Some(message_index);
                        }
                    });
                }
            });
            if let Some(summary) = &message.build_summary {
                ui.add_space(2.0);
                ui.add(
                    Label::new(RichText::new(summary).size(9.0).color(theme::text_dim())).wrap(),
                );
            }
            if let Some(output) = &message.build_output {
                let id = ui.make_persistent_id(("simple_embedded_ai_build_output", message_index));
                let mut open = ui
                    .data_mut(|data| data.get_persisted::<bool>(id))
                    .unwrap_or(matches!(
                        message.build_status,
                        AiBuildStatus::Failed | AiBuildStatus::Error
                    ));
                ui.add_space(4.0);
                ui.horizontal(|ui| {
                    if ai_ghost_button(ui, if open { "收起输出" } else { "构建输出" }) {
                        open = !open;
                        ui.data_mut(|data| data.insert_persisted(id, open));
                    }
                });
                if open {
                    Frame::new()
                        .fill(theme::editor_bg())
                        .stroke(Stroke::new(1.0_f32, theme::border_soft()))
                        .corner_radius(6)
                        .inner_margin(Margin::symmetric(7, 5))
                        .show(ui, |ui| {
                            ScrollArea::vertical()
                                .id_salt(("simple_embedded_ai_build_output_scroll", message_index))
                                .auto_shrink([false, true])
                                .max_height(220.0)
                                .show(ui, |ui| {
                                    ui.set_width(ui.available_width());
                                    ui.add(
                                        Label::new(
                                            RichText::new(output)
                                                .monospace()
                                                .size(8.8)
                                                .color(theme::text_dim()),
                                        )
                                        .wrap(),
                                    );
                                });
                        });
                }
            }
        });
}

fn ai_evolution_card(
    ui: &mut Ui,
    message: &AiChatMessage,
    index: usize,
    apply: &mut Option<usize>,
    cancel: &mut Option<usize>,
) {
    let color = match message.evolution_status {
        AiEditStatus::Applied => theme::success(),
        AiEditStatus::Cancelled => theme::text_faint(),
        AiEditStatus::Pending => theme::accent(),
    };
    Frame::new()
        .fill(theme::with_alpha(color, 12))
        .stroke(Stroke::new(1.0_f32, theme::with_alpha(color, 80)))
        .corner_radius(8)
        .inner_margin(Margin::symmetric(9, 7))
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.horizontal(|ui| {
                paint_dot(ui, color);
                ui.label(
                    RichText::new("AI 技能进化")
                        .size(10.0)
                        .strong()
                        .color(theme::text()),
                );
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    let status = match message.evolution_status {
                        AiEditStatus::Applied => "已保存",
                        AiEditStatus::Cancelled => "已忽略",
                        AiEditStatus::Pending => "待确认",
                    };
                    ui.label(RichText::new(status).size(8.5).color(color));
                });
            });
            ui.add(
                Label::new(
                    RichText::new("确认后追加到用户级 evolution.md，后续请求优先遵守。")
                        .size(8.8)
                        .color(theme::text_faint()),
                )
                .wrap(),
            );
            for rule in &message.evolutions {
                ui.add_space(3.0);
                ui.add(
                    Label::new(
                        RichText::new(format!("• {rule}"))
                            .size(9.2)
                            .color(theme::text()),
                    )
                    .wrap(),
                );
            }
            if let Some(error) = &message.evolution_error {
                ui.add_space(3.0);
                ui.add(Label::new(RichText::new(error).size(8.8).color(theme::error())).wrap());
            }
            if message.evolution_status == AiEditStatus::Pending {
                ui.add_space(6.0);
                ui.horizontal(|ui| {
                    if ui.add(theme::primary_button("保存规则")).clicked() {
                        *apply = Some(index);
                    }
                    if ai_ghost_button(ui, "忽略") {
                        *cancel = Some(index);
                    }
                });
            }
        });
}

fn ai_reasoning_block(ui: &mut Ui, message: &AiChatMessage, index: usize) {
    let id = ui.make_persistent_id(("simple_embedded_ai_reasoning", index));
    let mut open = ui
        .data_mut(|data| data.get_persisted::<bool>(id))
        .unwrap_or(true);
    Frame::new()
        .fill(theme::with_alpha(theme::surface(), 80))
        .stroke(Stroke::new(1.0_f32, theme::border_soft()))
        .corner_radius(7)
        .inner_margin(Margin::symmetric(8, 6))
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.horizontal(|ui| {
                ui.label(
                    RichText::new("思考过程")
                        .size(9.5)
                        .strong()
                        .color(theme::text_dim()),
                );
                if message.streaming {
                    ui.spinner();
                }
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    if ai_ghost_button(ui, if open { "收起" } else { "展开" }) {
                        open = !open;
                        ui.data_mut(|data| data.insert_persisted(id, open));
                    }
                    if ai_ghost_button(ui, "复制") {
                        ui.ctx().copy_text(message.reasoning.clone());
                    }
                });
            });
            if open {
                let live_tail = if message.streaming {
                    tail_preview(&message.reasoning, 12_000)
                } else {
                    None
                };
                let text = live_tail.as_deref().unwrap_or_else(|| {
                    if message.reasoning.trim().is_empty() {
                        "正在读取模型的思考过程…"
                    } else {
                        message.reasoning.as_str()
                    }
                });
                let mut scroll = ScrollArea::vertical()
                    .id_salt(("simple_embedded_ai_reasoning_scroll", index))
                    .max_height(220.0)
                    .auto_shrink([false, true]);
                if message.streaming {
                    scroll = scroll.stick_to_bottom(true);
                }
                scroll.show(ui, |ui| {
                    ui.set_width(ui.available_width());
                    ui.add(
                        Label::new(RichText::new(text).size(11.0).color(theme::text_dim())).wrap(),
                    );
                });
            }
        });
}

fn ai_asks_card(
    ui: &mut Ui,
    message: &mut AiChatMessage,
    message_index: usize,
    sending: bool,
    submit: &mut Option<usize>,
    cancel: &mut Option<usize>,
) {
    let pending = message.ask_status == AiEditStatus::Pending;
    let has_errors = message.asks.iter().any(|ask| ask.error.is_some());
    let complete = crate::ai_ask::answers_complete(&message.asks, &message.ask_drafts);
    let ask_count = message.asks.len();
    let (status, color) = match message.ask_status {
        AiEditStatus::Pending => ("待回答", theme::warning()),
        AiEditStatus::Applied => ("已回答", theme::success()),
        AiEditStatus::Cancelled => ("已取消", theme::text_faint()),
    };
    Frame::new()
        .fill(theme::with_alpha(theme::accent(), 10))
        .stroke(Stroke::new(1.0_f32, theme::with_alpha(theme::accent(), 85)))
        .corner_radius(8)
        .inner_margin(Margin::symmetric(10, 8))
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.horizontal(|ui| {
                ui.label(
                    RichText::new("AI 需要确认")
                        .size(10.5)
                        .strong()
                        .color(theme::text()),
                );
                ui.label(
                    RichText::new(format!("{ask_count} 个问题"))
                        .size(9.5)
                        .color(theme::text_faint()),
                );
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    ui.label(RichText::new(status).size(9.5).color(color));
                });
            });
            ui.add_space(2.0);
            ui.add(
                Label::new(
                    RichText::new(
                        "选择选项，或在下方填写自定义回答；提交后会作为你的下一条消息发送给 AI。",
                    )
                    .size(8.8)
                    .color(theme::text_faint()),
                )
                .wrap(),
            );
            for ask_index in 0..ask_count {
                let question = message.asks[ask_index].question.clone();
                let options = message.asks[ask_index].options.clone();
                let multiple = message.asks[ask_index].multiple;
                let ask_error = message.asks[ask_index].error.clone();
                let draft = &mut message.ask_drafts[ask_index];
                let suffix = if multiple { "（可多选）" } else { "" };
                ui.add_space(7.0);
                ui.add(
                    Label::new(
                        RichText::new(if ask_count > 1 {
                            format!("{}. {}{}", ask_index + 1, question, suffix)
                        } else {
                            format!("{question}{suffix}")
                        })
                        .size(10.2)
                        .strong()
                        .color(theme::text()),
                    )
                    .wrap(),
                );
                if pending {
                    if !options.is_empty() {
                        ui.add_space(4.0);
                        ui.horizontal_wrapped(|ui| {
                            for (option_index, option) in options.iter().enumerate() {
                                let selected =
                                    draft.selected.get(option_index).copied().unwrap_or(false);
                                if ai_choice_button(ui, option, selected).clicked() {
                                    if multiple {
                                        if let Some(slot) = draft.selected.get_mut(option_index) {
                                            *slot = !selected;
                                        }
                                    } else {
                                        let was_selected = selected;
                                        draft.selected.fill(false);
                                        if !was_selected {
                                            if let Some(slot) = draft.selected.get_mut(option_index)
                                            {
                                                *slot = true;
                                            }
                                        }
                                    }
                                }
                            }
                        });
                    }
                    ui.add_space(4.0);
                    ui.add(
                        TextEdit::multiline(&mut draft.custom)
                            .id_salt(("simple_embedded_ai_ask_custom", message_index, ask_index))
                            .frame(Frame::new().fill(theme::editor_bg()).corner_radius(6))
                            .margin(Margin::symmetric(7, 5))
                            .font(FontId::proportional(10.5))
                            .text_color(theme::text())
                            .hint_text(if options.is_empty() {
                                "输入你的回答…"
                            } else {
                                "自定义回答（可选）…"
                            })
                            .desired_rows(1)
                            .desired_width(f32::INFINITY),
                    );
                } else {
                    let selected = options
                        .iter()
                        .zip(&draft.selected)
                        .filter(|(_, selected)| **selected)
                        .map(|(option, _)| option.clone())
                        .collect::<Vec<_>>();
                    if !selected.is_empty() {
                        ui.add(
                            Label::new(
                                RichText::new(format!("选择：{}", selected.join("、")))
                                    .size(9.0)
                                    .color(theme::text_dim()),
                            )
                            .wrap(),
                        );
                    }
                    if !draft.custom.trim().is_empty() {
                        ui.add(
                            Label::new(
                                RichText::new(format!("自定义：{}", draft.custom.trim()))
                                    .size(9.0)
                                    .color(theme::text_dim()),
                            )
                            .wrap(),
                        );
                    }
                }
                if let Some(error) = ask_error {
                    ui.add_space(2.0);
                    ui.add(Label::new(RichText::new(error).size(9.0).color(theme::error())).wrap());
                }
            }
            if let Some(error) = &message.ask_error {
                ui.add_space(4.0);
                ui.add(Label::new(RichText::new(error).size(9.5).color(theme::error())).wrap());
            }
            if pending {
                ui.add_space(6.0);
                ui.horizontal(|ui| {
                    let can_submit = !sending && !has_errors && complete;
                    if ui
                        .add_enabled(can_submit, theme::primary_button("发送回答"))
                        .clicked()
                    {
                        *submit = Some(message_index);
                    }
                    if ui.add(theme::secondary_button("取消")).clicked() {
                        *cancel = Some(message_index);
                    }
                    if sending {
                        ui.label(
                            RichText::new("AI 正在生成，暂时不能提交")
                                .size(8.8)
                                .color(theme::text_faint()),
                        );
                    } else if !complete {
                        ui.label(
                            RichText::new("请为每个问题选择或填写回答")
                                .size(8.8)
                                .color(theme::text_faint()),
                        );
                    }
                });
            } else if let Some(answer) = &message.ask_answer {
                ui.add_space(4.0);
                ui.add(
                    Label::new(
                        RichText::new(format!("已发送回答：{answer}"))
                            .size(8.5)
                            .color(theme::text_faint()),
                    )
                    .wrap(),
                );
            }
        });
}

fn ai_choice_button(ui: &mut Ui, label: &str, selected: bool) -> egui::Response {
    let display = if label.chars().count() > 60 {
        let mut text = label.chars().take(60).collect::<String>();
        text.push('…');
        text
    } else {
        label.to_owned()
    };
    ui.add(
        egui::Button::new(RichText::new(display).size(9.5).color(if selected {
            theme::on_accent()
        } else {
            theme::text()
        }))
        .fill(if selected {
            theme::accent()
        } else {
            theme::surface()
        })
        .stroke(Stroke::new(
            1.0_f32,
            if selected {
                theme::accent()
            } else {
                theme::border_soft()
            },
        ))
        .corner_radius(6)
        .min_size(egui::vec2(0.0, 24.0)),
    )
}

fn ai_deletes_card(
    ui: &mut Ui,
    message: &AiChatMessage,
    message_index: usize,
    apply_request: &mut Option<usize>,
    cancel_request: &mut Option<usize>,
) {
    let has_errors = message.deletes.iter().any(|delete| delete.error.is_some());
    let color = theme::error();
    Frame::new()
        .fill(theme::with_alpha(color, 10))
        .stroke(Stroke::new(1.0_f32, theme::with_alpha(color, 90)))
        .corner_radius(8)
        .inner_margin(Margin::symmetric(10, 8))
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.horizontal(|ui| {
                ui.label(
                    RichText::new("AI 文件删除")
                        .size(10.5)
                        .strong()
                        .color(theme::text()),
                );
                ui.label(
                    RichText::new(format!("{} 个文件", message.deletes.len()))
                        .size(9.5)
                        .color(theme::text_faint()),
                );
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    match message.delete_status {
                        AiEditStatus::Pending => {
                            if has_errors {
                                ui.add_enabled(false, theme::primary_button("删除文件"));
                            } else if ai_danger_button(ui, "删除文件") {
                                *apply_request = Some(message_index);
                            }
                            ui.add_space(4.0);
                            if ui.add(theme::secondary_button("取消")).clicked() {
                                *cancel_request = Some(message_index);
                            }
                        }
                        AiEditStatus::Applied => {
                            let (label, color) = if message.delete_error.is_some() {
                                ("部分完成", theme::warning())
                            } else {
                                ("已删除", theme::success())
                            };
                            ui.label(RichText::new(label).size(10.0).strong().color(color));
                        }
                        AiEditStatus::Cancelled => {
                            ui.label(
                                RichText::new("已取消")
                                    .size(10.0)
                                    .color(theme::text_faint()),
                            );
                        }
                    }
                });
            });
            ui.add_space(2.0);
            ui.add(
                Label::new(
                    RichText::new("删除不可撤销；IDE 会先备份到用户目录，并关闭相关标签页。")
                        .size(8.8)
                        .color(theme::text_faint()),
                )
                .wrap(),
            );
            if let Some(error) = &message.delete_error {
                ui.add_space(4.0);
                ui.add(Label::new(RichText::new(error).size(9.5).color(theme::error())).wrap());
            }
            for delete in &message.deletes {
                ui.add_space(5.0);
                ui.horizontal(|ui| {
                    ui.label(RichText::new("删除").size(9.0).strong().color(color));
                    ui.add(
                        Label::new(
                            RichText::new(&delete.path)
                                .monospace()
                                .size(9.5)
                                .color(theme::text()),
                        )
                        .wrap(),
                    );
                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        if delete.size > 0 {
                            ui.label(
                                RichText::new(format_delete_size(delete.size))
                                    .size(8.5)
                                    .color(theme::text_faint()),
                            );
                        }
                    });
                });
                if !delete.reason.is_empty() {
                    ui.add(
                        Label::new(
                            RichText::new(&delete.reason)
                                .size(8.8)
                                .color(theme::text_dim()),
                        )
                        .wrap(),
                    );
                }
                if let Some(error) = &delete.error {
                    ui.add_space(2.0);
                    ui.add(Label::new(RichText::new(error).size(9.0).color(theme::error())).wrap());
                }
            }
            if let Some(backup) = &message.delete_backup_dir {
                ui.add_space(5.0);
                ui.add(
                    Label::new(
                        RichText::new(format!("备份：{}", crate::util::display_path(backup)))
                            .size(8.5)
                            .color(theme::text_faint()),
                    )
                    .wrap(),
                );
            }
        });
}

fn ai_danger_button(ui: &mut Ui, label: &str) -> bool {
    ui.add(
        egui::Button::new(
            RichText::new(label)
                .size(10.0)
                .strong()
                .color(theme::on_accent()),
        )
        .fill(theme::error().gamma_multiply(0.8))
        .stroke(Stroke::NONE)
        .corner_radius(6)
        .min_size(egui::vec2(0.0, 24.0)),
    )
    .clicked()
}

fn format_delete_size(bytes: u64) -> String {
    if bytes >= 1024 * 1024 {
        format!("{:.1} MB", bytes as f64 / (1024.0 * 1024.0))
    } else if bytes >= 1024 {
        format!("{:.1} KB", bytes as f64 / 1024.0)
    } else {
        format!("{bytes} B")
    }
}

fn ai_edits_card(
    ui: &mut Ui,
    message: &AiChatMessage,
    message_index: usize,
    requests: &mut AiCardRequests,
) {
    let applied_count = message.edits.iter().filter(|edit| edit.applied).count();
    let has_errors = message
        .edits
        .iter()
        .any(|edit| !edit.applied && edit.error.is_some());
    Frame::new()
        .fill(theme::with_alpha(theme::accent(), 10))
        .stroke(Stroke::new(1.0_f32, theme::with_alpha(theme::accent(), 75)))
        .corner_radius(8)
        .inner_margin(Margin::symmetric(10, 8))
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.horizontal(|ui| {
                ui.label(
                    RichText::new("AI 文件修改")
                        .size(10.5)
                        .strong()
                        .color(theme::text()),
                );
                ui.label(
                    RichText::new(if applied_count > 0 {
                        format!("{applied_count}/{} 个文件", message.edits.len())
                    } else {
                        format!("{} 个文件", message.edits.len())
                    })
                    .size(9.5)
                    .color(theme::text_faint()),
                );
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    match message.edit_status {
                        AiEditStatus::Pending => {
                            if has_errors {
                                ui.add_enabled(false, theme::primary_button("应用全部"))
                                    .on_disabled_hover_text(
                                        "还有无法匹配的文件：可以单独应用可用文件，或点“分析失败原因”让 AI 修复",
                                    );
                                ui.add_space(4.0);
                                if ui
                                    .add(theme::secondary_button("分析失败原因"))
                                    .clicked()
                                {
                                    requests.analyze_errors = Some(message_index);
                                }
                            } else if ui.add(theme::primary_button("应用全部")).clicked() {
                                requests.apply_edits = Some(message_index);
                            }
                            ui.add_space(4.0);
                            if ui
                                .add(theme::secondary_button(if applied_count > 0 {
                                    "取消剩余"
                                } else {
                                    "取消"
                                }))
                                .clicked()
                            {
                                requests.cancel_edits = Some(message_index);
                            }
                        }
                        AiEditStatus::Applied => {
                            ui.label(
                                RichText::new("已应用")
                                    .size(10.0)
                                    .strong()
                                    .color(theme::success()),
                            );
                        }
                        AiEditStatus::Cancelled => {
                            ui.label(
                                RichText::new("已取消")
                                    .size(10.0)
                                    .color(theme::text_faint()),
                            );
                        }
                    }
                });
            });
            if let Some(error) = &message.apply_error {
                ui.add_space(4.0);
                ui.add(Label::new(RichText::new(error).size(9.5).color(theme::error())).wrap());
            }
            for (edit_index, edit) in message.edits.iter().enumerate() {
                ui.add_space(5.0);
                let (label, color) = match edit.kind {
                    crate::ai_edit::AiEditKind::Write if edit.base_hash.is_some() => {
                        ("覆盖", theme::warning())
                    }
                    crate::ai_edit::AiEditKind::Write => ("新建", theme::success()),
                    crate::ai_edit::AiEditKind::Replace => ("替换", theme::accent()),
                };
                let id = ui.make_persistent_id((
                    "simple_embedded_ai_edit_open",
                    message_index,
                    edit_index,
                ));
                let mut open = ui
                    .data_mut(|data| data.get_persisted::<bool>(id))
                    .unwrap_or(message.edits.len() == 1 && edit.diff.len() <= 40);
                ui.horizontal(|ui| {
                    ui.label(RichText::new(label).size(9.0).strong().color(color));
                    ui.add(
                        Label::new(
                            RichText::new(&edit.path)
                                .monospace()
                                .size(9.5)
                                .color(theme::text()),
                        )
                        .wrap(),
                    );
                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        if ai_ghost_button(ui, if open { "收起" } else { "差异" }) {
                            open = !open;
                            ui.data_mut(|data| data.insert_persisted(id, open));
                        }
                        if message.edit_status == AiEditStatus::Pending {
                            if edit.applied {
                                ui.label(
                                    RichText::new("已应用")
                                        .size(9.0)
                                        .strong()
                                        .color(theme::success()),
                                );
                            } else if edit.error.is_some() {
                                if ai_ghost_button(ui, "分析原因") {
                                    requests.analyze_edit = Some((message_index, edit_index));
                                }
                            } else if ai_ghost_button(ui, "应用此文件") {
                                requests.apply_single_edit = Some((message_index, edit_index));
                            }
                        }
                    });
                });
                if let Some(error) = &edit.error {
                    ui.add_space(2.0);
                    ui.add(Label::new(RichText::new(error).size(9.0).color(theme::error())).wrap());
                    if message.edit_status == AiEditStatus::Pending && !edit.applied {
                        ui.add(
                            Label::new(
                                RichText::new(
                                    "可点右侧“分析原因”，让 AI 按当前文件重新生成这一处。",
                                )
                                .size(8.5)
                                .color(theme::text_faint()),
                            )
                            .wrap(),
                        );
                    }
                } else if open {
                    render_ai_diff(ui, edit, message_index, edit_index);
                }
            }
            if let Some(backup) = &message.backup_dir {
                ui.add_space(5.0);
                ui.add(
                    Label::new(
                        RichText::new(format!("备份：{}", crate::util::display_path(backup)))
                            .size(8.5)
                            .color(theme::text_faint()),
                    )
                    .wrap(),
                );
            }
        });
}

fn render_ai_diff(
    ui: &mut Ui,
    edit: &crate::ai_edit::AiEdit,
    message_index: usize,
    edit_index: usize,
) {
    Frame::new()
        .fill(theme::editor_bg())
        .stroke(Stroke::new(1.0_f32, theme::border_soft()))
        .corner_radius(6)
        .inner_margin(Margin::symmetric(7, 5))
        .show(ui, |ui| {
            ScrollArea::vertical()
                .id_salt(("simple_embedded_ai_diff", message_index, edit_index))
                .auto_shrink([false, true])
                .max_height(260.0)
                .show(ui, |ui| {
                    ui.set_width(ui.available_width());
                    for line in &edit.diff {
                        let (prefix, color, background) = match line.tag {
                            crate::ai_edit::DiffTag::Add => (
                                "+",
                                theme::success(),
                                theme::with_alpha(theme::success(), 18),
                            ),
                            crate::ai_edit::DiffTag::Remove => {
                                ("-", theme::error(), theme::with_alpha(theme::error(), 18))
                            }
                            crate::ai_edit::DiffTag::Context => {
                                (" ", theme::text_dim(), Color32::TRANSPARENT)
                            }
                            crate::ai_edit::DiffTag::Meta => {
                                ("i", theme::warning(), Color32::TRANSPARENT)
                            }
                        };
                        let mut text = RichText::new(format!("{prefix} {}", line.text))
                            .monospace()
                            .size(9.5)
                            .color(color);
                        if background != Color32::TRANSPARENT {
                            text = text.background_color(background);
                        }
                        ui.add(Label::new(text).wrap());
                    }
                });
        });
}

fn ai_empty_state(ui: &mut Ui, quick_prompt: &mut Option<&'static str>) {
    ui.add_space(8.0);
    let (orb_rect, _) =
        ui.allocate_exact_size(egui::vec2(ui.available_width(), 100.0), Sense::hover());
    paint_orb(
        ui,
        orb_rect.center(),
        30.0,
        ui.input(|input| input.time),
        false,
    );
    ui.vertical_centered(|ui| {
        ui.label(
            RichText::new("描述你的 STM32 需求")
                .size(14.0)
                .strong()
                .color(theme::text()),
        );
        ui.label(
            RichText::new("先读工程与 stm32-embedded-dev skill，生成文件修改并在你确认后写入")
                .size(10.0)
                .color(theme::text_dim()),
        );
    });
    ui.add_space(12.0);

    let prompts: [(&str, &'static str); 4] = [
        (
            "直接改文件",
            "请直接修改当前工程文件，不要只贴代码；使用 se-file / se-replace 协议，应用前给我看差异。",
        ),
        (
            "新建硬件台账",
            "请为本工程建立 hardware.dts 硬件台账，列出芯片、时钟、已用外设、引脚复用、中断和 DMA，并直接写入工程。",
        ),
        (
            "配置串口收发",
            "请为当前工程配置 USART1 中断收发，更新 hardware.dts、HAL 初始化和中断处理，并直接修改工程文件。",
        ),
        (
            "排查 HardFault",
            "工程运行出现 HardFault，请根据当前工程给出排查步骤、需要查看的寄存器和必要的诊断代码。",
        ),
    ];
    for (title, prompt) in prompts {
        let response = ui
            .add(
                Button::new(
                    RichText::new(format!("{title}  ·  {}", truncate_chars(prompt, 34)))
                        .size(10.0)
                        .color(theme::text_dim()),
                )
                .fill(theme::with_alpha(theme::surface(), 130))
                .stroke(Stroke::new(1.0_f32, theme::border_soft()))
                .corner_radius(6)
                .min_size(egui::vec2(ui.available_width(), 27.0)),
            )
            .on_hover_text(prompt);
        if response.clicked() {
            *quick_prompt = Some(prompt);
        }
        ui.add_space(4.0);
    }
}

fn render_markdown(ui: &mut Ui, text: &str, message_index: usize, error: bool) {
    let color = if error { theme::error() } else { theme::text() };
    let mut rest = text;
    let mut block = 0usize;
    while !rest.is_empty() {
        if let Some(start) = rest.find("```") {
            let prose = &rest[..start];
            render_prose(ui, prose, color);
            let after = &rest[start + 3..];
            let (language, body) = match after.find('\n') {
                Some(newline) => (&after[..newline], &after[newline + 1..]),
                None => ("", after),
            };
            if let Some(end) = body.find("```") {
                render_code_block(ui, language.trim(), &body[..end], message_index, block);
                block += 1;
                rest = &body[end + 3..];
            } else {
                render_code_block(ui, language.trim(), body, message_index, block);
                rest = "";
            }
        } else {
            render_prose(ui, rest, color);
            rest = "";
        }
    }
}

fn render_prose(ui: &mut Ui, text: &str, color: Color32) {
    let text = text.trim_matches('\n');
    if text.trim().is_empty() {
        return;
    }
    for line in text.lines() {
        let trimmed = line.trim_end();
        if trimmed.trim().is_empty() {
            ui.add_space(4.0);
            continue;
        }
        let (content, size, strong, prefix) = if let Some(rest) = trimmed.strip_prefix("### ") {
            (rest, 13.5, true, "")
        } else if let Some(rest) = trimmed.strip_prefix("## ") {
            (rest, 14.0, true, "")
        } else if let Some(rest) = trimmed.strip_prefix("# ") {
            (rest, 15.0, true, "")
        } else if let Some(rest) = trimmed
            .strip_prefix("- ")
            .or_else(|| trimmed.strip_prefix("* "))
        {
            (rest, 12.0, false, "· ")
        } else {
            (trimmed, 12.0, false, "")
        };
        ui.horizontal_wrapped(|ui| {
            ui.spacing_mut().item_spacing.x = 0.0;
            if !prefix.is_empty() {
                ui.label(RichText::new(prefix).size(size).color(theme::accent()));
            }
            render_inline(ui, content, size, strong, color);
        });
        ui.add_space(3.0);
    }
}

fn render_inline(ui: &mut Ui, text: &str, size: f32, strong: bool, color: Color32) {
    let mut rest = text;
    let mut bold = strong;
    while !rest.is_empty() {
        let next_tick = rest.find('`');
        let next_bold = rest.find("**");
        let next = match (next_tick, next_bold) {
            (Some(a), Some(b)) => Some((a.min(b), a <= b)),
            (Some(a), None) => Some((a, true)),
            (None, Some(b)) => Some((b, false)),
            (None, None) => None,
        };
        let Some((index, is_code)) = next else {
            push_inline_text(ui, rest, size, bold, color, false);
            break;
        };
        if index > 0 {
            push_inline_text(ui, &rest[..index], size, bold, color, false);
        }
        if is_code {
            let after = &rest[index + 1..];
            if let Some(end) = after.find('`') {
                push_inline_text(ui, &after[..end], size, false, theme::accent(), true);
                rest = &after[end + 1..];
            } else {
                push_inline_text(ui, "`", size, false, color, false);
                rest = after;
            }
        } else {
            bold = !bold;
            rest = &rest[index + 2..];
        }
    }
}

fn push_inline_text(ui: &mut Ui, text: &str, size: f32, strong: bool, color: Color32, code: bool) {
    if text.is_empty() {
        return;
    }
    let mut rich = RichText::new(text).size(size).color(color);
    if code {
        rich = rich
            .font(FontId::monospace(size - 0.5))
            .background_color(theme::with_alpha(theme::accent(), 26));
    }
    if strong {
        rich = rich.strong();
    }
    ui.label(rich);
}

fn render_code_block(ui: &mut Ui, language: &str, code: &str, message_index: usize, block: usize) {
    let code = code.trim_end_matches('\n');
    if code.is_empty() {
        return;
    }
    Frame::new()
        .fill(theme::editor_bg())
        .stroke(Stroke::new(1.0_f32, theme::border_soft()))
        .corner_radius(8)
        .inner_margin(Margin::symmetric(9, 7))
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.horizontal(|ui| {
                ui.label(
                    RichText::new(if language.is_empty() {
                        "代码"
                    } else {
                        language
                    })
                    .size(9.0)
                    .color(theme::text_faint()),
                );
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    if ai_ghost_button(ui, "复制") {
                        ui.ctx().copy_text(code.to_owned());
                    }
                });
            });
            ScrollArea::horizontal()
                .id_salt(("simple_embedded_ai_code", message_index, block))
                .auto_shrink([false, true])
                .show(ui, |ui| {
                    ui.add(
                        Label::new(
                            RichText::new(code)
                                .monospace()
                                .size(11.0)
                                .color(theme::text()),
                        )
                        .selectable(true),
                    );
                });
        });
    ui.add_space(6.0);
}

fn paint_orb(ui: &Ui, center: egui::Pos2, radius: f32, time: f64, active: bool) {
    let painter = ui.painter();
    let accent = theme::accent();
    let pulse = if active {
        0.72 + 0.28 * (time as f32 * 4.2).sin().abs()
    } else {
        0.58 + 0.42 * (time as f32 * 1.15).sin().abs()
    };
    for (scale, alpha) in [(2.0_f32, 10.0_f32), (1.6, 18.0), (1.28, 28.0)] {
        painter.circle_filled(
            center,
            radius * scale,
            theme::with_alpha(accent, (alpha * pulse) as u8),
        );
    }
    painter.circle_filled(
        center,
        radius * 0.92,
        theme::blend(theme::panel(), accent, 0.34),
    );
    painter.circle_stroke(
        center,
        radius,
        Stroke::new(1.2_f32, theme::with_alpha(accent, 150)),
    );
    for ring in 0..3 {
        let base = time as f32 * (0.5 + ring as f32 * 0.3) + ring as f32 * 2.2;
        let span = 1.1 + ring as f32 * 0.35;
        let points = (0..=28)
            .map(|step| {
                let angle = base + span * step as f32 / 28.0;
                center + egui::vec2(angle.cos(), angle.sin()) * radius * (1.18 + ring as f32 * 0.16)
            })
            .collect::<Vec<_>>();
        painter.add(egui::Shape::line(
            points,
            Stroke::new(1.1_f32, theme::with_alpha(accent, 130 - ring as u8 * 28)),
        ));
    }
    for dot in 0..3 {
        let angle = time as f32 * (0.9 + dot as f32 * 0.2) + dot as f32 * 2.4;
        let position =
            center + egui::vec2(angle.cos(), angle.sin()) * radius * (1.5 + dot as f32 * 0.1);
        painter.circle_filled(position, 2.2, theme::with_alpha(accent, 200));
    }
    painter.text(
        center,
        Align2::CENTER_CENTER,
        "AI",
        FontId::proportional(radius * 0.58),
        theme::text(),
    );
}

fn fog_image(size: usize) -> egui::ColorImage {
    let mut pixels = Vec::with_capacity(size * size);
    for y in 0..size {
        for x in 0..size {
            let u = x as f32 / size as f32;
            let v = y as f32 / size as f32;
            let mut value = 0.0;
            let mut amplitude = 0.55;
            let mut frequency = 4u32;
            for octave in 0..4u32 {
                value += amplitude * value_noise(u, v, frequency, octave * 97 + 13);
                amplitude *= 0.5;
                frequency *= 2;
            }
            let alpha = (value.clamp(0.0, 1.0) * 255.0) as u8;
            pixels.push(Color32::from_white_alpha(alpha));
        }
    }
    egui::ColorImage::new([size, size], pixels)
}

fn value_noise(u: f32, v: f32, frequency: u32, seed: u32) -> f32 {
    let x = u.clamp(0.0, 1.0) * frequency as f32;
    let y = v.clamp(0.0, 1.0) * frequency as f32;
    let x0 = x.floor() as u32;
    let y0 = y.floor() as u32;
    let tx = smoothstep(x - x.floor());
    let ty = smoothstep(y - y.floor());
    let wrap = |n: u32| n % frequency.max(1);
    let a = hash_noise(wrap(x0), wrap(y0), seed);
    let b = hash_noise(wrap(x0 + 1), wrap(y0), seed);
    let c = hash_noise(wrap(x0), wrap(y0 + 1), seed);
    let d = hash_noise(wrap(x0 + 1), wrap(y0 + 1), seed);
    let top = a + (b - a) * tx;
    let bottom = c + (d - c) * tx;
    top + (bottom - top) * ty
}

fn hash_noise(x: u32, y: u32, seed: u32) -> f32 {
    let mut h = x.wrapping_mul(374_761_393) ^ y.wrapping_mul(668_265_263) ^ seed;
    h = (h ^ (h >> 13)).wrapping_mul(1_274_126_177);
    ((h ^ (h >> 16)) & 0xFFFF) as f32 / 65_535.0
}

fn paint_transition(
    ui: &Ui,
    from: Option<Rect>,
    view: Rect,
    elapsed: f32,
    fog: Option<&egui::TextureHandle>,
    right_orb: egui::Pos2,
    output_orb: egui::Pos2,
) {
    let painter = ui.painter();
    let strength = fog_strength(elapsed);
    if strength > 0.001 {
        if let Some(fog) = fog {
            paint_fog(painter, view, from, fog, elapsed, strength);
        } else {
            paint_fog_fallback(painter, view, from, elapsed, strength);
        }
    }
    // One meteor lands on each AI core. The second one is slightly smaller and
    // later so the two impacts read as a pair rather than a single blink.
    paint_meteor(
        painter,
        view,
        MeteorSpec {
            target: right_orb,
            start: METEOR_RIGHT_START,
            end: METEOR_RIGHT_END,
            seed: 0.0,
            scale: 1.0,
        },
        elapsed,
    );
    paint_meteor(
        painter,
        view,
        MeteorSpec {
            target: output_orb,
            start: METEOR_OUTPUT_START,
            end: METEOR_OUTPUT_END,
            seed: 1.0,
            scale: 0.82,
        },
        elapsed,
    );
}

fn paint_fog(
    painter: &egui::Painter,
    view: Rect,
    editor: Option<Rect>,
    fog: &egui::TextureHandle,
    elapsed: f32,
    strength: f32,
) {
    let tint = theme::blend(Color32::WHITE, theme::accent(), 0.18);
    let drift = elapsed * 0.035;
    let first = Rect::from_min_max(
        egui::pos2(drift * 0.35, drift * 0.2),
        egui::pos2(1.0 + drift * 0.35, 1.0 + drift * 0.2),
    );
    painter.image(
        fog.id(),
        view.expand(90.0),
        first,
        theme::with_alpha(tint, (strength * 118.0) as u8),
    );
    let second = Rect::from_min_max(
        egui::pos2(0.55 - drift * 0.5, 0.15 + drift * 0.42),
        egui::pos2(1.55 - drift * 0.5, 1.15 + drift * 0.42),
    );
    painter.image(
        fog.id(),
        view.expand(140.0),
        second,
        theme::with_alpha(tint, (strength * 78.0) as u8),
    );
    paint_mist_particles(painter, view, editor, elapsed, strength, tint);
}

fn paint_fog_fallback(
    painter: &egui::Painter,
    view: Rect,
    editor: Option<Rect>,
    elapsed: f32,
    strength: f32,
) {
    let tint = theme::blend(theme::editor_bg(), theme::text(), 0.36);
    painter.rect_filled(
        view,
        CornerRadius::ZERO,
        theme::with_alpha(tint, (strength * 54.0) as u8),
    );
    paint_mist_particles(painter, view, editor, elapsed, strength, tint);
}

fn paint_mist_particles(
    painter: &egui::Painter,
    view: Rect,
    editor: Option<Rect>,
    elapsed: f32,
    strength: f32,
    tint: Color32,
) {
    let source = editor.unwrap_or(view);
    let mut rng = Lcg::new(0xF09_1A2B);
    for _ in 0..48 {
        let delay = rng.next() * 0.35;
        let life = 0.65 + rng.next() * 0.7;
        let local = ((elapsed - delay) / life).clamp(0.0, 1.0);
        if local >= 1.0 {
            continue;
        }
        let start = egui::pos2(
            source.left() + rng.next() * source.width(),
            source.top() + rng.next() * source.height(),
        );
        let drift = egui::vec2(
            (rng.next() - 0.5) * 190.0 * local,
            -35.0 * local - 110.0 * local * local,
        );
        let position = start + drift;
        let radius = 16.0 + rng.next() * 52.0;
        let alpha = ((1.0 - local) * strength * 26.0) as u8;
        for ring in 0..3 {
            painter.circle_filled(
                position,
                radius * (1.0 - ring as f32 * 0.22),
                theme::with_alpha(tint, alpha / (ring as u8 + 1)),
            );
        }
    }
}

fn paint_meteor(painter: &egui::Painter, view: Rect, spec: MeteorSpec, elapsed: f32) {
    let travel = ((elapsed - spec.start) / (spec.end - spec.start)).clamp(0.0, 1.0);
    if travel <= 0.0 {
        return;
    }
    let start = egui::pos2(
        view.right() + 140.0 + spec.seed * 90.0,
        view.bottom() + 190.0 + spec.seed * 70.0,
    );
    let control = if spec.target.x > view.center().x {
        egui::pos2(view.right() - 20.0, view.bottom() - 150.0)
    } else {
        egui::pos2(view.center().x + view.width() * 0.12, view.bottom() - 60.0)
    };
    let head = bezier(start, control, spec.target, ease_in_out(travel));

    let steps = 28;
    let mut previous = None;
    for step in (0..=steps).rev() {
        let t = (travel - step as f32 * 0.011).clamp(0.0, 1.0);
        let position = bezier(start, control, spec.target, ease_in_out(t));
        let fraction = step as f32 / steps as f32;
        let alpha = ((1.0 - fraction) * (travel * 1.8).min(1.0) * 190.0 * spec.scale) as u8;
        let radius = ((5.6 - fraction * 4.3) * spec.scale).max(0.5);
        if let Some(previous) = previous {
            painter.line_segment(
                [previous, position],
                Stroke::new(
                    (1.2 * spec.scale).max(0.5),
                    theme::with_alpha(theme::accent(), alpha),
                ),
            );
        }
        previous = Some(position);
        painter.circle_filled(position, radius, theme::with_alpha(theme::accent(), alpha));
    }
    for (radius, alpha) in [(30.0_f32, 16_u8), (18.0, 36), (10.0, 90), (4.4, 255)] {
        let color = if alpha == 255 {
            Color32::WHITE
        } else {
            theme::with_alpha(theme::accent(), alpha)
        };
        painter.circle_filled(head, radius * spec.scale, color);
    }

    if travel >= 1.0 {
        let impact = ((elapsed - spec.end) / 0.55).clamp(0.0, 1.0);
        if impact < 1.0 {
            for ring in 0..3 {
                let radius = (8.0 + (impact + ring as f32 * 0.18) * 140.0) * spec.scale;
                painter.circle_stroke(
                    spec.target,
                    radius,
                    Stroke::new(
                        2.0_f32,
                        theme::with_alpha(theme::accent(), ((1.0 - impact) * 120.0) as u8),
                    ),
                );
            }
            let flash = ((elapsed - spec.end) / 0.16).clamp(0.0, 1.0);
            if flash < 1.0 {
                painter.circle_filled(
                    spec.target,
                    (10.0 + flash * 22.0) * spec.scale,
                    theme::with_alpha(Color32::WHITE, ((1.0 - flash) * 180.0) as u8),
                );
            }
        }
    }
}

fn bezier(start: egui::Pos2, control: egui::Pos2, end: egui::Pos2, t: f32) -> egui::Pos2 {
    let u = 1.0 - t;
    egui::pos2(
        u * u * start.x + 2.0 * u * t * control.x + t * t * end.x,
        u * u * start.y + 2.0 * u * t * control.y + t * t * end.y,
    )
}

fn truncate_chars(text: &str, max: usize) -> String {
    if text.chars().count() <= max {
        return text.to_owned();
    }
    let mut result = text.chars().take(max).collect::<String>();
    result.push('…');
    result
}

/// While a long stream is arriving, only lay out the newest slice. Re-laying
/// out the entire reasoning/content on every token would make the UI stutter.
fn tail_preview(text: &str, max_bytes: usize) -> Option<String> {
    if text.len() <= max_bytes {
        return None;
    }
    let mut start = text.len() - max_bytes;
    while start < text.len() && !text.is_char_boundary(start) {
        start += 1;
    }
    Some(format!("…\n{}", &text[start..]))
}

struct Lcg(u32);

impl Lcg {
    fn new(seed: u32) -> Self {
        Self(seed | 1)
    }

    fn next(&mut self) -> f32 {
        self.0 = self.0.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
        ((self.0 >> 8) & 0x00FF_FFFF) as f32 / 16_777_216.0
    }
}

fn ai_resource_list(ui: &mut Ui, entries: &[crate::ai::ResourceEntry]) {
    for entry in entries.iter().take(8) {
        let response = Frame::new()
            .fill(theme::with_alpha(theme::surface(), 100))
            .stroke(Stroke::new(1.0_f32, theme::border_soft()))
            .corner_radius(5)
            .inner_margin(Margin::symmetric(7, 4))
            .show(ui, |ui| {
                ui.set_width(ui.available_width());
                ui.label(RichText::new(&entry.name).size(9.5).color(theme::text()));
                ui.label(
                    RichText::new(format!(
                        "{} 个文件 · {} · {}",
                        entry.files,
                        crate::memory::format_size(entry.bytes),
                        entry.note
                    ))
                    .size(8.3)
                    .color(theme::text_faint()),
                );
            });
        response
            .response
            .on_hover_text(crate::util::display_path(&entry.path));
        ui.add_space(3.0);
    }
    if entries.len() > 8 {
        ui.label(
            RichText::new(format!("另有 {} 项未展开", entries.len() - 8))
                .size(8.5)
                .color(theme::text_faint()),
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reveal_staggers_then_finishes() {
        let start = PANEL_START + 2.0 * PANEL_STAGGER;
        assert_eq!(reveal_at(start - 0.01, 2), 0.0);
        assert!(reveal_at(start + PANEL_DURATION * 0.5, 2) > 0.4);
        assert_eq!(reveal_at(start + PANEL_DURATION + 0.01, 2), 1.0);
    }

    #[test]
    fn transition_duration_covers_all_panels() {
        let end = PANEL_START + 2.0 * PANEL_STAGGER + PANEL_DURATION;
        assert!(end < TRANSITION_SECS);
        const { assert!(TRANSITION_SECS <= 1.0) };
    }

    #[test]
    fn window_alpha_stays_opaque_during_quick_transition() {
        assert_eq!(ai_window_alpha(0.0), 255);
        assert_eq!(ai_window_alpha(0.1), 255);
        assert_eq!(ai_window_alpha(2.4), 255);
    }

    #[test]
    fn fog_image_is_varied_noise() {
        let image = fog_image(32);
        assert_eq!(image.size, [32, 32]);
        assert!(image.pixels.iter().any(|pixel| pixel.a() > 0));
        assert!(image.pixels.iter().any(|pixel| pixel.a() < 255));
    }

    #[test]
    fn bezier_hits_end_points() {
        let start = egui::pos2(0.0, 0.0);
        let control = egui::pos2(50.0, 100.0);
        let end = egui::pos2(100.0, 0.0);
        assert_eq!(bezier(start, control, end, 0.0), start);
        assert_eq!(bezier(start, control, end, 1.0), end);
    }

    #[test]
    fn lcg_is_deterministic_and_in_range() {
        let mut a = Lcg::new(42);
        let mut b = Lcg::new(42);
        for _ in 0..64 {
            let value = a.next();
            assert_eq!(value, b.next());
            assert!((0.0..1.0).contains(&value));
        }
    }

    #[test]
    fn truncation_is_char_safe() {
        let text = "中文测试 abc";
        let short = truncate_chars(text, 4);
        assert_eq!(short.chars().count(), 5);
        assert!(short.ends_with('…'));
    }
}
