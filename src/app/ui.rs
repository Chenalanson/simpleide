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
                        let card_name = project_info
                            .as_ref()
                            .map(|(name, _)| name.as_str())
                            .unwrap_or("未导入工程");
                        let card_path = project_info
                            .as_ref()
                            .map(|(_, path)| path.as_str())
                            .unwrap_or("选择工程文件夹");
                        Frame::new()
                            .fill(theme::PANEL_RAISED)
                            .stroke(Stroke::new(1.0_f32, theme::BORDER_SOFT))
                            .corner_radius(6)
                            .inner_margin(Margin::symmetric(9, 7))
                            .show(ui, |ui| {
                                ui.horizontal(|ui| {
                                    ui.label(RichText::new(card_name).size(14.0).strong().color(
                                        if has_project {
                                            theme::TEXT
                                        } else {
                                            theme::TEXT_DIM
                                        },
                                    ));
                                    ui.add_space(8.0);
                                    let response = ui.add(
                                        Label::new(
                                            RichText::new(card_path)
                                                .size(11.5)
                                                .color(theme::TEXT_FAINT),
                                        )
                                        .truncate(),
                                    );
                                    response.on_hover_text(card_path);
                                });
                                ui.add_space(3.0);
                                let (dot_color, builder_text, builder_color) = if has_project {
                                    if has_command {
                                        (theme::SUCCESS, mode.clone(), theme::TEXT_DIM)
                                    } else {
                                        (theme::WARNING, "尚未配置构建".to_owned(), theme::WARNING)
                                    }
                                } else {
                                    (theme::WARNING, "构建器 · 未配置".to_owned(), theme::WARNING)
                                };
                                ui.horizontal(|ui| {
                                    colored_dot(ui, dot_color, 5.0);
                                    ui.label(
                                        RichText::new(builder_text).size(12.0).color(builder_color),
                                    );
                                });
                                if has_project && has_command {
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
                                } else if !has_project {
                                    ui.label(
                                        RichText::new("$ 未配置")
                                            .monospace()
                                            .size(12.0)
                                            .color(theme::TEXT_FAINT),
                                    );
                                }
                                ui.add_space(6.0);
                                let spacing = ui.spacing().item_spacing.x;
                                let button_width =
                                    ((ui.available_width() - spacing) / 2.0).max(0.0);
                                ui.horizontal(|ui| {
                                    let left_text = if has_project { "更换" } else { "导入" };
                                    let left = if has_project {
                                        theme::secondary_button(left_text)
                                    } else {
                                        egui::Button::new(
                                            RichText::new(left_text)
                                                .size(12.5)
                                                .color(theme::ACCENT),
                                        )
                                        .fill(theme::SURFACE)
                                        .stroke(Stroke::new(1.0_f32, theme::BORDER))
                                        .corner_radius(6)
                                    };
                                    if ui
                                        .add_sized([button_width, 28.0], left)
                                        .on_hover_text(if has_project {
                                            "打开另一个工程文件夹 (Ctrl+O)"
                                        } else {
                                            "导入工程文件夹 (Ctrl+O)"
                                        })
                                        .clicked()
                                    {
                                        actions.push(Action::OpenPicker);
                                    }
                                    let config = if has_project {
                                        egui::Button::new(
                                            RichText::new("配置").size(12.5).color(theme::ACCENT),
                                        )
                                        .fill(theme::SURFACE)
                                        .stroke(Stroke::new(1.0_f32, theme::BORDER))
                                        .corner_radius(6)
                                    } else {
                                        theme::secondary_button("配置")
                                    };
                                    if ui
                                        .add_sized([button_width, 28.0], config)
                                        .on_hover_text("编辑构建环境")
                                        .clicked()
                                    {
                                        actions.push(Action::OpenSettings);
                                    }
                                });
                            });
                    });

                let available_height = ui.available_height();
                let splitter_height = if has_project { 8.0 } else { 0.0 };
                let content_height = (available_height - splitter_height).max(120.0);
                let resource_height = if has_project {
                    (content_height * self.left_split)
                        .clamp(100.0, (content_height - 100.0).max(100.0))
                } else {
                    available_height
                };
                let info_height = (content_height - resource_height).max(100.0);

                ui.allocate_ui_with_layout(
                    egui::vec2(ui.available_width(), resource_height),
                    Layout::top_down(Align::Min),
                    |ui| {
                        ui.add_space(8.0);
                        light_separator(ui);
                        Frame::new()
                            .inner_margin(Margin::symmetric(11, 7))
                            .show(ui, |ui| {
                                ui.horizontal(|ui| {
                                    let (bar_rect, _) = ui
                                        .allocate_exact_size(egui::vec2(3.0, 13.0), Sense::hover());
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
                                        ui.with_layout(
                                            Layout::right_to_left(Align::Center),
                                            |ui| {
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
                                            },
                                        );
                                    }
                                });
                            });

                        light_separator(ui);
                        let tree_height = (ui.available_height() - 2.0).max(80.0);
                        ScrollArea::vertical()
                            .id_salt("simple_embedded_file_tree")
                            .max_height(tree_height)
                            .auto_shrink([false, false])
                            .show(ui, |ui| {
                                Frame::new()
                                    .inner_margin(Margin::symmetric(5, 4))
                                    .show(ui, |ui| {
                                        if has_project {
                                            if let Some(tree) = self.tree.as_mut() {
                                                let tree_actions =
                                                    tree.show(ui, active_path.as_deref());
                                                for tree_action in tree_actions {
                                                    actions.push(match tree_action {
                                                        TreeAction::OpenFile(path) => {
                                                            Action::OpenFile(path)
                                                        }
                                                        TreeAction::Reveal(path) => {
                                                            Action::Reveal(path)
                                                        }
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
                    },
                );

                if has_project {
                    let (split_rect, split_response) = ui.allocate_exact_size(
                        egui::vec2(ui.available_width(), splitter_height),
                        Sense::drag(),
                    );
                    let split_response =
                        split_response.on_hover_cursor(egui::CursorIcon::ResizeVertical);
                    if split_response.dragged() {
                        self.left_split = (self.left_split
                            + split_response.drag_delta().y / content_height)
                            .clamp(0.18, 0.82);
                    }
                    let line_color = if split_response.hovered() || split_response.dragged() {
                        theme::ACCENT
                    } else {
                        Color32::from_rgb(150, 160, 175)
                    };
                    let line_y = split_rect.center().y;
                    ui.painter().line_segment(
                        [
                            egui::pos2(split_rect.left(), line_y),
                            egui::pos2(split_rect.right(), line_y),
                        ],
                        Stroke::new(1.0_f32, line_color),
                    );
                    if split_response.hovered() || split_response.dragged() {
                        for offset in [-6.0_f32, 0.0, 6.0] {
                            ui.painter().circle_filled(
                                egui::pos2(split_rect.center().x + offset, line_y),
                                1.2,
                                line_color,
                            );
                        }
                    }

                    ui.allocate_ui_with_layout(
                        egui::vec2(ui.available_width(), info_height),
                        Layout::top_down(Align::Min),
                        |ui| {
                            Frame::new()
                                .inner_margin(Margin::symmetric(11, 5))
                                .show(ui, |ui| {
                                    ui.horizontal(|ui| {
                                        let (bar_rect, _) = ui.allocate_exact_size(
                                            egui::vec2(3.0, 13.0),
                                            Sense::hover(),
                                        );
                                        ui.painter().rect_filled(
                                            bar_rect,
                                            CornerRadius::same(1),
                                            theme::ACCENT,
                                        );
                                        ui.label(
                                            RichText::new("工程信息")
                                                .size(12.5)
                                                .strong()
                                                .color(theme::TEXT_DIM),
                                        );
                                    });
                                });
                            light_separator(ui);
                            Frame::new()
                                .inner_margin(Margin::symmetric(11, 0))
                                .show(ui, |ui| {
                                    let height = (ui.available_height() - 2.0).max(80.0);
                                    ScrollArea::vertical()
                                        .id_salt("simple_embedded_project_meta")
                                        .max_height(height)
                                        .auto_shrink([false, false])
                                        .show(ui, |ui| self.show_project_meta(ui, actions));
                                });
                        },
                    );
                }
            });
    }

    fn show_project_meta(&self, ui: &mut egui::Ui, actions: &mut Vec<Action>) {
        let meta = &self.project_meta;
        let toolchain = meta
            .toolchain
            .clone()
            .unwrap_or_else(|| self.build_config.tool_name().to_owned());

        ui.spacing_mut().item_spacing.y = 2.0;

        meta_group(ui, "构建配置", true, |ui| {
            meta_row(ui, "工具链", &toolchain);
            if let Some(target) = &meta.target {
                meta_row(ui, "工程目标", target);
            }
            meta_row(
                ui,
                "输出目录",
                meta.output_dir.as_deref().unwrap_or("未检测"),
            );
            if let Some(device) = &meta.device {
                meta_row(ui, "芯片型号", device);
            }
            if let Some(cpu) = &meta.cpu {
                meta_row(ui, "CPU 类型", cpu);
            }
            if let Some(fpu) = &meta.fpu {
                meta_row(ui, "硬件浮点", fpu);
            }
            if let Some(script) = &meta.linker_script {
                meta_row(ui, "链接脚本", script);
            }
            if let Some(layout) = &meta.memory_layout {
                meta_row(ui, "内存布局", layout);
            }
            if !self.build_config.build_command.trim().is_empty() {
                meta_row(ui, "构建命令", &self.build_config.build_command);
            }
            let option_lines = self.build_config.build_options.summary_lines();
            let mut edit_clicked = false;
            let row_height = 22.0 + option_lines.len().saturating_sub(1) as f32 * 17.0;
            let row = ui.allocate_ui_with_layout(
                egui::vec2(ui.available_width(), row_height),
                Layout::top_down(Align::Min),
                |ui| {
                    ui.horizontal(|ui| {
                        ui.add_sized(
                            [70.0, 18.0],
                            Label::new(
                                RichText::new("构建选项")
                                    .size(11.0)
                                    .color(theme::TEXT_FAINT),
                            ),
                        );
                        if let Some(first) = option_lines.first() {
                            let response = ui.add(
                                Label::new(RichText::new(first).size(11.5).color(theme::TEXT_DIM))
                                    .truncate(),
                            );
                            response.on_hover_text(first);
                        }
                        let remaining = ui.available_width();
                        if remaining > 60.0 {
                            ui.add_space(remaining - 60.0);
                        }
                        if ui
                            .add(small_flat_button("编辑", theme::ACCENT))
                            .on_hover_text("编辑构建选项")
                            .clicked()
                        {
                            edit_clicked = true;
                        }
                    });
                    for line in option_lines.iter().skip(1) {
                        ui.horizontal(|ui| {
                            ui.add_space(78.0);
                            let response = ui.add(
                                Label::new(RichText::new(line).size(11.5).color(theme::TEXT_DIM))
                                    .truncate(),
                            );
                            response.on_hover_text(line);
                        });
                    }
                },
            );
            let response = row
                .response
                .interact(Sense::click())
                .on_hover_cursor(egui::CursorIcon::PointingHand);
            if edit_clicked || response.clicked() {
                actions.push(Action::EditBuildOptions);
            }
        });

        meta_group(ui, "烧录配置", true, |ui| {
            let flash = self.build_config.flash_command.trim();
            meta_row(
                ui,
                "烧录命令",
                if flash.is_empty() { "未配置" } else { flash },
            );
            if let Some(device) = &meta.device {
                meta_row(ui, "芯片名称", device);
            }
            if let Some(driver) = &meta.flash_driver {
                meta_row(ui, "烧录驱动", driver);
            }
            if let Some(artifact) = &meta.artifact {
                meta_row(ui, "程序文件", artifact);
            }
        });

        meta_group(ui, "C/C++ 属性", true, |ui| {
            meta_editable_list(
                ui,
                "包含目录",
                self.effective_includes(),
                MetaListField::Includes,
                actions,
            );
            meta_editable_list(
                ui,
                "库搜索目录",
                self.effective_library_dirs(),
                MetaListField::LibraryDirs,
                actions,
            );
            meta_editable_list(
                ui,
                "预处理器定义",
                self.effective_defines(),
                MetaListField::Defines,
                actions,
            );
            if let Some(compiler) = &meta.compiler {
                meta_row(ui, "编译器", compiler);
            }
            if let Some(optimization) = &meta.optimization {
                meta_row(ui, "优化选项", optimization);
            }
            ui.add_space(2.0);
            if ui
                .add(small_flat_button("批量编辑", theme::TEXT_FAINT))
                .on_hover_text("编辑包含目录、库搜索目录和预处理定义")
                .clicked()
            {
                actions.push(Action::EditMeta);
            }
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
    pub(super) fn show_build_options_modal(&mut self, ctx: &egui::Context) {
        let Some(mut draft) = self.build_options_draft.take() else {
            return;
        };
        let mut close = false;
        let mut save = false;
        let mut tab = self.build_options_tab;

        let modal = egui::Modal::new(Id::new("simple_embedded_build_options_modal"))
            .frame(
                Frame::new()
                    .fill(theme::PANEL)
                    .stroke(Stroke::new(1.0_f32, theme::BORDER))
                    .corner_radius(10)
                    .inner_margin(Margin::symmetric(20, 16)),
            )
            .backdrop_color(Color32::from_black_alpha(170));
        modal.show(ctx, |ui| {
            ui.set_width(640.0);
            ui.horizontal(|ui| {
                ui.label(
                    RichText::new("构建选项")
                        .size(18.0)
                        .strong()
                        .color(theme::TEXT),
                );
                ui.label(
                    RichText::new("保存在本机工程设置")
                        .size(10.5)
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
                RichText::new(
                    "这些选项不会修改 Makefile 或 .uvprojx；实际参与编译需要按工具链接入。",
                )
                .size(10.5)
                .color(theme::TEXT_FAINT),
            );
            ui.add_space(10.0);

            ui.horizontal(|ui| {
                option_tab(ui, &mut tab, BuildOptionsTab::Global, "全局选项");
                option_tab(ui, &mut tab, BuildOptionsTab::Compiler, "C/C++ 编译器");
                option_tab(ui, &mut tab, BuildOptionsTab::Assembler, "汇编器");
                option_tab(ui, &mut tab, BuildOptionsTab::Linker, "链接器");
            });
            ui.add_space(10.0);

            ScrollArea::vertical()
                .id_salt("simple_embedded_build_options_scroll")
                .max_height(430.0)
                .auto_shrink([false, false])
                .show(ui, |ui| {
                    ui.spacing_mut().item_spacing.y = 8.0;
                    match tab {
                        BuildOptionsTab::Global => {
                            ui.checkbox(
                                &mut draft.use_microlib,
                                RichText::new("Use MicroLIB")
                                    .size(11.5)
                                    .color(theme::TEXT_DIM),
                            );
                            ui.checkbox(
                                &mut draft.big_endian,
                                RichText::new("Big Endian")
                                    .size(11.5)
                                    .color(theme::TEXT_DIM),
                            );
                            combo_row(
                                ui,
                                "输出调试信息",
                                "simple_embedded_debug_info",
                                &mut draft.debug_info,
                                &["enable", "disable"],
                            );
                        }
                        BuildOptionsTab::Compiler => {
                            combo_row(
                                ui,
                                "C 标准",
                                "simple_embedded_c_standard",
                                &mut draft.c_standard,
                                &["c90", "c99", "c11", "gnu90", "gnu99", "gnu11"],
                            );
                            combo_row(
                                ui,
                                "C++ 标准",
                                "simple_embedded_cpp_standard",
                                &mut draft.cpp_standard,
                                &["c++98", "c++03", "c++11", "c++14", "c++17", "gnu++17"],
                            );
                            combo_row(
                                ui,
                                "代码优化级别",
                                "simple_embedded_optimization",
                                &mut draft.optimization,
                                &["-O0", "-O1", "-O2", "-O3", "-Os", "-Oz"],
                            );
                            combo_row(
                                ui,
                                "警告等级",
                                "simple_embedded_warning",
                                &mut draft.warning,
                                &["-Wall -Wextra", "-Wall", "-Wextra", "-w", "ac5-like"],
                            );
                            ui.add_space(2.0);
                            ui.checkbox(
                                &mut draft.lto,
                                RichText::new("Link Time Optimization (LTO)")
                                    .size(11.5)
                                    .color(theme::TEXT_DIM),
                            );
                            ui.checkbox(
                                &mut draft.function_sections,
                                RichText::new("One ELF Section per Function")
                                    .size(11.5)
                                    .color(theme::TEXT_DIM),
                            );
                            ui.checkbox(
                                &mut draft.split_load_store,
                                RichText::new("Split Load and Store Multiple")
                                    .size(11.5)
                                    .color(theme::TEXT_DIM),
                            );
                            ui.checkbox(
                                &mut draft.plain_char_signed,
                                RichText::new("Plain Char is Signed")
                                    .size(11.5)
                                    .color(theme::TEXT_DIM),
                            );
                            ui.checkbox(
                                &mut draft.short_enums,
                                RichText::new("enum/wchar is short type")
                                    .size(11.5)
                                    .color(theme::TEXT_DIM),
                            );
                            ui.checkbox(
                                &mut draft.rtti,
                                RichText::new("use RTTI").size(11.5).color(theme::TEXT_DIM),
                            );
                        }
                        BuildOptionsTab::Assembler => {
                            option_list_editor(ui, "预处理器定义", &mut draft.assembler_defines);
                            combo_row(
                                ui,
                                "汇编器类型",
                                "simple_embedded_assembler_type",
                                &mut draft.assembler_type,
                                &["arm-auto (auto detect)", "armasm", "gas", "auto"],
                            );
                            ui.label(
                                RichText::new("汇编器附加选项")
                                    .size(11.5)
                                    .color(theme::TEXT_DIM),
                            );
                            Frame::new()
                                .fill(theme::BG)
                                .stroke(Stroke::new(1.0_f32, theme::BORDER_SOFT))
                                .corner_radius(6)
                                .inner_margin(Margin::symmetric(6, 5))
                                .show(ui, |ui| {
                                    ui.add(
                                        TextEdit::multiline(&mut draft.assembler_options)
                                            .font(FontId::monospace(12.0))
                                            .text_color(theme::TEXT)
                                            .background_color(theme::BG)
                                            .hint_text("汇编器附加选项")
                                            .desired_rows(3)
                                            .desired_width(f32::INFINITY),
                                    );
                                });
                        }
                        BuildOptionsTab::Linker => {
                            combo_row(
                                ui,
                                "输出格式",
                                "simple_embedded_linker_output",
                                &mut draft.linker_output_format,
                                &["AXF", "ELF", "Intel HEX", "BIN"],
                            );
                            ui.checkbox(
                                &mut draft.linker_no_binary,
                                RichText::new("不生成 Hex/Bin/S19 等二进制文件")
                                    .size(11.5)
                                    .color(theme::TEXT_DIM),
                            );
                            option_list_editor(
                                ui,
                                "不生成指定的二进制文件",
                                &mut draft.linker_binary_excludes,
                            );
                            option_text_row(ui, "X/O Base", &mut draft.linker_xo_base);
                            option_text_row(ui, "R/O Base", &mut draft.linker_ro_base);
                            option_text_row(ui, "R/W Base", &mut draft.linker_rw_base);
                            ui.label(
                                RichText::new("链接器附加选项")
                                    .size(11.5)
                                    .color(theme::TEXT_DIM),
                            );
                            Frame::new()
                                .fill(theme::BG)
                                .stroke(Stroke::new(1.0_f32, theme::BORDER_SOFT))
                                .corner_radius(6)
                                .inner_margin(Margin::symmetric(6, 5))
                                .show(ui, |ui| {
                                    ui.add(
                                        TextEdit::multiline(&mut draft.linker_options)
                                            .font(FontId::monospace(12.0))
                                            .text_color(theme::TEXT)
                                            .background_color(theme::BG)
                                            .hint_text("例如 --diag_suppress=L6329")
                                            .desired_rows(3)
                                            .desired_width(f32::INFINITY),
                                    );
                                });
                        }
                    }
                });

            ui.add_space(12.0);
            ui.horizontal(|ui| {
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

        self.build_options_tab = tab;
        if save {
            self.save_build_options(draft);
        } else if !close {
            self.build_options_draft = Some(draft);
        }
    }

    pub(super) fn show_meta_modal(&mut self, ctx: &egui::Context) {
        let Some(mut draft) = self.meta_draft.take() else {
            return;
        };
        let mut close = false;
        let mut save = false;
        let mut reset = false;

        let modal = egui::Modal::new(Id::new("simple_embedded_meta_modal"))
            .frame(
                Frame::new()
                    .fill(theme::PANEL)
                    .stroke(Stroke::new(1.0_f32, theme::BORDER))
                    .corner_radius(10)
                    .inner_margin(Margin::symmetric(20, 16)),
            )
            .backdrop_color(Color32::from_black_alpha(170));
        modal.show(ctx, |ui| {
            ui.set_width(520.0);
            ui.horizontal(|ui| {
                ui.label(
                    RichText::new("编辑 C/C++ 属性")
                        .size(18.0)
                        .strong()
                        .color(theme::TEXT),
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
                RichText::new("保存在本机工程设置中，不会修改 Makefile 或 .uvprojx。")
                    .size(10.5)
                    .color(theme::TEXT_FAINT),
            );
            ui.add_space(10.0);

            ui.label(
                RichText::new("包含目录（每行一个）")
                    .size(11.0)
                    .color(theme::TEXT_DIM),
            );
            ui.add_space(3.0);
            Frame::new()
                .fill(theme::BG)
                .stroke(Stroke::new(1.0_f32, theme::BORDER_SOFT))
                .corner_radius(6)
                .inner_margin(Margin::symmetric(6, 5))
                .show(ui, |ui| {
                    ui.add(
                        TextEdit::multiline(&mut draft.includes)
                            .font(FontId::monospace(12.0))
                            .text_color(theme::TEXT)
                            .background_color(theme::BG)
                            .hint_text("例如 ../Core/Inc")
                            .desired_rows(5)
                            .desired_width(f32::INFINITY),
                    );
                });

            ui.add_space(10.0);
            ui.label(
                RichText::new("库搜索目录（每行一个）")
                    .size(11.0)
                    .color(theme::TEXT_DIM),
            );
            ui.add_space(3.0);
            Frame::new()
                .fill(theme::BG)
                .stroke(Stroke::new(1.0_f32, theme::BORDER_SOFT))
                .corner_radius(6)
                .inner_margin(Margin::symmetric(6, 5))
                .show(ui, |ui| {
                    ui.add(
                        TextEdit::multiline(&mut draft.library_dirs)
                            .font(FontId::monospace(12.0))
                            .text_color(theme::TEXT)
                            .background_color(theme::BG)
                            .hint_text("例如 lib")
                            .desired_rows(3)
                            .desired_width(f32::INFINITY),
                    );
                });

            ui.add_space(10.0);
            ui.label(
                RichText::new("预处理器定义（每行一个）")
                    .size(11.0)
                    .color(theme::TEXT_DIM),
            );
            ui.add_space(3.0);
            Frame::new()
                .fill(theme::BG)
                .stroke(Stroke::new(1.0_f32, theme::BORDER_SOFT))
                .corner_radius(6)
                .inner_margin(Margin::symmetric(6, 5))
                .show(ui, |ui| {
                    ui.add(
                        TextEdit::multiline(&mut draft.defines)
                            .font(FontId::monospace(12.0))
                            .text_color(theme::TEXT)
                            .background_color(theme::BG)
                            .hint_text("例如 USE_HAL_DRIVER")
                            .desired_rows(4)
                            .desired_width(f32::INFINITY),
                    );
                });

            if let Some(note) = &draft.note {
                ui.add_space(8.0);
                ui.label(RichText::new(note).size(10.5).color(theme::WARNING));
            }

            ui.add_space(12.0);
            ui.horizontal(|ui| {
                if ui
                    .add(theme::secondary_button("恢复自动检测"))
                    .on_hover_text("清除手动覆盖，重新使用 Makefile / .uvprojx 中的值")
                    .clicked()
                {
                    reset = true;
                }
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

        if save {
            self.save_meta_overrides(
                parse_meta_lines(&draft.includes),
                parse_meta_lines(&draft.library_dirs),
                parse_meta_lines(&draft.defines),
            );
        } else if reset {
            self.clear_meta_overrides();
        } else if !close {
            self.meta_draft = Some(draft);
        }
    }

    pub(super) fn show_meta_add_modal(&mut self, ctx: &egui::Context) {
        let Some(mut draft) = self.meta_add.take() else {
            return;
        };
        let mut close = false;
        let mut add = false;

        let modal = egui::Modal::new(Id::new("simple_embedded_meta_add_modal"))
            .frame(
                Frame::new()
                    .fill(theme::PANEL)
                    .stroke(Stroke::new(1.0_f32, theme::BORDER))
                    .corner_radius(10)
                    .inner_margin(Margin::symmetric(20, 16)),
            )
            .backdrop_color(Color32::from_black_alpha(150));
        modal.show(ctx, |ui| {
            ui.set_width(430.0);
            ui.horizontal(|ui| {
                ui.label(
                    RichText::new(format!("添加{}", draft.field.label()))
                        .size(16.0)
                        .strong()
                        .color(theme::TEXT),
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
            ui.add_space(3.0);
            ui.label(
                RichText::new("保存在本机工程设置中，不会修改 Makefile 或 .uvprojx。")
                    .size(10.5)
                    .color(theme::TEXT_FAINT),
            );
            ui.add_space(10.0);
            Frame::new()
                .fill(theme::BG)
                .stroke(Stroke::new(1.0_f32, theme::BORDER_SOFT))
                .corner_radius(6)
                .inner_margin(Margin::symmetric(7, 5))
                .show(ui, |ui| {
                    let response = ui.add(
                        TextEdit::singleline(&mut draft.value)
                            .font(FontId::monospace(12.0))
                            .text_color(theme::TEXT)
                            .background_color(theme::BG)
                            .hint_text(match draft.field {
                                MetaListField::Includes => "例如 ../Core/Inc",
                                MetaListField::LibraryDirs => "例如 lib",
                                MetaListField::Defines => "例如 USE_HAL_DRIVER",
                            })
                            .desired_width(f32::INFINITY),
                    );
                    response.request_focus();
                });
            ui.add_space(12.0);
            ui.horizontal(|ui| {
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    if ui.add(theme::primary_button("添加")).clicked() {
                        add = true;
                    }
                    if ui.add(theme::secondary_button("取消")).clicked() {
                        close = true;
                    }
                });
            });
            if ui.input(|input| input.key_pressed(egui::Key::Enter)) {
                add = true;
            }
        });

        if add {
            if draft.value.trim().is_empty() {
                self.meta_add = Some(draft);
            } else {
                self.add_meta_value(draft.field, &draft.value);
            }
        } else if !close {
            self.meta_add = Some(draft);
        }
    }

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

fn parse_meta_lines(text: &str) -> Vec<String> {
    let mut lines = Vec::new();
    for line in text.lines().map(str::trim).filter(|line| !line.is_empty()) {
        if !lines.iter().any(|value| value == line) {
            lines.push(line.to_owned());
        }
    }
    lines
}

fn light_separator(ui: &mut egui::Ui) {
    let (rect, _) = ui.allocate_exact_size(egui::vec2(ui.available_width(), 1.0), Sense::hover());
    ui.painter()
        .rect_filled(rect, CornerRadius::ZERO, Color32::from_rgb(200, 208, 220));
}

fn option_tab(
    ui: &mut egui::Ui,
    selected: &mut BuildOptionsTab,
    tab: BuildOptionsTab,
    label: &str,
) {
    let active = *selected == tab;
    let button = Button::new(RichText::new(label).size(11.5).strong().color(if active {
        theme::ON_ACCENT
    } else {
        theme::TEXT_DIM
    }))
    .fill(if active {
        theme::ACCENT
    } else {
        theme::SURFACE
    })
    .stroke(Stroke::new(
        1.0_f32,
        if active { theme::ACCENT } else { theme::BORDER },
    ))
    .corner_radius(5)
    .min_size(egui::vec2(128.0, 30.0));
    if ui.add(button).clicked() {
        *selected = tab;
    }
}

fn option_list_editor(ui: &mut egui::Ui, label: &str, items: &mut Vec<String>) {
    ui.horizontal(|ui| {
        ui.label(RichText::new(label).size(11.5).color(theme::TEXT_DIM));
        let add = Button::new(
            RichText::new("添加")
                .size(11.5)
                .strong()
                .color(theme::ON_ACCENT),
        )
        .fill(theme::SUCCESS)
        .stroke(Stroke::NONE)
        .corner_radius(5)
        .min_size(egui::vec2(48.0, 24.0));
        if ui.add(add).clicked() {
            items.push(String::new());
        }
    });

    let mut remove = None;
    for (index, item) in items.iter_mut().enumerate() {
        ui.horizontal(|ui| {
            Frame::new()
                .fill(theme::BG)
                .stroke(Stroke::new(1.0_f32, theme::BORDER_SOFT))
                .corner_radius(5)
                .inner_margin(Margin::symmetric(6, 3))
                .show(ui, |ui| {
                    ui.add(
                        TextEdit::singleline(item)
                            .font(FontId::monospace(11.5))
                            .text_color(theme::TEXT)
                            .background_color(theme::BG)
                            .desired_width(470.0),
                    );
                });
            let delete = Button::new(RichText::new("删除").size(11.0).strong().color(theme::TEXT))
                .fill(theme::ERROR.gamma_multiply(0.75))
                .stroke(Stroke::NONE)
                .corner_radius(5)
                .min_size(egui::vec2(46.0, 24.0));
            if ui.add(delete).clicked() {
                remove = Some(index);
            }
        });
    }
    if let Some(index) = remove {
        items.remove(index);
    }
    if items.is_empty() {
        ui.label(RichText::new("未添加").size(10.5).color(theme::TEXT_FAINT));
    }
}

fn option_text_row(ui: &mut egui::Ui, label: &str, value: &mut String) {
    ui.horizontal(|ui| {
        ui.add_sized(
            [120.0, 24.0],
            Label::new(RichText::new(label).size(11.5).color(theme::TEXT_DIM)),
        );
        Frame::new()
            .fill(theme::BG)
            .stroke(Stroke::new(1.0_f32, theme::BORDER_SOFT))
            .corner_radius(5)
            .inner_margin(Margin::symmetric(6, 3))
            .show(ui, |ui| {
                ui.add(
                    TextEdit::singleline(value)
                        .font(FontId::monospace(11.5))
                        .text_color(theme::TEXT)
                        .background_color(theme::BG)
                        .desired_width(330.0),
                );
            });
    });
}

fn combo_row(
    ui: &mut egui::Ui,
    label: &str,
    id_source: &str,
    value: &mut String,
    options: &[&str],
) {
    ui.horizontal(|ui| {
        ui.add_sized(
            [120.0, 26.0],
            Label::new(RichText::new(label).size(11.5).color(theme::TEXT_DIM)),
        );
        egui::ComboBox::from_id_salt(id_source)
            .selected_text(RichText::new(value.as_str()).size(11.5))
            .width(190.0)
            .show_ui(ui, |ui| {
                for option in options {
                    ui.selectable_value(value, (*option).to_owned(), *option);
                }
            });
    });
}

fn meta_group(
    ui: &mut egui::Ui,
    title: &str,
    default_open: bool,
    content: impl FnOnce(&mut egui::Ui),
) {
    let id = ui.make_persistent_id(("simple_embedded_meta_group", title));
    let selected_id = egui::Id::new("simple_embedded_meta_list_selected");
    let mut open = ui
        .data_mut(|data| data.get_persisted::<bool>(id))
        .unwrap_or(default_open);
    let selected = ui.data_mut(|data| data.get_persisted::<egui::Id>(selected_id)) == Some(id);

    let row_height = 22.0;
    let row_rect = egui::Rect::from_min_size(
        ui.cursor().min,
        egui::vec2(ui.available_width(), row_height),
    );
    let hovered = ui.rect_contains_pointer(row_rect);
    let fill = if selected {
        Color32::from_rgb(36, 84, 108)
    } else if hovered {
        Color32::from_rgb(28, 49, 63)
    } else {
        Color32::TRANSPARENT
    };
    if fill != Color32::TRANSPARENT {
        ui.painter()
            .rect_filled(row_rect, CornerRadius::same(4), fill);
    }

    let row = ui.allocate_ui_with_layout(
        egui::vec2(ui.available_width(), row_height),
        Layout::left_to_right(Align::Center),
        |ui| {
            ui.style_mut().interaction.selectable_labels = false;
            let (chevron_rect, _) = ui.allocate_exact_size(egui::vec2(14.0, 18.0), Sense::hover());
            paint_chevron(ui.painter(), chevron_rect, open, theme::ACCENT);
            ui.label(RichText::new(title).size(12.5).strong().color(if selected {
                theme::TEXT
            } else {
                theme::TEXT_DIM
            }));
        },
    );
    let response = row
        .response
        .interact(Sense::click())
        .on_hover_cursor(egui::CursorIcon::PointingHand);
    if response.clicked() {
        ui.data_mut(|data| data.insert_persisted(selected_id, id));
        open = !open;
        ui.data_mut(|data| data.insert_persisted(id, open));
    }
    if open {
        ui.add_space(2.0);
        ui.indent(id.with("body"), |ui| content(ui));
        ui.add_space(3.0);
    }
}

fn paint_chevron(painter: &egui::Painter, rect: egui::Rect, open: bool, color: Color32) {
    let center = rect.center();
    let stroke = Stroke::new(1.6_f32, color);
    if open {
        painter.line_segment(
            [
                egui::pos2(center.x - 4.0, center.y - 2.0),
                egui::pos2(center.x, center.y + 2.0),
            ],
            stroke,
        );
        painter.line_segment(
            [
                egui::pos2(center.x, center.y + 2.0),
                egui::pos2(center.x + 4.0, center.y - 2.0),
            ],
            stroke,
        );
    } else {
        painter.line_segment(
            [
                egui::pos2(center.x - 2.0, center.y - 4.0),
                egui::pos2(center.x + 2.0, center.y),
            ],
            stroke,
        );
        painter.line_segment(
            [
                egui::pos2(center.x + 2.0, center.y),
                egui::pos2(center.x - 2.0, center.y + 4.0),
            ],
            stroke,
        );
    }
}

fn meta_row(ui: &mut egui::Ui, label: &str, value: &str) {
    if value.trim().is_empty() {
        return;
    }
    ui.horizontal(|ui| {
        ui.add_sized(
            [70.0, 18.0],
            Label::new(RichText::new(label).size(11.0).color(theme::TEXT_FAINT)),
        );
        let response =
            ui.add(Label::new(RichText::new(value).size(11.5).color(theme::TEXT_DIM)).truncate());
        response.on_hover_text(value);
    });
}

fn meta_editable_list(
    ui: &mut egui::Ui,
    label: &str,
    items: &[String],
    field: MetaListField,
    actions: &mut Vec<Action>,
) {
    let id = ui.make_persistent_id(("simple_embedded_meta_list", label));
    let selected_id = egui::Id::new("simple_embedded_meta_list_selected");
    let mut open = ui
        .data_mut(|data| data.get_persisted::<bool>(id))
        .unwrap_or(true);
    let selected = ui.data_mut(|data| data.get_persisted::<egui::Id>(selected_id)) == Some(id);

    let row_height = 22.0;
    let row_rect = egui::Rect::from_min_size(
        ui.cursor().min,
        egui::vec2(ui.available_width(), row_height),
    );
    let hovered = ui.rect_contains_pointer(row_rect);
    let fill = if selected {
        Color32::from_rgb(36, 84, 108)
    } else if hovered {
        Color32::from_rgb(28, 49, 63)
    } else {
        Color32::TRANSPARENT
    };
    if fill != Color32::TRANSPARENT {
        ui.painter()
            .rect_filled(row_rect, CornerRadius::same(4), fill);
    }

    let mut plus_clicked = false;
    let row = ui.allocate_ui_with_layout(
        egui::vec2(ui.available_width(), row_height),
        Layout::left_to_right(Align::Center),
        |ui| {
            ui.style_mut().interaction.selectable_labels = false;
            let (chevron_rect, _) = ui.allocate_exact_size(egui::vec2(14.0, 18.0), Sense::hover());
            paint_chevron(ui.painter(), chevron_rect, open, theme::ACCENT);
            ui.label(RichText::new(label).size(11.0).color(if selected {
                theme::TEXT
            } else {
                theme::TEXT_FAINT
            }));
            let plus_width = 22.0;
            let remaining = ui.available_width();
            let reserved = plus_width + 8.0;
            if remaining > reserved {
                ui.add_space(remaining - reserved);
            }
            let plus = paint_plus_button(ui);
            plus_clicked = plus.clicked();
            plus.on_hover_text(format!("添加{label}"));
        },
    );
    let response = row
        .response
        .interact(Sense::click())
        .on_hover_cursor(egui::CursorIcon::PointingHand);
    if response.clicked() || plus_clicked {
        ui.data_mut(|data| data.insert_persisted(selected_id, id));
    }
    if plus_clicked {
        actions.push(Action::AddMetaItem(field));
    }
    if response.clicked() && !plus_clicked {
        open = !open;
        ui.data_mut(|data| data.insert_persisted(id, open));
    }
    if !open {
        return;
    }

    if items.is_empty() {
        ui.horizontal(|ui| {
            ui.add_space(14.0);
            ui.label(RichText::new("未检测").size(11.0).color(theme::TEXT_FAINT));
        });
    } else {
        for item in items {
            ui.horizontal(|ui| {
                ui.add_space(14.0);
                let response = ui.add(
                    Label::new(
                        RichText::new(format!("• {item}"))
                            .monospace()
                            .size(11.0)
                            .color(theme::TEXT_DIM),
                    )
                    .truncate(),
                );
                response.on_hover_text(item);
            });
        }
    }
}

fn paint_plus_button(ui: &mut egui::Ui) -> egui::Response {
    let (rect, response) = ui.allocate_exact_size(egui::vec2(22.0, 22.0), Sense::click());
    let response = response.on_hover_cursor(egui::CursorIcon::PointingHand);
    if response.hovered() {
        ui.painter()
            .rect_filled(rect, CornerRadius::same(4), Color32::from_rgb(28, 55, 43));
    }
    let center = rect.center();
    let arm = 5.0;
    let stroke = Stroke::new(2.4_f32, theme::SUCCESS);
    ui.painter().line_segment(
        [
            egui::pos2(center.x - arm, center.y),
            egui::pos2(center.x + arm, center.y),
        ],
        stroke,
    );
    ui.painter().line_segment(
        [
            egui::pos2(center.x, center.y - arm),
            egui::pos2(center.x, center.y + arm),
        ],
        stroke,
    );
    response
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
