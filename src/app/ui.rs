//! Workspace UI: explorer, tabs and the text editor surface.

use super::*;
use eframe::egui::{
    self, Align, Align2, Button, Color32, FontId, Frame, Id, Label, Layout, Margin, RichText,
    ScrollArea, Sense, Stroke, TextEdit,
};

impl App {
    pub(super) fn show_side_panel(&mut self, ui: &mut egui::Ui, actions: &mut Vec<Action>) {
        let Some(project) = self.project.as_ref() else {
            return;
        };
        let project_name = project.name.clone();
        let project_path = util::display_path(&project.root);
        let mode = self.build_config.mode_summary();
        let command = self.build_config.build_command.clone();
        let has_command = !command.trim().is_empty();
        let active_path = self.active_document().map(|doc| doc.path.clone());
        let show_hidden = self.settings.show_hidden;

        egui::Panel::left("mini_eide_project_panel")
            .resizable(true)
            .default_size(262.0)
            .size_range(210.0..=430.0)
            .frame(
                Frame::new()
                    .fill(theme::PANEL)
                    .inner_margin(Margin::same(0)),
            )
            .show_inside(ui, |ui| {
                ui.add_space(10.0);
                ui.horizontal(|ui| {
                    ui.add_space(11.0);
                    let (bar_rect, _) =
                        ui.allocate_exact_size(egui::vec2(3.0, 13.0), Sense::hover());
                    ui.painter()
                        .rect_filled(bar_rect, CornerRadius::same(1), theme::ACCENT);
                    ui.label(
                        RichText::new("资源管理器")
                            .size(11.0)
                            .strong()
                            .color(theme::TEXT_DIM),
                    );
                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        ui.add_space(8.0);
                        if ui
                            .add(
                                Button::new(
                                    RichText::new("刷新").size(10.0).color(theme::TEXT_FAINT),
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
                                    RichText::new("隐藏文件").size(10.0).color(hidden_color),
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
                });

                ui.add_space(8.0);
                Frame::new()
                    .inner_margin(Margin::symmetric(12, 0))
                    .show(ui, |ui| {
                        ui.label(
                            RichText::new(&project_name)
                                .size(13.5)
                                .strong()
                                .color(theme::TEXT),
                        );
                        ui.add(
                            Label::new(
                                RichText::new(&project_path)
                                    .size(9.5)
                                    .color(theme::TEXT_FAINT),
                            )
                            .truncate(),
                        )
                        .on_hover_text(&project_path);

                        ui.add_space(8.0);
                        Frame::new()
                            .fill(theme::PANEL_RAISED)
                            .stroke(Stroke::new(1.0_f32, theme::BORDER_SOFT))
                            .corner_radius(6)
                            .inner_margin(Margin::symmetric(9, 7))
                            .show(ui, |ui| {
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
                                        .size(10.5)
                                        .color(
                                            if has_command {
                                                theme::TEXT_DIM
                                            } else {
                                                theme::WARNING
                                            },
                                        ),
                                    );
                                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                                        if ui
                                            .add(small_flat_button("配置", theme::ACCENT))
                                            .on_hover_text("编辑构建命令")
                                            .clicked()
                                        {
                                            actions.push(Action::OpenSettings);
                                        }
                                    });
                                });
                                if has_command {
                                    ui.add(
                                        Label::new(
                                            RichText::new(format!("$ {command}"))
                                                .monospace()
                                                .size(9.5)
                                                .color(theme::TEXT_FAINT),
                                        )
                                        .truncate(),
                                    )
                                    .on_hover_text(&command);
                                }
                            });
                    });

                ui.add_space(8.0);
                ui.separator();

                ScrollArea::vertical()
                    .id_salt("mini_eide_file_tree")
                    .auto_shrink([false, false])
                    .show(ui, |ui| {
                        Frame::new()
                            .inner_margin(Margin::symmetric(5, 4))
                            .show(ui, |ui| {
                                if let Some(tree) = self.tree.as_mut() {
                                    let tree_actions = tree.show(ui, active_path.as_deref());
                                    for tree_action in tree_actions {
                                        actions.push(match tree_action {
                                            TreeAction::OpenFile(path) => Action::OpenFile(path),
                                            TreeAction::Reveal(path) => Action::Reveal(path),
                                            TreeAction::OpenInVscode(path) => {
                                                Action::OpenVscode(path)
                                            }
                                        });
                                    }
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
                if self.project.is_none() {
                    self.show_landing(ui, actions);
                    return;
                }
                if self.tabs.is_empty() {
                    self.show_project_home(ui, actions);
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
                    .id_salt("mini_eide_tabs")
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
                                    Id::new(("mini_eide_close_tab", index, &doc.path)),
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
                ui.spacing_mut().interact_size.y = 22.0;
                ui.horizontal(|ui| {
                    let response = ui.add_sized(
                        [ui.available_width().min(400.0), 20.0],
                        Label::new(RichText::new(&path).size(10.5).color(theme::TEXT_FAINT))
                            .truncate(),
                    );
                    response.on_hover_text(&full_path);
                    if dirty {
                        ui.label(RichText::new("● 未保存").size(9.5).color(theme::WARNING));
                    }
                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        let save_color = if dirty {
                            theme::ACCENT
                        } else {
                            theme::TEXT_FAINT
                        };
                        if ui
                            .add(
                                Button::new(RichText::new("保存").size(10.5).color(save_color))
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
                                Button::new(RichText::new("自动换行").size(10.0).color(wrap_color))
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
                        ui.label(RichText::new(&language).size(9.5).color(theme::INFO));
                        ui.label(RichText::new(newline).size(9.5).color(theme::TEXT_FAINT));
                        ui.label(RichText::new("UTF-8").size(9.5).color(theme::TEXT_FAINT));
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
        let editor_id = Id::new(("mini_eide_code_editor", &doc.path));
        let scroll_id = Id::new(("mini_eide_code_scroll", &doc.path));

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
    fn show_landing(&mut self, ui: &mut egui::Ui, actions: &mut Vec<Action>) {
        paint_dot_grid(ui);
        let recents = self.settings.recent_projects.clone();
        let config_path = settings_path();
        ScrollArea::vertical()
            .id_salt("mini_eide_landing_scroll")
            .auto_shrink([false, false])
            .show(ui, |ui| {
                ui.vertical_centered(|ui| {
                    ui.add_space((ui.available_height() * 0.07).clamp(20.0, 56.0));
                    let (logo_rect, _) =
                        ui.allocate_exact_size(egui::vec2(66.0, 66.0), Sense::hover());
                    paint_logo(ui.painter(), logo_rect);
                    ui.add_space(14.0);
                    ui.label(
                        RichText::new("把工程文件夹，直接变成 IDE")
                            .size(27.0)
                            .strong()
                            .color(theme::TEXT),
                    );
                    ui.add_space(6.0);
                    ui.label(
                        RichText::new("打开目录即工程 · 不生成项目文件 · 构建命令自动检测")
                            .size(12.5)
                            .color(theme::TEXT_DIM),
                    );
                    ui.add_space(20.0);
                    let open = Button::new(
                        RichText::new("打开工程文件夹")
                            .size(13.5)
                            .strong()
                            .color(theme::ON_ACCENT),
                    )
                    .fill(theme::ACCENT)
                    .stroke(Stroke::NONE)
                    .corner_radius(7)
                    .min_size(egui::vec2(220.0, 40.0));
                    if ui.add(open).clicked() {
                        actions.push(Action::OpenPicker);
                    }
                    ui.add_space(7.0);
                    ui.label(
                        RichText::new("Ctrl+O  ·  也可以把文件夹直接拖入窗口")
                            .size(10.0)
                            .color(theme::TEXT_FAINT),
                    );

                    ui.add_space(18.0);
                    ui.horizontal(|ui| {
                        let center = (ui.available_width() * 0.5 - 155.0).max(0.0);
                        ui.add_space(center);
                        feature_pill(ui, "Makefile", theme::DIR);
                        feature_pill(ui, "Cargo", theme::ACCENT);
                        feature_pill(ui, "CMake", theme::INFO);
                        feature_pill(ui, "SSH", theme::SUCCESS);
                    });

                    if !recents.is_empty() {
                        ui.add_space(24.0);
                        let width = (ui.available_width() * 0.72).clamp(360.0, 560.0);
                        ui.allocate_ui_with_layout(
                            egui::vec2(width, 0.0),
                            Layout::top_down(Align::Min),
                            |ui| {
                                Frame::new()
                                    .fill(theme::PANEL_RAISED.gamma_multiply(0.7))
                                    .stroke(Stroke::new(1.0_f32, theme::BORDER_SOFT))
                                    .corner_radius(9)
                                    .inner_margin(Margin::symmetric(12, 10))
                                    .show(ui, |ui| {
                                        ui.set_min_width(width - 24.0);
                                        ui.horizontal(|ui| {
                                            ui.label(
                                                RichText::new("最近打开")
                                                    .size(10.5)
                                                    .strong()
                                                    .color(theme::TEXT_DIM),
                                            );
                                            ui.with_layout(
                                                Layout::right_to_left(Align::Center),
                                                |ui| {
                                                    ui.label(
                                                        RichText::new("点击继续")
                                                            .size(9.5)
                                                            .color(theme::TEXT_FAINT),
                                                    );
                                                },
                                            );
                                        });
                                        ui.add_space(5.0);
                                        for path in recents.iter().take(4) {
                                            if recent_project_row(ui, path) {
                                                actions.push(Action::OpenProject(path.clone()));
                                            }
                                        }
                                    });
                            },
                        );
                    }

                    ui.add_space(22.0);
                    ui.label(
                        RichText::new(format!(
                            "设置只保存在 {}，工程目录保持干净。",
                            config_path.display()
                        ))
                        .size(9.5)
                        .color(theme::TEXT_FAINT),
                    );
                    ui.add_space(20.0);
                });
            });
    }

    fn show_project_home(&mut self, ui: &mut egui::Ui, actions: &mut Vec<Action>) {
        paint_dot_grid(ui);
        let Some(project) = self.project.as_ref() else {
            return;
        };
        let project_name = project.name.clone();
        let project_path = util::display_path(&project.root);
        let tool = self.build_config.tool_name();
        let mode = self.build_config.target.label();
        let command = self.build_config.build_command.clone();
        let has_command = !command.trim().is_empty();
        let dirty = self.dirty_count();

        ScrollArea::vertical()
            .id_salt("mini_eide_project_home_scroll")
            .auto_shrink([false, false])
            .show(ui, |ui| {
                ui.vertical_centered(|ui| {
                    ui.add_space((ui.available_height() * 0.10).clamp(26.0, 70.0));
                    let (logo_rect, _) =
                        ui.allocate_exact_size(egui::vec2(54.0, 54.0), Sense::hover());
                    paint_logo(ui.painter(), logo_rect);
                    ui.add_space(14.0);
                    ui.label(
                        RichText::new(format!("{project_name} 已就绪"))
                            .size(23.0)
                            .strong()
                            .color(theme::TEXT),
                    );
                    ui.add_space(3.0);
                    ui.label(
                        RichText::new(&project_path)
                            .size(10.5)
                            .color(theme::TEXT_FAINT),
                    );
                    ui.add_space(8.0);
                    ui.label(
                        RichText::new("从左侧资源管理器打开文件，修改后按 Ctrl+S 保存。")
                            .size(12.0)
                            .color(theme::TEXT_DIM),
                    );

                    ui.add_space(20.0);
                    let width = (ui.available_width() * 0.64).clamp(340.0, 520.0);
                    ui.allocate_ui_with_layout(
                        egui::vec2(width, 0.0),
                        Layout::top_down(Align::Min),
                        |ui| {
                            Frame::new()
                                .fill(theme::PANEL_RAISED)
                                .stroke(Stroke::new(1.0_f32, theme::BORDER_SOFT))
                                .corner_radius(8)
                                .inner_margin(Margin::symmetric(14, 11))
                                .show(ui, |ui| {
                                    ui.set_min_width(width - 28.0);
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
                                            RichText::new(format!("{tool} · {mode}"))
                                                .size(11.0)
                                                .strong()
                                                .color(theme::TEXT_DIM),
                                        );
                                        if dirty > 0 {
                                            ui.with_layout(
                                                Layout::right_to_left(Align::Center),
                                                |ui| {
                                                    ui.label(
                                                        RichText::new(format!(
                                                            "{dirty} 个未保存文件"
                                                        ))
                                                        .size(9.5)
                                                        .color(theme::WARNING),
                                                    );
                                                },
                                            );
                                        }
                                    });
                                    ui.add_space(4.0);
                                    if has_command {
                                        ui.label(
                                            RichText::new(format!("$ {command}"))
                                                .monospace()
                                                .size(10.5)
                                                .color(theme::TEXT_FAINT),
                                        );
                                    } else {
                                        ui.label(
                                            RichText::new(
                                                "未检测到 Makefile / Cargo.toml / CMakeLists.txt，请手动配置构建命令。",
                                            )
                                            .size(10.5)
                                            .color(theme::WARNING),
                                        );
                                    }
                                });
                        },
                    );

                    ui.add_space(16.0);
                    ui.horizontal(|ui| {
                        if has_command {
                            let build = Button::new(
                                RichText::new("构建工程")
                                    .size(12.5)
                                    .strong()
                                    .color(theme::ON_ACCENT),
                            )
                            .fill(theme::ACCENT)
                            .stroke(Stroke::NONE)
                            .corner_radius(6)
                            .min_size(egui::vec2(116.0, 34.0));
                            if ui.add(build).clicked() {
                                actions.push(Action::Run(TaskKind::Build));
                            }
                        }
                        if ui.add(theme::secondary_button("构建设置")).clicked() {
                            actions.push(Action::OpenSettings);
                        }
                    });

                    ui.add_space(14.0);
                    ui.label(
                        RichText::new("提示：右键文件可在 VS Code 或资源管理器中打开")
                            .size(9.5)
                            .color(theme::TEXT_FAINT),
                    );
                });
            });
    }
}

fn paint_dot_grid(ui: &egui::Ui) {
    let rect = ui.max_rect();
    let painter = ui.painter();
    let color = Color32::from_rgba_unmultiplied(96, 112, 132, 24);
    let step = 30.0;
    let mut y = rect.top() + 15.0;
    while y < rect.bottom() {
        let mut x = rect.left() + 15.0;
        while x < rect.right() {
            painter.circle_filled(egui::pos2(x, y), 0.9, color);
            x += step;
        }
        y += step;
    }
}

fn feature_pill(ui: &mut egui::Ui, text: &str, color: Color32) {
    Frame::new()
        .fill(theme::PANEL_RAISED)
        .stroke(Stroke::new(1.0_f32, theme::BORDER_SOFT))
        .corner_radius(20)
        .inner_margin(Margin::symmetric(9, 4))
        .show(ui, |ui| {
            ui.horizontal(|ui| {
                colored_dot(ui, color, 4.5);
                ui.label(RichText::new(text).size(9.5).color(theme::TEXT_DIM));
            });
        });
}

fn recent_project_row(ui: &mut egui::Ui, path: &Path) -> bool {
    let name = path
        .file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .unwrap_or_else(|| path.display().to_string());
    let parent = path.parent().map(util::display_path).unwrap_or_default();
    let (rect, response) =
        ui.allocate_exact_size(egui::vec2(ui.available_width(), 38.0), Sense::click());
    let painter = ui.painter_at(rect);
    if response.hovered() {
        painter.rect_filled(rect, CornerRadius::same(5), theme::HOVER);
    }
    paint_mini_folder(&painter, egui::pos2(rect.left() + 12.0, rect.center().y));
    let text_rect = egui::Rect::from_min_max(
        egui::pos2(rect.left() + 30.0, rect.top()),
        egui::pos2(rect.right() - 12.0, rect.bottom()),
    );
    let text_painter = ui.painter_at(text_rect);
    text_painter.text(
        egui::pos2(text_rect.left(), rect.center().y - 7.0),
        Align2::LEFT_CENTER,
        &name,
        FontId::proportional(11.5),
        theme::TEXT,
    );
    text_painter.text(
        egui::pos2(text_rect.left(), rect.center().y + 8.0),
        Align2::LEFT_CENTER,
        truncate_chars(&parent, 68),
        FontId::proportional(9.0),
        theme::TEXT_FAINT,
    );
    response.on_hover_text(util::display_path(path)).clicked()
}

fn paint_mini_folder(painter: &egui::Painter, pos: egui::Pos2) {
    let body =
        egui::Rect::from_min_size(egui::pos2(pos.x - 5.0, pos.y - 3.5), egui::vec2(11.0, 8.0));
    let tab = egui::Rect::from_min_size(egui::pos2(pos.x - 5.0, pos.y - 5.0), egui::vec2(5.0, 3.0));
    painter.rect_filled(tab, CornerRadius::same(1), theme::DIR.gamma_multiply(0.7));
    painter.rect_filled(body, CornerRadius::same(1), theme::DIR);
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

        let modal = egui::Modal::new(Id::new("mini_eide_settings_modal"))
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
                .id_salt("mini_eide_settings_scroll")
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
                "退出 MiniEIDE 前保存更改？",
                format!("有 {dirty} 个文件尚未保存。"),
                "保存并退出",
            ),
        };

        let mut save = false;
        let mut discard = false;
        let mut cancel = false;
        let modal = egui::Modal::new(Id::new("mini_eide_unsaved_modal"))
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
            ui.label(RichText::new(body).size(11.5).color(theme::TEXT_DIM));
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
        egui::Area::new(Id::new("mini_eide_toast"))
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
