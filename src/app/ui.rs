//! Workspace UI: explorer, tabs and the text editor surface.

use super::*;
use crate::config::autodetect;
use eframe::egui::{
    self, Align, Align2, Button, Color32, FontId, Frame, Id, Label, Layout, Margin, RichText,
    ScrollArea, Sense, Stroke, TextEdit,
};

impl App {
    pub(super) fn show_side_panel(&mut self, ui: &mut egui::Ui, actions: &mut Vec<Action>) {
        let project_info = self
            .project
            .as_ref()
            .map(|project| (project.name.clone(), util::display_path(&project.root)));
        let has_project = project_info.is_some();
        let mode = self.build_config.mode_summary();
        let command = self.build_config.build_command.clone();
        let has_command = !command.trim().is_empty();
        let active_path = self.active_document().map(|doc| doc.path.clone());
        let show_hidden = self.settings.show_hidden;

        egui::Panel::left("simple_embedded_project_panel")
            .resizable(true)
            .default_size(272.0)
            .size_range(230.0..=440.0)
            .frame(
                Frame::new()
                    .fill(theme::PANEL)
                    .inner_margin(Margin::same(0)),
            )
            .show_inside(ui, |ui| {
                ui.add_space(14.0);
                Frame::new()
                    .inner_margin(Margin::symmetric(12, 0))
                    .show(ui, |ui| {
                        if let Some((name, path)) = &project_info {
                            Frame::new()
                                .fill(theme::PANEL_RAISED)
                                .stroke(Stroke::new(1.0_f32, theme::BORDER_SOFT))
                                .corner_radius(6)
                                .inner_margin(Margin::symmetric(9, 7))
                                .show(ui, |ui| {
                                    ui.horizontal(|ui| {
                                        ui.label(
                                            RichText::new(name)
                                                .size(14.0)
                                                .strong()
                                                .color(theme::TEXT),
                                        );
                                        ui.add_space(8.0);
                                        let response = ui.add(
                                            Label::new(
                                                RichText::new(path)
                                                    .size(11.5)
                                                    .color(theme::TEXT_FAINT),
                                            )
                                            .truncate(),
                                        );
                                        response.on_hover_text(path);
                                    });
                                    ui.add_space(3.0);
                                    ui.horizontal(|ui| {
                                        colored_dot(
                                            ui,
                                            if has_command {
                                                theme::SUCCESS
                                            } else {
                                                theme::WARNING
                                            },
                                            5.0,
                                        );
                                        ui.label(
                                            RichText::new(if has_command {
                                                mode.clone()
                                            } else {
                                                "尚未配置构建".to_owned()
                                            })
                                            .size(12.0)
                                            .color(
                                                if has_command {
                                                    theme::TEXT_DIM
                                                } else {
                                                    theme::WARNING
                                                },
                                            ),
                                        );
                                        let links_width = 38.0;
                                        let remaining = ui.available_width();
                                        if remaining > links_width {
                                            ui.add_space(remaining - links_width);
                                        }
                                        ui.vertical(|ui| {
                                            if ui
                                                .add(small_flat_button("配置", theme::ACCENT))
                                                .on_hover_text("编辑构建环境")
                                                .clicked()
                                            {
                                                actions.push(Action::OpenSettings);
                                            }
                                            if ui
                                                .add(small_flat_button("更换", theme::TEXT_FAINT))
                                                .on_hover_text("打开另一个工程文件夹 (Ctrl+O)")
                                                .clicked()
                                            {
                                                actions.push(Action::OpenPicker);
                                            }
                                        });
                                    });
                                    if has_command {
                                        ui.add(
                                            Label::new(
                                                RichText::new(format!("$ {command}"))
                                                    .monospace()
                                                    .size(12.0)
                                                    .color(theme::TEXT_FAINT),
                                            )
                                            .truncate(),
                                        )
                                        .on_hover_text(&command);
                                    }
                                });
                        } else {
                            Frame::new()
                                .fill(theme::PANEL_RAISED)
                                .stroke(Stroke::new(1.0_f32, theme::BORDER_SOFT))
                                .corner_radius(7)
                                .inner_margin(Margin::symmetric(12, 12))
                                .show(ui, |ui| {
                                    ui.label(
                                        RichText::new("导入工程")
                                            .size(12.5)
                                            .strong()
                                            .color(theme::TEXT),
                                    );
                                    ui.add_space(3.0);
                                    ui.label(
                                        RichText::new("选择一个已有的工程文件夹")
                                            .size(11.0)
                                            .color(theme::TEXT_FAINT),
                                    );
                                    ui.add_space(8.0);
                                    if ui
                                        .add_sized(
                                            [ui.available_width(), 30.0],
                                            theme::primary_button("选择工程文件夹"),
                                        )
                                        .on_hover_text("打开文件夹选择器 (Ctrl+O)")
                                        .clicked()
                                    {
                                        actions.push(Action::OpenPicker);
                                    }
                                });
                        }
                    });

                ui.add_space(10.0);
                ui.separator();
                Frame::new()
                    .inner_margin(Margin::symmetric(11, 7))
                    .show(ui, |ui| {
                        ui.horizontal(|ui| {
                            let (bar_rect, _) =
                                ui.allocate_exact_size(egui::vec2(3.0, 13.0), Sense::hover());
                            ui.painter().rect_filled(
                                bar_rect,
                                CornerRadius::same(1),
                                theme::ACCENT,
                            );
                            ui.label(
                                RichText::new("资源管理器")
                                    .size(12.0)
                                    .strong()
                                    .color(theme::TEXT_DIM),
                            );
                            if has_project {
                                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                                    if ui
                                        .add(
                                            Button::new(
                                                RichText::new("刷新")
                                                    .size(12.0)
                                                    .color(theme::TEXT_FAINT),
                                            )
                                            .frame(false),
                                        )
                                        .on_hover_text("重新读取目录")
                                        .clicked()
                                    {
                                        actions.push(Action::RefreshTree);
                                    }
                                    let hidden_color = if show_hidden {
                                        theme::ACCENT
                                    } else {
                                        theme::TEXT_FAINT
                                    };
                                    if ui
                                        .add(
                                            Button::new(
                                                RichText::new("隐藏文件")
                                                    .size(12.0)
                                                    .color(hidden_color),
                                            )
                                            .frame(false),
                                        )
                                        .on_hover_text(if show_hidden {
                                            "当前显示以 . 开头的文件，点击隐藏"
                                        } else {
                                            "显示以 . 开头的文件"
                                        })
                                        .clicked()
                                    {
                                        actions.push(Action::ToggleHidden);
                                    }
                                });
                            }
                        });
                    });

                ui.separator();
                ScrollArea::vertical()
                    .id_salt("simple_embedded_file_tree")
                    .auto_shrink([false, false])
                    .show(ui, |ui| {
                        Frame::new()
                            .inner_margin(Margin::symmetric(5, 4))
                            .show(ui, |ui| {
                                if has_project {
                                    if let Some(tree) = self.tree.as_mut() {
                                        let tree_actions = tree.show(ui, active_path.as_deref());
                                        for tree_action in tree_actions {
                                            actions.push(match tree_action {
                                                TreeAction::OpenFile(path) => {
                                                    Action::OpenFile(path)
                                                }
                                                TreeAction::Reveal(path) => Action::Reveal(path),
                                                TreeAction::OpenInVscode(path) => {
                                                    Action::OpenVscode(path)
                                                }
                                            });
                                        }
                                    }
                                } else {
                                    ui.add_space(16.0);
                                    ui.vertical_centered(|ui| {
                                        ui.label(
                                            RichText::new("工程文件会显示在这里")
                                                .size(11.0)
                                                .color(theme::TEXT_FAINT),
                                        );
                                    });
                                }
                                ui.add_space(8.0);
                            });
                    });
            });
    }

    pub(super) fn show_center(&mut self, ui: &mut egui::Ui, actions: &mut Vec<Action>) {
        egui::CentralPanel::default()
            .frame(
                Frame::new()
                    .fill(theme::EDITOR_BG)
                    .inner_margin(Margin::same(0)),
            )
            .show_inside(ui, |ui| {
                if self.tabs.is_empty() {
                    let hint = if self.project.is_some() {
                        "从左侧资源管理器打开文件开始编辑"
                    } else {
                        "请在左上角选择工程并开始"
                    };
                    let rect = ui.max_rect();
                    ui.painter().text(
                        rect.center(),
                        Align2::CENTER_CENTER,
                        hint,
                        FontId::proportional(11.5),
                        theme::TEXT_FAINT,
                    );
                    return;
                }
                if self.active.is_none_or(|index| index >= self.tabs.len()) {
                    self.active = Some(0);
                }

                self.show_tab_strip(ui, actions);
                self.show_editor_toolbar(ui, actions);
                ui.separator();
                self.show_code_editor(ui);
            });
    }

    fn show_tab_strip(&mut self, ui: &mut egui::Ui, actions: &mut Vec<Action>) {
        Frame::new()
            .fill(theme::PANEL)
            .inner_margin(Margin::symmetric(8, 4))
            .show(ui, |ui| {
                ScrollArea::horizontal()
                    .id_salt("simple_embedded_tabs")
                    .auto_shrink([false, true])
                    .scroll_bar_visibility(egui::scroll_area::ScrollBarVisibility::AlwaysHidden)
                    .max_height(34.0)
                    .show(ui, |ui| {
                        ui.horizontal(|ui| {
                            ui.spacing_mut().item_spacing.x = 2.0;
                            for (index, doc) in self.tabs.iter().enumerate() {
                                let is_active = self.active == Some(index);
                                let dirty = doc.is_dirty();
                                let font = FontId::proportional(12.0);
                                let galley = ui.painter().layout_no_wrap(
                                    doc.name.clone(),
                                    font.clone(),
                                    theme::TEXT,
                                );
                                let width = (galley.size().x + 48.0).clamp(108.0, 224.0);
                                let (rect, response) =
                                    ui.allocate_exact_size(egui::vec2(width, 30.0), Sense::click());
                                let close_rect = egui::Rect::from_center_size(
                                    egui::pos2(rect.right() - 14.0, rect.center().y),
                                    egui::vec2(18.0, 18.0),
                                );
                                let close_response = ui.interact(
                                    close_rect,
                                    Id::new(("simple_embedded_close_tab", index, &doc.path)),
                                    Sense::click(),
                                );

                                let painter = ui.painter_at(rect);
                                if is_active {
                                    painter.rect_filled(
                                        rect,
                                        CornerRadius::same(5),
                                        theme::EDITOR_BG,
                                    );
                                    painter.rect_filled(
                                        egui::Rect::from_min_size(
                                            egui::pos2(rect.left() + 5.0, rect.bottom() - 2.0),
                                            egui::vec2(rect.width() - 10.0, 2.0),
                                        ),
                                        CornerRadius::same(1),
                                        theme::ACCENT,
                                    );
                                } else if response.hovered() {
                                    painter.rect_filled(
                                        rect,
                                        CornerRadius::same(5),
                                        theme::SURFACE,
                                    );
                                }

                                let text_rect = egui::Rect::from_min_max(
                                    egui::pos2(rect.left() + 10.0, rect.top()),
                                    egui::pos2(rect.right() - 34.0, rect.bottom()),
                                );
                                let text_color = if is_active || response.hovered() {
                                    theme::TEXT
                                } else {
                                    theme::TEXT_DIM
                                };
                                ui.painter_at(text_rect).text(
                                    egui::pos2(text_rect.left(), rect.center().y - 1.0),
                                    Align2::LEFT_CENTER,
                                    &doc.name,
                                    font,
                                    text_color,
                                );

                                if dirty {
                                    painter.circle_filled(
                                        egui::pos2(rect.right() - 27.0, rect.center().y),
                                        2.6,
                                        theme::WARNING,
                                    );
                                }

                                let close_color = if close_response.hovered() {
                                    theme::ERROR
                                } else if is_active || response.hovered() {
                                    theme::TEXT_DIM
                                } else {
                                    theme::TEXT_FAINT.gamma_multiply(0.55)
                                };
                                let center = close_rect.center();
                                painter.line_segment(
                                    [
                                        egui::pos2(center.x - 3.5, center.y - 3.5),
                                        egui::pos2(center.x + 3.5, center.y + 3.5),
                                    ],
                                    Stroke::new(1.2_f32, close_color),
                                );
                                painter.line_segment(
                                    [
                                        egui::pos2(center.x + 3.5, center.y - 3.5),
                                        egui::pos2(center.x - 3.5, center.y + 3.5),
                                    ],
                                    Stroke::new(1.2_f32, close_color),
                                );

                                if close_response.clicked() || response.middle_clicked() {
                                    actions.push(Action::CloseTab(index));
                                } else if response.clicked() {
                                    actions.push(Action::SelectTab(index));
                                }
                                response.on_hover_text(util::display_path(&doc.path));
                            }
                        });
                    });
            });
    }

    fn show_editor_toolbar(&mut self, ui: &mut egui::Ui, actions: &mut Vec<Action>) {
        let Some(active) = self.active else {
            return;
        };
        let Some(doc) = self.tabs.get(active) else {
            return;
        };
        let path = doc.rel_path.clone();
        let full_path = util::display_path(&doc.path);
        let language = doc.language.clone();
        let newline = doc.newline_label();
        let dirty = doc.is_dirty();
        let word_wrap = self.settings.word_wrap;

        Frame::new()
            .fill(theme::EDITOR_BG)
            .inner_margin(Margin::symmetric(12, 4))
            .show(ui, |ui| {
                ui.spacing_mut().interact_size.y = 24.0;
                ui.horizontal(|ui| {
                    let response = ui.add_sized(
                        [ui.available_width().min(400.0), 22.0],
                        Label::new(RichText::new(&path).size(12.0).color(theme::TEXT_FAINT))
                            .truncate(),
                    );
                    response.on_hover_text(&full_path);
                    if dirty {
                        ui.label(RichText::new("● 未保存").size(11.0).color(theme::WARNING));
                    }
                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        let save_color = if dirty {
                            theme::ACCENT
                        } else {
                            theme::TEXT_FAINT
                        };
                        if ui
                            .add(
                                Button::new(RichText::new("保存").size(12.0).color(save_color))
                                    .frame(false),
                            )
                            .on_hover_text("保存当前文件 (Ctrl+S)")
                            .clicked()
                        {
                            actions.push(Action::SaveActive);
                        }
                        let wrap_color = if word_wrap {
                            theme::ACCENT
                        } else {
                            theme::TEXT_FAINT
                        };
                        if ui
                            .add(
                                Button::new(RichText::new("自动换行").size(12.0).color(wrap_color))
                                    .fill(if word_wrap {
                                        theme::ACCENT_SOFT
                                    } else {
                                        Color32::TRANSPARENT
                                    })
                                    .frame(word_wrap)
                                    .corner_radius(4),
                            )
                            .on_hover_text("切换长行自动换行")
                            .clicked()
                        {
                            actions.push(Action::ToggleWrap);
                        }
                        ui.label(RichText::new(&language).size(12.0).color(theme::INFO));
                        ui.label(RichText::new(newline).size(12.0).color(theme::TEXT_FAINT));
                        ui.label(RichText::new("UTF-8").size(12.0).color(theme::TEXT_FAINT));
                    });
                });
            });
    }

    fn show_code_editor(&mut self, ui: &mut egui::Ui) {
        let Some(active) = self.active else {
            return;
        };
        let Some(doc) = self.tabs.get_mut(active) else {
            return;
        };
        let font_size = self.settings.font_size;
        let word_wrap = self.settings.word_wrap;
        let focus_editor = std::mem::take(&mut self.focus_editor);
        let font_id = FontId::monospace(font_size);
        let viewport_width = ui.available_width();
        let rows = ((ui.available_height() / (font_size * 1.35)).ceil() as usize).max(20);
        let cache = doc.layout_cache.clone();
        let revision = doc.buffer.revision_handle();
        let syntax_name = doc.syntax_name.clone();
        let highlighter = Arc::clone(&self.highlighter);
        let editor_id = Id::new(("simple_embedded_code_editor", &doc.path));
        let scroll_id = Id::new(("simple_embedded_code_scroll", &doc.path));

        let mut layouter = move |ui: &egui::Ui, buffer: &dyn egui::TextBuffer, wrap_width: f32| {
            let effective_wrap = if word_wrap { wrap_width } else { f32::INFINITY };
            highlight::layout_cached(
                &highlighter,
                ui,
                buffer.as_str(),
                &highlight::LayoutSpec {
                    syntax_name: syntax_name.as_deref(),
                    revision: revision.get(),
                    wrap_width: effective_wrap,
                    font_id: font_id.clone(),
                },
                &cache,
            )
        };

        let mut response: Option<egui::Response> = None;
        ScrollArea::both()
            .id_salt(scroll_id)
            .auto_shrink([false, false])
            .max_height(ui.available_height())
            .show(ui, |ui| {
                ui.set_min_width(viewport_width.max(80.0));
                let output = TextEdit::multiline(&mut doc.buffer)
                    .id(editor_id)
                    .code_editor()
                    .font(FontId::monospace(font_size))
                    .desired_width(if word_wrap {
                        viewport_width.max(80.0)
                    } else {
                        f32::INFINITY
                    })
                    .desired_rows(rows)
                    .margin(Margin::symmetric(14, 8))
                    .frame(Frame::NONE)
                    .layouter(&mut layouter)
                    .hint_text("开始输入代码…")
                    .show(ui);

                if let Some(cursor_range) = output.cursor_range {
                    doc.update_cursor(cursor_range.primary.index);
                    if focus_editor {
                        // Clicking a tab or file should place the caret, not
                        // preserve a stale cross-tab selection.
                        let mut state = output.state;
                        state
                            .cursor
                            .set_char_range(Some(egui::text::CCursorRange::one(
                                cursor_range.primary,
                            )));
                        state.store(ui.ctx(), output.response.response.id);
                    }
                }
                response = Some(output.response.response);
            });

        if focus_editor {
            if let Some(response) = response {
                response.request_focus();
            }
        }
    }
}

impl App {
    pub(super) fn show_settings_modal(&mut self, ctx: &egui::Context) {
        let Some(mut draft) = self.settings_draft.take() else {
            return;
        };
        let project_root = self.project.as_ref().map(|project| project.root.clone());
        let project_name = self
            .project
            .as_ref()
            .map(|project| project.name.clone())
            .unwrap_or_else(|| "未打开工程".to_owned());
        let mut close = false;
        let mut save = false;

        let modal = egui::Modal::new(Id::new("simple_embedded_settings_modal"))
            .frame(
                Frame::new()
                    .fill(theme::PANEL)
                    .stroke(Stroke::new(1.0_f32, theme::BORDER))
                    .corner_radius(10)
                    .inner_margin(Margin::symmetric(20, 16)),
            )
            .backdrop_color(Color32::from_black_alpha(170));
        modal.show(ctx, |ui| {
            ui.set_width(650.0);
            ui.horizontal(|ui| {
                ui.label(
                    RichText::new("设置")
                        .size(18.0)
                        .strong()
                        .color(theme::TEXT),
                );
                ui.label(
                    RichText::new("零项目文件 · 构建配置保存在本机")
                        .size(10.0)
                        .color(theme::TEXT_FAINT),
                );
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    if ui
                        .add(
                            Button::new(RichText::new("×").size(16.0).color(theme::TEXT_DIM))
                                .frame(false),
                        )
                        .on_hover_text("取消")
                        .clicked()
                    {
                        close = true;
                    }
                });
            });
            ui.add_space(2.0);
            ui.label(
                RichText::new(format!("当前工程：{project_name}"))
                    .size(10.0)
                    .color(theme::TEXT_FAINT),
            );
            ui.add_space(8.0);
            ui.separator();

            ScrollArea::vertical()
                .id_salt("simple_embedded_settings_scroll")
                .max_height(440.0)
                .auto_shrink([false, false])
                .show(ui, |ui| {
                    if let Some(root) = &project_root {
                        ui.add_space(8.0);
                        ui.horizontal(|ui| {
                            settings_section(ui, "构建");
                            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                                if ui
                                    .add(theme::secondary_button("自动检测"))
                                    .on_hover_text("重新识别 Makefile / Cargo.toml / CMakeLists.txt")
                                    .clicked()
                                {
                                    let detected = autodetect(root);
                                    let tool = detected.tool_name();
                                    draft.extra_paths_text =
                                        detected.extra_paths.join("\n");
                                    draft.config.build_command = detected.build_command;
                                    draft.config.clean_command = detected.clean_command;
                                    draft.config.rebuild_command = detected.rebuild_command;
                                    draft.note = if tool == "未配置" {
                                        Some("未识别到构建系统，请手动填写命令。".to_owned())
                                    } else {
                                        Some(format!("已识别为 {tool}。"))
                                    };
                                }
                            });
                        });
                        ui.add(
                            Label::new(
                                RichText::new(util::display_path(root))
                                    .size(9.5)
                                    .color(theme::TEXT_FAINT),
                            )
                            .truncate(),
                        );
                        ui.add_space(8.0);

                        ui.horizontal(|ui| {
                            ui.add_sized(
                                [86.0, 22.0],
                                Label::new(
                                    RichText::new("构建位置")
                                        .size(10.5)
                                        .color(theme::TEXT_DIM),
                                ),
                            );
                            if segment_button(
                                ui,
                                draft.config.target == BuildTarget::Local,
                                "本地",
                            ) {
                                draft.config.target = BuildTarget::Local;
                            }
                            if segment_button(
                                ui,
                                draft.config.target == BuildTarget::Ssh,
                                "远程 SSH",
                            ) {
                                draft.config.target = BuildTarget::Ssh;
                            }
                            ui.label(
                                RichText::new(match draft.config.target {
                                    BuildTarget::Local => "在工程目录直接执行命令",
                                    BuildTarget::Ssh => "通过系统 OpenSSH 在虚拟机中执行",
                                })
                                .size(9.5)
                                .color(theme::TEXT_FAINT),
                            );
                        });

                        if draft.config.target == BuildTarget::Ssh {
                            ui.add_space(8.0);
                            Frame::new()
                                .fill(theme::BG)
                                .stroke(Stroke::new(1.0_f32, theme::BORDER_SOFT))
                                .corner_radius(7)
                                .inner_margin(Margin::symmetric(12, 9))
                                .show(ui, |ui| {
                                    form_line(
                                        ui,
                                        "SSH 主机",
                                        &mut draft.config.ssh_host,
                                        "user@192.168.1.10 或 ~/.ssh/config 中的别名",
                                    );
                                    ui.add_space(3.0);
                                    ui.horizontal(|ui| {
                                        ui.add_sized(
                                            [86.0, 22.0],
                                            Label::new(
                                                RichText::new("SSH 端口")
                                                    .size(10.5)
                                                    .color(theme::TEXT_DIM),
                                            ),
                                        );
                                        ui.add(
                                            egui::DragValue::new(&mut draft.config.ssh_port)
                                                .range(1..=65535)
                                                .speed(1),
                                        );
                                        ui.label(
                                            RichText::new("默认 22")
                                                .size(9.0)
                                                .color(theme::TEXT_FAINT),
                                        );
                                    });
                                    ui.add_space(3.0);
                                    form_line(
                                        ui,
                                        "远程目录",
                                        &mut draft.config.remote_dir,
                                        "/home/user/firmware",
                                    );
                                    ui.add_space(4.0);
                                    ui.label(
                                        RichText::new(
                                            "使用 OpenSSH BatchMode，请提前配置好 SSH key 免密登录。命令会先 cd 到远程目录。",
                                        )
                                        .size(9.0)
                                        .color(theme::WARNING),
                                    );
                                });
                        }

                        if let Some(note) = &draft.note {
                            ui.add_space(5.0);
                            ui.label(
                                RichText::new(note)
                                    .size(9.5)
                                    .color(theme::SUCCESS),
                            );
                        }

                        ui.add_space(10.0);
                        form_line(ui, "Build", &mut draft.config.build_command, "例如 make -j8");
                        ui.add_space(3.0);
                        form_line(ui, "Clean", &mut draft.config.clean_command, "例如 make clean");
                        ui.add_space(3.0);
                        form_line(
                            ui,
                            "Rebuild",
                            &mut draft.config.rebuild_command,
                            "例如 make clean && make",
                        );
                        ui.add_space(3.0);
                        form_line(
                            ui,
                            "Flash",
                            &mut draft.config.flash_command,
                            "可选，例如 JLink.exe -CommanderScript flash.jlink",
                        );
                        ui.add_space(4.0);
                        ui.label(
                            RichText::new(
                                "Flash 始终在本地工程目录执行，方便直接访问 J-Link / OpenOCD。留空则烧录按钮禁用。",
                            )
                            .size(9.0)
                            .color(theme::TEXT_FAINT),
                        );

                        ui.add_space(12.0);
                        settings_section(ui, "工具链路径 / 额外 PATH");
                        ui.add_space(3.0);
                        ui.label(
                            RichText::new(
                                "每行一个目录，执行构建时会加到 PATH 前面。适合编译器、make、Keil UV4 等不在系统 PATH 的情况；仅本地构建生效。",
                            )
                            .size(9.0)
                            .color(theme::TEXT_FAINT),
                        );
                        ui.add_space(4.0);
                        Frame::new()
                            .fill(theme::BG)
                            .stroke(Stroke::new(1.0_f32, theme::BORDER_SOFT))
                            .corner_radius(5)
                            .inner_margin(Margin::symmetric(4, 3))
                            .show(ui, |ui| {
                                ui.add(
                                    TextEdit::multiline(&mut draft.extra_paths_text)
                                        .font(FontId::monospace(10.5))
                                        .text_color(theme::TEXT)
                                        .background_color(theme::BG)
                                        .hint_text("例如 C:\\toolchain\\bin")
                                        .desired_rows(3)
                                        .desired_width(f32::INFINITY),
                                );
                            });
                    } else {
                        ui.add_space(10.0);
                        ui.label(
                            RichText::new("打开一个工程文件夹后，即可配置构建与烧录命令。")
                                .size(11.0)
                                .color(theme::WARNING),
                        );
                    }

                    ui.add_space(14.0);
                    ui.separator();
                    ui.add_space(8.0);
                    settings_section(ui, "编辑器");
                    ui.add_space(4.0);
                    ui.add(
                        egui::Slider::new(&mut draft.font_size, 10.0..=28.0)
                            .text("代码字号")
                            .fixed_decimals(0),
                    );
                    ui.horizontal(|ui| {
                        ui.checkbox(&mut draft.word_wrap, "长行自动换行");
                        ui.checkbox(&mut draft.auto_scroll_output, "构建输出自动滚动");
                        ui.checkbox(&mut draft.show_hidden, "显示隐藏文件");
                    });
                    ui.add_space(6.0);
                    ui.label(
                        RichText::new(format!(
                            "构建命令保存在 {}，不会向工程目录写入任何文件。",
                            settings_path().display()
                        ))
                        .size(9.0)
                        .color(theme::TEXT_FAINT),
                    );
                });

            if let Some(error) = &draft.error {
                ui.add_space(6.0);
                ui.label(RichText::new(error).size(10.5).color(theme::ERROR));
            }

            ui.add_space(10.0);
            ui.separator();
            ui.add_space(5.0);
            ui.horizontal(|ui| {
                ui.label(
                    RichText::new("本地或 SSH 构建共用同一组命令")
                        .size(9.0)
                        .color(theme::TEXT_FAINT),
                );
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    if ui.add(theme::primary_button("保存")).clicked() {
                        save = true;
                    }
                    if ui.add(theme::secondary_button("取消")).clicked() {
                        close = true;
                    }
                });
            });
        });

        if close {
            self.settings_draft = None;
            return;
        }

        if save {
            draft.config.build_command = draft.config.build_command.trim().to_owned();
            draft.config.clean_command = draft.config.clean_command.trim().to_owned();
            draft.config.rebuild_command = draft.config.rebuild_command.trim().to_owned();
            draft.config.flash_command = draft.config.flash_command.trim().to_owned();
            draft.config.ssh_host = draft.config.ssh_host.trim().to_owned();
            draft.config.remote_dir = draft.config.remote_dir.trim().to_owned();
            let mut extra_paths = Vec::new();
            for path in draft.extra_paths_text.lines() {
                let path = path.trim();
                if !path.is_empty() && !extra_paths.iter().any(|existing| existing == path) {
                    extra_paths.push(path.to_owned());
                }
            }
            draft.config.extra_paths = extra_paths;

            if project_root.is_some() {
                if let Err(err) = draft.config.validate() {
                    draft.error = Some(err);
                    self.settings_draft = Some(draft);
                    return;
                }
            }

            self.settings.font_size = draft.font_size.clamp(10.0, 28.0);
            self.settings.word_wrap = draft.word_wrap;
            self.settings.auto_scroll_output = draft.auto_scroll_output;
            self.settings.show_hidden = draft.show_hidden;
            self.settings.save();

            if let Some(root) = &project_root {
                self.build_config = draft.config.clone();
                self.settings.set_build_config(root, &draft.config);
                self.settings.save();
            }
            if let Some(tree) = &mut self.tree {
                tree.set_show_hidden(draft.show_hidden);
            }
            self.build.auto_scroll = draft.auto_scroll_output;
            self.settings_draft = None;
            self.set_toast("设置已保存", Severity::Success);
        } else {
            self.settings_draft = Some(draft);
        }
    }
}

fn settings_section(ui: &mut egui::Ui, title: &str) {
    ui.label(
        RichText::new(title)
            .size(11.0)
            .strong()
            .color(theme::TEXT_DIM),
    );
}

fn segment_button(ui: &mut egui::Ui, selected: bool, text: &str) -> bool {
    ui.add(
        Button::new(RichText::new(text).size(10.5).color(if selected {
            theme::ON_ACCENT
        } else {
            theme::TEXT_DIM
        }))
        .fill(if selected {
            theme::ACCENT
        } else {
            theme::SURFACE
        })
        .stroke(if selected {
            Stroke::NONE
        } else {
            Stroke::new(1.0_f32, theme::BORDER)
        })
        .corner_radius(5)
        .min_size(egui::vec2(74.0, 25.0)),
    )
    .clicked()
}

fn form_line(ui: &mut egui::Ui, label: &str, value: &mut String, hint: &str) {
    ui.horizontal(|ui| {
        ui.add_sized(
            [86.0, 22.0],
            Label::new(RichText::new(label).size(10.5).color(theme::TEXT_DIM)),
        );
        ui.add(
            TextEdit::singleline(value)
                .font(FontId::monospace(11.0))
                .text_color(theme::TEXT)
                .background_color(theme::BG)
                .hint_text(hint)
                .desired_width(ui.available_width()),
        );
    });
}

impl App {
    pub(super) fn show_pending_modal(&mut self, ctx: &egui::Context) {
        let Some(mut dialog) = self.pending.take() else {
            return;
        };
        let dirty = self.dirty_count();
        let (title, body, save_label) = match dialog.action {
            PendingAction::OpenProject(_) => (
                "切换工程前保存更改？",
                format!("当前工程有 {dirty} 个文件尚未保存，切换后将关闭这些标签页。"),
                "保存并切换",
            ),
            PendingAction::CloseTab(_) => (
                "关闭文件前保存更改？",
                format!("当前文件有未保存的修改（共 {dirty} 个文件未保存）。"),
                "保存并关闭",
            ),
            PendingAction::CloseWindow => (
                "退出 Simple embedded 前保存更改？",
                format!("有 {dirty} 个文件尚未保存。"),
                "保存并退出",
            ),
        };

        let mut save = false;
        let mut discard = false;
        let mut cancel = false;
        let modal = egui::Modal::new(Id::new("simple_embedded_unsaved_modal"))
            .frame(
                Frame::new()
                    .fill(theme::PANEL)
                    .stroke(Stroke::new(1.0_f32, theme::BORDER))
                    .corner_radius(10)
                    .inner_margin(Margin::symmetric(20, 16)),
            )
            .backdrop_color(Color32::from_black_alpha(170));
        let response = modal.show(ctx, |ui| {
            ui.set_width(440.0);
            ui.horizontal(|ui| {
                colored_dot(ui, theme::WARNING, 8.0);
                ui.label(RichText::new(title).size(16.0).strong().color(theme::TEXT));
            });
            ui.add_space(10.0);
            ui.label(RichText::new(body).size(12.0).color(theme::TEXT_DIM));
            ui.add_space(4.0);
            ui.label(
                RichText::new("未命名的外部修改不会被覆盖，但未保存内容会丢失。")
                    .size(9.5)
                    .color(theme::TEXT_FAINT),
            );

            if let Some(error) = &dialog.error {
                ui.add_space(8.0);
                Frame::new()
                    .fill(theme::ERROR.gamma_multiply(0.08))
                    .stroke(Stroke::new(1.0_f32, theme::ERROR.gamma_multiply(0.4)))
                    .corner_radius(5)
                    .inner_margin(Margin::symmetric(9, 6))
                    .show(ui, |ui| {
                        ui.label(RichText::new(error).size(10.0).color(theme::ERROR));
                    });
            }

            ui.add_space(14.0);
            ui.separator();
            ui.add_space(6.0);
            ui.horizontal(|ui| {
                if ui.add(theme::secondary_button("取消")).clicked() {
                    cancel = true;
                }
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    if ui.add(theme::primary_button(save_label)).clicked() {
                        save = true;
                    }
                    let discard_button =
                        Button::new(RichText::new("放弃更改").size(12.0).color(theme::ERROR))
                            .fill(theme::SURFACE)
                            .stroke(Stroke::new(1.0_f32, theme::ERROR.gamma_multiply(0.35)))
                            .corner_radius(6)
                            .min_size(egui::vec2(0.0, 28.0));
                    if ui
                        .add(discard_button)
                        .on_hover_text("不保存并继续")
                        .clicked()
                    {
                        discard = true;
                    }
                });
            });
        });

        if response.should_close() {
            cancel = true;
        }

        if save {
            match self.save_all_dirty() {
                Ok(_) => self.finish_pending(dialog.action, ctx),
                Err(err) => {
                    dialog.error = Some(err);
                    self.pending = Some(dialog);
                }
            }
        } else if discard {
            self.finish_pending(dialog.action, ctx);
        } else if !cancel {
            self.pending = Some(dialog);
        }
    }

    fn finish_pending(&mut self, action: PendingAction, ctx: &egui::Context) {
        match action {
            PendingAction::OpenProject(path) => {
                self.load_project(&path, true);
            }
            PendingAction::CloseTab(index) => {
                self.close_tab_now(index);
            }
            PendingAction::CloseWindow => {
                self.allow_close = true;
                self.settings.save();
                ctx.send_viewport_cmd(ViewportCommand::Close);
            }
        }
    }

    pub(super) fn show_toast(&mut self, ctx: &egui::Context) {
        let Some(toast) = &self.toast else {
            return;
        };
        let color = toast.severity.color();
        let message = toast.message.clone();
        let severity = toast.severity;
        egui::Area::new(Id::new("simple_embedded_toast"))
            .anchor(Align2::RIGHT_BOTTOM, egui::vec2(-18.0, -38.0))
            .order(egui::Order::Foreground)
            .show(ctx, |ui| {
                Frame::new()
                    .fill(theme::PANEL_RAISED)
                    .stroke(Stroke::new(1.0_f32, color.gamma_multiply(0.65)))
                    .corner_radius(7)
                    .inner_margin(Margin::symmetric(13, 9))
                    .show(ui, |ui| {
                        ui.set_max_width(400.0);
                        ui.horizontal(|ui| {
                            colored_dot(ui, color, 6.0);
                            let icon = match severity {
                                Severity::Error => "错误",
                                Severity::Warning => "提示",
                                Severity::Success => "完成",
                                _ => "信息",
                            };
                            ui.label(RichText::new(icon).size(9.5).strong().color(color));
                            ui.add(
                                Label::new(RichText::new(&message).size(11.0).color(theme::TEXT))
                                    .wrap(),
                            );
                        });
                    });
            });
    }
}
