//! AI mode layout: three large panels, chat rendering and the enter transition.
//!
//! The layout intentionally does not reuse the editor panels. AI mode owns the
//! whole window so the "shatter into a meteor" transition can cover every
//! widget without fighting panel clipping.

use std::time::Duration;

use eframe::egui::{
    self, Align, Align2, Button, Color32, CornerRadius, FontId, Frame, Id, Label, Layout, Margin,
    Rect, RichText, ScrollArea, Sense, Stroke, TextEdit, Ui, UiBuilder,
};

use super::{AiChatMessage, App};
use crate::log::Severity;
use crate::theme;

const TRANSITION_SECS: f32 = 2.75;
const METEOR_START: f32 = 0.72;
const METEOR_END: f32 = 1.72;
const PANEL_START: f32 = 1.48;
const PANEL_STAGGER: f32 = 0.14;
const PANEL_DURATION: f32 = 0.92;
const INPUT_ID: &str = "simple_embedded_ai_input";

impl App {
    pub(super) fn show_ai_mode(&mut self, ui: &mut egui::Ui) {
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
        let right_width = (area.width() * 0.28)
            .clamp(250.0, 360.0)
            .min((area.width() * 0.4).max(220.0));
        let left_width = (area.width() - right_width - gap).max(220.0);
        let left = Rect::from_min_size(area.min, egui::vec2(left_width, area.height()));
        let right = Rect::from_min_size(
            egui::pos2(left.right() + gap, area.top()),
            egui::vec2(right_width, area.height()),
        );
        let output_height =
            ((left.height() - gap) * 0.64).clamp(110.0, (left.height() - gap - 70.0).max(110.0));
        let output = Rect::from_min_size(left.min, egui::vec2(left.width(), output_height));
        let input = Rect::from_min_max(egui::pos2(left.left(), output.bottom() + gap), left.max);

        let reveal_right = reveal_at(elapsed, 0);
        let reveal_input = reveal_at(elapsed, 1);
        let reveal_output = reveal_at(elapsed, 2);

        let mut send = false;
        let mut clear = false;
        let mut quick_prompt = None;
        let mut open_settings = false;
        let mut original_mode = false;
        let mut refresh_skill = false;

        self.ai_output_panel(ui, output, reveal_output, &mut clear, &mut quick_prompt);
        self.ai_input_panel(ui, input, reveal_input, &mut send);
        self.ai_workbench_panel(
            ui,
            right,
            reveal_right,
            &mut original_mode,
            &mut open_settings,
            &mut refresh_skill,
        );

        if clear {
            self.clear_ai_chat();
        }
        if let Some(prompt) = quick_prompt {
            self.ai.input = prompt.to_owned();
            self.ai.focus_input = true;
        }
        if refresh_skill {
            self.refresh_ai_skill();
        }
        if open_settings {
            self.open_ai_settings();
        }
        if original_mode {
            self.exit_ai_mode();
        }
        if send {
            self.send_ai_message();
        }

        if progress < 1.0 {
            ui.ctx().request_repaint();
            let from = transition
                .map(|(_, from)| from)
                .filter(|from| from.width() > 1.0 && from.height() > 1.0);
            paint_transition(ui, from, view, right, elapsed);
        } else if self.ai.transition.is_some() {
            self.ai.transition = None;
        }

        // Slow orb animation while AI mode is idle; faster while waiting for a
        // reply so the panel still feels alive during long requests.
        let repaint = if self.ai.sending { 60 } else { 180 };
        ui.ctx()
            .request_repaint_after(Duration::from_millis(repaint));
    }

    fn ai_output_panel(
        &mut self,
        ui: &mut egui::Ui,
        rect: Rect,
        reveal: f32,
        clear: &mut bool,
        quick_prompt: &mut Option<&'static str>,
    ) {
        ai_panel(ui, rect, reveal, |ui| {
            ui.horizontal(|ui| {
                ui.label(
                    RichText::new("AI 输出")
                        .size(13.0)
                        .strong()
                        .color(theme::text()),
                );
                let (status, color) = if self.ai.sending {
                    ("生成中", theme::accent())
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
                        RichText::new(format!("{} 条消息", self.ai.messages.len()))
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
                    if self.ai.messages.is_empty() {
                        ai_empty_state(ui, quick_prompt);
                    } else {
                        for (index, message) in self.ai.messages.iter().enumerate() {
                            ai_message(ui, message, index);
                            ui.add_space(8.0);
                        }
                    }
                    if self.ai.sending {
                        ui.horizontal(|ui| {
                            ui.spinner();
                            ui.label(
                                RichText::new("正在结合工程上下文生成，请稍候…")
                                    .size(10.5)
                                    .color(theme::text_dim()),
                            );
                        });
                        ui.add_space(6.0);
                    }
                    ui.add_space(4.0);
                });
        });
    }

    fn ai_input_panel(&mut self, ui: &mut egui::Ui, rect: Rect, reveal: f32, send: &mut bool) {
        let input_id = Id::new(INPUT_ID);
        let enter_pressed = ui.memory(|memory| memory.has_focus(input_id))
            && ui.input_mut(|input| input.consume_key(egui::Modifiers::CTRL, egui::Key::Enter))
            && !self.ai.sending
            && !self.ai.input.trim().is_empty();
        let mut remove_context = false;

        ai_panel(ui, rect, reveal, |ui| {
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
                            RichText::new(format!("上下文 · {name}"))
                                .size(9.5)
                                .color(theme::accent()),
                        )
                        .on_hover_text(crate::util::display_path(path));
                    }
                });
            });
            ai_rule(ui);
            ui.add_space(4.0);

            let footer = 44.0;
            let editor_height = (ui.available_height() - footer).max(40.0);
            Frame::new()
                .fill(theme::editor_bg())
                .stroke(Stroke::new(1.0_f32, theme::border_soft()))
                .corner_radius(9)
                .inner_margin(Margin::symmetric(8, 6))
                .show(ui, |ui| {
                    ui.set_width(ui.available_width());
                    let response = ui.add_sized(
                        [ui.available_width(), (editor_height - 14.0).max(28.0)],
                        TextEdit::multiline(&mut self.ai.input)
                            .id(input_id)
                            .frame(Frame::NONE)
                            .margin(Margin::symmetric(0, 0))
                            .font(FontId::proportional(13.0))
                            .text_color(theme::text())
                            .hint_text("描述要新增的外设、要修改的逻辑，或贴上报错信息…")
                            .desired_width(f32::INFINITY),
                    );
                    if self.ai.focus_input {
                        response.request_focus();
                        self.ai.focus_input = false;
                    }
                });

            ui.add_space(5.0);
            ui.horizontal(|ui| {
                ui.label(
                    RichText::new("AI 读取已保存的工程文件、芯片信息和资源报告")
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
        original_mode: &mut bool,
        open_settings: &mut bool,
        refresh_skill: &mut bool,
    ) {
        ai_panel(ui, rect, reveal, |ui| {
            let footer = 42.0;
            let content_height = (ui.available_height() - footer).max(60.0);
            ScrollArea::vertical()
                .id_salt("simple_embedded_ai_workbench_scroll")
                .auto_shrink([false, false])
                .max_height(content_height)
                .show(ui, |ui| {
                    ui.set_width(ui.available_width());
                    let (orb_rect, _) = ui.allocate_exact_size(
                        egui::vec2(ui.available_width(), 116.0),
                        Sense::hover(),
                    );
                    paint_orb(
                        ui,
                        orb_rect.center() - egui::vec2(0.0, 4.0),
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
                        ai_kv(ui, "当前文件", &name);
                    }

                    ui.add_space(10.0);
                    ai_section(ui, "已加载 Skill");
                    ai_skill_card(
                        ui,
                        &self.ai.skill_source,
                        self.ai.skill_files,
                        self.ai.skill_builtin,
                        self.ai.skill_error.as_deref(),
                        refresh_skill,
                    );

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
                    *open_settings = true;
                }
                ui.add_space(4.0);
                if ui
                    .add(theme::secondary_button("原始模式"))
                    .on_hover_text("返回编辑器（Ctrl+Alt+A）")
                    .clicked()
                {
                    *original_mode = true;
                }
            });
        });
    }

    pub(super) fn show_ai_settings_modal(&mut self, ctx: &egui::Context) {
        let Some(mut draft) = self.ai_settings_draft.take() else {
            return;
        };
        let mut close = false;
        let mut save = false;
        let mut reload_skill = false;

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
            ui.set_width(540.0);
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
                    "使用 OpenAI 兼容接口。设置保存在用户目录，不写入当前工程；进入 AI 模式后会自动加载 stm32-embedded-dev skill。",
                )
                .size(10.5)
                .color(theme::text_dim()),
            );
            ui.add_space(12.0);

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

            ai_section(ui, "Skill 状态");
            ai_skill_card(
                ui,
                &self.ai.skill_source,
                self.ai.skill_files,
                self.ai.skill_builtin,
                self.ai.skill_error.as_deref(),
                &mut reload_skill,
            );

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

fn reveal_at(elapsed: f32, index: usize) -> f32 {
    let start = PANEL_START + index as f32 * PANEL_STAGGER;
    smoothstep(((elapsed - start) / PANEL_DURATION).clamp(0.0, 1.0))
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

fn ai_panel(ui: &mut Ui, rect: Rect, reveal: f32, add: impl FnOnce(&mut Ui)) {
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
    let anchor = rect.right_bottom();
    let eased = ease_out_cubic(p);
    let revealed = Rect::from_min_max(
        egui::pos2(
            anchor.x + (rect.left() - anchor.x) * eased,
            anchor.y + (rect.top() - anchor.y) * eased,
        ),
        anchor,
    );
    let clip = revealed.intersect(content_rect);
    if p < 0.999 {
        let glow = theme::with_alpha(theme::accent(), ((1.0 - p) * 150.0) as u8);
        painter.line_segment(
            [clip.left_top(), clip.right_top()],
            Stroke::new(1.3_f32, glow),
        );
        painter.line_segment(
            [clip.left_top(), clip.left_bottom()],
            Stroke::new(1.3_f32, glow),
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

fn ai_message(ui: &mut Ui, message: &AiChatMessage, index: usize) {
    let user = message.role == "user";
    let (fill, border) = if message.error {
        (
            theme::with_alpha(theme::error(), 22),
            theme::with_alpha(theme::error(), 110),
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
                } else if user {
                    ("你", theme::accent())
                } else {
                    ("AI", theme::success())
                };
                ui.label(RichText::new(role).size(10.0).strong().color(color));
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    if ai_ghost_button(ui, "复制") {
                        ui.ctx().copy_text(message.content.clone());
                    }
                });
            });
            ui.add_space(3.0);
            render_markdown(ui, &message.content, index, message.error);
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
            RichText::new("先读工程与 stm32-embedded-dev skill，再按硬件台账输出可落地代码")
                .size(10.0)
                .color(theme::text_dim()),
        );
    });
    ui.add_space(12.0);

    let prompts: [(&str, &'static str); 3] = [
        (
            "新建硬件台账",
            "请先为本工程建立 hardware.dts 硬件台账，列出芯片、时钟、已用外设、引脚复用、中断和 DMA。",
        ),
        (
            "配置串口收发",
            "请为当前工程配置 USART1 中断收发，给出 hardware.dts 节点、HAL 初始化代码和中断处理。",
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

fn paint_transition(ui: &Ui, from: Option<Rect>, view: Rect, target: Rect, elapsed: f32) {
    let painter = ui.painter();
    let veil = if elapsed < 0.55 {
        (elapsed / 0.55 * 120.0).clamp(0.0, 120.0)
    } else {
        120.0 * (1.0 - ((elapsed - 1.1) / 0.55).clamp(0.0, 1.0))
    };
    painter.rect_filled(
        view,
        CornerRadius::ZERO,
        Color32::from_black_alpha(veil as u8),
    );
    if let Some(from) = from {
        paint_shatter(painter, from, elapsed);
    }
    paint_meteor(painter, view, target, elapsed);
}

fn paint_shatter(painter: &egui::Painter, from: Rect, elapsed: f32) {
    if from.width() < 24.0 || from.height() < 24.0 {
        return;
    }
    let cols = 18usize;
    let rows = ((from.height() / from.width() * cols as f32).round() as usize).clamp(6, 30);
    let cell_w = from.width() / cols as f32;
    let cell_h = from.height() / rows as f32;
    let center = from.center();
    let palette = [
        theme::text(),
        theme::accent(),
        theme::success(),
        theme::warning(),
        theme::info(),
    ];

    for row in 0..rows {
        for col in 0..cols {
            let seed = (row * cols + col) as u32 * 2_654_435_761 + 0x9E37;
            let mut rng = Lcg::new(seed);
            let delay = rng.next() * 0.22;
            let life = 0.55 + rng.next() * 0.45;
            let local = ((elapsed - delay) / life).clamp(0.0, 1.0);
            if local >= 1.0 {
                continue;
            }
            let cell = Rect::from_min_size(
                egui::pos2(
                    from.left() + col as f32 * cell_w,
                    from.top() + row as f32 * cell_h,
                ),
                egui::vec2(cell_w, cell_h),
            );
            let tint = palette[(rng.next() * palette.len() as f32) as usize % palette.len()];
            let base = theme::blend(theme::editor_bg(), tint, 0.04 + rng.next() * 0.11);
            let alpha = ((1.0 - local) * 255.0) as u8;
            let fill = theme::with_alpha(base, alpha);

            let mut direction = cell.center() - center;
            if direction.length_sq() < 1.0 {
                direction = egui::vec2(1.0, -1.0);
            }
            let jitter = egui::vec2(rng.next() - 0.5, rng.next() - 0.5) * 0.9;
            let speed = 40.0 + rng.next() * 250.0;
            let travel = local * local;
            let offset = (direction.normalized() + jitter) * (speed * travel)
                + egui::vec2(0.0, 80.0 * travel);
            let angle = (rng.next() - 0.5) * 2.4 * local;

            let corners = rotated_corners(cell.shrink(0.6), angle)
                .into_iter()
                .map(|point| point + offset)
                .collect::<Vec<_>>();
            painter.add(egui::Shape::convex_polygon(
                corners.clone(),
                fill,
                Stroke::NONE,
            ));

            if cell_w > 16.0 && cell_h > 8.0 {
                for line in 0..2 {
                    let ratio = if line == 0 { 0.36 } else { 0.68 };
                    let y = cell.top() + cell_h * ratio;
                    let start = egui::pos2(cell.left() + cell_w * 0.12, y);
                    let end = egui::pos2(cell.right() - cell_w * (0.12 + rng.next() * 0.42), y);
                    let start = rotate_point(start, cell.center(), angle) + offset;
                    let end = rotate_point(end, cell.center(), angle) + offset;
                    painter.line_segment(
                        [start, end],
                        Stroke::new(
                            1.0_f32,
                            theme::with_alpha(tint, (f32::from(alpha) * 0.5) as u8),
                        ),
                    );
                }
            }
            if local < 0.3 && rng.next() > 0.82 {
                painter.line_segment(
                    [corners[0], corners[1]],
                    Stroke::new(
                        0.8_f32,
                        theme::with_alpha(Color32::WHITE, (f32::from(alpha) * 0.35) as u8),
                    ),
                );
            }
        }
    }

    let spark = (elapsed / 0.95).clamp(0.0, 1.0);
    if spark < 1.0 {
        let mut rng = Lcg::new(0x51A7_9E01);
        for _ in 0..44 {
            let angle = rng.next() * std::f32::consts::TAU;
            let speed = 30.0 + rng.next() * 280.0;
            let direction = egui::vec2(angle.cos(), angle.sin());
            let position =
                center + direction * (speed * spark) + egui::vec2(0.0, 120.0 * spark * spark);
            let alpha = ((1.0 - spark) * 200.0) as u8;
            painter.circle_filled(
                position,
                1.0 + rng.next() * 1.9,
                theme::with_alpha(theme::accent(), alpha),
            );
        }
    }
}

fn paint_meteor(painter: &egui::Painter, view: Rect, target: Rect, elapsed: f32) {
    let travel = ((elapsed - METEOR_START) / (METEOR_END - METEOR_START)).clamp(0.0, 1.0);
    if travel <= 0.0 {
        return;
    }
    let start = egui::pos2(view.right() + 140.0, view.bottom() + 190.0);
    let end = egui::pos2(target.right() - 26.0, target.bottom() - 24.0);
    let control = egui::pos2(view.right() - 20.0, view.bottom() - 150.0);
    let head = bezier(start, control, end, ease_in_out(travel));

    for step in (0..=28).rev() {
        let t = (travel - step as f32 * 0.011).clamp(0.0, 1.0);
        let position = bezier(start, control, end, ease_in_out(t));
        let fraction = step as f32 / 28.0;
        let alpha = ((1.0 - fraction) * (travel * 1.8).min(1.0) * 190.0) as u8;
        let radius = (5.6 - fraction * 4.3).max(0.5);
        painter.circle_filled(position, radius, theme::with_alpha(theme::accent(), alpha));
    }
    for (radius, alpha) in [(30.0_f32, 16_u8), (18.0, 36), (10.0, 90), (4.4, 255)] {
        let color = if alpha == 255 {
            Color32::WHITE
        } else {
            theme::with_alpha(theme::accent(), alpha)
        };
        painter.circle_filled(head, radius, color);
    }

    if travel >= 1.0 {
        let impact = ((elapsed - METEOR_END) / 0.55).clamp(0.0, 1.0);
        if impact < 1.0 {
            for ring in 0..3 {
                let radius = 8.0 + (impact + ring as f32 * 0.18) * 140.0;
                painter.circle_stroke(
                    end,
                    radius,
                    Stroke::new(
                        2.0_f32,
                        theme::with_alpha(theme::accent(), ((1.0 - impact) * 120.0) as u8),
                    ),
                );
            }
        }
    }
}

fn rotated_corners(rect: Rect, angle: f32) -> [egui::Pos2; 4] {
    let center = rect.center();
    [
        rotate_point(rect.left_top(), center, angle),
        rotate_point(rect.right_top(), center, angle),
        rotate_point(rect.right_bottom(), center, angle),
        rotate_point(rect.left_bottom(), center, angle),
    ]
}

fn rotate_point(point: egui::Pos2, center: egui::Pos2, angle: f32) -> egui::Pos2 {
    let (sin, cos) = angle.sin_cos();
    let offset = point - center;
    center
        + egui::vec2(
            offset.x * cos - offset.y * sin,
            offset.x * sin + offset.y * cos,
        )
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
        assert!(METEOR_END < end);
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
