//! Workspace UI: explorer, tabs and the text editor surface.

use super::*;
use crate::config::autodetect;
use crate::makefile;
use eframe::egui::{
    self, Align, Align2, Button, Checkbox, Color32, FontId, Frame, Id, Label, Layout, Margin,
    RadioButton, RichText, ScrollArea, Sense, Shape, Stroke, TextEdit,
};

impl App {
    pub(super) fn show_status_bar(&mut self, ui: &mut egui::Ui, actions: &mut Vec<Action>) {
        let info = self.active_document().map(|doc| {
            (
                doc.cursor_line,
                doc.cursor_column,
                doc.encoding,
                doc.newline_label(),
                doc.language.clone(),
                doc.is_dirty(),
                doc.read_only,
            )
        });
        egui::Panel::bottom("simple_embedded_status_bar")
            .show_separator_line(false)
            .exact_size(30.0)
            .frame(
                Frame::new()
                    .fill(theme::PANEL)
                    .inner_margin(Margin::symmetric(10, 0)),
            )
            .show_inside(ui, |ui| {
                ui.horizontal_centered(|ui| {
                    ui.spacing_mut().item_spacing.x = 8.0;
                    let text_color = theme::TEXT_FAINT;
                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        if let Some((_, _, _, _, _, dirty, read_only)) = info {
                            let save_color = if dirty {
                                theme::ACCENT
                            } else {
                                theme::TEXT_FAINT
                            };
                            if ui
                                .add(small_flat_button("保存", save_color))
                                .on_hover_text("保存当前文件 (Ctrl+S)")
                                .clicked()
                            {
                                actions.push(Action::SaveActive);
                            }
                            let wrap_color = if self.settings.word_wrap {
                                theme::ACCENT
                            } else {
                                theme::TEXT_FAINT
                            };
                            if ui
                                .add(small_flat_button("自动换行", wrap_color))
                                .on_hover_text("切换长行自动换行")
                                .clicked()
                            {
                                actions.push(Action::ToggleWrap);
                            }
                            if ui
                                .add(small_flat_button("格式化", theme::TEXT_DIM))
                                .on_hover_text("用 clang-format 格式化当前文件")
                                .clicked()
                            {
                                actions.push(Action::FormatDocument);
                            }
                            if ui
                                .add(small_flat_button("符号", theme::TEXT_DIM))
                                .on_hover_text("工程符号搜索 (Ctrl+Shift+O)")
                                .clicked()
                            {
                                actions.push(Action::OpenSymbols);
                            }
                            if ui
                                .add(small_flat_button("数组", theme::TEXT_DIM))
                                .on_hover_text("生成 C 数组 (Ctrl+Shift+A)")
                                .clicked()
                            {
                                actions.push(Action::OpenArrayGenerator);
                            }
                            if ui
                                .add(small_flat_button("寄存器", theme::TEXT_DIM))
                                .on_hover_text("外设寄存器浏览 (Ctrl+Shift+R)")
                                .clicked()
                            {
                                actions.push(Action::OpenRegisters);
                            }
                            let read_only_color = if read_only {
                                theme::WARNING
                            } else {
                                theme::TEXT_FAINT
                            };
                            if ui
                                .add(small_flat_button("只读", read_only_color))
                                .on_hover_text("切换只读")
                                .clicked()
                            {
                                actions.push(Action::ToggleReadOnly);
                            }
                        }
                        if let Some((line, column, encoding, newline, language, _, _)) = info {
                            ui.add_space(8.0);
                            ui.label(RichText::new(language).size(10.5).color(theme::INFO));
                            ui.add_space(6.0);
                            ui.label(
                                RichText::new(format!("行尾: {newline}"))
                                    .size(10.5)
                                    .color(text_color),
                            );
                            ui.add_space(6.0);
                            ui.scope(|ui| {
                                let visuals = ui.visuals_mut();
                                visuals.widgets.inactive.weak_bg_fill = theme::SURFACE;
                                visuals.widgets.inactive.bg_fill = theme::SURFACE;
                                visuals.widgets.hovered.weak_bg_fill = theme::HOVER;
                                visuals.widgets.hovered.bg_fill = theme::HOVER;
                                visuals.widgets.open.weak_bg_fill = theme::SURFACE;
                                visuals.widgets.open.bg_fill = theme::SURFACE;
                                ui.menu_button(
                                    RichText::new(encoding.label()).size(10.5).color(text_color),
                                    |ui| {
                                        for value in [
                                            crate::util::TextEncoding::Utf8,
                                            crate::util::TextEncoding::Gbk,
                                        ] {
                                            if ui
                                                .selectable_label(encoding == value, value.label())
                                                .clicked()
                                            {
                                                actions.push(Action::SetEncoding(value));
                                                ui.close();
                                            }
                                        }
                                    },
                                );
                            });
                            ui.add_space(6.0);
                            let indent = if self.settings.use_spaces {
                                format!("缩进: 空格 {}", self.settings.indent_width)
                            } else {
                                format!("缩进: Tab {}", self.settings.indent_width)
                            };
                            ui.label(RichText::new(indent).size(10.5).color(text_color));
                            ui.add_space(6.0);
                            ui.label(
                                RichText::new(format!("行 {line}，列 {column}"))
                                    .size(10.5)
                                    .color(text_color),
                            );
                        } else {
                            ui.label(RichText::new("就绪").size(10.5).color(text_color));
                        }
                    });
                });
            });
    }

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
            .show_separator_line(false)
            .resizable(true)
            .default_size(272.0)
            .size_range(230.0..=440.0)
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
                let panel_bottom = ui.max_rect().bottom();
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
                                let columns = 3.0;
                                let button_width =
                                    ((ui.available_width() - spacing * (columns - 1.0)) / columns)
                                        .max(0.0);
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
                                    let workspace = if has_project {
                                        egui::Button::new(
                                            RichText::new("工作区")
                                                .size(12.5)
                                                .color(theme::ACCENT),
                                        )
                                        .fill(theme::SURFACE)
                                        .stroke(Stroke::new(1.0_f32, theme::BORDER))
                                        .corner_radius(6)
                                    } else {
                                        theme::secondary_button("工作区")
                                    };
                                    let workspace_response = ui
                                        .add_sized([button_width, 28.0], workspace)
                                        .on_hover_text(
                                            "生成 .se-workspace，双击即可打开本工程",
                                        );
                                    egui::Popup::menu(&workspace_response).show(|ui| {
                                        ui.set_min_width(190.0);
                                        if has_project
                                            && ui.button("生成并关联工作区文件").clicked()
                                        {
                                            actions.push(Action::CreateWorkspace);
                                            ui.close();
                                        }
                                        if ui.button("打开工作区文件...").clicked() {
                                            actions.push(Action::OpenWorkspacePicker);
                                            ui.close();
                                        }
                                        if has_project {
                                            ui.separator();
                                            if ui.button("打开工程配置 settings.json").clicked()
                                            {
                                                actions.push(Action::OpenProjectConfig);
                                                ui.close();
                                            }
                                        }
                                    });
                                });
                            });
                    });

                let mut resource_open = self.resource_open;
                let mut info_open = self.info_open;
                let mut tree_clipped = false;
                let mut resource_state =
                    egui::collapsing_header::CollapsingState::load_with_default_open(
                        ui.ctx(),
                        Id::new("simple_embedded_resource_body"),
                        resource_open,
                    );
                resource_state.set_open(resource_open);
                let resource_openness = resource_state.openness(ui.ctx());
                let mut info_state =
                    egui::collapsing_header::CollapsingState::load_with_default_open(
                        ui.ctx(),
                        Id::new("simple_embedded_info_body"),
                        info_open,
                    );
                info_state.set_open(info_open);
                let info_openness = info_state.openness(ui.ctx());
                if (resource_openness > 0.0 && resource_openness < 1.0)
                    || (info_openness > 0.0 && info_openness < 1.0)
                {
                    ui.ctx().request_repaint();
                }

                ui.allocate_ui_with_layout(
                    egui::vec2(ui.available_width(), ui.available_height()),
                    Layout::top_down(Align::Min),
                    |ui| {
                        ui.add_space(10.0);
                        ui.spacing_mut().item_spacing.y = 0.0;
                        Frame::new()
                            .inner_margin(Margin::symmetric(11, 4))
                            .show(ui, |ui| {
                                ui.spacing_mut().interact_size.y = 18.0;
                                ui.horizontal(|ui| {
                                    if section_chevron(ui, resource_openness).clicked() {
                                        resource_open = !resource_open;
                                        resource_state.set_open(resource_open);
                                    }
                                    let (bar_rect, _) = ui
                                        .allocate_exact_size(egui::vec2(3.0, 13.0), Sense::hover());
                                    ui.painter().rect_filled(
                                        bar_rect,
                                        CornerRadius::same(1),
                                        theme::ACCENT,
                                    );
                                    if ui
                                        .add(
                                            Button::new(
                                                RichText::new("资源管理器")
                                                    .size(12.0)
                                                    .strong()
                                                    .color(theme::TEXT_DIM),
                                            )
                                            .frame(false),
                                        )
                                        .clicked()
                                    {
                                        resource_open = !resource_open;
                                    }
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
                                                if ui
                                                    .add(
                                                        Button::new(
                                                            RichText::new("Git")
                                                                .size(12.0)
                                                                .color(theme::TEXT_FAINT),
                                                        )
                                                        .frame(false),
                                                    )
                                                    .on_hover_text("查看 Git 状态 (Ctrl+Shift+G)")
                                                    .clicked()
                                                {
                                                    actions.push(Action::OpenGitStatus);
                                                }
                                                if ui
                                                    .add(
                                                        Button::new(
                                                            RichText::new("TODO")
                                                                .size(12.0)
                                                                .color(theme::TEXT_FAINT),
                                                        )
                                                        .frame(false),
                                                    )
                                                    .on_hover_text("扫描 TODO/FIXME (Ctrl+Shift+T)")
                                                    .clicked()
                                                {
                                                    actions.push(Action::OpenTodos);
                                                }
                                            },
                                        );
                                    }
                                });
                            });

                        resource_state.show_body_unindented(ui, |ui| {
                            ui.add_space(6.0);
                            let tree_budget = if has_project {
                                (ui.available_height() - 56.0).max(80.0)
                            } else {
                                ui.available_height().max(80.0)
                            };
                            let tree_cap = if has_project && info_open {
                                (tree_budget * self.left_split).clamp(80.0, tree_budget.max(80.0))
                            } else {
                                tree_budget
                            };
                            let tree_output = ScrollArea::vertical()
                                .id_salt("simple_embedded_file_tree")
                                .max_height(tree_cap)
                                .auto_shrink([false, true])
                                .show(ui, |ui| {
                                    Frame::new().inner_margin(Margin::symmetric(5, 4)).show(
                                        ui,
                                        |ui| {
                                            if has_project {
                                                if let Some(tree) = self.tree.as_mut() {
                                                    let tree_actions = tree.show(
                                                        ui,
                                                        active_path.as_deref(),
                                                        self.file_clipboard.is_some(),
                                                    );
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
                                                            TreeAction::HexView(path) => {
                                                                Action::OpenHex(path)
                                                            }
                                                            TreeAction::PreviewImage(path) => {
                                                                Action::PreviewImage(path)
                                                            }
                                                            TreeAction::NewFile(path) => {
                                                                Action::NewFile(path)
                                                            }
                                                            TreeAction::NewFolder(path) => {
                                                                Action::NewFolder(path)
                                                            }
                                                            TreeAction::CopyPath(path) => {
                                                                Action::CopyPath(path)
                                                            }
                                                            TreeAction::CopyRelativePath(path) => {
                                                                Action::CopyRelativePath(path)
                                                            }
                                                            TreeAction::OpenToSide(path) => {
                                                                Action::OpenToSide(path)
                                                            }
                                                            TreeAction::Cut(path) => {
                                                                Action::CutFile(path)
                                                            }
                                                            TreeAction::Copy(path) => {
                                                                Action::CopyFile(path)
                                                            }
                                                            TreeAction::Paste(path) => {
                                                                Action::PasteFile(path)
                                                            }
                                                            TreeAction::Rename(path) => {
                                                                Action::RenameFile(path)
                                                            }
                                                            TreeAction::Delete(path) => {
                                                                Action::DeleteFile(path)
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
                                        },
                                    );
                                });

                            if has_project {
                                let clipped = tree_output.content_size.y
                                    > tree_output.inner_rect.height() + 0.5;
                                tree_clipped = clipped;
                                if resource_open && resource_openness >= 0.999 {
                                    let can_resize = info_open && clipped;
                                    let (split_rect, split_response) = ui.allocate_exact_size(
                                        egui::vec2(ui.available_width(), 8.0),
                                        if can_resize {
                                            Sense::drag()
                                        } else {
                                            Sense::hover()
                                        },
                                    );
                                    let split_response = if can_resize {
                                        split_response
                                            .on_hover_cursor(egui::CursorIcon::ResizeVertical)
                                    } else {
                                        split_response
                                    };
                                    if split_response.dragged() {
                                        self.left_split = (self.left_split
                                            + split_response.drag_delta().y / tree_budget)
                                            .clamp(0.18, 0.82);
                                        self.settings.left_split = self.left_split;
                                    }
                                    if split_response.drag_stopped() {
                                        self.settings.save();
                                    }
                                    let _ = split_rect;
                                }
                            }
                        });

                        if has_project {
                            let header_height = 32.0;
                            let closed_pad = if !info_open && tree_clipped {
                                // Center the collapsed project-info header in the
                                // space left below the file tree so the gaps above
                                // and below the header have the same height.
                                let remaining = (panel_bottom - ui.cursor().top()).max(0.0);
                                ((remaining - header_height) * 0.5).max(0.0)
                            } else {
                                0.0
                            };
                            let leading =
                                6.0 * info_openness + closed_pad * (1.0 - info_openness);
                            ui.add_space(leading);
                            info_header_row(ui, &mut info_open, info_openness);
                            info_state.set_open(info_open);
                            info_state.show_body_unindented(ui, |ui| {
                                ui.add_space(6.0);
                                let height = (ui.available_height() - 2.0).max(80.0);
                                Frame::new().inner_margin(Margin::symmetric(11, 0)).show(
                                    ui,
                                    |ui| {
                                        let mut area = ScrollArea::vertical()
                                            .id_salt("simple_embedded_project_meta")
                                            .max_height(height)
                                            .auto_shrink([false, false]);
                                        if !(info_open && info_openness >= 0.999) {
                                            area = area.scroll_bar_visibility(
                                                egui::scroll_area::ScrollBarVisibility::AlwaysHidden,
                                            );
                                        }
                                        area.show(ui, |ui| self.show_project_meta(ui, actions));
                                    },
                                );
                            });
                        }
                    },
                );
                if self.resource_open != resource_open || self.info_open != info_open {
                    self.resource_open = resource_open;
                    self.info_open = info_open;
                    self.settings.resource_open = resource_open;
                    self.settings.info_open = info_open;
                    self.settings.save();
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
            let jobs = if self.build_config.build_jobs == 0 {
                "未启用".to_owned()
            } else {
                format!("{} 个任务", self.build_config.build_jobs)
            };
            meta_row(ui, "并行任务", &jobs);
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

        let mut add_support_pack = false;
        meta_group_with_add(
            ui,
            "芯片支持包",
            true,
            "添加 Keil/CMSIS 支持包（.pack / .pdsc）",
            &mut add_support_pack,
            |ui| {
                if self.support_packs.is_empty() {
                    ui.label(RichText::new("未添加").size(11.0).color(theme::TEXT_FAINT));
                    ui.label(
                        RichText::new("点击右侧 + 选择 .pack / .pdsc")
                            .size(9.0)
                            .color(theme::TEXT_FAINT),
                    );
                } else {
                    for (index, pack) in self.support_packs.iter().enumerate() {
                        if index > 0 {
                            ui.add_space(6.0);
                        }
                        let external = self
                            .project
                            .as_ref()
                            .is_some_and(|project| !pack.path.starts_with(&project.root));
                        ui.horizontal(|ui| {
                            colored_dot(
                                ui,
                                if pack.error.is_none() {
                                    theme::SUCCESS
                                } else {
                                    theme::WARNING
                                },
                                4.0,
                            );
                            ui.label(
                                RichText::new(&pack.name)
                                    .size(11.5)
                                    .strong()
                                    .color(theme::TEXT),
                            );
                            if let Some(version) = &pack.version {
                                ui.label(RichText::new(version).size(9.5).color(theme::TEXT_FAINT));
                            }
                            if external {
                                ui.label(RichText::new("Keil 安装").size(8.5).color(theme::INFO));
                                if ui
                                    .add(small_flat_button("导入", theme::ACCENT))
                                    .on_hover_text("复制到工程根目录")
                                    .clicked()
                                {
                                    actions.push(Action::ImportSupportPack(pack.path.clone()));
                                }
                            }
                        });
                        let response = ui.add(
                            Label::new(
                                RichText::new(&pack.file_name)
                                    .size(9.0)
                                    .color(theme::TEXT_FAINT),
                            )
                            .truncate(),
                        );
                        response.on_hover_text(util::display_path(&pack.path));
                        if let Some(vendor) = &pack.vendor {
                            ui.label(
                                RichText::new(format!("厂商：{vendor}"))
                                    .size(9.5)
                                    .color(theme::TEXT_DIM),
                            );
                        }
                        ui.label(
                            RichText::new(pack.device_summary())
                                .size(9.5)
                                .color(theme::TEXT_DIM),
                        );
                        if pack.svd_count > 0 {
                            ui.label(
                                RichText::new(format!("SVD：{} 个", pack.svd_count))
                                    .size(9.0)
                                    .color(theme::TEXT_FAINT),
                            );
                        }
                        if let Some(error) = &pack.error {
                            ui.label(RichText::new(error).size(9.0).color(theme::WARNING));
                        }
                    }
                }
            },
        );
        if add_support_pack {
            actions.push(Action::AddSupportPack);
        }

        meta_group(ui, "烧录配置", true, |ui| {
            let mut tool = self.build_config.flash_tool;
            ui.horizontal(|ui| {
                ui.add_sized(
                    [70.0, 18.0],
                    Label::new(
                        RichText::new("烧录工具")
                            .size(11.0)
                            .color(theme::TEXT_FAINT),
                    ),
                );
                egui::ComboBox::from_id_salt("simple_embedded_flash_tool")
                    .selected_text(
                        RichText::new(tool.label())
                            .size(11.5)
                            .color(theme::TEXT_DIM),
                    )
                    .width(190.0)
                    .show_ui(ui, |ui| {
                        ui.selectable_value(&mut tool, FlashTool::Custom, "自定义命令");
                        ui.selectable_value(&mut tool, FlashTool::OpenOcd, "OpenOCD");
                        ui.selectable_value(&mut tool, FlashTool::PyOcd, "pyOCD");
                    });
            });
            if tool != self.build_config.flash_tool {
                actions.push(Action::SetFlashTool(tool));
            }

            if self.build_config.flash_tool == FlashTool::OpenOcd {
                let current = self.build_config.openocd.executable.trim().to_owned();
                let mut chosen = current.clone();
                let mut browse = false;
                ui.horizontal(|ui| {
                    ui.add_sized(
                        [70.0, 18.0],
                        Label::new(
                            RichText::new("工具路径")
                                .size(11.0)
                                .color(theme::TEXT_FAINT),
                        ),
                    );
                    let short = if current.is_empty() || current.eq_ignore_ascii_case("openocd") {
                        "openocd".to_owned()
                    } else {
                        Path::new(&current)
                            .file_name()
                            .map(|name| name.to_string_lossy().into_owned())
                            .unwrap_or_else(|| current.clone())
                    };
                    let response = egui::ComboBox::from_id_salt("simple_embedded_openocd_path")
                        .selected_text(RichText::new(short).size(11.5).color(theme::TEXT_DIM))
                        .width(174.0)
                        .show_ui(ui, |ui| {
                            for candidate in flash::openocd_candidates(&current) {
                                let label = if candidate.eq_ignore_ascii_case("openocd") {
                                    "openocd（PATH）".to_owned()
                                } else {
                                    candidate.clone()
                                };
                                ui.selectable_value(&mut chosen, candidate, label);
                            }
                        });
                    response.response.on_hover_text(if current.is_empty() {
                        "openocd"
                    } else {
                        current.as_str()
                    });
                    if ui
                        .add(small_flat_button("浏览", theme::TEXT_FAINT))
                        .on_hover_text("选择 openocd 可执行文件")
                        .clicked()
                    {
                        browse = true;
                    }
                });
                if browse {
                    actions.push(Action::BrowseOpenOcd);
                } else if chosen != current {
                    actions.push(Action::SetOpenOcdPath(chosen));
                }
                if self
                    .project
                    .as_ref()
                    .is_some_and(|project| !flash::openocd_available(&current, &project.root))
                {
                    ui.label(
                        RichText::new("未找到 OpenOCD，请选择可执行文件")
                            .size(9.0)
                            .color(theme::WARNING),
                    );
                }

                let openocd = &self.build_config.openocd;
                let program_file = openocd.program_file.trim();
                let target_config = openocd.target_config.trim();
                let interface_config = openocd.interface_config.trim();
                meta_row(
                    ui,
                    "程序文件",
                    if program_file.is_empty() {
                        "未配置"
                    } else {
                        program_file
                    },
                );
                meta_row(
                    ui,
                    "芯片配置",
                    if target_config.is_empty() {
                        "未配置"
                    } else {
                        target_config
                    },
                );
                meta_row(
                    ui,
                    "接口配置",
                    if interface_config.is_empty() {
                        "未配置"
                    } else {
                        interface_config
                    },
                );
                if let Some(device) = &meta.device {
                    meta_row(ui, "芯片名称", device);
                }
            } else if self.build_config.flash_tool == FlashTool::PyOcd {
                let current = self.build_config.pyocd.executable.trim().to_owned();
                let mut chosen = current.clone();
                let mut browse = false;
                ui.horizontal(|ui| {
                    ui.add_sized(
                        [70.0, 18.0],
                        Label::new(
                            RichText::new("工具路径")
                                .size(11.0)
                                .color(theme::TEXT_FAINT),
                        ),
                    );
                    let short = if current.is_empty() || current.eq_ignore_ascii_case("pyocd") {
                        "pyocd".to_owned()
                    } else {
                        Path::new(&current)
                            .file_name()
                            .map(|name| name.to_string_lossy().into_owned())
                            .unwrap_or_else(|| current.clone())
                    };
                    let response = egui::ComboBox::from_id_salt("simple_embedded_pyocd_path")
                        .selected_text(RichText::new(short).size(11.5).color(theme::TEXT_DIM))
                        .width(174.0)
                        .show_ui(ui, |ui| {
                            for candidate in flash::pyocd_candidates(&current) {
                                let label = if candidate.eq_ignore_ascii_case("pyocd") {
                                    "pyocd（PATH）".to_owned()
                                } else {
                                    candidate.clone()
                                };
                                ui.selectable_value(&mut chosen, candidate, label);
                            }
                        });
                    response.response.on_hover_text(if current.is_empty() {
                        "pyocd"
                    } else {
                        current.as_str()
                    });
                    if ui
                        .add(small_flat_button("浏览", theme::TEXT_FAINT))
                        .on_hover_text("选择 pyocd 可执行文件")
                        .clicked()
                    {
                        browse = true;
                    }
                });
                if browse {
                    actions.push(Action::BrowsePyOcd);
                } else if chosen != current {
                    actions.push(Action::SetPyOcdPath(chosen));
                }
                if self
                    .project
                    .as_ref()
                    .is_some_and(|project| !flash::pyocd_available(&current, &project.root))
                {
                    ui.label(
                        RichText::new("未找到 pyOCD，请选择可执行文件")
                            .size(9.0)
                            .color(theme::WARNING),
                    );
                }

                let pyocd = &self.build_config.pyocd;
                let program_file = pyocd.program_file.trim();
                let target = pyocd.target.trim();
                let pack = pyocd.pack.trim();
                meta_row(
                    ui,
                    "程序文件",
                    if program_file.is_empty() {
                        "未配置"
                    } else {
                        program_file
                    },
                );
                meta_row(
                    ui,
                    "目标芯片",
                    if target.is_empty() {
                        "未配置"
                    } else {
                        target
                    },
                );
                meta_row(ui, "Pack", if pack.is_empty() { "未使用" } else { pack });
                if let Some(device) = &meta.device {
                    meta_row(ui, "芯片名称", device);
                }
            } else {
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
            }

            let memory_hint = self
                .memory_layout_summary()
                .unwrap_or_else(|| "设置 Flash / RAM 起始地址、大小和下载起始地址".to_owned());
            if let Some(resolved) = &self.memory_layout {
                let layout = resolved.layout;
                if layout.flash.is_valid() {
                    meta_row(
                        ui,
                        "Flash",
                        &format!(
                            "{} @ {}",
                            crate::memory::format_size(layout.flash.size),
                            crate::memory::format_hex(layout.flash.start)
                        ),
                    );
                }
                if layout.ram.is_valid() {
                    meta_row(
                        ui,
                        "RAM",
                        &format!(
                            "{} @ {}",
                            crate::memory::format_size(layout.ram.size),
                            crate::memory::format_hex(layout.ram.start)
                        ),
                    );
                }
            } else {
                meta_row(ui, "内存布局", "未识别，可手动填写");
            }

            ui.add_space(2.0);
            ui.horizontal(|ui| {
                if ui
                    .add(small_flat_button("编辑", theme::ACCENT))
                    .on_hover_text("编辑烧录工具、程序文件、OpenOCD / pyOCD 参数")
                    .clicked()
                {
                    actions.push(Action::EditFlashConfig);
                }
                if ui
                    .add(small_flat_button("内存布局...", theme::TEXT_DIM))
                    .on_hover_text(memory_hint)
                    .clicked()
                {
                    actions.push(Action::EditMemoryLayout);
                }
            });
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
                    .fill(theme::GUTTER)
                    .inner_margin(Margin::same(4)),
            )
            .show_inside(ui, |ui| {
                ui.painter().rect(
                    ui.max_rect(),
                    CornerRadius::same(8),
                    theme::EDITOR_BG,
                    Stroke::NONE,
                    egui::StrokeKind::Inside,
                );
                if self.tabs.is_empty() {
                    let hint = if self.project.is_some() {
                        "从左侧资源管理器打开文件开始编辑"
                    } else {
                        "请在左上角选择工程并开始"
                    };
                    let rect = ui.max_rect();
                    self.last_editor_rect = Some(rect);
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
                let editor_rect = ui.available_rect_before_wrap();
                self.last_editor_rect = Some(editor_rect);
                let active_index = self.active.unwrap_or(0);
                let primary_index = self
                    .split_primary
                    .as_ref()
                    .and_then(|path| self.tabs.iter().position(|doc| same_path(&doc.path, path)))
                    .or(Some(active_index));
                let secondary_index = self
                    .split_path
                    .as_ref()
                    .and_then(|path| self.tabs.iter().position(|doc| same_path(&doc.path, path)));
                if let (Some(primary), Some(secondary)) = (primary_index, secondary_index) {
                    if primary != secondary {
                        match self.split_direction {
                            SplitDirection::Right | SplitDirection::Left => {
                                ui.columns(2, |columns| {
                                    self.show_code_editor(
                                        &mut columns[0],
                                        primary,
                                        Some(0),
                                        actions,
                                    );
                                    self.show_code_editor(
                                        &mut columns[1],
                                        secondary,
                                        Some(1),
                                        actions,
                                    );
                                });
                            }
                            SplitDirection::Up | SplitDirection::Down => {
                                let available = ui.available_height();
                                let top =
                                    (available * 0.5).clamp(120.0, (available - 80.0).max(120.0));
                                ui.allocate_ui_with_layout(
                                    egui::vec2(ui.available_width(), top),
                                    Layout::top_down(Align::Min),
                                    |ui| {
                                        self.show_code_editor(ui, primary, Some(0), actions);
                                    },
                                );
                                let (gap_rect, _) = ui.allocate_exact_size(
                                    egui::vec2(ui.available_width(), 4.0),
                                    Sense::hover(),
                                );
                                ui.painter().rect_filled(
                                    gap_rect,
                                    CornerRadius::ZERO,
                                    theme::GUTTER,
                                );
                                self.show_code_editor(ui, secondary, Some(1), actions);
                            }
                        }
                    } else {
                        self.split_path = None;
                        self.split_primary = None;
                        self.show_code_editor(ui, active_index, None, actions);
                    }
                } else {
                    if self.split_path.is_some() || self.split_primary.is_some() {
                        self.split_path = None;
                        self.split_primary = None;
                    }
                    self.show_code_editor(ui, active_index, None, actions);
                }
                let ctx = ui.ctx().clone();
                if self.find.open {
                    self.show_find_overlay(&ctx, editor_rect);
                }
                if self.goto_line.open {
                    self.show_goto_overlay(&ctx, editor_rect);
                }
                paint_bottom_corners(ui, editor_rect, 8.0, theme::GUTTER);
            });
    }

    fn show_goto_overlay(&mut self, ctx: &egui::Context, editor_rect: egui::Rect) {
        let line_count = self
            .active_document()
            .map(|doc| doc.buffer.as_string().split('\n').count().max(1))
            .unwrap_or(1);
        let width = 420.0_f32.min((editor_rect.width() - 24.0).max(280.0));
        let position = egui::pos2(editor_rect.left() + 48.0, editor_rect.top() + 8.0);
        let mut apply = false;
        egui::Area::new(Id::new("simple_embedded_goto_line"))
            .order(egui::Order::Foreground)
            .fixed_pos(position)
            .show(ctx, |ui| {
                Frame::new()
                    .fill(theme::PANEL_RAISED)
                    .stroke(Stroke::new(1.0_f32, theme::BORDER))
                    .corner_radius(6)
                    .shadow(egui::epaint::Shadow {
                        offset: [0, 6],
                        blur: 14,
                        spread: 0,
                        color: Color32::from_black_alpha(70),
                    })
                    .inner_margin(Margin::symmetric(10, 5))
                    .show(ui, |ui| {
                        ui.set_width(width - 20.0);
                        let (response, _) = find_text_field(
                            ui,
                            Id::new("simple_embedded_goto_line_input"),
                            &mut self.goto_line.input,
                            "行号",
                            width - 20.0,
                            4.0,
                        );
                        if self.goto_line.focus_input {
                            response.request_focus();
                            self.goto_line.focus_input = false;
                        }
                        let enter = ui.input(|input| input.key_pressed(egui::Key::Enter));
                        if enter && (response.has_focus() || response.lost_focus()) {
                            apply = true;
                        }
                        ui.add_space(4.0);
                        if let Some(error) = &self.goto_line.error {
                            ui.label(RichText::new(error).size(10.5).color(theme::ERROR));
                        } else {
                            ui.label(
                                RichText::new(format!(
                                    "键入要跳转的行号（从 1 到 {line_count}）。"
                                ))
                                .size(10.5)
                                .color(theme::TEXT_FAINT),
                            );
                        }
                    });
            });
        if apply {
            self.goto_line_apply(ctx);
        }
    }

    fn goto_line_apply(&mut self, ctx: &egui::Context) {
        let Some(doc) = self.active_document() else {
            return;
        };
        let text = doc.buffer.as_string().to_owned();
        let line_count = text.split('\n').count().max(1);
        let input = self.goto_line.input.trim().to_owned();
        let Ok(line) = input.parse::<usize>() else {
            self.goto_line.error = Some("请输入数字行号".to_owned());
            self.goto_line.focus_input = true;
            return;
        };
        if line == 0 || line > line_count {
            self.goto_line.error = Some(format!("行号必须在 1 到 {line_count} 之间"));
            self.goto_line.focus_input = true;
            return;
        }
        let char_index = crate::util::char_index_at_line(&text, line);
        let Some(doc) = self.tabs.get_mut(self.active.unwrap_or(0)) else {
            return;
        };
        doc.update_cursor(char_index);
        set_editor_cursor(ctx, &doc.path, char_index);
        self.goto_line.open = false;
        self.goto_line.error = None;
        self.focus_editor = true;
        self.scroll_to_cursor = true;
    }

    fn show_find_overlay(&mut self, ctx: &egui::Context, editor_rect: egui::Rect) {
        let width = 420.0_f32.min((editor_rect.width() - 24.0).max(280.0));
        let position = egui::pos2(editor_rect.right() - width - 12.0, editor_rect.top() + 8.0);
        egui::Area::new(Id::new("simple_embedded_find_overlay"))
            .order(egui::Order::Foreground)
            .fixed_pos(position)
            .show(ctx, |ui| {
                Frame::new()
                    .fill(theme::PANEL_RAISED)
                    .stroke(Stroke::new(1.0_f32, theme::BORDER))
                    .corner_radius(6)
                    .shadow(egui::epaint::Shadow {
                        offset: [0, 6],
                        blur: 14,
                        spread: 0,
                        color: Color32::from_black_alpha(70),
                    })
                    .inner_margin(Margin::symmetric(10, 5))
                    .show(ui, |ui| {
                        ui.set_width(width - 20.0);
                        self.show_find_bar(ui);
                    });
            });
    }

    fn show_tab_strip(&mut self, ui: &mut egui::Ui, actions: &mut Vec<Action>) {
        Frame::new()
            .fill(theme::PANEL)
            .corner_radius(CornerRadius {
                nw: 8,
                ne: 8,
                sw: 0,
                se: 0,
            })
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
                                let tab_path = doc.path.clone();
                                let path_text = util::display_path(&tab_path);
                                let relative_text =
                                    doc.rel_path.replace('/', std::path::MAIN_SEPARATOR_STR);
                                let breadcrumb_text = doc.rel_path.replace('/', " > ");
                                let split_active = self.split_primary.is_some();
                                response.context_menu(|ui| {
                                    ui.set_min_width(264.0);
                                    if tab_menu_item(ui, "关闭", "Ctrl+F4") {
                                        actions.push(Action::CloseTab(index));
                                        ui.close();
                                    }
                                    if tab_menu_item(ui, "关闭其他", "") {
                                        actions.push(Action::CloseOtherTabs(index));
                                        ui.close();
                                    }
                                    if tab_menu_item(ui, "关闭右侧标签页", "") {
                                        actions.push(Action::CloseTabsToRight(index));
                                        ui.close();
                                    }
                                    if tab_menu_item(ui, "关闭已保存", "Ctrl+K U") {
                                        actions.push(Action::CloseSavedTabs);
                                        ui.close();
                                    }
                                    if tab_menu_item(ui, "全部关闭", "Ctrl+K W") {
                                        actions.push(Action::CloseAllTabs);
                                        ui.close();
                                    }
                                    ui.separator();
                                    if tab_menu_item(ui, "保存", "Ctrl+S") {
                                        actions.push(Action::SelectTab(index));
                                        actions.push(Action::SaveActive);
                                        ui.close();
                                    }
                                    if tab_menu_item(ui, "另存为...", "Ctrl+Shift+S") {
                                        actions.push(Action::SaveAs(index));
                                        ui.close();
                                    }
                                    ui.separator();
                                    if tab_menu_item(ui, "复制路径", "Shift+Alt+C") {
                                        actions.push(Action::CopyText(path_text.clone()));
                                        ui.close();
                                    }
                                    if tab_menu_item(ui, "复制相对路径", "Ctrl+K Ctrl+Shift+C")
                                    {
                                        actions.push(Action::CopyText(relative_text.clone()));
                                        ui.close();
                                    }
                                    if tab_menu_item(ui, "复制痕迹导航路径", "") {
                                        actions.push(Action::CopyText(breadcrumb_text.clone()));
                                        ui.close();
                                    }
                                    ui.separator();
                                    if tab_menu_item(ui, "在文件资源管理器中显示", "Shift+Alt+R")
                                    {
                                        actions.push(Action::Reveal(tab_path.clone()));
                                        ui.close();
                                    }
                                    if tab_menu_item(ui, "在资源管理器视图中显示", "") {
                                        actions.push(Action::RevealInTree(tab_path.clone()));
                                        ui.close();
                                    }
                                    ui.separator();
                                    if tab_menu_item(ui, "向右拆分", "Ctrl+\\") {
                                        actions.push(Action::SplitTab(
                                            tab_path.clone(),
                                            SplitDirection::Right,
                                        ));
                                        ui.close();
                                    }
                                    let split_menu =
                                        egui::containers::menu::SubMenuButton::from_button(
                                            Button::new(
                                                RichText::new("拆分和移动")
                                                    .size(11.5)
                                                    .color(theme::TEXT),
                                            )
                                            .right_text(
                                                egui::containers::menu::SubMenuButton::RIGHT_ARROW,
                                            )
                                            .frame(true)
                                            .min_size(egui::vec2(264.0, 24.0)),
                                        );
                                    split_menu.ui(ui, |ui| {
                                        ui.set_min_width(264.0);
                                        if tab_menu_item(ui, "向上拆分", "Ctrl+K Ctrl+\\") {
                                            actions.push(Action::SplitTab(
                                                tab_path.clone(),
                                                SplitDirection::Up,
                                            ));
                                            ui.close();
                                        }
                                        if tab_menu_item(ui, "向下拆分", "") {
                                            actions.push(Action::SplitTab(
                                                tab_path.clone(),
                                                SplitDirection::Down,
                                            ));
                                            ui.close();
                                        }
                                        if tab_menu_item(ui, "向左拆分", "") {
                                            actions.push(Action::SplitTab(
                                                tab_path.clone(),
                                                SplitDirection::Left,
                                            ));
                                            ui.close();
                                        }
                                        if tab_menu_item(ui, "向右拆分", "Ctrl+\\") {
                                            actions.push(Action::SplitTab(
                                                tab_path.clone(),
                                                SplitDirection::Right,
                                            ));
                                            ui.close();
                                        }
                                        ui.separator();
                                        if tab_menu_item(ui, "上移", "") {
                                            actions.push(Action::MoveSplit(SplitDirection::Up));
                                            ui.close();
                                        }
                                        if tab_menu_item(ui, "下移", "") {
                                            actions.push(Action::MoveSplit(SplitDirection::Down));
                                            ui.close();
                                        }
                                        if tab_menu_item(ui, "左移", "") {
                                            actions.push(Action::MoveSplit(SplitDirection::Left));
                                            ui.close();
                                        }
                                        if tab_menu_item(ui, "右移", "") {
                                            actions.push(Action::MoveSplit(SplitDirection::Right));
                                            ui.close();
                                        }
                                        ui.separator();
                                        if tab_menu_item(ui, "在组中拆分", "Ctrl+K Ctrl+Shift+\\")
                                        {
                                            actions.push(Action::SplitGroup);
                                            ui.close();
                                        }
                                    });
                                    if split_active && tab_menu_item(ui, "取消拆分", "") {
                                        actions.push(Action::CloseSplit);
                                        ui.close();
                                    }
                                });
                                response.on_hover_text(util::display_path(&doc.path));
                            }
                        });
                    });
            });
    }

    fn show_editor_toolbar(&mut self, ui: &mut egui::Ui, _actions: &mut Vec<Action>) {
        let Some(active) = self.active else {
            return;
        };
        let Some(doc) = self.tabs.get(active) else {
            return;
        };
        let breadcrumb = doc.rel_path.replace('/', " > ");
        let full_path = util::display_path(&doc.path);
        let dirty = doc.is_dirty();

        Frame::new()
            .fill(theme::EDITOR_BG)
            .inner_margin(Margin::symmetric(12, 4))
            .show(ui, |ui| {
                ui.spacing_mut().interact_size.y = 24.0;
                ui.horizontal(|ui| {
                    let response = ui.add(
                        Label::new(
                            RichText::new(&breadcrumb)
                                .size(12.0)
                                .color(theme::TEXT_FAINT),
                        )
                        .truncate(),
                    );
                    response.on_hover_text(&full_path);
                    if dirty {
                        ui.label(RichText::new("● 未保存").size(11.0).color(theme::WARNING));
                    }
                });
            });
    }

    fn show_find_bar(&mut self, ui: &mut egui::Ui) {
        let mut next = false;
        let mut previous = false;
        let mut replace_current = false;
        let mut replace_all = false;

        let (alt_case, alt_word, alt_regex, alt_preserve) = ui.input_mut(|input| {
            let case = input.modifiers.alt && input.key_pressed(egui::Key::C);
            let word = input.modifiers.alt && input.key_pressed(egui::Key::W);
            let regex = input.modifiers.alt && input.key_pressed(egui::Key::R);
            let preserve = input.modifiers.alt && input.key_pressed(egui::Key::P);
            if case {
                input.consume_key(egui::Modifiers::ALT, egui::Key::C);
            }
            if word {
                input.consume_key(egui::Modifiers::ALT, egui::Key::W);
            }
            if regex {
                input.consume_key(egui::Modifiers::ALT, egui::Key::R);
            }
            if preserve {
                input.consume_key(egui::Modifiers::ALT, egui::Key::P);
            }
            (case, word, regex, preserve)
        });
        if alt_case {
            self.find.case_sensitive = !self.find.case_sensitive;
            self.find.dirty = true;
        }
        if alt_word {
            self.find.whole_word = !self.find.whole_word;
            self.find.dirty = true;
        }
        if alt_regex {
            self.find.use_regex = !self.find.use_regex;
            self.find.dirty = true;
        }
        if alt_preserve {
            self.find.preserve_case = !self.find.preserve_case;
        }

        if let Some(active) = self.active {
            if let Some(doc) = self.tabs.get(active) {
                self.find
                    .ensure_matches(doc.buffer.as_string(), doc.buffer.revision());
            }
        }
        let error_text = self.find.error.clone();
        let status = if error_text.is_some() {
            "!".to_owned()
        } else if self.find.query.is_empty() {
            String::new()
        } else if self.find.matches.is_empty() {
            "0/0".to_owned()
        } else {
            format!("{}/{}", self.find.active + 1, self.find.matches.len())
        };
        let status_color = if error_text.is_some() {
            theme::ERROR
        } else {
            theme::TEXT_FAINT
        };

        const ROW_HEIGHT: f32 = 24.0;
        const ROW_GAP: f32 = 4.0;
        const CHEVRON_WIDTH: f32 = 16.0;
        const CHEVRON_GAP: f32 = 4.0;
        const ICONS_WIDTH: f32 = 6.0 + 3.0 * 22.0 + 6.0 + 2.0 * 22.0 + 6.0 + 22.0 + 2.0 + 22.0;
        let content_width = ui.available_width();
        let right_width = (content_width - CHEVRON_WIDTH - CHEVRON_GAP).max(80.0);
        let input_width = (right_width - ICONS_WIDTH).max(70.0);
        let content_height = if self.find.replace_open {
            ROW_HEIGHT * 2.0 + ROW_GAP
        } else {
            ROW_HEIGHT
        };

        ui.spacing_mut().item_spacing = egui::vec2(0.0, 0.0);
        ui.horizontal(|ui| {
            let (chevron_rect, chevron_response) =
                ui.allocate_exact_size(egui::vec2(CHEVRON_WIDTH, content_height), Sense::click());
            let chevron = chevron_response
                .on_hover_cursor(egui::CursorIcon::PointingHand)
                .on_hover_text(if self.find.replace_open {
                    "收起替换"
                } else {
                    "展开替换"
                });
            if chevron.hovered() {
                ui.painter().rect_filled(
                    egui::Rect::from_center_size(chevron_rect.center(), egui::vec2(18.0, 22.0)),
                    CornerRadius::same(3),
                    theme::SURFACE,
                );
            }
            let chevron_color = if chevron.hovered() {
                theme::TEXT
            } else {
                theme::TEXT_DIM
            };
            let center = chevron_rect.center();
            let chevron_stroke = Stroke::new(1.3_f32, chevron_color);
            if self.find.replace_open {
                ui.painter().line_segment(
                    [
                        egui::pos2(center.x - 4.5, center.y - 1.5),
                        egui::pos2(center.x, center.y + 2.5),
                    ],
                    chevron_stroke,
                );
                ui.painter().line_segment(
                    [
                        egui::pos2(center.x, center.y + 2.5),
                        egui::pos2(center.x + 4.5, center.y - 1.5),
                    ],
                    chevron_stroke,
                );
            } else {
                ui.painter().line_segment(
                    [
                        egui::pos2(center.x - 2.5, center.y - 5.0),
                        egui::pos2(center.x + 2.5, center.y),
                    ],
                    chevron_stroke,
                );
                ui.painter().line_segment(
                    [
                        egui::pos2(center.x + 2.5, center.y),
                        egui::pos2(center.x - 2.5, center.y + 5.0),
                    ],
                    chevron_stroke,
                );
            }
            if chevron.clicked() {
                self.find.replace_open = !self.find.replace_open;
            }
            ui.add_space(CHEVRON_GAP);

            ui.vertical(|ui| {
                ui.spacing_mut().item_spacing = egui::vec2(0.0, ROW_GAP);
                ui.horizontal(|ui| {
                    ui.spacing_mut().item_spacing.x = 0.0;
                    let (query, query_rect) = find_text_field(
                        ui,
                        Id::new("simple_embedded_find_query"),
                        &mut self.find.query,
                        "查找",
                        input_width,
                        if status.is_empty() { 4.0 } else { 46.0 },
                    );
                    if self.find.focus_query {
                        query.request_focus();
                        self.find.focus_query = false;
                    }
                    if query.changed() {
                        self.find.dirty = true;
                    }
                    if query.lost_focus() && ui.input(|input| input.key_pressed(egui::Key::Enter)) {
                        if ui.input(|input| input.modifiers.shift) {
                            previous = true;
                        } else {
                            next = true;
                        }
                    }
                    if !status.is_empty() {
                        ui.painter().text(
                            egui::pos2(query_rect.right() - 8.0, query_rect.center().y),
                            Align2::RIGHT_CENTER,
                            &status,
                            FontId::proportional(10.0),
                            status_color,
                        );
                    }
                    if let Some(error) = &error_text {
                        let _ = query.on_hover_text(error);
                    }
                    ui.add_space(6.0);
                    if find_toggle(ui, "Aa", self.find.case_sensitive, "区分大小写 (Alt+C)") {
                        self.find.case_sensitive = !self.find.case_sensitive;
                        self.find.dirty = true;
                    }
                    if find_toggle(ui, "ab", self.find.whole_word, "全字匹配 (Alt+W)") {
                        self.find.whole_word = !self.find.whole_word;
                        self.find.dirty = true;
                    }
                    if find_toggle(ui, ".*", self.find.use_regex, "使用正则表达式 (Alt+R)") {
                        self.find.use_regex = !self.find.use_regex;
                        self.find.dirty = true;
                    }
                    ui.add_space(6.0);
                    if find_icon_button(ui, FindGlyph::Previous, "上一个匹配 (Shift+Enter)") {
                        previous = true;
                    }
                    if find_icon_button(ui, FindGlyph::Next, "下一个匹配 (Enter)") {
                        next = true;
                    }
                    ui.add_space(6.0);
                    if find_icon_button(ui, FindGlyph::Menu, "全工程搜索 (Ctrl+Shift+F)") {
                        self.project_search.open = true;
                        self.project_search.focus_query = true;
                    }
                    ui.add_space(2.0);
                    if find_icon_button(ui, FindGlyph::Close, "关闭查找 (Esc)") {
                        self.find.open = false;
                        self.focus_editor = true;
                    }
                });

                if self.find.replace_open {
                    ui.horizontal(|ui| {
                        ui.spacing_mut().item_spacing.x = 0.0;
                        let (replace, replace_rect) = find_text_field(
                            ui,
                            Id::new("simple_embedded_find_replace"),
                            &mut self.find.replacement,
                            "替换",
                            input_width,
                            28.0,
                        );
                        if replace.lost_focus()
                            && ui.input(|input| input.key_pressed(egui::Key::Enter))
                        {
                            replace_current = true;
                        }
                        let ab_rect = egui::Rect::from_min_max(
                            egui::pos2(replace_rect.right() - 26.0, replace_rect.top() + 2.0),
                            egui::pos2(replace_rect.right() - 3.0, replace_rect.bottom() - 2.0),
                        );
                        let ab = ui
                            .interact(
                                ab_rect,
                                Id::new("simple_embedded_find_preserve_case"),
                                Sense::click(),
                            )
                            .on_hover_cursor(egui::CursorIcon::PointingHand)
                            .on_hover_text("保留大小写 (Alt+P)");
                        if ab.hovered() {
                            ui.painter()
                                .rect_filled(ab_rect, CornerRadius::same(3), theme::HOVER);
                        }
                        ui.painter().text(
                            ab_rect.center(),
                            Align2::CENTER_CENTER,
                            "AB",
                            FontId::proportional(10.5),
                            if self.find.preserve_case {
                                theme::ACCENT
                            } else {
                                theme::TEXT_DIM
                            },
                        );
                        if ab.clicked() {
                            self.find.preserve_case = !self.find.preserve_case;
                        }
                        ui.add_space(6.0);
                        if find_icon_button(ui, FindGlyph::Replace, "替换当前匹配") {
                            replace_current = true;
                        }
                        ui.add_space(2.0);
                        if find_icon_button(ui, FindGlyph::ReplaceAll, "全部替换") {
                            replace_all = true;
                        }
                    });
                }
            });
        });

        if next {
            self.find_next(ui.ctx(), true);
        }
        if previous {
            self.find_next(ui.ctx(), false);
        }
        if replace_current {
            self.replace_current_match(ui.ctx());
        }
        if replace_all {
            self.replace_all_matches(ui.ctx());
        }
    }

    pub(super) fn show_project_search(&mut self, ctx: &egui::Context, actions: &mut Vec<Action>) {
        let root = self.project.as_ref().map(|project| project.root.clone());
        let mut open = self.project_search.open;
        let mut run = false;
        let mut clicked = None;
        egui::Window::new("全工程搜索")
            .id(Id::new("simple_embedded_project_search"))
            .title_bar(false)
            .collapsible(false)
            .resizable(true)
            .default_size([760.0, 480.0])
            .min_height(420.0)
            .frame(
                Frame::new()
                    .fill(theme::PANEL)
                    .stroke(Stroke::new(1.0_f32, theme::BORDER))
                    .inner_margin(Margin::same(12))
                    .corner_radius(6),
            )
            .show(ctx, |ui| {
                ui.set_min_width(720.0);
                ui.set_min_height(380.0);
                ui.visuals_mut().override_text_color = Some(theme::TEXT);
                ui.horizontal(|ui| {
                    ui.label(
                        RichText::new("全工程搜索")
                            .size(16.0)
                            .strong()
                            .color(theme::TEXT),
                    );
                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        if window_close_button(ui) {
                            open = false;
                        }
                    });
                });
                ui.add_space(4.0);
                ui.horizontal(|ui| {
                    let query = ui.add(
                        TextEdit::singleline(&mut self.project_search.query)
                            .id(Id::new("simple_embedded_project_search_query"))
                            .font(FontId::monospace(12.0))
                            .text_color(theme::TEXT)
                            .background_color(theme::BG)
                            .desired_width(300.0)
                            .hint_text(RichText::new("搜索内容").color(theme::TEXT_FAINT)),
                    );
                    if self.project_search.focus_query {
                        query.request_focus();
                        self.project_search.focus_query = false;
                    }
                    if query.lost_focus() && ui.input(|input| input.key_pressed(egui::Key::Enter)) {
                        run = true;
                    }
                    if ui.add(theme::primary_button("搜索")).clicked() {
                        run = true;
                    }
                    if ui
                        .selectable_label(
                            self.project_search.case_sensitive,
                            RichText::new("Aa").size(11.0).color(theme::TEXT_DIM),
                        )
                        .on_hover_text("区分大小写")
                        .clicked()
                    {
                        self.project_search.case_sensitive = !self.project_search.case_sensitive;
                    }
                    if ui
                        .selectable_label(
                            self.project_search.use_regex,
                            RichText::new(".*").size(11.0).color(theme::TEXT_DIM),
                        )
                        .on_hover_text("正则表达式")
                        .clicked()
                    {
                        self.project_search.use_regex = !self.project_search.use_regex;
                    }
                });
                ui.add_space(4.0);
                ui.horizontal(|ui| {
                    if let Some(error) = &self.project_search.error {
                        ui.label(RichText::new(error).size(11.0).color(theme::ERROR));
                    } else if self.project_search.searched {
                        ui.label(
                            RichText::new(format!(
                                "{} 个结果 · 扫描 {} 个文件",
                                self.project_search.results.len(),
                                self.project_search.scanned_files
                            ))
                            .size(11.0)
                            .color(theme::TEXT_FAINT),
                        );
                        if self.project_search.truncated {
                            ui.label(
                                RichText::new("结果过多，已截断")
                                    .size(11.0)
                                    .color(theme::WARNING),
                            );
                        }
                    } else {
                        ui.label(
                            RichText::new("输入内容后按 Enter 或点击搜索")
                                .size(11.0)
                                .color(theme::TEXT_FAINT),
                        );
                    }
                });
                ui.separator();
                if self.project_search.results.is_empty() {
                    if self.project_search.searched && self.project_search.error.is_none() {
                        ui.label(
                            RichText::new("没有匹配项")
                                .size(11.5)
                                .color(theme::TEXT_FAINT),
                        );
                    }
                } else {
                    ScrollArea::vertical()
                        .id_salt("simple_embedded_project_search_results")
                        .max_height(380.0)
                        .auto_shrink([false, false])
                        .show(ui, |ui| {
                            for hit in &self.project_search.results {
                                let path = root
                                    .as_ref()
                                    .map(|root| util::relative_display(root, &hit.path))
                                    .unwrap_or_else(|| util::display_path(&hit.path));
                                let preview = hit
                                    .preview
                                    .replace('\t', "    ")
                                    .chars()
                                    .take(160)
                                    .collect::<String>();
                                let label =
                                    format!("{}:{}:{}    {}", path, hit.line, hit.column, preview);
                                let response = ui.selectable_label(
                                    false,
                                    RichText::new(label).size(11.5).color(theme::TEXT_DIM),
                                );
                                if response.clicked() {
                                    clicked = Some(hit.clone());
                                }
                            }
                        });
                }
            });
        self.project_search.open = open;
        if run {
            self.run_project_search();
        }
        if let Some(hit) = clicked {
            actions.push(Action::OpenAt(
                hit.path.to_string_lossy().into_owned(),
                hit.line,
                hit.column,
            ));
        }
    }

    pub(super) fn show_git_status(&mut self, ctx: &egui::Context, actions: &mut Vec<Action>) {
        let root = self.project.as_ref().map(|project| project.root.clone());
        let mut open = self.git_open;
        let mut refresh = false;
        let mut clicked = None;
        let git = self.git.clone();
        egui::Window::new("Git 状态")
            .id(Id::new("simple_embedded_git_status"))
            .title_bar(false)
            .collapsible(false)
            .resizable(true)
            .default_size([640.0, 440.0])
            .min_height(360.0)
            .frame(
                Frame::new()
                    .fill(theme::PANEL)
                    .stroke(Stroke::new(1.0_f32, theme::BORDER))
                    .inner_margin(Margin::same(12))
                    .corner_radius(6),
            )
            .show(ctx, |ui| {
                ui.set_min_width(600.0);
                ui.set_min_height(340.0);
                ui.visuals_mut().override_text_color = Some(theme::TEXT);
                ui.horizontal(|ui| {
                    ui.label(
                        RichText::new("Git 状态")
                            .size(16.0)
                            .strong()
                            .color(theme::TEXT),
                    );
                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        if window_close_button(ui) {
                            open = false;
                        }
                        if ui.add(small_flat_button("刷新", theme::TEXT_DIM)).clicked() {
                            refresh = true;
                        }
                    });
                });
                ui.add_space(4.0);
                if !git.is_repo {
                    ui.label(
                        RichText::new(
                            git.error
                                .clone()
                                .unwrap_or_else(|| "当前工程不是 Git 仓库".to_owned()),
                        )
                        .size(11.5)
                        .color(theme::TEXT_FAINT),
                    );
                } else {
                    ui.horizontal(|ui| {
                        if !git.branch.is_empty() {
                            ui.label(
                                RichText::new(format!("分支 {}", git.branch))
                                    .size(11.5)
                                    .color(theme::ACCENT),
                            );
                        }
                        ui.label(
                            RichText::new(format!("{} 个变更", git.files.len()))
                                .size(11.5)
                                .color(theme::TEXT_FAINT),
                        );
                    });
                    ui.separator();
                    if git.files.is_empty() {
                        ui.label(
                            RichText::new("工作区干净")
                                .size(11.5)
                                .color(theme::TEXT_FAINT),
                        );
                    } else {
                        ScrollArea::vertical()
                            .id_salt("simple_embedded_git_files")
                            .max_height(360.0)
                            .auto_shrink([false, false])
                            .show(ui, |ui| {
                                for file in &git.files {
                                    let relative = root
                                        .as_ref()
                                        .map(|root| util::relative_display(root, &file.path))
                                        .unwrap_or_else(|| util::display_path(&file.path));
                                    let badge =
                                        crate::git::badge_for_code(&file.code).unwrap_or('?');
                                    let color = crate::git::badge_color(badge);
                                    let label = format!("{}  {}", badge, relative);
                                    if ui
                                        .selectable_label(
                                            false,
                                            RichText::new(label)
                                                .size(11.5)
                                                .monospace()
                                                .color(color),
                                        )
                                        .on_hover_text(format!(
                                            "{}  {}",
                                            file.code.trim(),
                                            util::display_path(&file.path)
                                        ))
                                        .clicked()
                                    {
                                        clicked = Some(file.path.clone());
                                    }
                                }
                            });
                    }
                }
            });
        self.git_open = open;
        if refresh {
            self.refresh_git_status();
        }
        if let Some(path) = clicked {
            actions.push(Action::OpenAt(path.to_string_lossy().into_owned(), 1, 1));
        }
    }

    pub(super) fn show_todos(&mut self, ctx: &egui::Context, actions: &mut Vec<Action>) {
        use crate::todo::TodoKind;

        let root = self.project.as_ref().map(|project| project.root.clone());
        let mut open = self.todo.open;
        let mut refresh = false;
        let mut clicked = None;
        let filtered = self
            .todo
            .items
            .iter()
            .filter(|item| self.todo.filter.is_none_or(|filter| filter == item.kind))
            .cloned()
            .collect::<Vec<_>>();
        egui::Window::new("TODO")
            .id(Id::new("simple_embedded_todo"))
            .title_bar(false)
            .collapsible(false)
            .resizable(true)
            .default_size([700.0, 460.0])
            .min_height(360.0)
            .frame(
                Frame::new()
                    .fill(theme::PANEL)
                    .stroke(Stroke::new(1.0_f32, theme::BORDER))
                    .inner_margin(Margin::same(12))
                    .corner_radius(6),
            )
            .show(ctx, |ui| {
                ui.set_min_width(660.0);
                ui.set_min_height(330.0);
                ui.visuals_mut().override_text_color = Some(theme::TEXT);
                ui.horizontal(|ui| {
                    ui.label(
                        RichText::new("TODO / 注释")
                            .size(16.0)
                            .strong()
                            .color(theme::TEXT),
                    );
                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        if window_close_button(ui) {
                            open = false;
                        }
                        if ui.add(small_flat_button("刷新", theme::TEXT_DIM)).clicked() {
                            refresh = true;
                        }
                    });
                });
                ui.add_space(4.0);
                ui.horizontal(|ui| {
                    ui.label(
                        RichText::new(format!("{} 条", filtered.len()))
                            .size(11.0)
                            .color(theme::TEXT_FAINT),
                    );
                    for (value, label) in [
                        (None, "全部"),
                        (Some(TodoKind::Todo), "TODO"),
                        (Some(TodoKind::Fixme), "FIXME"),
                        (Some(TodoKind::Hack), "HACK"),
                        (Some(TodoKind::Note), "NOTE"),
                    ] {
                        let active = self.todo.filter == value;
                        let color = if active {
                            value.map_or(theme::ACCENT, TodoKind::color)
                        } else {
                            theme::TEXT_FAINT
                        };
                        if ui
                            .selectable_label(active, RichText::new(label).size(11.0).color(color))
                            .clicked()
                        {
                            self.todo.filter = value;
                        }
                    }
                    if self.todo.truncated {
                        ui.label(
                            RichText::new("结果过多，已截断")
                                .size(11.0)
                                .color(theme::WARNING),
                        );
                    }
                });
                ui.separator();
                if let Some(error) = &self.todo.error {
                    ui.label(RichText::new(error).size(11.5).color(theme::ERROR));
                } else if filtered.is_empty() {
                    ui.label(
                        RichText::new("没有找到 TODO / FIXME 注释")
                            .size(11.5)
                            .color(theme::TEXT_FAINT),
                    );
                } else {
                    ScrollArea::vertical()
                        .id_salt("simple_embedded_todo_list")
                        .max_height(380.0)
                        .auto_shrink([false, false])
                        .show(ui, |ui| {
                            for item in &filtered {
                                let path = root
                                    .as_ref()
                                    .map(|root| util::relative_display(root, &item.path))
                                    .unwrap_or_else(|| util::display_path(&item.path));
                                let text = item.text.chars().take(180).collect::<String>();
                                let label = format!(
                                    "{:<5} {}:{}  {}",
                                    item.kind.label(),
                                    path,
                                    item.line,
                                    text
                                );
                                if ui
                                    .selectable_label(
                                        false,
                                        RichText::new(label)
                                            .size(11.5)
                                            .monospace()
                                            .color(item.kind.color()),
                                    )
                                    .clicked()
                                {
                                    clicked = Some(item.clone());
                                }
                            }
                        });
                }
            });
        self.todo.open = open;
        if refresh {
            self.refresh_todos();
        }
        if let Some(item) = clicked {
            actions.push(Action::OpenAt(
                item.path.to_string_lossy().into_owned(),
                item.line,
                1,
            ));
        }
    }

    pub(super) fn show_symbols(&mut self, ctx: &egui::Context, actions: &mut Vec<Action>) {
        let root = self.project.as_ref().map(|project| project.root.clone());
        let mut open = self.symbols.open;
        let mut refresh = false;
        let mut clicked = None;
        let query = self.symbols.query.trim().to_lowercase();
        let filtered = self
            .symbols
            .items
            .iter()
            .filter(|symbol| {
                query.is_empty()
                    || symbol.name.to_lowercase().contains(&query)
                    || symbol
                        .path
                        .to_string_lossy()
                        .to_lowercase()
                        .contains(&query)
            })
            .cloned()
            .collect::<Vec<_>>();
        egui::Window::new("符号搜索")
            .id(Id::new("simple_embedded_symbols"))
            .title_bar(false)
            .collapsible(false)
            .resizable(true)
            .default_size([700.0, 460.0])
            .min_height(360.0)
            .frame(
                Frame::new()
                    .fill(theme::PANEL)
                    .stroke(Stroke::new(1.0_f32, theme::BORDER))
                    .inner_margin(Margin::same(12))
                    .corner_radius(6),
            )
            .show(ctx, |ui| {
                ui.set_min_width(660.0);
                ui.set_min_height(330.0);
                ui.visuals_mut().override_text_color = Some(theme::TEXT);
                ui.horizontal(|ui| {
                    ui.label(
                        RichText::new("符号搜索")
                            .size(16.0)
                            .strong()
                            .color(theme::TEXT),
                    );
                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        if window_close_button(ui) {
                            open = false;
                        }
                        if ui.add(small_flat_button("刷新", theme::TEXT_DIM)).clicked() {
                            refresh = true;
                        }
                    });
                });
                ui.add_space(4.0);
                ui.horizontal(|ui| {
                    ui.add(
                        TextEdit::singleline(&mut self.symbols.query)
                            .id(Id::new("simple_embedded_symbol_filter"))
                            .font(FontId::monospace(12.0))
                            .text_color(theme::TEXT)
                            .background_color(theme::BG)
                            .desired_width(300.0)
                            .hint_text(RichText::new("按名称或路径过滤").color(theme::TEXT_FAINT)),
                    );
                    ui.label(
                        RichText::new(format!("{} 个符号", filtered.len()))
                            .size(11.0)
                            .color(theme::TEXT_FAINT),
                    );
                    if self.symbols.truncated {
                        ui.label(
                            RichText::new("索引过多，已截断")
                                .size(11.0)
                                .color(theme::WARNING),
                        );
                    }
                });
                ui.separator();
                if let Some(error) = &self.symbols.error {
                    ui.label(RichText::new(error).size(11.5).color(theme::ERROR));
                } else if filtered.is_empty() {
                    ui.label(
                        RichText::new("没有匹配的符号")
                            .size(11.5)
                            .color(theme::TEXT_FAINT),
                    );
                } else {
                    ScrollArea::vertical()
                        .id_salt("simple_embedded_symbol_list")
                        .max_height(380.0)
                        .auto_shrink([false, false])
                        .show_rows(ui, 20.0, filtered.len(), |ui, range| {
                            for index in range {
                                let symbol = &filtered[index];
                                let path = root
                                    .as_ref()
                                    .map(|root| util::relative_display(root, &symbol.path))
                                    .unwrap_or_else(|| util::display_path(&symbol.path));
                                let label = format!(
                                    "{:<6} {:<28} {}:{}",
                                    symbol.kind.label(),
                                    symbol.name,
                                    path,
                                    symbol.line
                                );
                                if ui
                                    .selectable_label(
                                        false,
                                        RichText::new(label)
                                            .size(11.5)
                                            .monospace()
                                            .color(symbol.kind.color()),
                                    )
                                    .clicked()
                                {
                                    clicked = Some(symbol.clone());
                                }
                            }
                        });
                }
            });
        self.symbols.open = open;
        if refresh {
            self.refresh_symbols();
        }
        if let Some(symbol) = clicked {
            actions.push(Action::OpenAt(
                symbol.path.to_string_lossy().into_owned(),
                symbol.line,
                symbol.column,
            ));
        }
    }

    pub(super) fn show_hex_viewer(&mut self, ctx: &egui::Context) {
        let mut open = self.hex.open;
        let mut to_array = false;
        let name = util::file_name(&self.hex.dump.path);
        let path = util::display_path(&self.hex.dump.path);
        let size = self.hex.dump.data.len();
        egui::Window::new("十六进制查看")
            .id(Id::new("simple_embedded_hex_viewer"))
            .title_bar(false)
            .collapsible(false)
            .resizable(true)
            .default_size([760.0, 480.0])
            .min_height(360.0)
            .frame(
                Frame::new()
                    .fill(theme::PANEL)
                    .stroke(Stroke::new(1.0_f32, theme::BORDER))
                    .inner_margin(Margin::same(12))
                    .corner_radius(6),
            )
            .show(ctx, |ui| {
                ui.set_min_width(720.0);
                ui.set_min_height(330.0);
                ui.visuals_mut().override_text_color = Some(theme::TEXT);
                ui.horizontal(|ui| {
                    ui.label(
                        RichText::new(format!("十六进制 · {name}"))
                            .size(16.0)
                            .strong()
                            .color(theme::TEXT),
                    );
                    ui.label(
                        RichText::new(format!("{size} 字节"))
                            .size(11.0)
                            .color(theme::TEXT_FAINT),
                    );
                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        if window_close_button(ui) {
                            open = false;
                        }
                        if ui
                            .add(small_flat_button("生成数组", theme::TEXT_DIM))
                            .on_hover_text("把这批字节生成 C 数组")
                            .clicked()
                        {
                            to_array = true;
                        }
                    });
                });
                ui.label(RichText::new(&path).size(10.5).color(theme::TEXT_FAINT));
                ui.separator();
                if let Some(error) = &self.hex.error {
                    ui.label(RichText::new(error).size(11.5).color(theme::ERROR));
                } else if size == 0 {
                    ui.label(RichText::new("空文件").size(11.5).color(theme::TEXT_FAINT));
                } else {
                    ui.spacing_mut().item_spacing.y = 0.0;
                    ScrollArea::vertical()
                        .id_salt("simple_embedded_hex_rows")
                        .auto_shrink([false, false])
                        .show_rows(ui, 18.0, self.hex.dump.row_count(), |ui, range| {
                            for index in range {
                                if let Some(row) = self.hex.dump.row(index) {
                                    ui.label(
                                        RichText::new(row)
                                            .monospace()
                                            .size(12.0)
                                            .color(theme::TEXT_DIM),
                                    );
                                }
                            }
                        });
                }
            });
        self.hex.open = open;
        if to_array {
            let mut bytes = self.hex.dump.data.clone();
            let truncated = bytes.len() > MAX_ARRAY_BYTES;
            bytes.truncate(MAX_ARRAY_BYTES);
            let name = self
                .hex
                .dump
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
                source_label: "十六进制视图".to_owned(),
                truncated,
            };
        }
    }

    pub(super) fn show_array_generator(&mut self, ctx: &egui::Context) {
        let mut open = self.array.open;
        let mut copied = false;
        let preview = crate::arraygen::generate(
            &self.array.source,
            &self.array.name,
            self.array.hex,
            self.array.per_line,
        );
        egui::Window::new("生成 C 数组")
            .id(Id::new("simple_embedded_array_generator"))
            .title_bar(false)
            .collapsible(false)
            .resizable(true)
            .default_size([780.0, 520.0])
            .min_height(420.0)
            .frame(
                Frame::new()
                    .fill(theme::PANEL)
                    .stroke(Stroke::new(1.0_f32, theme::BORDER))
                    .inner_margin(Margin::same(12))
                    .corner_radius(6),
            )
            .show(ctx, |ui| {
                ui.set_min_width(740.0);
                ui.set_min_height(390.0);
                ui.visuals_mut().override_text_color = Some(theme::TEXT);
                ui.horizontal(|ui| {
                    ui.label(
                        RichText::new("生成 C 数组")
                            .size(16.0)
                            .strong()
                            .color(theme::TEXT),
                    );
                    ui.label(
                        RichText::new(format!(
                            "{} · {} 字节",
                            self.array.source_label,
                            self.array.source.len()
                        ))
                        .size(11.0)
                        .color(theme::TEXT_FAINT),
                    );
                    if self.array.truncated {
                        ui.label(
                            RichText::new("已截断到 8 KB")
                                .size(11.0)
                                .color(theme::WARNING),
                        );
                    }
                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        if window_close_button(ui) {
                            open = false;
                        }
                    });
                });
                ui.add_space(4.0);
                ui.horizontal(|ui| {
                    ui.label(RichText::new("变量名").size(11.5).color(theme::TEXT_DIM));
                    ui.add(
                        TextEdit::singleline(&mut self.array.name)
                            .id(Id::new("simple_embedded_array_name"))
                            .font(FontId::monospace(12.0))
                            .text_color(theme::TEXT)
                            .background_color(theme::BG)
                            .desired_width(220.0),
                    );
                    ui.checkbox(&mut self.array.hex, "十六进制");
                    ui.add(egui::Slider::new(&mut self.array.per_line, 4..=32).text("每行字节"));
                    if ui.button("复制").clicked() {
                        ui.ctx().copy_text(preview.clone());
                        copied = true;
                    }
                });
                ui.separator();
                ScrollArea::both()
                    .id_salt("simple_embedded_array_preview")
                    .max_height(330.0)
                    .auto_shrink([false, false])
                    .show(ui, |ui| {
                        ui.label(
                            RichText::new(&preview)
                                .monospace()
                                .size(12.0)
                                .color(theme::TEXT_DIM),
                        );
                    });
            });
        self.array.open = open;
        if copied {
            self.set_toast("C 数组已复制到剪贴板", Severity::Success);
        }
    }

    pub(super) fn show_registers(&mut self, ctx: &egui::Context, actions: &mut Vec<Action>) {
        let mut open = self.registers.open;
        let mut refresh = false;
        let mut clicked = None;
        let query = self.registers.query.trim().to_lowercase();
        let filtered = self
            .registers
            .items
            .iter()
            .filter(|item| {
                query.is_empty()
                    || item.name.to_lowercase().contains(&query)
                    || item.description.to_lowercase().contains(&query)
                    || format!("{:08X}", item.address)
                        .to_lowercase()
                        .contains(&query)
            })
            .cloned()
            .collect::<Vec<_>>();
        egui::Window::new("寄存器")
            .id(Id::new("simple_embedded_registers"))
            .title_bar(false)
            .collapsible(false)
            .resizable(true)
            .default_size([780.0, 460.0])
            .min_height(360.0)
            .frame(
                Frame::new()
                    .fill(theme::PANEL)
                    .stroke(Stroke::new(1.0_f32, theme::BORDER))
                    .inner_margin(Margin::same(12))
                    .corner_radius(6),
            )
            .show(ctx, |ui| {
                ui.set_min_width(740.0);
                ui.set_min_height(330.0);
                ui.visuals_mut().override_text_color = Some(theme::TEXT);
                ui.horizontal(|ui| {
                    ui.label(
                        RichText::new("寄存器")
                            .size(16.0)
                            .strong()
                            .color(theme::TEXT),
                    );
                    if !self.registers.source.is_empty() {
                        ui.label(
                            RichText::new(&self.registers.source)
                                .size(11.0)
                                .color(theme::ACCENT),
                        );
                    }
                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        if window_close_button(ui) {
                            open = false;
                        }
                        if ui.add(small_flat_button("刷新", theme::TEXT_DIM)).clicked() {
                            refresh = true;
                        }
                    });
                });
                ui.add_space(4.0);
                ui.horizontal(|ui| {
                    ui.add(
                        TextEdit::singleline(&mut self.registers.query)
                            .id(Id::new("simple_embedded_register_filter"))
                            .font(FontId::monospace(12.0))
                            .text_color(theme::TEXT)
                            .background_color(theme::BG)
                            .desired_width(300.0)
                            .hint_text(
                                RichText::new("按名称、地址或说明过滤").color(theme::TEXT_FAINT),
                            ),
                    );
                    ui.label(
                        RichText::new(format!("{} 项", filtered.len()))
                            .size(11.0)
                            .color(theme::TEXT_FAINT),
                    );
                    if self.registers.truncated {
                        ui.label(
                            RichText::new("结果过多，已截断")
                                .size(11.0)
                                .color(theme::WARNING),
                        );
                    }
                });
                ui.separator();
                if let Some(error) = &self.registers.error {
                    ui.label(RichText::new(error).size(11.5).color(theme::ERROR));
                } else if filtered.is_empty() {
                    ui.label(
                        RichText::new("未找到 SVD 文件或 *_BASE / *_ADDR 寄存器宏")
                            .size(11.5)
                            .color(theme::TEXT_FAINT),
                    );
                } else {
                    ScrollArea::vertical()
                        .id_salt("simple_embedded_register_list")
                        .max_height(380.0)
                        .auto_shrink([false, false])
                        .show_rows(ui, 20.0, filtered.len(), |ui, range| {
                            for index in range {
                                let item = &filtered[index];
                                let label = format!(
                                    "0x{:08X}  {:<28} {}",
                                    item.address, item.name, item.description
                                );
                                if ui
                                    .selectable_label(
                                        false,
                                        RichText::new(label)
                                            .size(11.5)
                                            .monospace()
                                            .color(theme::ACCENT),
                                    )
                                    .on_hover_text(
                                        item.path
                                            .as_ref()
                                            .map(|path| util::display_path(path))
                                            .unwrap_or_else(|| "来自芯片支持包内的 SVD".to_owned()),
                                    )
                                    .clicked()
                                {
                                    clicked = Some(item.clone());
                                }
                            }
                        });
                }
            });
        self.registers.open = open;
        if refresh {
            self.refresh_registers();
        }
        if let Some(item) = clicked {
            if let Some(path) = item.path {
                actions.push(Action::OpenAt(
                    path.to_string_lossy().into_owned(),
                    item.line,
                    1,
                ));
            }
        }
    }

    pub(super) fn show_create_entry_modal(&mut self, ctx: &egui::Context) {
        let Some(mut draft) = self.create_entry.take() else {
            return;
        };
        let mut close = false;
        let mut create = false;
        let modal = egui::Modal::new(Id::new("simple_embedded_create_entry_modal"))
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
                    RichText::new(draft.kind.title())
                        .size(18.0)
                        .strong()
                        .color(theme::TEXT),
                );
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    if window_close_button(ui) {
                        close = true;
                    }
                });
            });
            ui.add_space(3.0);
            ui.label(
                RichText::new(format!("位置：{}", util::display_path(&draft.dir)))
                    .size(10.0)
                    .color(theme::TEXT_FAINT),
            );
            ui.add_space(8.0);
            let hint = match draft.kind {
                CreateKind::File => "文件名，例如 main.c",
                CreateKind::Folder => "文件夹名，例如 Drivers",
            };
            let response = ui.add(
                TextEdit::singleline(&mut draft.name)
                    .id(Id::new("simple_embedded_create_entry_name"))
                    .desired_width(f32::INFINITY)
                    .hint_text(hint),
            );
            if draft.focus {
                response.request_focus();
                draft.focus = false;
            }
            if let Some(error) = &draft.error {
                ui.add_space(5.0);
                ui.label(RichText::new(error).size(10.5).color(theme::ERROR));
            }
            if (response.has_focus() || response.lost_focus())
                && ui.input(|input| input.key_pressed(egui::Key::Enter))
            {
                create = true;
            }
            ui.add_space(10.0);
            ui.horizontal(|ui| {
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    if ui.add(theme::primary_button(draft.kind.action())).clicked() {
                        create = true;
                    }
                    if ui.add(theme::secondary_button("取消")).clicked() {
                        close = true;
                    }
                });
            });
        });
        if create {
            self.commit_create_entry(draft);
        } else if !close {
            self.create_entry = Some(draft);
        }
    }

    pub(super) fn show_rename_entry_modal(&mut self, ctx: &egui::Context) {
        let Some(mut draft) = self.rename_entry.take() else {
            return;
        };
        let mut close = false;
        let mut rename = false;
        let modal = egui::Modal::new(Id::new("simple_embedded_rename_entry_modal"))
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
                    RichText::new("重命名")
                        .size(18.0)
                        .strong()
                        .color(theme::TEXT),
                );
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    if window_close_button(ui) {
                        close = true;
                    }
                });
            });
            ui.add_space(3.0);
            ui.label(
                RichText::new(format!("位置：{}", util::display_path(&draft.path)))
                    .size(10.0)
                    .color(theme::TEXT_FAINT),
            );
            ui.add_space(8.0);
            let response = ui.add(
                TextEdit::singleline(&mut draft.name)
                    .id(Id::new("simple_embedded_rename_entry_name"))
                    .desired_width(f32::INFINITY)
                    .hint_text("新名称"),
            );
            if draft.focus {
                response.request_focus();
                draft.focus = false;
            }
            if let Some(error) = &draft.error {
                ui.add_space(5.0);
                ui.label(RichText::new(error).size(10.5).color(theme::ERROR));
            }
            if (response.has_focus() || response.lost_focus())
                && ui.input(|input| input.key_pressed(egui::Key::Enter))
            {
                rename = true;
            }
            ui.add_space(10.0);
            ui.horizontal(|ui| {
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    if ui.add(theme::primary_button("重命名")).clicked() {
                        rename = true;
                    }
                    if ui.add(theme::secondary_button("取消")).clicked() {
                        close = true;
                    }
                });
            });
        });
        if rename {
            self.commit_rename(draft);
        } else if !close {
            self.rename_entry = Some(draft);
        }
    }

    pub(super) fn show_delete_entry_modal(&mut self, ctx: &egui::Context) {
        let Some(draft) = self.delete_entry.take() else {
            return;
        };
        let mut close = false;
        let mut delete = false;
        let modal = egui::Modal::new(Id::new("simple_embedded_delete_entry_modal"))
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
                    RichText::new(if draft.is_dir {
                        "删除文件夹"
                    } else {
                        "删除文件"
                    })
                    .size(18.0)
                    .strong()
                    .color(theme::TEXT),
                );
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    if window_close_button(ui) {
                        close = true;
                    }
                });
            });
            ui.add_space(8.0);
            ui.label(
                RichText::new(format!(
                    "确定要删除 {} 吗？",
                    util::display_path(&draft.path)
                ))
                .size(12.0)
                .color(theme::TEXT),
            );
            if draft.is_dir {
                ui.add_space(4.0);
                ui.label(
                    RichText::new("文件夹中的所有内容都会一并删除。")
                        .size(10.5)
                        .color(theme::WARNING),
                );
            }
            ui.add_space(4.0);
            ui.label(
                RichText::new("此操作不可撤销。")
                    .size(10.5)
                    .color(theme::TEXT_FAINT),
            );
            ui.add_space(12.0);
            ui.horizontal(|ui| {
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    if ui.add(theme::primary_button("删除")).clicked() {
                        delete = true;
                    }
                    if ui.add(theme::secondary_button("取消")).clicked() {
                        close = true;
                    }
                });
            });
        });
        if delete {
            self.commit_delete();
        } else if !close {
            self.delete_entry = Some(draft);
        }
    }

    pub(super) fn show_memory_layout_modal(&mut self, ctx: &egui::Context) {
        let Some(mut draft) = self.memory_draft.take() else {
            return;
        };
        let source = self
            .memory_layout
            .as_ref()
            .map(|memory| memory.source.clone())
            .unwrap_or_else(|| "手动设置".to_owned());
        let mut close = false;
        let mut save = false;
        let mut restore = false;
        let modal = egui::Modal::new(Id::new("simple_embedded_memory_layout_modal"))
            .frame(
                Frame::new()
                    .fill(theme::PANEL)
                    .stroke(Stroke::new(1.0_f32, theme::BORDER))
                    .corner_radius(10)
                    .inner_margin(Margin::symmetric(20, 16)),
            )
            .backdrop_color(Color32::from_black_alpha(150));
        modal.show(ctx, |ui| {
            ui.set_width(860.0);
            ui.horizontal(|ui| {
                ui.label(
                    RichText::new("内存布局")
                        .size(18.0)
                        .strong()
                        .color(theme::TEXT),
                );
                ui.label(
                    RichText::new(format!(
                        "来源：{}",
                        if draft.restored {
                            draft
                                .auto
                                .as_ref()
                                .map(|auto| auto.source.as_str())
                                .unwrap_or("自动识别")
                        } else {
                            source.as_str()
                        }
                    ))
                    .size(10.0)
                    .color(theme::TEXT_FAINT),
                );
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    if window_close_button(ui) {
                        close = true;
                    }
                });
            });
            ui.add_space(2.0);
            ui.label(
                RichText::new("地址和大小支持 0x08000000、65536、64K、1M 等写法；Default 勾选的区域计入总量。")
                    .size(10.0)
                    .color(theme::TEXT_FAINT),
            );
            ui.add_space(8.0);

            ui.columns(2, |columns| {
                memory_column(
                    &mut columns[0],
                    "Read/Write Memory Areas",
                    crate::config::MemoryKind::Ram,
                    &mut draft.regions[..5],
                );
                memory_column(
                    &mut columns[1],
                    "Read/Only Memory Areas",
                    crate::config::MemoryKind::Rom,
                    &mut draft.regions[5..],
                );
            });

            ui.add_space(8.0);
            ui.columns(2, |columns| {
                memory_total(
                    &mut columns[0],
                    "RAM Total",
                    crate::config::MemoryKind::Ram,
                    &draft.regions[..5],
                );
                memory_total(
                    &mut columns[1],
                    "ROM Total",
                    crate::config::MemoryKind::Rom,
                    &draft.regions[5..],
                );
            });

            ui.add_space(10.0);
            ui.horizontal(|ui| {
                ui.label(
                    RichText::new("下载起始地址")
                        .size(11.0)
                        .color(theme::TEXT_DIM),
                );
                let response = ui.add(
                    TextEdit::singleline(&mut draft.download_address)
                        .desired_width(170.0)
                        .hint_text("留空使用启动 ROM 地址"),
                );
                if response.changed() {
                    draft.restored = false;
                }
            });
            ui.add_space(3.0);
            ui.label(
                RichText::new(
                    "烧录 .bin 时使用此地址；.hex / .elf / .axf 使用文件内记录的地址。修改内存布局不会自动改写链接脚本。",
                )
                .size(9.5)
                .color(theme::TEXT_FAINT),
            );
            if let Some(error) = &draft.error {
                ui.add_space(5.0);
                ui.label(RichText::new(error).size(10.5).color(theme::ERROR));
            }
            ui.add_space(12.0);
            ui.horizontal(|ui| {
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    if ui.add(theme::primary_button("保存")).clicked() {
                        save = true;
                    }
                    if ui.add(theme::secondary_button("取消")).clicked() {
                        close = true;
                    }
                    if ui.add(theme::secondary_button("恢复自动识别")).clicked() {
                        restore = true;
                    }
                });
            });
        });
        if restore {
            if let Some(auto) = draft.auto.clone() {
                draft.regions = crate::memory::default_regions(Some(auto.layout));
                draft.download_address =
                    crate::memory::primary_start(&draft.regions, crate::config::MemoryKind::Rom)
                        .map(crate::memory::format_hex)
                        .unwrap_or_default();
                draft.restored = true;
            } else {
                draft.regions = crate::memory::default_regions(None);
                draft.download_address.clear();
                draft.restored = true;
            }
            draft.error = None;
            self.memory_draft = Some(draft);
        } else if save {
            self.save_memory_layout(draft);
        } else if !close {
            self.memory_draft = Some(draft);
        }
    }
    pub(super) fn show_image_preview(&mut self, ctx: &egui::Context) {
        let Some(state) = self.image_preview.take() else {
            return;
        };
        let mut open = true;
        let title = format!("图像预览 - {}", util::file_name(&state.path));
        egui::Window::new(title)
            .id(Id::new("simple_embedded_image_preview"))
            .open(&mut open)
            .resizable(true)
            .default_size([820.0, 620.0])
            .min_size([320.0, 240.0])
            .frame(
                Frame::new()
                    .fill(theme::PANEL)
                    .stroke(Stroke::new(1.0_f32, theme::BORDER))
                    .corner_radius(8)
                    .inner_margin(Margin::same(8)),
            )
            .show(ctx, |ui| {
                if let Some(error) = &state.error {
                    ui.label(RichText::new(error).size(11.0).color(theme::ERROR));
                    return;
                }
                let Some(texture) = &state.texture else {
                    ui.label(
                        RichText::new("无法预览该图片")
                            .size(11.0)
                            .color(theme::TEXT_FAINT),
                    );
                    return;
                };
                let available = ui.available_size();
                let [width, height] = state.size;
                let scale = (available.x / width as f32)
                    .min(available.y / height as f32)
                    .clamp(0.02, 1.0);
                let size = egui::vec2(width as f32 * scale, height as f32 * scale);
                ui.vertical_centered(|ui| {
                    ui.add(egui::Image::new((texture.id(), size)));
                    ui.add_space(4.0);
                    ui.label(
                        RichText::new(format!("{} × {} · {:.0}%", width, height, scale * 100.0))
                            .size(10.0)
                            .color(theme::TEXT_FAINT),
                    );
                });
            });
        if open {
            self.image_preview = Some(state);
        }
    }

    pub(super) fn show_debug_side_panel(&mut self, ui: &mut egui::Ui) {
        let mut auto = self.debug.auto_watch;
        let mut refresh = false;
        let mut add_watch = false;
        let mut remove_watch = None;
        let has_session = self.debug.session.is_some();
        let pending = if self.debug.watch_tab == DebugSideTab::Peripherals {
            self.debug.peripheral_refresh.is_some()
        } else {
            self.debug.watch_refresh.is_some()
        };
        Frame::new()
            .fill(theme::PANEL)
            .stroke(Stroke::new(1.0_f32, theme::BORDER_SOFT))
            .corner_radius(6)
            .inner_margin(Margin::symmetric(8, 6))
            .show(ui, |ui| {
                ui.set_min_width(ui.available_width());
                ui.set_min_height(ui.available_height());
                ui.horizontal(|ui| {
                    if debug_side_tab(
                        ui,
                        "变量",
                        self.debug.watch_tab == DebugSideTab::Variables,
                    ) {
                        self.debug.watch_tab = DebugSideTab::Variables;
                    }
                    if debug_side_tab(
                        ui,
                        "寄存器",
                        self.debug.watch_tab == DebugSideTab::Registers,
                    ) {
                        self.debug.watch_tab = DebugSideTab::Registers;
                    }
                    if debug_side_tab(
                        ui,
                        "外设",
                        self.debug.watch_tab == DebugSideTab::Peripherals,
                    ) {
                        self.debug.watch_tab = DebugSideTab::Peripherals;
                        if self.registers.items.is_empty() {
                            self.refresh_registers();
                        }
                    }
                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        let refresh_label = if pending { "刷新中" } else { "刷新" };
                        let color = if pending {
                            theme::TEXT_FAINT
                        } else {
                            theme::TEXT_DIM
                        };
                        if ui
                            .add_enabled(
                                has_session && !pending,
                                small_flat_button(refresh_label, color),
                            )
                            .on_hover_text("刷新当前页面数据")
                            .clicked()
                        {
                            refresh = true;
                        }
                        ui.checkbox(
                            &mut auto,
                            RichText::new("自动").size(10.5).color(theme::TEXT_DIM),
                        )
                        .on_hover_text("目标停止时每 1.2 秒刷新");
                    });
                });
                ui.add_space(2.0);
                match self.debug.watch_tab {
                    DebugSideTab::Variables => {
                        ui.add(
                            TextEdit::singleline(&mut self.debug.watch_filter)
                                .id(Id::new("simple_embedded_watch_filter"))
                                .desired_width(f32::INFINITY)
                                .hint_text("搜索变量"),
                        );
                        ui.add_space(3.0);
                        ui.horizontal(|ui| {
                            let add_width = 46.0;
                            ui.add(
                                TextEdit::singleline(&mut self.debug.watch_input)
                                    .id(Id::new("simple_embedded_watch_input"))
                                    .desired_width(
                                        (ui.available_width() - add_width - 4.0).max(80.0),
                                    )
                                    .hint_text("变量/表达式，留空用光标处"),
                            );
                            if ui
                                .add(small_flat_button("添加", theme::ACCENT))
                                .clicked()
                            {
                                add_watch = true;
                            }
                        });
                        ui.add_space(4.0);
                        let filter = self.debug.watch_filter.trim().to_lowercase();
                        let rows = self
                            .debug
                            .watches
                            .iter()
                            .enumerate()
                            .filter(|(_, watch)| {
                                filter.is_empty()
                                    || watch.expr.to_lowercase().contains(&filter)
                                    || watch.value.to_lowercase().contains(&filter)
                            })
                            .map(|(index, _)| index)
                            .collect::<Vec<_>>();
                        let remaining = ui.available_height();
                        ScrollArea::vertical()
                            .id_salt("simple_embedded_watch_list")
                            .max_height(remaining)
                            .auto_shrink([false, false])
                            .show(ui, |ui| {
                                if self.debug.watches.is_empty() {
                                    ui.label(
                                        RichText::new(
                                            "还没有监视变量。输入表达式，或把光标放在变量上点“添加”。",
                                        )
                                        .size(10.5)
                                        .color(theme::TEXT_FAINT),
                                    );
                                    return;
                                }
                                if rows.is_empty() {
                                    ui.label(
                                        RichText::new("没有匹配的变量")
                                            .size(10.5)
                                            .color(theme::TEXT_FAINT),
                                    );
                                    return;
                                }
                                let width = ui.available_width();
                                let spacing = ui.spacing().item_spacing.x;
                                let name_width = 90.0;
                                let type_width = 76.0;
                                let remove_width = 18.0;
                                let value_width =
                                    (width - name_width - type_width - remove_width - spacing * 3.0)
                                        .max(50.0);
                                ui.horizontal(|ui| {
                                    ui.add_sized(
                                        [name_width, 16.0],
                                        Label::new(
                                            RichText::new("名称")
                                                .size(10.0)
                                                .color(theme::TEXT_FAINT),
                                        ),
                                    );
                                    ui.add_sized(
                                        [value_width, 16.0],
                                        Label::new(
                                            RichText::new("值")
                                                .size(10.0)
                                                .color(theme::TEXT_FAINT),
                                        ),
                                    );
                                    ui.add_sized(
                                        [type_width, 16.0],
                                        Label::new(
                                            RichText::new("类型")
                                                .size(10.0)
                                                .color(theme::TEXT_FAINT),
                                        ),
                                    );
                                    ui.add_space(remove_width);
                                });
                                for index in rows {
                                    let watch = &self.debug.watches[index];
                                    let value_text = if watch.value.is_empty() {
                                        "…"
                                    } else {
                                        watch.value.as_str()
                                    };
                                    let value_color = if value_text.starts_with("No ")
                                        || value_text.contains("Cannot")
                                        || value_text.contains("error")
                                    {
                                        theme::ERROR
                                    } else {
                                        theme::TEXT_DIM
                                    };
                                    let type_name = if watch.type_name.is_empty() {
                                        "-"
                                    } else {
                                        watch.type_name.as_str()
                                    };
                                    ui.horizontal(|ui| {
                                        let name = ui.add_sized(
                                            [name_width, 18.0],
                                            Label::new(
                                                RichText::new(&watch.expr)
                                                    .size(10.5)
                                                    .color(theme::TEXT),
                                            )
                                            .truncate(),
                                        );
                                        name.on_hover_text(&watch.expr);
                                        let value_response = ui.add_sized(
                                            [value_width, 18.0],
                                            Label::new(
                                                RichText::new(value_text)
                                                    .monospace()
                                                    .size(10.0)
                                                    .color(value_color),
                                            )
                                            .truncate(),
                                        );
                                        value_response.on_hover_text(value_text);
                                        let type_label = ui.add_sized(
                                            [type_width, 18.0],
                                            Label::new(
                                                RichText::new(type_name)
                                                    .size(9.5)
                                                    .color(theme::TEXT_FAINT),
                                            )
                                            .truncate(),
                                        );
                                        type_label.on_hover_text(type_name);
                                        if ui
                                            .add_sized(
                                                [remove_width, 18.0],
                                                Button::new(
                                                    RichText::new("×")
                                                        .size(12.0)
                                                        .color(theme::TEXT_FAINT),
                                                )
                                                .frame(false)
                                                .corner_radius(3),
                                            )
                                            .on_hover_text("移除监视")
                                            .clicked()
                                        {
                                            remove_watch = Some(index);
                                        }
                                    });
                                }
                            });
                    }
                    DebugSideTab::Registers => {
                        ui.add(
                            TextEdit::singleline(&mut self.debug.register_filter)
                                .id(Id::new("simple_embedded_register_filter"))
                                .desired_width(f32::INFINITY)
                                .hint_text("搜索寄存器名或值"),
                        );
                        ui.add_space(4.0);
                        let filter = self.debug.register_filter.trim().to_lowercase();
                        let rows = self
                            .debug
                            .registers
                            .iter()
                            .filter(|register| {
                                filter.is_empty()
                                    || register.name.to_lowercase().contains(&filter)
                                    || register.value.to_lowercase().contains(&filter)
                            })
                            .collect::<Vec<_>>();
                        let remaining = ui.available_height();
                        ScrollArea::vertical()
                            .id_salt("simple_embedded_register_list")
                            .max_height(remaining)
                            .auto_shrink([false, false])
                            .show(ui, |ui| {
                                if self.debug.registers.is_empty() {
                                    ui.label(
                                        RichText::new(
                                            "启动调试后显示 CPU 寄存器；目标停止时会自动刷新。",
                                        )
                                        .size(10.5)
                                        .color(theme::TEXT_FAINT),
                                    );
                                    return;
                                }
                                if rows.is_empty() {
                                    ui.label(
                                        RichText::new("没有匹配的寄存器")
                                            .size(10.5)
                                            .color(theme::TEXT_FAINT),
                                    );
                                    return;
                                }
                                let width = ui.available_width();
                                let spacing = ui.spacing().item_spacing.x;
                                let name_width = 88.0;
                                let value_width = (width - name_width - spacing).max(60.0);
                                ui.horizontal(|ui| {
                                    ui.add_sized(
                                        [name_width, 16.0],
                                        Label::new(
                                            RichText::new("寄存器")
                                                .size(10.0)
                                                .color(theme::TEXT_FAINT),
                                        ),
                                    );
                                    ui.add_sized(
                                        [value_width, 16.0],
                                        Label::new(
                                            RichText::new("值")
                                                .size(10.0)
                                                .color(theme::TEXT_FAINT),
                                        ),
                                    );
                                });
                                for register in rows {
                                    ui.horizontal(|ui| {
                                        let name = ui.add_sized(
                                            [name_width, 18.0],
                                            Label::new(
                                                RichText::new(&register.name)
                                                    .monospace()
                                                    .size(10.5)
                                                    .color(theme::TEXT),
                                            )
                                            .truncate(),
                                        );
                                        name.on_hover_text(&register.name);
                                        let value = ui.add_sized(
                                            [value_width, 18.0],
                                            Label::new(
                                                RichText::new(&register.value)
                                                    .monospace()
                                                    .size(10.5)
                                                    .color(theme::TEXT_DIM),
                                            )
                                            .truncate(),
                                        );
                                        value.on_hover_text(&register.raw);
                                    });
                                }
                            });
                    }
                    DebugSideTab::Peripherals => {
                        ui.add(
                            TextEdit::singleline(&mut self.debug.peripheral_filter)
                                .id(Id::new("simple_embedded_peripheral_filter"))
                                .desired_width(f32::INFINITY)
                                .hint_text("搜索外设寄存器名、地址或说明"),
                        );
                        ui.add_space(3.0);
                        if self.registers.items.is_empty() {
                            let message = self.registers.error.as_deref().unwrap_or(
                                "未找到 SVD/寄存器定义；请把芯片支持包（.pack）放到工程根目录",
                            );
                            ui.label(RichText::new(message).size(10.5).color(theme::WARNING));
                            return;
                        }
                        let filter = self.debug.peripheral_filter.trim().to_lowercase();
                        let rows = self
                            .registers
                            .items
                            .iter()
                            .filter(|item| {
                                filter.is_empty()
                                    || item.name.to_lowercase().contains(&filter)
                                    || item.description.to_lowercase().contains(&filter)
                                    || format!("0x{:08X}", item.address)
                                        .to_lowercase()
                                        .contains(&filter)
                            })
                            .take(160)
                            .collect::<Vec<_>>();
                        ui.horizontal(|ui| {
                            ui.label(
                                RichText::new(&self.registers.source)
                                    .size(9.5)
                                    .color(theme::TEXT_FAINT),
                            );
                            if rows.len() == 160 {
                                ui.label(
                                    RichText::new("仅显示前 160 个")
                                        .size(9.0)
                                        .color(theme::WARNING),
                                );
                            }
                        });
                        ui.add_space(3.0);
                        let remaining = ui.available_height();
                        ScrollArea::vertical()
                            .id_salt("simple_embedded_peripheral_list")
                            .max_height(remaining)
                            .auto_shrink([false, false])
                            .show(ui, |ui| {
                                if rows.is_empty() {
                                    ui.label(
                                        RichText::new("没有匹配的外设寄存器")
                                            .size(10.5)
                                            .color(theme::TEXT_FAINT),
                                    );
                                    return;
                                }
                                let width = ui.available_width();
                                let spacing = ui.spacing().item_spacing.x;
                                let address_width = 84.0;
                                let value_width = 76.0;
                                let name_width =
                                    (width - address_width - value_width - spacing * 2.0)
                                        .max(70.0);
                                ui.horizontal(|ui| {
                                    for (label, size) in [
                                        ("寄存器", name_width),
                                        ("地址", address_width),
                                        ("值", value_width),
                                    ] {
                                        ui.add_sized(
                                            [size, 16.0],
                                            Label::new(
                                                RichText::new(label)
                                                    .size(9.5)
                                                    .color(theme::TEXT_FAINT),
                                            ),
                                        );
                                    }
                                });
                                for item in rows {
                                    let value = self
                                        .debug
                                        .peripheral_values
                                        .get(&item.address)
                                        .map(String::as_str)
                                        .unwrap_or("—");
                                    ui.horizontal(|ui| {
                                        let name = ui.add_sized(
                                            [name_width, 18.0],
                                            Label::new(
                                                RichText::new(&item.name)
                                                    .monospace()
                                                    .size(10.0)
                                                    .color(theme::TEXT),
                                            )
                                            .truncate(),
                                        );
                                        name.on_hover_text(if item.description.is_empty() {
                                            &item.name
                                        } else {
                                            &item.description
                                        });
                                        ui.add_sized(
                                            [address_width, 18.0],
                                            Label::new(
                                                RichText::new(format!("0x{:08X}", item.address))
                                                    .monospace()
                                                    .size(9.5)
                                                    .color(theme::TEXT_FAINT),
                                            ),
                                        );
                                        let color = if value == "无法读取" {
                                            theme::ERROR
                                        } else if value == "—" {
                                            theme::TEXT_FAINT
                                        } else {
                                            theme::SUCCESS
                                        };
                                        ui.add_sized(
                                            [value_width, 18.0],
                                            Label::new(
                                                RichText::new(value)
                                                    .monospace()
                                                    .size(10.0)
                                                    .color(color),
                                            ),
                                        );
                                    });
                                }
                            });
                    }
                }
            });
        if auto != self.debug.auto_watch {
            self.debug.auto_watch = auto;
            self.project_debug.auto_refresh = auto;
            if self.project.is_some() {
                self.persist_build_config();
            }
            if auto {
                self.debug.watch_refresh = None;
                self.debug.peripheral_refresh = None;
                if self.debug.watch_tab == DebugSideTab::Peripherals {
                    self.refresh_debug_peripheral();
                } else {
                    self.refresh_debug_watch();
                }
            }
        }
        if refresh {
            if self.debug.watch_tab == DebugSideTab::Peripherals {
                self.debug.peripheral_refresh = None;
                self.refresh_debug_peripheral();
            } else {
                self.debug.watch_refresh = None;
                self.refresh_debug_watch();
            }
        }
        if add_watch {
            let expression = if self.debug.watch_input.trim().is_empty() {
                self.current_word().unwrap_or_default()
            } else {
                self.debug.watch_input.clone()
            };
            self.add_debug_watch(expression);
        }
        if let Some(index) = remove_watch {
            self.remove_debug_watch(index);
        }
    }

    pub(super) fn show_debug_overlay(&mut self, ctx: &egui::Context, actions: &mut Vec<Action>) {
        let process_running = self
            .debug
            .session
            .as_mut()
            .map(|session| session.is_running())
            .unwrap_or(false);
        let target_running = self.debug.running;
        let started = self
            .debug
            .session
            .as_ref()
            .is_some_and(|session| session.started);
        let has_session = self.debug.session.is_some();
        let starting = self.debug.starting;
        let (status_color, status) = if self.debug.error.is_some() {
            (theme::ERROR, "错误")
        } else if starting {
            (theme::ACCENT, "启动中")
        } else if has_session {
            if !process_running {
                (theme::TEXT_FAINT, "已停止")
            } else if target_running {
                (theme::ACCENT, "运行中")
            } else {
                (theme::SUCCESS, "已暂停")
            }
        } else {
            (theme::TEXT_FAINT, "未启动")
        };
        let target_path = self.debug.target.clone();
        let target_name = target_path
            .as_ref()
            .map(|path| util::file_name(path))
            .unwrap_or_else(|| "未选择调试程序".to_owned());
        let target_full = target_path.as_ref().map(|path| util::display_path(path));
        let breakpoints = self.debug.breakpoints.len();
        let screen = ctx.content_rect();
        let anchor = self.last_editor_rect.unwrap_or(screen);
        let width = 300.0_f32.min((anchor.width() - 20.0).max(260.0));
        let max_x = (screen.right() - width - 8.0).max(screen.left() + 8.0);
        let position = egui::pos2(
            (anchor.right() - width - 12.0).clamp(screen.left() + 8.0, max_x),
            (anchor.top() + 8.0).max(screen.top() + 8.0),
        );
        let mut close = false;
        let mut action = None;
        egui::Area::new(Id::new("simple_embedded_debug_overlay"))
            .order(egui::Order::Foreground)
            .fixed_pos(position)
            .show(ctx, |ui| {
                Frame::new()
                    .fill(theme::PANEL_RAISED)
                    .stroke(Stroke::new(1.0_f32, theme::BORDER))
                    .corner_radius(8)
                    .inner_margin(Margin::symmetric(10, 7))
                    .show(ui, |ui| {
                        ui.set_width(width - 20.0);
                        ui.spacing_mut().item_spacing = egui::vec2(6.0, 4.0);
                        ui.horizontal(|ui| {
                            colored_dot(ui, status_color, 6.0);
                            ui.label(RichText::new("调试").size(12.5).strong().color(theme::TEXT));
                            ui.label(RichText::new(status).size(10.5).color(status_color));
                            let target = ui.add(
                                Label::new(
                                    RichText::new(target_name.clone())
                                        .size(10.5)
                                        .color(theme::TEXT_FAINT),
                                )
                                .truncate(),
                            );
                            if let Some(full) = &target_full {
                                target.on_hover_text(full);
                            }
                            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                                if window_close_button(ui) {
                                    close = true;
                                }
                            });
                        });
                        if let Some(error) = &self.debug.error {
                            ui.label(RichText::new(error).size(10.5).color(theme::ERROR));
                        }
                        ui.horizontal(|ui| {
                            if target_running {
                                if debug_icon_button(ui, DebugIcon::Pause, true, "暂停 (F5)")
                                    .clicked()
                                {
                                    action = Some(Action::DebugPause);
                                }
                            } else {
                                let continue_tip = if starting {
                                    "启动完成后继续 (F5)"
                                } else if !has_session {
                                    "启动调试 (F5)"
                                } else if started {
                                    "继续 (F5)"
                                } else {
                                    "运行 (F5)"
                                };
                                if debug_icon_button(ui, DebugIcon::Continue, true, continue_tip)
                                    .clicked()
                                {
                                    action = Some(Action::DebugContinue);
                                }
                            }
                            if debug_icon_button(
                                ui,
                                DebugIcon::StepOver,
                                has_session && !starting && !target_running,
                                "下一步 (F10)",
                            )
                            .clicked()
                            {
                                action = Some(Action::DebugStepOver);
                            }
                            if debug_icon_button(
                                ui,
                                DebugIcon::StepInto,
                                has_session && !starting && !target_running,
                                "单步进入 (F11)",
                            )
                            .clicked()
                            {
                                action = Some(Action::DebugStepInto);
                            }
                            if debug_icon_button(
                                ui,
                                DebugIcon::StepOut,
                                has_session && !starting && !target_running,
                                "跳出 (Shift+F11)",
                            )
                            .clicked()
                            {
                                action = Some(Action::DebugFinish);
                            }
                            if debug_icon_button(
                                ui,
                                DebugIcon::Stop,
                                has_session || starting,
                                "停止 (Shift+F5)",
                            )
                            .clicked()
                            {
                                action = Some(Action::DebugStop);
                            }
                            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                                ui.label(
                                    RichText::new(format!("{breakpoints} 断点"))
                                        .size(10.0)
                                        .color(theme::TEXT_FAINT),
                                );
                            });
                        });
                    });
            });
        if close {
            self.debug.open = false;
        }
        if let Some(action) = action {
            actions.push(action);
        }
    }

    pub(super) fn show_completion(&mut self, ctx: &egui::Context) {
        if !self.completion.open || self.has_modal() {
            return;
        }
        let Some(anchor) = self.completion.anchor else {
            return;
        };
        let filtered = self.completion_filtered();
        if filtered.is_empty() {
            self.completion.open = false;
            return;
        }
        self.completion.selected = self.completion.selected.min(filtered.len() - 1);
        let selected = self.completion.selected;
        let row_height = 20.0;
        let width = 360.0;
        let visible = filtered.len().min(9);
        let height = visible as f32 * row_height + 10.0;
        let screen = ctx.content_rect();
        let mut position = egui::pos2(anchor.x, anchor.y + 4.0);
        if position.y + height > screen.bottom() - 6.0 {
            position.y = (anchor.y - height - 6.0).max(screen.top() + 6.0);
        }
        position.x = position.x.clamp(
            screen.left() + 6.0,
            (screen.right() - width - 6.0).max(screen.left() + 6.0),
        );

        let mut chosen = None;
        egui::Area::new(Id::new("simple_embedded_completion_popup"))
            .order(egui::Order::Foreground)
            .fixed_pos(position)
            .show(ctx, |ui| {
                ui.set_width(width);
                Frame::new()
                    .fill(theme::PANEL)
                    .stroke(Stroke::new(1.0_f32, theme::BORDER))
                    .corner_radius(6)
                    .shadow(ui.style().visuals.popup_shadow)
                    .inner_margin(Margin::symmetric(4, 4))
                    .show(ui, |ui| {
                        ui.spacing_mut().item_spacing.y = 0.0;
                        ScrollArea::vertical()
                            .id_salt("simple_embedded_completion_list")
                            .max_height(row_height * 9.0)
                            .auto_shrink([false, true])
                            .show(ui, |ui| {
                                for (row, index) in filtered.iter().enumerate() {
                                    let item = &self.completion.items[*index];
                                    let (rect, response) = ui.allocate_exact_size(
                                        egui::vec2(ui.available_width(), row_height),
                                        Sense::click(),
                                    );
                                    let response =
                                        response.on_hover_cursor(egui::CursorIcon::PointingHand);
                                    if row == selected {
                                        ui.painter().rect_filled(
                                            rect,
                                            CornerRadius::same(3),
                                            Color32::from_rgb(36, 84, 108),
                                        );
                                    } else if response.hovered() {
                                        ui.painter().rect_filled(
                                            rect,
                                            CornerRadius::same(3),
                                            Color32::from_rgb(28, 49, 63),
                                        );
                                    }
                                    let name = item.name.chars().take(40).collect::<String>();
                                    ui.painter().text(
                                        egui::pos2(rect.left() + 6.0, rect.center().y),
                                        Align2::LEFT_CENTER,
                                        name,
                                        FontId::monospace(12.0),
                                        theme::TEXT,
                                    );
                                    ui.painter().text(
                                        egui::pos2(rect.right() - 6.0, rect.center().y),
                                        Align2::RIGHT_CENTER,
                                        item.kind.label(),
                                        FontId::proportional(9.0),
                                        item.kind.color(),
                                    );
                                    let hover = match &item.detail {
                                        Some(detail) => format!("{}  {}", item.name, detail),
                                        None => item.name.clone(),
                                    };
                                    if response.on_hover_text(hover).clicked() {
                                        chosen = Some(*index);
                                    }
                                    if row == selected {
                                        ui.scroll_to_rect(rect, None);
                                    }
                                }
                            });
                    });
            });
        if let Some(index) = chosen {
            let name = self.completion.items[index].name.clone();
            self.insert_completion(ctx, &name);
        }
    }

    pub(super) fn show_column_edit(&mut self, ctx: &egui::Context) {
        use crate::column::ColumnOperation;

        let mut open = self.column_edit.open;
        let mut apply = false;
        egui::Window::new("列编辑")
            .id(Id::new("simple_embedded_column_edit"))
            .title_bar(false)
            .collapsible(false)
            .resizable(false)
            .default_size([540.0, 260.0])
            .frame(
                Frame::new()
                    .fill(theme::PANEL)
                    .stroke(Stroke::new(1.0_f32, theme::BORDER))
                    .inner_margin(Margin::same(12))
                    .corner_radius(6),
            )
            .show(ctx, |ui| {
                ui.set_min_width(500.0);
                ui.set_min_height(190.0);
                ui.visuals_mut().override_text_color = Some(theme::TEXT);
                ui.horizontal(|ui| {
                    ui.label(
                        RichText::new("列编辑")
                            .size(16.0)
                            .strong()
                            .color(theme::TEXT),
                    );
                    ui.label(
                        RichText::new(&self.column_edit.source_label)
                            .size(11.0)
                            .color(theme::ACCENT),
                    );
                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        if window_close_button(ui) {
                            open = false;
                        }
                    });
                });
                ui.add_space(4.0);
                ui.horizontal(|ui| {
                    ui.label(
                        RichText::new("列号")
                            .size(11.5)
                            .color(theme::TEXT_DIM),
                    );
                    ui.add(egui::DragValue::new(&mut self.column_edit.column).range(1..=999));
                    ui.label(
                        RichText::new("操作")
                            .size(11.5)
                            .color(theme::TEXT_DIM),
                    );
                    egui::ComboBox::from_id_salt("simple_embedded_column_operation")
                        .selected_text(self.column_edit.operation.label())
                        .width(90.0)
                        .show_ui(ui, |ui| {
                            for value in [
                                ColumnOperation::Insert,
                                ColumnOperation::Delete,
                                ColumnOperation::Replace,
                            ] {
                                ui.selectable_value(
                                    &mut self.column_edit.operation,
                                    value,
                                    value.label(),
                                );
                            }
                        });
                    if self.column_edit.operation != ColumnOperation::Insert {
                        ui.label(
                            RichText::new("字符数")
                                .size(11.5)
                                .color(theme::TEXT_DIM),
                        );
                        ui.add(
                            egui::DragValue::new(&mut self.column_edit.count).range(1..=999),
                        );
                    }
                });
                ui.horizontal(|ui| {
                    ui.label(
                        RichText::new("内容")
                            .size(11.5)
                            .color(theme::TEXT_DIM),
                    );
                    ui.add(
                        TextEdit::singleline(&mut self.column_edit.insert)
                            .id(Id::new("simple_embedded_column_insert"))
                            .font(FontId::monospace(12.0))
                            .text_color(theme::TEXT)
                            .background_color(theme::BG)
                            .desired_width(340.0)
                            .hint_text(
                                RichText::new("插入/替换的文本")
                                    .color(theme::TEXT_FAINT),
                            ),
                    );
                });
                ui.add_space(4.0);
                ui.label(
                    RichText::new(
                        "会对选中的每一行在同一列执行操作；短行自动补空格。没有选区时作用于当前行。",
                    )
                    .size(10.5)
                    .color(theme::TEXT_FAINT),
                );
                ui.add_space(6.0);
                ui.horizontal(|ui| {
                    if ui.add(theme::primary_button("应用")).clicked() {
                        apply = true;
                    }
                    if ui.add(theme::secondary_button("取消")).clicked() {
                        open = false;
                    }
                });
            });
        self.column_edit.open = open;
        if apply {
            self.apply_column_edit(ctx);
        }
    }

    fn show_code_editor(
        &mut self,
        ui: &mut egui::Ui,
        pane_index: usize,
        pane_slot: Option<usize>,
        actions: &mut Vec<Action>,
    ) {
        let is_active_pane = self.active == Some(pane_index);
        if is_active_pane {
            if let Some(doc) = self.tabs.get(pane_index) {
                self.find
                    .ensure_matches(doc.buffer.as_string(), doc.buffer.revision());
            }
        }
        let markers = self
            .tabs
            .get(pane_index)
            .map(|doc| {
                self.diagnostics
                    .iter()
                    .filter(|diagnostic| same_path(&diagnostic.path, &doc.path))
                    .map(|diagnostic| (diagnostic.line, diagnostic.severity))
                    .collect::<BTreeMap<usize, Severity>>()
            })
            .unwrap_or_default();
        let git_marks = self
            .tabs
            .get(pane_index)
            .map(|doc| {
                if self
                    .git_line_key
                    .as_ref()
                    .is_some_and(|(path, _)| same_path(path, &doc.path))
                {
                    self.git_line_marks.clone()
                } else {
                    BTreeSet::new()
                }
            })
            .unwrap_or_default();
        let breakpoints = self.debug.breakpoints.clone();
        let debug_line = self.tabs.get(pane_index).and_then(|doc| {
            self.debug
                .current_source
                .as_ref()
                .filter(|(path, _)| same_path(path, &doc.path))
                .map(|(_, line)| *line)
        });
        let Some(doc) = self.tabs.get_mut(pane_index) else {
            return;
        };
        let editor_id = Id::new(("simple_embedded_code_editor", &doc.path));
        let scroll_id = Id::new(("simple_embedded_code_scroll", &doc.path));
        let read_only = doc.read_only;
        let has_column_selection = self
            .column_selection
            .as_ref()
            .is_some_and(|selection| same_path(&selection.path, &doc.path));
        if has_column_selection && !read_only && ui.memory(|memory| memory.has_focus(editor_id)) {
            let mut copy = false;
            let mut remove = false;
            let mut typed = None;
            ui.ctx().input_mut(|input| {
                if input.consume_key(egui::Modifiers::COMMAND, egui::Key::C) {
                    copy = true;
                }
                if input.consume_key(egui::Modifiers::NONE, egui::Key::Backspace)
                    || input.consume_key(egui::Modifiers::NONE, egui::Key::Delete)
                {
                    remove = true;
                }
                let mut text = String::new();
                input.events.retain(|event| {
                    if let egui::Event::Text(value) = event {
                        text.push_str(value);
                        false
                    } else {
                        true
                    }
                });
                if !text.is_empty() {
                    typed = Some(text);
                }
            });
            if let Some(selection) = self.column_selection.clone() {
                let range = crate::column_select::ColumnRange::normalized(
                    selection.anchor_line,
                    selection.anchor_col,
                    selection.cursor_line,
                    selection.cursor_col,
                );
                if copy {
                    let selected =
                        crate::column_select::selected_text(doc.buffer.as_string(), range);
                    ui.ctx().copy_text(selected);
                } else if remove || typed.is_some() {
                    let insert = typed.as_deref().unwrap_or("");
                    let new_text =
                        crate::column_select::replace(doc.buffer.as_string(), range, insert);
                    let line_start =
                        crate::util::char_index_at_line(&new_text, range.start_line + 1);
                    let line_len = new_text
                        .split('\n')
                        .nth(range.start_line)
                        .map(|value| value.chars().count())
                        .unwrap_or(0);
                    let cursor = line_start + range.start_col.min(line_len);
                    doc.buffer.set_text(new_text);
                    doc.update_cursor(cursor);
                    set_editor_cursor(ui.ctx(), &doc.path, cursor);
                    self.column_selection = None;
                    self.find.dirty = true;
                }
            }
        }
        if !read_only && ui.memory(|memory| memory.has_focus(editor_id)) {
            let pasted = ui.ctx().input(|input| {
                input
                    .events
                    .iter()
                    .filter_map(|event| {
                        if let egui::Event::Paste(text) = event {
                            Some(text.clone())
                        } else {
                            None
                        }
                    })
                    .collect::<Vec<_>>()
            });
            if let Some(text) = pasted.first() {
                ui.ctx().input_mut(|input| {
                    input
                        .events
                        .retain(|event| !matches!(event, egui::Event::Paste(_)))
                });
                apply_paste_indent(doc, ui.ctx(), editor_id, text);
                self.find.dirty = true;
                self.find
                    .ensure_matches(doc.buffer.as_string(), doc.buffer.revision());
            }
            let shift_tab =
                ui.input_mut(|input| input.consume_key(egui::Modifiers::SHIFT, egui::Key::Tab));
            let tab = if shift_tab {
                false
            } else {
                ui.input_mut(|input| input.consume_key(egui::Modifiers::NONE, egui::Key::Tab))
            };
            if tab || shift_tab {
                apply_tab_indent(
                    doc,
                    ui.ctx(),
                    editor_id,
                    shift_tab,
                    self.settings.indent_width as usize,
                    self.settings.use_spaces,
                );
                self.find.dirty = true;
                self.find
                    .ensure_matches(doc.buffer.as_string(), doc.buffer.revision());
            }
            let enter =
                ui.input_mut(|input| input.consume_key(egui::Modifiers::NONE, egui::Key::Enter));
            if enter {
                apply_enter_indent(
                    doc,
                    ui.ctx(),
                    editor_id,
                    self.settings.indent_width as usize,
                    self.settings.use_spaces,
                );
                self.find.dirty = true;
                self.find
                    .ensure_matches(doc.buffer.as_string(), doc.buffer.revision());
            }
        }
        let search_ranges = if self.find.open && is_active_pane {
            self.find.decorations()
        } else {
            Vec::new()
        };
        let decorations_key = self
            .find
            .version
            .wrapping_mul(2)
            .wrapping_add(u64::from(self.find.open));
        let font_size = self.settings.font_size;
        let word_wrap = self.settings.word_wrap;
        let relative_line_numbers = self.settings.relative_line_numbers;
        let show_whitespace = self.settings.show_whitespace;
        let show_indent_guides = self.settings.show_indent_guides;
        let indent_width = self.settings.indent_width.max(1) as usize;
        let ruler_column = self.settings.ruler_column;
        let focus_editor = if is_active_pane {
            std::mem::take(&mut self.focus_editor)
        } else {
            false
        };
        let scroll_cursor = if is_active_pane {
            std::mem::take(&mut self.scroll_to_cursor)
        } else {
            false
        };
        let font_id = FontId::monospace(font_size);
        let available_width = ui.available_width();
        let full_height = ui.available_height();
        let show_minimap = self.settings.show_minimap && available_width > 520.0;
        let minimap_width = crate::minimap::WIDTH;
        let viewport_width = if show_minimap {
            (available_width - minimap_width - 8.0).max(180.0)
        } else {
            available_width
        };
        let rows = ((ui.available_height() / (font_size * 1.35)).ceil() as usize).max(20);
        let cache = doc.layout_cache.clone();
        let revision = doc.buffer.revision_handle();
        let syntax_name = doc.syntax_name.clone();
        let highlighter = Arc::clone(&self.highlighter);
        let line_count = doc
            .buffer
            .as_string()
            .bytes()
            .filter(|byte| *byte == b'\n')
            .count()
            + 1;
        let gutter_font = FontId::monospace((font_size - 1.0).max(9.0));
        let digit_width = ui
            .ctx()
            .fonts_mut(|fonts| fonts.glyph_width(&gutter_font, '0'));
        let digits = line_count.to_string().len().max(2) as f32;
        let gutter_width = (digits * digit_width + 20.0).clamp(34.0, 96.0);
        let text_margin_left = (gutter_width + 12.0).round() as i8;

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
                    decorations_key,
                    search_ranges: &search_ranges,
                },
                &cache,
            )
        };

        let mut response: Option<egui::Response> = None;
        let mut gutter: Option<(Arc<egui::Galley>, egui::Pos2)> = None;
        let mut scroll_output = None;
        let mut completion_anchor = None;
        let mut editor_clicked = false;
        ui.horizontal_top(|ui| {
            ui.spacing_mut().item_spacing.x = 0.0;
            ui.allocate_ui_with_layout(
                egui::vec2(viewport_width, full_height),
                Layout::top_down(Align::Min),
                |ui| {
                    scroll_output = Some(
                        ScrollArea::both()
                            .id_salt(scroll_id)
                            .auto_shrink([false, false])
                            .max_height(ui.available_height())
                            .show(ui, |ui| {
                                ui.set_min_width(viewport_width.max(80.0));
                                let output = TextEdit::multiline(&mut doc.buffer)
                                    .id(editor_id)
                                    .code_editor()
                                    .interactive(!read_only)
                                    .font(FontId::monospace(font_size))
                                    .desired_width(if word_wrap {
                                        viewport_width.max(80.0)
                                    } else {
                                        f32::INFINITY
                                    })
                                    .desired_rows(rows)
                                    .frame(Frame::new().inner_margin(Margin {
                                        left: text_margin_left,
                                        right: 14,
                                        top: 8,
                                        bottom: 8,
                                    }))
                                    .layouter(&mut layouter)
                                    .hint_text("开始输入代码…")
                                    .show(ui);

                                if let Some(cursor_range) = output.cursor_range {
                                    doc.update_cursor(cursor_range.primary.index);
                                    if focus_editor {
                                        // Clicking a tab or file should place the caret, not
                                        // preserve a stale cross-tab selection.
                                        let mut state = output.state;
                                        state.cursor.set_char_range(Some(
                                            egui::text::CCursorRange::one(cursor_range.primary),
                                        ));
                                        state.store(ui.ctx(), output.response.response.id);
                                    }
                                    let caret = output
                                        .galley
                                        .pos_from_cursor(egui::text::CCursor::new(doc.cursor_index))
                                        .translate(output.galley_pos.to_vec2());
                                    completion_anchor = Some(caret.left_bottom());
                                }
                                editor_clicked = output.response.response.clicked();
                                if scroll_cursor {
                                    let caret = output
                                        .galley
                                        .pos_from_cursor(egui::text::CCursor::new(doc.cursor_index))
                                        .translate(output.galley_pos.to_vec2());
                                    ui.scroll_to_rect(caret, None);
                                }
                                gutter = Some((Arc::clone(&output.galley), output.galley_pos));
                                response = Some(output.response.response);
                            }),
                    );
                },
            );
            if show_minimap {
                ui.add_space(8.0);
                if let Some(output) = &scroll_output {
                    crate::minimap::show(
                        ui,
                        crate::minimap::View {
                            text: doc.buffer.as_string(),
                            revision: doc.buffer.revision(),
                            scroll: output,
                            width: minimap_width,
                            height: full_height,
                            line_count,
                        },
                        &mut doc.minimap_cache,
                    );
                }
            }
        });
        let scroll_output = scroll_output.expect("scroll area is always created");
        if is_active_pane {
            if let Some(anchor) = completion_anchor {
                self.completion.anchor = Some(anchor);
            }
            if editor_clicked {
                self.completion.open = false;
            }
        }

        if let Some((galley, galley_pos)) = &gutter {
            let viewport = scroll_output.inner_rect;
            let pointer = ui.ctx().input(|input| input.pointer.interact_pos());
            let pointer_pressed = ui.ctx().input(|input| input.pointer.primary_pressed());
            let pointer_down = ui.ctx().input(|input| input.pointer.primary_down());
            let ctrl_alt = ui
                .ctx()
                .input(|input| input.modifiers.ctrl && input.modifiers.alt);
            if let Some(pointer) = pointer.filter(|pointer| viewport.contains(*pointer)) {
                let cursor = galley.cursor_from_pos(pointer - *galley_pos);
                let (line, column) =
                    crate::util::line_column_at_char(doc.buffer.as_string(), cursor.index);
                let line = line.saturating_sub(1);
                let column = column.saturating_sub(1);
                if ctrl_alt && pointer_pressed {
                    self.column_selection = Some(ColumnSelectionState {
                        path: doc.path.clone(),
                        anchor_line: line,
                        anchor_col: column,
                        cursor_line: line,
                        cursor_col: column,
                    });
                } else if ctrl_alt && pointer_down {
                    if let Some(selection) = self
                        .column_selection
                        .as_mut()
                        .filter(|selection| same_path(&selection.path, &doc.path))
                    {
                        selection.cursor_line = line;
                        selection.cursor_col = column;
                    }
                } else if pointer_pressed {
                    self.column_selection = None;
                }
            }
            if let Some(selection) = self
                .column_selection
                .as_ref()
                .filter(|selection| same_path(&selection.path, &doc.path))
            {
                let range = crate::column_select::ColumnRange::normalized(
                    selection.anchor_line,
                    selection.anchor_col,
                    selection.cursor_line,
                    selection.cursor_col,
                );
                let char_width = ui
                    .ctx()
                    .fonts_mut(|fonts| fonts.glyph_width(&FontId::monospace(font_size), '0'));
                let painter = ui.painter().with_clip_rect(viewport);
                let text = doc.buffer.as_string();
                for line in range.start_line..=range.end_line {
                    let line_start = crate::util::char_index_at_line(text, line + 1);
                    let line_len = text
                        .split('\n')
                        .nth(line)
                        .map(|value| value.chars().count())
                        .unwrap_or(0);
                    let start_char = line_start + range.start_col.min(line_len);
                    let end_char = line_start + range.end_col.min(line_len);
                    let start_rect = galley
                        .pos_from_cursor(egui::text::CCursor::new(start_char))
                        .translate(galley_pos.to_vec2());
                    let end_rect = galley
                        .pos_from_cursor(egui::text::CCursor::new(end_char))
                        .translate(galley_pos.to_vec2());
                    let mut x1 = start_rect.left();
                    let mut x2 = end_rect.left();
                    if range.start_col > line_len {
                        x1 += (range.start_col - line_len) as f32 * char_width;
                    }
                    if range.end_col > line_len {
                        x2 += (range.end_col - line_len) as f32 * char_width;
                    }
                    let top = start_rect.top().min(end_rect.top());
                    let height = start_rect
                        .height()
                        .max(end_rect.height())
                        .max(font_size * 1.2);
                    if top + height >= viewport.top() && top <= viewport.bottom() {
                        painter.rect_filled(
                            egui::Rect::from_min_max(
                                egui::pos2(x1, top),
                                egui::pos2(x2.max(x1 + 1.0), top + height),
                            ),
                            CornerRadius::ZERO,
                            Color32::from_rgba_unmultiplied(88, 196, 221, 55),
                        );
                    }
                }
            }
        }

        let bracket_match =
            crate::brackets::matching_bracket(doc.buffer.as_string(), doc.cursor_index);
        if let Some((galley, galley_pos)) = gutter {
            let viewport = scroll_output.inner_rect;
            let gutter_rect = egui::Rect::from_min_max(
                egui::pos2(viewport.left(), viewport.top()),
                egui::pos2(
                    (viewport.left() + gutter_width).min(viewport.right()),
                    viewport.bottom(),
                ),
            );
            let painter = ui.painter().with_clip_rect(viewport);
            if show_indent_guides {
                let active_level = doc
                    .buffer
                    .as_string()
                    .split('\n')
                    .nth(doc.cursor_line.saturating_sub(1))
                    .map(|line| leading_indent_columns(line, indent_width) / indent_width)
                    .unwrap_or(0);
                paint_indent_guides(
                    &painter,
                    &galley,
                    galley_pos,
                    viewport,
                    indent_width,
                    active_level,
                );
            }
            if ruler_column > 0 {
                let char_width = ui
                    .ctx()
                    .fonts_mut(|fonts| fonts.glyph_width(&FontId::monospace(font_size), '0'));
                let x = galley_pos.x + f32::from(ruler_column) * char_width;
                if x > gutter_rect.right() && x < viewport.right() {
                    painter.line_segment(
                        [
                            egui::pos2(x, viewport.top()),
                            egui::pos2(x, viewport.bottom()),
                        ],
                        Stroke::new(1.0_f32, Color32::from_rgba_unmultiplied(91, 103, 120, 90)),
                    );
                }
            }
            if let Some((first, second)) = bracket_match {
                let char_width = ui
                    .ctx()
                    .fonts_mut(|fonts| fonts.glyph_width(&FontId::monospace(font_size), '0'));
                for index in [first, second] {
                    let caret = galley
                        .pos_from_cursor(egui::text::CCursor::new(index))
                        .translate(galley_pos.to_vec2());
                    let rect = egui::Rect::from_min_size(
                        caret.min,
                        egui::vec2(char_width, caret.height()),
                    );
                    if rect.intersects(viewport) {
                        painter.rect_filled(
                            rect.expand(0.5),
                            CornerRadius::same(2),
                            Color32::from_rgba_unmultiplied(88, 196, 221, 45),
                        );
                        painter.rect_filled(
                            egui::Rect::from_min_max(
                                egui::pos2(rect.left(), rect.bottom() - 1.5),
                                egui::pos2(rect.right(), rect.bottom()),
                            ),
                            CornerRadius::ZERO,
                            theme::ACCENT,
                        );
                    }
                }
            }
            if show_whitespace {
                let color = Color32::from_rgba_unmultiplied(145, 157, 174, 120);
                for row in &galley.rows {
                    let row_top = galley_pos.y + row.pos.y;
                    if row_top + row.size.y < viewport.top() || row_top > viewport.bottom() {
                        continue;
                    }
                    let center_y = row_top + row.size.y * 0.5;
                    for glyph in &row.glyphs {
                        let x = galley_pos.x + row.pos.x + glyph.pos.x;
                        match glyph.chr {
                            ' ' => {
                                painter.circle_filled(
                                    egui::pos2(x + glyph.advance_width * 0.5, center_y),
                                    1.2,
                                    color,
                                );
                            }
                            '\t' => {
                                let y = center_y + 2.0;
                                let end = x + glyph.advance_width - 2.0;
                                painter.line_segment(
                                    [egui::pos2(x + 2.0, y), egui::pos2(end, y)],
                                    Stroke::new(1.0_f32, color),
                                );
                                painter.line_segment(
                                    [egui::pos2(end - 3.0, y - 2.0), egui::pos2(end, y)],
                                    Stroke::new(1.0_f32, color),
                                );
                                painter.line_segment(
                                    [egui::pos2(end - 3.0, y + 2.0), egui::pos2(end, y)],
                                    Stroke::new(1.0_f32, color),
                                );
                            }
                            _ => {}
                        }
                    }
                    if row.ends_with_newline {
                        let x = galley_pos.x + row.pos.x + row.size.x + 4.0;
                        let y = center_y - 1.0;
                        painter.line_segment(
                            [egui::pos2(x, y - 3.0), egui::pos2(x, y + 2.0)],
                            Stroke::new(1.0_f32, color),
                        );
                        painter.line_segment(
                            [egui::pos2(x - 4.0, y - 1.0), egui::pos2(x, y + 2.0)],
                            Stroke::new(1.0_f32, color),
                        );
                    }
                }
            }
            painter.rect_filled(gutter_rect, CornerRadius::ZERO, theme::EDITOR_BG);
            painter.line_segment(
                [
                    egui::pos2(gutter_rect.right(), gutter_rect.top()),
                    egui::pos2(gutter_rect.right(), gutter_rect.bottom()),
                ],
                Stroke::new(1.0_f32, theme::BORDER_SOFT),
            );

            let mut line_no = 1usize;
            let mut starts_line = true;
            for row in &galley.rows {
                let top = galley_pos.y + row.pos.y;
                let bottom = top + row.size.y;
                if starts_line && bottom >= viewport.top() && top <= viewport.bottom() {
                    if debug_line == Some(line_no) {
                        painter.rect_filled(
                            egui::Rect::from_min_max(
                                egui::pos2(gutter_rect.left(), top),
                                egui::pos2(viewport.right(), bottom),
                            ),
                            CornerRadius::ZERO,
                            Color32::from_rgba_unmultiplied(255, 196, 0, 26),
                        );
                    }
                    let color = if line_no == doc.cursor_line {
                        theme::TEXT
                    } else {
                        theme::TEXT_FAINT
                    };
                    let display_no = if relative_line_numbers && line_no != doc.cursor_line {
                        line_no.abs_diff(doc.cursor_line)
                    } else {
                        line_no
                    };
                    painter.text(
                        egui::pos2(gutter_rect.right() - 9.0, top + row.size.y * 0.5),
                        Align2::RIGHT_CENTER,
                        display_no.to_string(),
                        gutter_font.clone(),
                        color,
                    );
                    if doc.bookmarks.contains(&line_no) {
                        painter.rect_filled(
                            egui::Rect::from_min_size(
                                egui::pos2(gutter_rect.left() + 2.0, top + 3.0),
                                egui::vec2(2.0, (row.size.y - 6.0).max(4.0)),
                            ),
                            CornerRadius::same(1),
                            theme::ACCENT,
                        );
                    }
                    if breakpoints
                        .iter()
                        .any(|(path, line)| *line == line_no && same_path(path, &doc.path))
                    {
                        painter.circle_filled(
                            egui::pos2(gutter_rect.left() + 13.0, top + row.size.y * 0.5),
                            3.0,
                            theme::ERROR,
                        );
                    }
                    if debug_line == Some(line_no) {
                        let center = top + row.size.y * 0.5;
                        painter.add(Shape::convex_polygon(
                            vec![
                                egui::pos2(gutter_rect.left() + 3.0, center - 5.0),
                                egui::pos2(gutter_rect.left() + 3.0, center + 5.0),
                                egui::pos2(gutter_rect.left() + 9.0, center),
                            ],
                            theme::WARNING,
                            Stroke::NONE,
                        ));
                    }
                    if git_marks.contains(&line_no) {
                        painter.rect_filled(
                            egui::Rect::from_min_size(
                                egui::pos2(gutter_rect.left() + 7.0, top + 3.0),
                                egui::vec2(2.0, (row.size.y - 6.0).max(4.0)),
                            ),
                            CornerRadius::same(1),
                            theme::WARNING,
                        );
                    }
                    if let Some(severity) = markers.get(&line_no) {
                        let color = if *severity == Severity::Error {
                            theme::ERROR
                        } else {
                            theme::WARNING
                        };
                        painter.rect_filled(
                            egui::Rect::from_min_size(
                                egui::pos2(gutter_rect.right() - 3.0, top + 3.0),
                                egui::vec2(2.0, (row.size.y - 6.0).max(4.0)),
                            ),
                            CornerRadius::same(1),
                            color,
                        );
                    }
                }
                if row.ends_with_newline {
                    line_no += 1;
                    starts_line = true;
                } else {
                    starts_line = false;
                }
            }

            let gutter_response = ui
                .interact(
                    gutter_rect,
                    Id::new(("simple_embedded_gutter", &doc.path)),
                    Sense::click(),
                )
                .on_hover_cursor(egui::CursorIcon::PointingHand);
            // The text editor's frame spans the gutter, so the response above
            // can lose the click to the editor. Read the raw click as well to
            // keep the whole gutter clickable like VS Code.
            let (clicked, pointer) = ui.ctx().input(|input| {
                let clicked = input.pointer.primary_clicked() && !input.modifiers.ctrl;
                (clicked, input.pointer.interact_pos())
            });
            if gutter_response.clicked()
                || (clicked && pointer.is_some_and(|p| gutter_rect.contains(p)))
            {
                if let Some(pointer) = pointer.or_else(|| gutter_response.interact_pointer_pos()) {
                    // Use the same galley hit-testing as the text editor. Deriving
                    // the line from row positions alone was off by the scroll
                    // offset and could set breakpoints on unrelated lines.
                    let cursor = galley.cursor_from_pos(pointer - galley_pos);
                    let (line, _) =
                        crate::util::line_column_at_char(doc.buffer.as_string(), cursor.index);
                    if line > 0 {
                        actions.push(Action::ToggleBreakpointAt(doc.path.clone(), line));
                    }
                }
            }
        }

        if let Some(response) = &response {
            if response.has_focus() {
                self.active = Some(pane_index);
                if let Some(slot) = pane_slot {
                    self.split_focus = slot;
                }
            }
        }
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
        let has_makefile = self
            .project
            .as_ref()
            .is_some_and(|project| makefile::find_makefile(&project.root).is_some());
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
                    RichText::new("保存在工程 .simple-embedded/settings.json")
                        .size(10.5)
                        .color(theme::TEXT_FAINT),
                );
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    if window_close_button(ui) {
                        close = true;
                    }
                });
            });
            ui.add_space(2.0);
            ui.label(
                RichText::new(
                    "Make 工程保存时会按差异同步到 Makefile；Keil 工程写入工程配置，不修改 .uvprojx。",
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
                                &["-O0", "-O1", "-O2", "-O3", "-Og", "-Os", "-Oz"],
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
            if has_makefile {
                ui.label(
                    RichText::new("保存后会同步 GCC 支持的选项到 Makefile（修改前自动备份）")
                        .size(10.5)
                        .color(theme::TEXT_FAINT),
                );
                ui.add_space(4.0);
            }
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

    pub(super) fn show_flash_modal(&mut self, ctx: &egui::Context) {
        let Some(mut draft) = self.flash_draft.take() else {
            return;
        };
        let root = self.project.as_ref().map(|project| project.root.clone());
        let download_address = self.build_config.download_address();
        let mut close = false;
        let mut save = false;

        let modal = egui::Modal::new(Id::new("simple_embedded_flash_modal"))
            .frame(
                Frame::new()
                    .fill(theme::PANEL)
                    .stroke(Stroke::new(1.0_f32, theme::BORDER))
                    .corner_radius(10)
                    .inner_margin(Margin::symmetric(20, 16)),
            )
            .backdrop_color(Color32::from_black_alpha(170));
        modal.show(ctx, |ui| {
            ui.set_width(560.0);
            ui.horizontal(|ui| {
                ui.label(
                    RichText::new("烧录配置")
                        .size(18.0)
                        .strong()
                        .color(theme::TEXT),
                );
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    if window_close_button(ui) {
                        close = true;
                    }
                });
            });
            ui.add_space(2.0);
            ui.label(
                RichText::new("保存在工程 .simple-embedded/settings.json；烧录始终在本地工程目录执行。")
                    .size(10.5)
                    .color(theme::TEXT_FAINT),
            );
            ui.add_space(10.0);

            ui.horizontal(|ui| {
                if segment_button(ui, draft.tool == FlashTool::Custom, "自定义命令") {
                    draft.tool = FlashTool::Custom;
                    draft.error = None;
                }
                if segment_button(ui, draft.tool == FlashTool::OpenOcd, "OpenOCD") {
                    draft.tool = FlashTool::OpenOcd;
                    self.apply_openocd_defaults(&mut draft.openocd);
                    draft.error = None;
                }
                if segment_button(ui, draft.tool == FlashTool::PyOcd, "pyOCD") {
                    draft.tool = FlashTool::PyOcd;
                    self.apply_pyocd_defaults(&mut draft.pyocd);
                    draft.error = None;
                }
            });
            ui.add_space(10.0);

            match draft.tool {
                FlashTool::Custom => {
                    form_line(
                        ui,
                        "命令",
                        &mut draft.command,
                        "例如 JLink.exe -CommanderScript flash.jlink",
                    );
                    ui.add_space(4.0);
                    ui.label(
                        RichText::new(
                            "命令在工程根目录执行，留空则烧录按钮禁用；需要自行保证工具在 PATH 中。",
                        )
                        .size(9.5)
                        .color(theme::TEXT_FAINT),
                    );
                }
                FlashTool::OpenOcd => {
                    settings_path_row(
                        ui,
                        "OpenOCD",
                        &mut draft.openocd.executable,
                        "选择 openocd 可执行文件",
                        "openocd",
                    );
                    ui.add_space(4.0);
                    form_line(
                        ui,
                        "程序文件",
                        &mut draft.openocd.program_file,
                        "${ExecutableName}.hex",
                    );
                    ui.add_space(4.0);
                    form_line(
                        ui,
                        "芯片配置",
                        &mut draft.openocd.target_config,
                        "例如 stm32f1x.cfg",
                    );
                    ui.add_space(4.0);
                    form_line(
                        ui,
                        "接口配置",
                        &mut draft.openocd.interface_config,
                        "例如 stlink.cfg",
                    );
                    ui.add_space(5.0);
                    ui.label(
                        RichText::new(
                            "程序文件支持 ${ExecutableName}、${ProjectName}、${workspaceFolder}；芯片/接口只写文件名时自动补 target/ 或 interface/。",
                        )
                        .size(9.5)
                        .color(theme::TEXT_FAINT),
                    );
                    if root.as_deref().is_some_and(|root| {
                        !flash::openocd_available(&draft.openocd.executable, root)
                    }) {
                        ui.add_space(4.0);
                        ui.label(
                            RichText::new(
                                "当前路径未找到 OpenOCD；可以点“浏览”选择 openocd.exe，或稍后再安装。",
                            )
                            .size(9.5)
                            .color(theme::WARNING),
                        );
                    }
                    if let Some(root) = &root {
                        ui.add_space(8.0);
                        ui.label(
                            RichText::new("命令预览")
                                .size(10.5)
                                .color(theme::TEXT_DIM),
                        );
                        ui.add_space(3.0);
                        match flash::preview(&draft.openocd, root, download_address.as_deref()) {
                            Ok(command) => {
                                Frame::new()
                                    .fill(theme::BG)
                                    .stroke(Stroke::new(1.0_f32, theme::BORDER_SOFT))
                                    .corner_radius(6)
                                    .inner_margin(Margin::symmetric(7, 5))
                                    .show(ui, |ui| {
                                        ui.add(
                                            Label::new(
                                                RichText::new(command)
                                                    .size(10.5)
                                                    .monospace()
                                                    .color(theme::TEXT_DIM),
                                            )
                                            .wrap(),
                                        );
                                    });
                            }
                            Err(err) => {
                                ui.label(
                                    RichText::new(format!("命令预览不可用：{err}"))
                                        .size(9.5)
                                        .color(theme::WARNING),
                                );
                            }
                        }
                    }
                }
                FlashTool::PyOcd => {
                    settings_path_row(
                        ui,
                        "pyOCD",
                        &mut draft.pyocd.executable,
                        "选择 pyocd 可执行文件",
                        "pyocd",
                    );
                    ui.add_space(4.0);
                    form_line(
                        ui,
                        "目标芯片",
                        &mut draft.pyocd.target,
                        "例如 stm32f103xb",
                    );
                    ui.add_space(4.0);
                    settings_path_row(
                        ui,
                        "Pack",
                        &mut draft.pyocd.pack,
                        "选择 CMSIS Pack",
                        "可选，例如 Keil.STM32F1xx_DFP.pack",
                    );
                    ui.add_space(4.0);
                    form_line(
                        ui,
                        "程序文件",
                        &mut draft.pyocd.program_file,
                        "${ExecutableName}.hex",
                    );
                    ui.add_space(5.0);
                    ui.label(
                        RichText::new(
                            "目标芯片来自工程芯片名时自动转小写；Pack 可留空，程序文件支持 ${ExecutableName} 等变量。",
                        )
                        .size(9.5)
                        .color(theme::TEXT_FAINT),
                    );
                    if root
                        .as_deref()
                        .is_some_and(|root| !flash::pyocd_available(&draft.pyocd.executable, root))
                    {
                        ui.add_space(4.0);
                        ui.label(
                            RichText::new(
                                "当前路径未找到 pyOCD；可以点“浏览”选择 pyocd.exe，或先运行 pip install pyocd。",
                            )
                            .size(9.5)
                            .color(theme::WARNING),
                        );
                    }
                    if let Some(root) = &root {
                        ui.add_space(8.0);
                        ui.label(
                            RichText::new("命令预览")
                                .size(10.5)
                                .color(theme::TEXT_DIM),
                        );
                        ui.add_space(3.0);
                        match flash::preview_pyocd(
                            &draft.pyocd,
                            root,
                            download_address.as_deref(),
                        ) {
                            Ok(command) => {
                                Frame::new()
                                    .fill(theme::BG)
                                    .stroke(Stroke::new(1.0_f32, theme::BORDER_SOFT))
                                    .corner_radius(6)
                                    .inner_margin(Margin::symmetric(7, 5))
                                    .show(ui, |ui| {
                                        ui.add(
                                            Label::new(
                                                RichText::new(command)
                                                    .size(10.5)
                                                    .monospace()
                                                    .color(theme::TEXT_DIM),
                                            )
                                            .wrap(),
                                        );
                                    });
                            }
                            Err(err) => {
                                ui.label(
                                    RichText::new(format!("命令预览不可用：{err}"))
                                        .size(9.5)
                                        .color(theme::WARNING),
                                );
                            }
                        }
                    }
                }
            }

            if let Some(error) = &draft.error {
                ui.add_space(8.0);
                ui.label(RichText::new(error).size(10.5).color(theme::ERROR));
            }

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

        if save {
            let valid = match draft.tool {
                FlashTool::Custom => true,
                FlashTool::OpenOcd => {
                    draft.openocd = draft.openocd.normalized();
                    if !flash::openocd_ready(&draft.openocd) {
                        draft.error =
                            Some("OpenOCD 需要填写程序文件、芯片配置和接口配置".to_owned());
                        false
                    } else {
                        true
                    }
                }
                FlashTool::PyOcd => {
                    draft.pyocd = draft.pyocd.normalized();
                    if !flash::pyocd_ready(&draft.pyocd) {
                        draft.error = Some("pyOCD 需要填写目标芯片和程序文件".to_owned());
                        false
                    } else {
                        true
                    }
                }
            };
            if !valid {
                self.flash_draft = Some(draft);
                return;
            }
            self.save_flash_config(draft);
        } else if !close {
            self.flash_draft = Some(draft);
        }
    }

    pub(super) fn show_meta_modal(&mut self, ctx: &egui::Context) {
        let Some(mut draft) = self.meta_draft.take() else {
            return;
        };
        let has_makefile = self
            .project
            .as_ref()
            .is_some_and(|project| makefile::find_makefile(&project.root).is_some());
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
                    if window_close_button(ui) {
                        close = true;
                    }
                });
            });
            ui.add_space(2.0);
            ui.label(
                RichText::new("保存在工程 .simple-embedded/settings.json；Makefile 工程会按差异同步，Keil .uvprojx 不会被修改。")
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
            if has_makefile {
                ui.add_space(8.0);
                ui.label(
                    RichText::new(
                        "保存后会同步包含目录 / 库目录 / 宏定义到 Makefile（修改前自动备份）",
                    )
                    .size(10.5)
                    .color(theme::TEXT_FAINT),
                );
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
                    if window_close_button(ui) {
                        close = true;
                    }
                });
            });
            ui.add_space(3.0);
            ui.label(
                RichText::new("保存在工程 .simple-embedded/settings.json；Makefile 工程会按差异同步，Keil .uvprojx 不会被修改。")
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
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    if window_close_button(ui) {
                        close = true;
                    }
                });
            });
            ui.add_space(8.0);
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing.x = 4.0;
                for (tab, label) in [
                    (SettingsTab::Build, "构建"),
                    (SettingsTab::Toolchain, "构建工具链"),
                    (SettingsTab::Editor, "编辑器"),
                    (SettingsTab::Debug, "调试"),
                    (SettingsTab::Other, "其他"),
                ] {
                    if settings_tab_button(ui, draft.tab, tab, label) {
                        draft.tab = tab;
                    }
                }
            });
            ui.add_space(6.0);

            ScrollArea::vertical()
                .id_salt("simple_embedded_settings_scroll")
                .max_height(440.0)
                .auto_shrink([false, false])
                .show(ui, |ui| {
                    if draft.tab == SettingsTab::Build {
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
                        if draft.config.flash_tool == crate::flash::FlashTool::Custom {
                            form_line(
                                ui,
                                "Flash",
                                &mut draft.config.flash_command,
                                "可选，例如 JLink.exe -CommanderScript flash.jlink",
                            );
                            ui.add_space(4.0);
                            ui.label(
                                RichText::new(
                                    "Flash 始终在本地工程目录执行；留空则烧录按钮禁用。",
                                )
                                .size(9.0)
                                .color(theme::TEXT_FAINT),
                            );
                        } else {
                            ui.label(
                                RichText::new(format!(
                                    "当前烧录工具为 {}。参数请在左侧“工程信息 → 烧录配置”中编辑。",
                                    draft.config.flash_tool.label()
                                ))
                                .size(9.5)
                                .color(theme::TEXT_FAINT),
                            );
                        }

                        ui.add_space(12.0);
                        settings_section(ui, "并行构建");
                        ui.add_space(3.0);
                        ui.horizontal(|ui| {
                            ui.add_sized(
                                [86.0, 22.0],
                                Label::new(
                                    RichText::new("并行任务数")
                                        .size(10.5)
                                        .color(theme::TEXT_DIM),
                                ),
                            );
                            ui.add(
                                egui::DragValue::new(&mut draft.config.build_jobs)
                                    .range(0..=128)
                                    .speed(1),
                            );
                            if ui
                                .add(small_flat_button("使用 CPU 核心数", theme::TEXT_FAINT))
                                .on_hover_text("把并行任务数设置为本机逻辑核心数")
                                .clicked()
                            {
                                draft.config.build_jobs = std::thread::available_parallelism()
                                    .map(|count| count.get())
                                    .unwrap_or(1)
                                    .min(128) as u16;
                            }
                        });
                        ui.add_space(3.0);
                        ui.label(
                            RichText::new(
                                "0 = 不启用。Make 使用 MAKEFLAGS=-jN，Cargo 使用 CARGO_BUILD_JOBS，CMake 使用 CMAKE_BUILD_PARALLEL_LEVEL；自定义脚本需要自行处理。",
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
                    }

                    if draft.tab == SettingsTab::Toolchain {
                        self.settings_toolchain_tab(ui, &mut draft);
                    }
                    if draft.tab == SettingsTab::Editor {
                    ui.add_space(8.0);
                    settings_section(ui, "编辑器");
                    ui.add_space(4.0);
                    ui.add(
                        egui::Slider::new(&mut draft.font_size, 10.0..=28.0)
                            .text("代码字号")
                            .fixed_decimals(0),
                    );
                    ui.checkbox(&mut draft.word_wrap, "长行自动换行");
                    ui.horizontal(|ui| {
                        ui.checkbox(&mut draft.show_minimap, "代码预览小地图");
                        ui.checkbox(&mut draft.show_indent_guides, "缩进参考线");
                    });
                    ui.horizontal(|ui| {
                        ui.checkbox(&mut draft.relative_line_numbers, "相对行号");
                        ui.checkbox(&mut draft.show_whitespace, "显示空白字符");
                        ui.label(
                            RichText::new("换行标尺")
                                .size(11.5)
                                .color(theme::TEXT_DIM),
                        );
                        egui::ComboBox::from_id_salt("simple_embedded_ruler_column")
                            .selected_text(match draft.ruler_column {
                                80 => "80 列",
                                120 => "120 列",
                                _ => "关闭",
                            })
                            .show_ui(ui, |ui| {
                                ui.selectable_value(&mut draft.ruler_column, 0, "关闭");
                                ui.selectable_value(&mut draft.ruler_column, 80, "80 列");
                                ui.selectable_value(&mut draft.ruler_column, 120, "120 列");
                            });
                    });
                    ui.horizontal(|ui| {
                        ui.checkbox(&mut draft.use_spaces, "空格缩进");
                        ui.add(
                            egui::Slider::new(&mut draft.indent_width, 1..=8)
                                .text("缩进宽度"),
                        );
                    });
                    ui.horizontal(|ui| {
                        ui.checkbox(&mut draft.autosave, "自动保存");
                        ui.add(
                            egui::Slider::new(&mut draft.autosave_delay_ms, 500..=5000)
                                .text("停止输入后 (ms)"),
                        );
                        ui.checkbox(&mut draft.format_on_save, "保存时 clang-format");
                    });
                    ui.horizontal(|ui| {
                        ui.label(
                            RichText::new("clang-format")
                                .size(11.5)
                                .color(theme::TEXT_DIM),
                        );
                        ui.add(
                            TextEdit::singleline(&mut draft.clang_format_path)
                                .desired_width(360.0)
                                .hint_text("留空自动查找，或填写 clang-format.exe 路径"),
                        );
                    });
                    }

                    if draft.tab == SettingsTab::Debug {
                        ui.add_space(8.0);
                        settings_section(ui, "调试");
                        if self.project.is_some() {
                            ui.add_space(4.0);
                            ui.label(
                                RichText::new("调试程序与 gdb 路径随当前工程保存")
                                    .size(9.5)
                                    .color(theme::TEXT_FAINT),
                            );
                        }
                        ui.add_space(4.0);
                        ui.horizontal(|ui| {
                            ui.label(
                                RichText::new("调试程序")
                                    .size(11.5)
                                    .color(theme::TEXT_DIM),
                            );
                            ui.add(
                                TextEdit::singleline(&mut draft.debug_program)
                                    .desired_width(260.0)
                                    .hint_text("留空自动查找 build/*.elf / *.axf / *.exe"),
                            );
                            if ui.button("浏览").clicked() {
                                let mut dialog = rfd::FileDialog::new()
                                    .set_title("选择调试程序")
                                    .add_filter("调试程序", &["elf", "axf", "exe", "out"]);
                                let current = draft.debug_program.trim();
                                if let Some(parent) = Path::new(current)
                                    .parent()
                                    .filter(|parent| parent.is_dir())
                                {
                                    dialog = dialog.set_directory(parent);
                                }
                                if let Some(path) = dialog.pick_file() {
                                    draft.debug_program = util::display_path(&path);
                                }
                            }
                        });
                        ui.add_space(4.0);
                        ui.horizontal(|ui| {
                            ui.label(
                                RichText::new("gdb 路径")
                                    .size(11.5)
                                    .color(theme::TEXT_DIM),
                            );
                            ui.add(
                                TextEdit::singleline(&mut draft.gdb_path)
                                    .desired_width(260.0)
                                    .hint_text("留空自动查找；ARM 用 arm-none-eabi-gdb"),
                            );
                            if ui.button("浏览").clicked() {
                                let mut dialog = rfd::FileDialog::new()
                                    .set_title("选择 GDB 可执行文件")
                                    .add_filter("GDB", &["exe"])
                                    .add_filter("所有文件", &["*"]);
                                let current = draft.gdb_path.trim();
                                if let Some(parent) = Path::new(current)
                                    .parent()
                                    .filter(|parent| parent.is_dir())
                                {
                                    dialog = dialog.set_directory(parent);
                                }
                                if let Some(path) = dialog.pick_file() {
                                    draft.gdb_path = util::display_path(&path);
                                }
                            }
                            if ui
                                .button("自动检测")
                                .on_hover_text("查找 arm-none-eabi-gdb")
                                .clicked()
                            {
                                let dirs = self.arm_gdb_dirs();
                                if let Some(path) =
                                    crate::debugger::detect_arm_gdb(&draft.gdb_path, &dirs)
                                {
                                    draft.gdb_path = util::display_path(&path);
                                    draft.error = None;
                                    draft.note =
                                        Some(format!("已找到 gdb：{}", draft.gdb_path));
                                } else {
                                    draft.note = None;
                                    draft.error = Some(
                                        "未找到 arm-none-eabi-gdb，请用“浏览”选择文件"
                                            .to_owned(),
                                    );
                                }
                            }
                        });
                        ui.add_space(6.0);
                        ui.label(
                            RichText::new(
                                "调试程序留空时会从 build/、Debug/、Release/ 中查找最新的 ELF/AXF/EXE。gdb 路径可以填 arm-none-eabi-gdb.exe，也可以直接填它的 bin 目录。ARM 目标会复用烧录配置中的 OpenOCD / pyOCD 参数，自动启动 GDB server 后连接 arm-none-eabi-gdb；主机 EXE 使用普通 gdb。",
                            )
                            .size(9.0)
                            .color(theme::TEXT_FAINT),
                        );
                        if draft.config.flash_tool == FlashTool::Custom {
                            ui.add_space(4.0);
                            ui.label(
                                RichText::new(
                                    "当前烧录工具是“自定义命令”。调试 ARM 目标前，请先在“工程信息 → 烧录配置”选择 OpenOCD 或 pyOCD。",
                                )
                                .size(9.5)
                                .color(theme::WARNING),
                            );
                        }
                    }

                    if draft.tab == SettingsTab::Other {
                        ui.add_space(8.0);
                        settings_section(ui, "文件与输出");
                        ui.add_space(4.0);
                        ui.horizontal(|ui| {
                            ui.checkbox(&mut draft.show_hidden, "显示隐藏文件");
                            ui.checkbox(&mut draft.auto_scroll_output, "输出自动滚动");
                        });
                        ui.horizontal(|ui| {
                            ui.checkbox(&mut draft.respect_gitignore, "遵循 .gitignore");
                            ui.label(
                                RichText::new("额外忽略规则")
                                    .size(11.5)
                                    .color(theme::TEXT_DIM),
                            );
                            ui.add(
                                TextEdit::singleline(&mut draft.ignore_patterns)
                                    .desired_width(260.0)
                                    .hint_text("build/; *.tmp"),
                            );
                        });
                        ui.add_space(6.0);
                        ui.label(
                            RichText::new(format!(
                                "全局设置保存在 {}；工程构建、烧录与调试配置保存在工程的 .simple-embedded/settings.json。",
                                settings_path().display()
                            ))
                            .size(9.0)
                            .color(theme::TEXT_FAINT),
                        );
                    }
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

            let mut tool_error = None;
            for (label, value) in [
                ("编译器", draft.compiler_path.trim()),
                ("make", draft.make_path.trim()),
            ] {
                if !value.is_empty() && !Path::new(value).is_file() {
                    tool_error = Some(format!("{label}路径不存在：{value}"));
                    break;
                }
            }
            if tool_error.is_none() && draft.use_bundled_toolchain {
                let root = crate::toolchain::resolve_tools_dir(&draft.bundled_tools_dir);
                let tools = crate::toolchain::detect(&root);
                if tools.compiler.is_none() || tools.make.is_none() {
                    tool_error = Some(format!(
                        "软件自带工具未安装，请先点击“安装当前工具到此目录”，或切换到系统工具。目录：{}",
                        util::display_path(&root)
                    ));
                }
            }
            if let Some(err) = tool_error {
                draft.error = Some(err);
                draft.tab = SettingsTab::Toolchain;
                self.settings_draft = Some(draft);
                return;
            }

            self.settings.font_size = draft.font_size.clamp(10.0, 28.0);
            self.settings.word_wrap = draft.word_wrap;
            self.settings.relative_line_numbers = draft.relative_line_numbers;
            self.settings.show_whitespace = draft.show_whitespace;
            self.settings.ruler_column = if matches!(draft.ruler_column, 80 | 120) {
                draft.ruler_column
            } else {
                0
            };
            self.settings.use_spaces = draft.use_spaces;
            self.settings.indent_width = draft.indent_width.clamp(1, 8);
            self.settings.autosave = draft.autosave;
            self.settings.autosave_delay_ms = draft.autosave_delay_ms.clamp(500, 10_000);
            self.settings.format_on_save = draft.format_on_save;
            self.settings.clang_format_path = draft.clang_format_path.trim().to_owned();
            self.settings.compiler_path = draft.compiler_path.trim().to_owned();
            self.settings.make_path = draft.make_path.trim().to_owned();
            self.settings.use_bundled_toolchain = draft.use_bundled_toolchain;
            self.settings.bundled_tools_dir = draft.bundled_tools_dir.trim().to_owned();
            if project_root.is_some() {
                self.project_debug.program = draft.debug_program.trim().to_owned();
                self.project_debug.gdb_path = draft.gdb_path.trim().to_owned();
                self.settings.gdb_path = draft.gdb_path.trim().to_owned();
            } else {
                self.settings.debug_program = draft.debug_program.trim().to_owned();
                self.settings.gdb_path = draft.gdb_path.trim().to_owned();
            }
            self.settings.auto_scroll_output = draft.auto_scroll_output;
            self.settings.show_minimap = draft.show_minimap;
            self.settings.show_indent_guides = draft.show_indent_guides;
            self.settings.show_hidden = draft.show_hidden;
            self.settings.respect_gitignore = draft.respect_gitignore;
            self.settings.ignore_patterns = draft.ignore_patterns.trim().to_owned();
            self.settings.save();

            if project_root.is_some() {
                self.build_config = draft.config.clone();
                self.persist_build_config();
                self.debug.target = self.debug_target();
            }
            if let Some(tree) = &mut self.tree {
                tree.set_show_hidden(draft.show_hidden);
                tree.set_ignore_rules(
                    self.settings.respect_gitignore,
                    &self.settings.ignore_patterns,
                );
            }
            self.build.auto_scroll = draft.auto_scroll_output;
            self.settings_draft = None;
            self.set_toast("设置已保存", Severity::Success);
        } else {
            self.settings_draft = Some(draft);
        }
    }

    fn settings_toolchain_tab(&mut self, ui: &mut egui::Ui, draft: &mut SettingsDraft) {
        ui.add_space(8.0);
        settings_section(ui, "工具来源");
        ui.add_space(2.0);
        ui.label(
            RichText::new(
                "系统工具会从 PATH 查找；也可以填写编译器 / make 的完整路径，构建时自动把所在目录加入 PATH。",
            )
            .size(9.0)
            .color(theme::TEXT_FAINT),
        );
        ui.add_space(5.0);
        ui.horizontal(|ui| {
            if segment_button(ui, !draft.use_bundled_toolchain, "系统已安装") {
                draft.use_bundled_toolchain = false;
            }
            if segment_button(ui, draft.use_bundled_toolchain, "软件自带") {
                draft.use_bundled_toolchain = true;
            }
        });
        ui.add_space(8.0);
        settings_path_row(
            ui,
            "编译器",
            &mut draft.compiler_path,
            "选择编译器",
            "例如 arm-none-eabi-gcc.exe；留空使用 PATH",
        );
        ui.add_space(3.0);
        settings_path_row(
            ui,
            "make 工具",
            &mut draft.make_path,
            "选择 make",
            "例如 make.exe / mingw32-make.exe；留空使用 PATH",
        );
        ui.add_space(5.0);
        if ui
            .add(theme::secondary_button("从 PATH 自动检测"))
            .on_hover_text("查找 arm-none-eabi-gcc / gcc / clang 以及 make / mingw32-make")
            .clicked()
        {
            let (compiler, make) = crate::toolchain::detect_installed();
            let mut found = false;
            if let Some(compiler) = compiler {
                draft.compiler_path = util::display_path(&compiler);
                found = true;
            }
            if let Some(make) = make {
                draft.make_path = util::display_path(&make);
                found = true;
            }
            draft.error = if found {
                None
            } else {
                Some("PATH 中没有找到编译器或 make，请手动浏览选择。".to_owned())
            };
        }

        ui.add_space(12.0);
        settings_section(ui, "软件自带工具");
        ui.add_space(2.0);
        ui.label(
            RichText::new(
                "把当前选择的编译器和 make 复制到软件目录，之后即使系统 PATH 变化也能直接构建。复制编译器会包含整个工具链目录，可能需要一些时间。",
            )
            .size(9.0)
            .color(theme::TEXT_FAINT),
        );
        ui.add_space(5.0);
        settings_folder_row(
            ui,
            "工具目录",
            &mut draft.bundled_tools_dir,
            "选择软件自带工具目录",
            "留空使用用户目录；相对路径相对于程序目录",
        );
        let tools_root = crate::toolchain::resolve_tools_dir(&draft.bundled_tools_dir);
        let tools = crate::toolchain::detect(&tools_root);
        ui.add_space(4.0);
        ui.label(
            RichText::new(format!("当前目录：{}", util::display_path(&tools_root)))
                .size(9.0)
                .color(theme::TEXT_FAINT),
        );
        tool_status_row(ui, "编译器", tools.compiler.as_deref());
        tool_status_row(ui, "make", tools.make.as_deref());
        ui.add_space(6.0);
        let installing = self.toolchain_install.is_some();
        let can_install = !installing
            && !draft.compiler_path.trim().is_empty()
            && !draft.make_path.trim().is_empty();
        ui.horizontal(|ui| {
            let install =
                ui.add_enabled(can_install, theme::primary_button("安装当前工具到此目录"));
            if install
                .on_hover_text("复制编译器和 make 到上面的软件目录")
                .clicked()
            {
                let compiler = PathBuf::from(draft.compiler_path.trim());
                let make = PathBuf::from(draft.make_path.trim());
                if !compiler.is_file() {
                    draft.error = Some(format!("编译器不存在：{}", compiler.display()));
                } else if !make.is_file() {
                    draft.error = Some(format!("make 不存在：{}", make.display()));
                } else if let Err(err) = std::fs::create_dir_all(&tools_root) {
                    draft.error = Some(format!("无法创建 {}：{err}", tools_root.display()));
                } else {
                    draft.error = None;
                    self.toolchain_status = "准备安装…".to_owned();
                    self.toolchain_install = Some(crate::toolchain::install_async(
                        compiler,
                        make,
                        tools_root.clone(),
                    ));
                }
            }
            if ui.add(theme::secondary_button("打开目录")).clicked() {
                let _ = std::fs::create_dir_all(&tools_root);
                util::open_in_file_manager(&tools_root);
            }
            if installing {
                ui.spinner();
            }
        });
        if !self.toolchain_status.is_empty() {
            ui.add_space(3.0);
            ui.label(
                RichText::new(&self.toolchain_status)
                    .size(9.5)
                    .color(theme::TEXT_DIM),
            );
        }
        ui.add_space(6.0);
        ui.label(
            RichText::new("软件自带模式只影响本地构建和本地烧录；远程 SSH 构建仍使用远端工具。")
                .size(9.0)
                .color(theme::TEXT_FAINT),
        );
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

fn apply_paste_indent(doc: &mut Document, ctx: &egui::Context, editor_id: Id, pasted: &str) {
    let mut state = egui::text_edit::TextEditState::load(ctx, editor_id).unwrap_or_default();
    let (start, end) = state
        .cursor
        .char_range()
        .map(|range| {
            let first = range.primary.index.min(range.secondary.index);
            let last = range.primary.index.max(range.secondary.index);
            (first, last)
        })
        .unwrap_or((doc.cursor_index, doc.cursor_index));
    let text = doc.buffer.as_string();
    let chars = text.chars().collect::<Vec<_>>();
    let mut line_start = start.min(chars.len());
    while line_start > 0 && chars[line_start - 1] != '\n' {
        line_start -= 1;
    }
    let indent = chars[line_start..start.min(chars.len())]
        .iter()
        .take_while(|ch| **ch == ' ' || **ch == '\t')
        .collect::<String>();
    let adjusted = crate::indent::adjust_pasted(pasted, &indent);
    let (new_text, cursor) = crate::navigation::replace_range_chars(text, start, end, &adjusted);
    doc.buffer.set_text(new_text);
    doc.update_cursor(cursor);
    state
        .cursor
        .set_char_range(Some(egui::text::CCursorRange::one(
            egui::text::CCursor::new(cursor),
        )));
    state.store(ctx, editor_id);
}

fn apply_enter_indent(
    doc: &mut Document,
    ctx: &egui::Context,
    editor_id: Id,
    width: usize,
    use_spaces: bool,
) {
    let mut state = egui::text_edit::TextEditState::load(ctx, editor_id).unwrap_or_default();
    let (start, end) = state
        .cursor
        .char_range()
        .map(|range| {
            let first = range.primary.index.min(range.secondary.index);
            let last = range.primary.index.max(range.secondary.index);
            (first, last)
        })
        .unwrap_or((doc.cursor_index, doc.cursor_index));
    let (new_text, cursor) =
        crate::indent::auto_indent(doc.buffer.as_string(), start, end, width, use_spaces);
    doc.buffer.set_text(new_text);
    doc.update_cursor(cursor);
    state
        .cursor
        .set_char_range(Some(egui::text::CCursorRange::one(
            egui::text::CCursor::new(cursor),
        )));
    state.store(ctx, editor_id);
}

fn apply_tab_indent(
    doc: &mut Document,
    ctx: &egui::Context,
    editor_id: Id,
    unindent: bool,
    width: usize,
    use_spaces: bool,
) {
    let mut state = egui::text_edit::TextEditState::load(ctx, editor_id).unwrap_or_default();
    let (start, end) = state
        .cursor
        .char_range()
        .map(|range| {
            let first = range.primary.index.min(range.secondary.index);
            let last = range.primary.index.max(range.secondary.index);
            (first, last)
        })
        .unwrap_or((doc.cursor_index, doc.cursor_index));
    let text = doc.buffer.as_string();
    if start == end && !unindent {
        let (new_text, cursor) = crate::indent::insert_at_cursor(text, start, width, use_spaces);
        doc.buffer.set_text(new_text);
        doc.update_cursor(cursor);
        state
            .cursor
            .set_char_range(Some(egui::text::CCursorRange::one(
                egui::text::CCursor::new(cursor),
            )));
        state.store(ctx, editor_id);
    } else {
        let (new_text, selection_start, selection_end) =
            crate::indent::change_lines(text, start, end, width, use_spaces, unindent);
        doc.buffer.set_text(new_text);
        doc.update_cursor(selection_start);
        let range = if start == end {
            egui::text::CCursorRange::one(egui::text::CCursor::new(selection_start))
        } else {
            egui::text::CCursorRange::two(
                egui::text::CCursor::new(selection_start),
                egui::text::CCursor::new(selection_end),
            )
        };
        state.cursor.set_char_range(Some(range));
        state.store(ctx, editor_id);
    }
}

fn info_header_row(ui: &mut egui::Ui, info_open: &mut bool, openness: f32) {
    Frame::new()
        .inner_margin(Margin::symmetric(11, 6))
        .show(ui, |ui| {
            ui.set_min_height(20.0);
            ui.horizontal(|ui| {
                if section_chevron(ui, openness).clicked() {
                    *info_open = !*info_open;
                }
                let (bar_rect, _) = ui.allocate_exact_size(egui::vec2(3.0, 13.0), Sense::hover());
                ui.painter()
                    .rect_filled(bar_rect, CornerRadius::same(1), theme::ACCENT);
                if ui
                    .add(
                        Button::new(
                            RichText::new("工程信息")
                                .size(12.5)
                                .strong()
                                .color(theme::TEXT_DIM),
                        )
                        .frame(false),
                    )
                    .clicked()
                {
                    *info_open = !*info_open;
                }
            });
        });
}

fn section_chevron(ui: &mut egui::Ui, openness: f32) -> egui::Response {
    let (rect, response) = ui.allocate_exact_size(egui::vec2(16.0, 18.0), Sense::click());
    paint_chevron_openness(ui.painter(), rect, openness, theme::ACCENT);
    response.on_hover_cursor(egui::CursorIcon::PointingHand)
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
    meta_group_impl(ui, title, default_open, None, content);
}

fn meta_group_with_add(
    ui: &mut egui::Ui,
    title: &str,
    default_open: bool,
    add_tooltip: &str,
    add_clicked: &mut bool,
    content: impl FnOnce(&mut egui::Ui),
) {
    meta_group_impl(
        ui,
        title,
        default_open,
        Some((add_tooltip, add_clicked)),
        content,
    );
}

fn meta_group_impl(
    ui: &mut egui::Ui,
    title: &str,
    default_open: bool,
    add: Option<(&str, &mut bool)>,
    content: impl FnOnce(&mut egui::Ui),
) {
    let (add_tooltip, mut add_clicked) = match add {
        Some((tooltip, clicked)) => (Some(tooltip), Some(clicked)),
        None => (None, None),
    };
    let id = ui.make_persistent_id(("simple_embedded_meta_group", title));
    let selected_id = egui::Id::new("simple_embedded_meta_list_selected");
    let old_open = ui.data_mut(|data| data.get_persisted::<bool>(id));
    let anim_id = id.with("anim");
    let mut state = egui::collapsing_header::CollapsingState::load_with_default_open(
        ui.ctx(),
        anim_id,
        old_open.unwrap_or(default_open),
    );
    let openness = state.openness(ui.ctx());
    if openness > 0.0 && openness < 1.0 {
        ui.ctx().request_repaint();
    }
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
            paint_chevron_openness(ui.painter(), chevron_rect, openness, theme::ACCENT);
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

    let mut plus_clicked = false;
    if let Some(add_tooltip) = add_tooltip {
        let plus_rect = egui::Rect::from_center_size(
            egui::pos2(row_rect.right() - 15.0, row_rect.center().y),
            egui::vec2(22.0, 22.0),
        );
        let plus = ui
            .interact(plus_rect, id.with("add"), Sense::click())
            .on_hover_cursor(egui::CursorIcon::PointingHand)
            .on_hover_text(add_tooltip);
        if plus.hovered() {
            ui.painter().rect_filled(
                plus_rect,
                CornerRadius::same(4),
                Color32::from_rgb(28, 55, 43),
            );
        }
        let center = plus_rect.center();
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
        plus_clicked = plus.clicked();
    }

    if plus_clicked {
        if let Some(clicked) = add_clicked.as_mut() {
            **clicked = true;
        }
        state.set_open(true);
        ui.ctx().request_repaint();
        ui.data_mut(|data| data.insert_persisted(selected_id, id));
    } else if response.clicked() {
        ui.data_mut(|data| data.insert_persisted(selected_id, id));
        state.toggle(ui);
    }
    state.show_body_unindented(ui, |ui| {
        ui.add_space(2.0);
        ui.indent(id.with("body"), |ui| content(ui));
        ui.add_space(3.0);
    });
}

fn paint_chevron_openness(
    painter: &egui::Painter,
    rect: egui::Rect,
    openness: f32,
    color: Color32,
) {
    let center = rect.center();
    let stroke = Stroke::new(1.6_f32, color);
    let angle = openness.clamp(0.0, 1.0) * std::f32::consts::FRAC_PI_2;
    let (sin, cos) = angle.sin_cos();
    let rotate = |x: f32, y: f32| egui::vec2(x * cos - y * sin, x * sin + y * cos);
    let first = center + rotate(-2.0, -4.0);
    let middle = center + rotate(2.0, 0.0);
    let last = center + rotate(-2.0, 4.0);
    painter.line_segment([first, middle], stroke);
    painter.line_segment([middle, last], stroke);
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
    let old_open = ui.data_mut(|data| data.get_persisted::<bool>(id));
    let anim_id = id.with("anim");
    let mut state = egui::collapsing_header::CollapsingState::load_with_default_open(
        ui.ctx(),
        anim_id,
        old_open.unwrap_or(true),
    );
    let openness = state.openness(ui.ctx());
    if openness > 0.0 && openness < 1.0 {
        ui.ctx().request_repaint();
    }
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
            paint_chevron_openness(ui.painter(), chevron_rect, openness, theme::ACCENT);
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
        state.toggle(ui);
    }
    state.show_body_unindented(ui, |ui| {
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
    });
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

fn settings_tab_button(
    ui: &mut egui::Ui,
    selected: SettingsTab,
    tab: SettingsTab,
    label: &str,
) -> bool {
    let (rect, response) = ui.allocate_exact_size(egui::vec2(112.0, 30.0), Sense::click());
    let response = response.on_hover_cursor(egui::CursorIcon::PointingHand);
    let active = selected == tab;
    if active {
        ui.painter()
            .rect_filled(rect, CornerRadius::same(5), theme::ACCENT_SOFT);
        ui.painter().rect_filled(
            egui::Rect::from_min_max(
                egui::pos2(rect.left() + 7.0, rect.bottom() - 2.0),
                egui::pos2(rect.right() - 7.0, rect.bottom()),
            ),
            CornerRadius::same(1),
            theme::ACCENT,
        );
    } else if response.hovered() {
        ui.painter()
            .rect_filled(rect, CornerRadius::same(5), theme::SURFACE);
    }
    let color = if active {
        theme::ACCENT
    } else if response.hovered() {
        theme::TEXT
    } else {
        theme::TEXT_DIM
    };
    ui.painter().text(
        rect.center(),
        Align2::CENTER_CENTER,
        label,
        FontId::proportional(11.5),
        color,
    );
    response.clicked()
}

fn settings_path_row(ui: &mut egui::Ui, label: &str, value: &mut String, title: &str, hint: &str) {
    ui.horizontal(|ui| {
        ui.add_sized(
            [86.0, 22.0],
            Label::new(RichText::new(label).size(10.5).color(theme::TEXT_DIM)),
        );
        ui.add(
            TextEdit::singleline(value)
                .desired_width(388.0)
                .hint_text(hint),
        );
        if ui.add(theme::secondary_button("浏览")).clicked() {
            let mut dialog = rfd::FileDialog::new().set_title(title);
            if let Some(parent) = Path::new(value.trim())
                .parent()
                .filter(|parent| parent.is_dir())
            {
                dialog = dialog.set_directory(parent);
            }
            if let Some(path) = dialog.pick_file() {
                *value = util::display_path(&path);
            }
        }
    });
}

fn settings_folder_row(
    ui: &mut egui::Ui,
    label: &str,
    value: &mut String,
    title: &str,
    hint: &str,
) {
    ui.horizontal(|ui| {
        ui.add_sized(
            [86.0, 22.0],
            Label::new(RichText::new(label).size(10.5).color(theme::TEXT_DIM)),
        );
        ui.add(
            TextEdit::singleline(value)
                .desired_width(388.0)
                .hint_text(hint),
        );
        if ui.add(theme::secondary_button("浏览")).clicked() {
            let mut dialog = rfd::FileDialog::new().set_title(title);
            let current = crate::toolchain::resolve_tools_dir(value);
            if current.is_dir() {
                dialog = dialog.set_directory(&current);
            }
            if let Some(path) = dialog.pick_folder() {
                *value = util::display_path(&path);
            }
        }
    });
}

fn tool_status_row(ui: &mut egui::Ui, label: &str, path: Option<&Path>) {
    ui.horizontal(|ui| {
        let (color, text) = match path {
            Some(path) => (
                theme::SUCCESS,
                format!("已找到 {}", util::display_path(path)),
            ),
            None => (theme::WARNING, "未找到".to_owned()),
        };
        colored_dot(ui, color, 5.0);
        ui.add(
            Label::new(RichText::new(format!("{label}：{text}")).size(9.5).color(
                if path.is_some() {
                    theme::TEXT_DIM
                } else {
                    theme::WARNING
                },
            ))
            .truncate(),
        );
    });
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
            PendingAction::CloseTabs { .. } => (
                "关闭多个文件前保存更改？",
                format!("选中的标签页中有未保存的修改（共 {dirty} 个文件未保存）。"),
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
            PendingAction::CloseTabs { paths, focus } => {
                self.close_tab_paths_now(&paths, focus.as_deref());
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
                    .inner_margin(Margin::symmetric(14, 10))
                    .show(ui, |ui| {
                        ui.set_max_width(460.0);
                        ui.spacing_mut().item_spacing.x = 8.0;
                        ui.horizontal(|ui| {
                            colored_dot(ui, color, 6.5);
                            let icon = match severity {
                                Severity::Error => "错误",
                                Severity::Warning => "提示",
                                Severity::Success => "完成",
                                _ => "信息",
                            };
                            ui.label(RichText::new(icon).size(10.5).strong().color(color));
                            ui.add(
                                Label::new(RichText::new(&message).size(12.5).color(theme::TEXT))
                                    .wrap(),
                            );
                        });
                    });
            });
    }
}

fn tab_menu_item(ui: &mut egui::Ui, label: &str, shortcut: &str) -> bool {
    let mut button = Button::new(RichText::new(label).size(11.5).color(theme::TEXT))
        .frame(true)
        .min_size(egui::vec2(264.0, 24.0));
    if !shortcut.is_empty() {
        button = button.shortcut_text(RichText::new(shortcut).size(10.0).color(theme::TEXT_FAINT));
    }
    ui.add(button).clicked()
}

fn debug_side_tab(ui: &mut egui::Ui, label: &str, active: bool) -> bool {
    ui.add(
        Button::new(RichText::new(label).size(11.5).color(if active {
            theme::ACCENT
        } else {
            theme::TEXT_FAINT
        }))
        .fill(if active {
            theme::ACCENT_SOFT
        } else {
            Color32::TRANSPARENT
        })
        .frame(active)
        .corner_radius(3)
        .min_size(egui::vec2(48.0, 22.0)),
    )
    .clicked()
}

fn window_close_button(ui: &mut egui::Ui) -> bool {
    let (rect, response) = ui.allocate_exact_size(egui::vec2(22.0, 22.0), Sense::click());
    let response = response
        .on_hover_cursor(egui::CursorIcon::PointingHand)
        .on_hover_text("关闭");
    if response.hovered() {
        ui.painter()
            .rect_filled(rect.shrink(1.0), CornerRadius::same(3), theme::SURFACE);
    }
    paint_find_glyph(
        ui.painter(),
        rect,
        FindGlyph::Close,
        if response.hovered() {
            theme::TEXT
        } else {
            theme::TEXT_DIM
        },
    );
    response.clicked()
}

fn memory_column(
    ui: &mut egui::Ui,
    title: &str,
    kind: crate::config::MemoryKind,
    regions: &mut [crate::config::MemoryRegionConfig],
) {
    ui.vertical_centered(|ui| {
        ui.label(RichText::new(title).size(12.0).strong().color(theme::TEXT));
    });
    ui.add_space(5.0);
    let mut startup = None;
    if let Some(index) = memory_section(ui, "On-Chip", kind, &mut regions[..2]) {
        startup = Some(index);
    }
    ui.add_space(7.0);
    if let Some(index) = memory_section(ui, "Off-Chip", kind, &mut regions[2..]) {
        startup = Some(index + 2);
    }
    if let Some(index) = startup {
        for region in regions.iter_mut() {
            region.startup = false;
        }
        regions[index].startup = true;
    }
}

fn memory_section(
    ui: &mut egui::Ui,
    title: &str,
    kind: crate::config::MemoryKind,
    regions: &mut [crate::config::MemoryRegionConfig],
) -> Option<usize> {
    ui.vertical_centered(|ui| {
        ui.label(RichText::new(title).size(11.0).color(theme::TEXT_DIM));
    });
    ui.add_space(3.0);
    const DEFAULT_W: f32 = 20.0;
    const NAME_W: f32 = 44.0;
    const START_W: f32 = 100.0;
    const SIZE_W: f32 = 76.0;
    const TAIL_W: f32 = 40.0;
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = 4.0;
        ui.add_sized(
            [DEFAULT_W, 16.0],
            Label::new(RichText::new("默认").size(9.5).color(theme::TEXT_FAINT)),
        );
        ui.add_sized([NAME_W, 16.0], Label::new(""));
        ui.add_sized(
            [START_W, 16.0],
            Label::new(RichText::new("起始地址").size(9.5).color(theme::TEXT_FAINT)),
        );
        ui.add_sized(
            [SIZE_W, 16.0],
            Label::new(RichText::new("大小").size(9.5).color(theme::TEXT_FAINT)),
        );
        ui.add_sized(
            [TAIL_W, 16.0],
            Label::new(
                RichText::new(if kind == crate::config::MemoryKind::Ram {
                    "NoInit"
                } else {
                    "启动"
                })
                .size(9.5)
                .color(theme::TEXT_FAINT),
            ),
        );
    });
    ui.add_space(2.0);
    let mut startup = None;
    for (index, region) in regions.iter_mut().enumerate() {
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = 4.0;
            ui.add_sized(
                [DEFAULT_W, 20.0],
                Checkbox::without_text(&mut region.enabled),
            );
            ui.add_sized(
                [NAME_W, 20.0],
                Label::new(
                    RichText::new(&region.name)
                        .size(10.5)
                        .color(if region.enabled {
                            theme::TEXT
                        } else {
                            theme::TEXT_FAINT
                        }),
                ),
            );
            ui.add_sized(
                [START_W, 20.0],
                TextEdit::singleline(&mut region.start)
                    .font(FontId::monospace(10.5))
                    .hint_text("0x0"),
            );
            ui.add_sized(
                [SIZE_W, 20.0],
                TextEdit::singleline(&mut region.size)
                    .font(FontId::monospace(10.5))
                    .hint_text("0x0"),
            );
            match kind {
                crate::config::MemoryKind::Ram => {
                    ui.add_sized([TAIL_W, 20.0], Checkbox::without_text(&mut region.no_init));
                }
                crate::config::MemoryKind::Rom => {
                    if ui
                        .add_sized([TAIL_W, 20.0], RadioButton::new(region.startup, ""))
                        .clicked()
                    {
                        startup = Some(index);
                    }
                }
            }
        });
        ui.add_space(3.0);
    }
    startup
}

fn memory_total(
    ui: &mut egui::Ui,
    label: &str,
    kind: crate::config::MemoryKind,
    regions: &[crate::config::MemoryRegionConfig],
) {
    let total = crate::memory::regions_total(regions, kind);
    ui.horizontal(|ui| {
        ui.label(
            RichText::new(format!("{label}:"))
                .size(11.5)
                .strong()
                .color(theme::TEXT_DIM),
        );
        ui.label(
            RichText::new(format!(
                "{} ({} Bytes)",
                crate::memory::format_size(total),
                total
            ))
            .size(11.5)
            .color(theme::TEXT),
        );
    });
}

#[derive(Clone, Copy)]
enum DebugIcon {
    Continue,
    Pause,
    StepOver,
    StepInto,
    StepOut,
    Stop,
}

fn debug_icon_button(
    ui: &mut egui::Ui,
    icon: DebugIcon,
    enabled: bool,
    tip: &str,
) -> egui::Response {
    let (rect, response) = ui.allocate_exact_size(
        egui::vec2(30.0, 26.0),
        if enabled {
            Sense::click()
        } else {
            Sense::hover()
        },
    );
    if enabled && response.hovered() {
        ui.painter()
            .rect_filled(rect.shrink(2.0), CornerRadius::same(4), theme::HOVER);
    }
    let color = if !enabled {
        theme::TEXT_FAINT
    } else if matches!(icon, DebugIcon::Stop) {
        theme::ERROR
    } else if response.hovered() {
        theme::ACCENT
    } else {
        theme::TEXT_DIM
    };
    paint_debug_icon(ui.painter(), rect, icon, color);
    if enabled {
        response.on_hover_text(tip)
    } else {
        response.on_disabled_hover_text(tip)
    }
}

fn paint_debug_icon(painter: &egui::Painter, rect: egui::Rect, icon: DebugIcon, color: Color32) {
    let center = rect.center();
    let stroke = Stroke::new(1.6_f32, color);
    match icon {
        DebugIcon::Continue => {
            painter.add(egui::Shape::convex_polygon(
                vec![
                    center + egui::vec2(-4.0, -6.0),
                    center + egui::vec2(-4.0, 6.0),
                    center + egui::vec2(6.0, 0.0),
                ],
                color,
                Stroke::NONE,
            ));
        }
        DebugIcon::Stop => {
            painter.rect_filled(
                egui::Rect::from_center_size(center, egui::vec2(9.0, 9.0)),
                CornerRadius::same(1),
                color,
            );
        }
        DebugIcon::Pause => {
            for offset in [-3.0_f32, 2.5] {
                painter.rect_filled(
                    egui::Rect::from_center_size(
                        center + egui::vec2(offset, 0.0),
                        egui::vec2(4.0, 12.0),
                    ),
                    CornerRadius::same(1),
                    color,
                );
            }
        }
        DebugIcon::StepOver => {
            painter.add(egui::Shape::line(
                vec![
                    center + egui::vec2(-7.0, -1.0),
                    center + egui::vec2(-4.0, -5.0),
                    center + egui::vec2(1.0, -5.0),
                    center + egui::vec2(4.0, -2.0),
                ],
                stroke,
            ));
            paint_debug_arrow(
                painter,
                center + egui::vec2(5.0, -1.0),
                egui::vec2(1.0, 0.4),
                color,
            );
            painter.circle_filled(center + egui::vec2(-1.0, 4.0), 2.0, color);
        }
        DebugIcon::StepInto => {
            painter.add(egui::Shape::line(
                vec![
                    center + egui::vec2(-7.0, -5.0),
                    center + egui::vec2(-7.0, 4.0),
                    center + egui::vec2(7.0, 4.0),
                    center + egui::vec2(7.0, -5.0),
                ],
                stroke,
            ));
            painter.line_segment(
                [
                    center + egui::vec2(0.0, -7.0),
                    center + egui::vec2(0.0, 1.0),
                ],
                stroke,
            );
            paint_debug_arrow(
                painter,
                center + egui::vec2(0.0, 2.0),
                egui::vec2(0.0, 1.0),
                color,
            );
        }
        DebugIcon::StepOut => {
            painter.add(egui::Shape::line(
                vec![
                    center + egui::vec2(-7.0, 5.0),
                    center + egui::vec2(-7.0, -4.0),
                    center + egui::vec2(7.0, -4.0),
                    center + egui::vec2(7.0, 5.0),
                ],
                stroke,
            ));
            painter.line_segment(
                [
                    center + egui::vec2(0.0, 7.0),
                    center + egui::vec2(0.0, -2.0),
                ],
                stroke,
            );
            paint_debug_arrow(
                painter,
                center + egui::vec2(0.0, -3.0),
                egui::vec2(0.0, -1.0),
                color,
            );
        }
    }
}

fn paint_debug_arrow(
    painter: &egui::Painter,
    tip: egui::Pos2,
    direction: egui::Vec2,
    color: Color32,
) {
    let direction = direction.normalized();
    let side = egui::vec2(-direction.y, direction.x);
    let back = tip - direction * 4.5;
    painter.add(egui::Shape::convex_polygon(
        vec![tip, back + side * 3.0, back - side * 3.0],
        color,
        Stroke::NONE,
    ));
}

fn find_text_field(
    ui: &mut egui::Ui,
    id: Id,
    text: &mut String,
    hint: &str,
    width: f32,
    reserved_right: f32,
) -> (egui::Response, egui::Rect) {
    let (rect, _) = ui.allocate_exact_size(egui::vec2(width, 24.0), Sense::hover());
    ui.painter().rect(
        rect,
        CornerRadius::same(4),
        theme::SURFACE,
        Stroke::new(1.0_f32, theme::BORDER_SOFT),
        egui::StrokeKind::Inside,
    );
    let inner = egui::Rect::from_min_max(
        egui::pos2(rect.left() + 7.0, rect.top() + 1.0),
        egui::pos2(rect.right() - reserved_right.max(3.0), rect.bottom() - 1.0),
    );
    let hit = ui
        .interact(rect, id.with("field-hit"), Sense::click())
        .on_hover_cursor(egui::CursorIcon::Text);
    let mut child = ui.new_child(
        egui::UiBuilder::new()
            .max_rect(inner)
            .layout(Layout::left_to_right(Align::Center)),
    );
    let response = child.add_sized(
        [inner.width().max(8.0), inner.height()],
        TextEdit::singleline(text)
            .id(id)
            .frame(Frame::NONE)
            .margin(Margin::symmetric(0, 2))
            .font(FontId::proportional(12.0))
            .text_color(theme::TEXT)
            .background_color(Color32::TRANSPARENT)
            .hint_text(RichText::new(hint).size(12.0).color(theme::TEXT_FAINT)),
    );
    if hit.clicked() {
        ui.memory_mut(|memory| memory.request_focus(id));
    }
    if response.has_focus() {
        ui.painter().rect_stroke(
            rect,
            CornerRadius::same(4),
            Stroke::new(1.0_f32, theme::ACCENT),
            egui::StrokeKind::Inside,
        );
    }
    (response, rect)
}

fn find_toggle(ui: &mut egui::Ui, label: &str, active: bool, tooltip: &str) -> bool {
    let (rect, response) = ui.allocate_exact_size(egui::vec2(22.0, 22.0), Sense::click());
    let response = response
        .on_hover_cursor(egui::CursorIcon::PointingHand)
        .on_hover_text(tooltip);
    let visual_rect = rect.shrink(1.0);
    if active {
        ui.painter().rect(
            visual_rect,
            CornerRadius::same(3),
            theme::ACCENT_SOFT,
            Stroke::new(1.0_f32, theme::ACCENT_DARK),
            egui::StrokeKind::Inside,
        );
    } else if response.hovered() {
        ui.painter()
            .rect_filled(visual_rect, CornerRadius::same(3), theme::SURFACE);
    }
    let color = if active {
        theme::ACCENT
    } else if response.hovered() {
        theme::TEXT
    } else {
        theme::TEXT_DIM
    };
    ui.painter().text(
        rect.center(),
        Align2::CENTER_CENTER,
        label,
        FontId::proportional(11.0),
        color,
    );
    response.clicked()
}

#[derive(Clone, Copy)]
enum FindGlyph {
    Previous,
    Next,
    Menu,
    Close,
    Replace,
    ReplaceAll,
}

fn find_icon_button(ui: &mut egui::Ui, glyph: FindGlyph, tooltip: &str) -> bool {
    let (rect, response) = ui.allocate_exact_size(egui::vec2(22.0, 22.0), Sense::click());
    let response = response
        .on_hover_cursor(egui::CursorIcon::PointingHand)
        .on_hover_text(tooltip);
    if response.hovered() {
        ui.painter()
            .rect_filled(rect.shrink(1.0), CornerRadius::same(3), theme::SURFACE);
    }
    let color = if response.hovered() {
        theme::TEXT
    } else {
        theme::TEXT_DIM
    };
    paint_find_glyph(ui.painter(), rect, glyph, color);
    response.clicked()
}

fn paint_find_glyph(painter: &egui::Painter, rect: egui::Rect, glyph: FindGlyph, color: Color32) {
    let center = rect.center();
    let stroke = Stroke::new(1.2_f32, color);
    match glyph {
        FindGlyph::Previous | FindGlyph::Next => {
            let direction = if matches!(glyph, FindGlyph::Previous) {
                -1.0
            } else {
                1.0
            };
            let tip = egui::pos2(center.x, center.y + 5.0 * direction);
            let tail = egui::pos2(center.x, center.y - 5.0 * direction);
            painter.line_segment([tail, tip], stroke);
            painter.line_segment(
                [egui::pos2(center.x - 3.5, tip.y - 3.0 * direction), tip],
                stroke,
            );
            painter.line_segment(
                [egui::pos2(center.x + 3.5, tip.y - 3.0 * direction), tip],
                stroke,
            );
        }
        FindGlyph::Menu => {
            for offset in [-4.0_f32, 0.0, 4.0] {
                painter.line_segment(
                    [
                        egui::pos2(center.x - 6.0, center.y + offset),
                        egui::pos2(center.x + 6.0, center.y + offset),
                    ],
                    stroke,
                );
            }
        }
        FindGlyph::Close => {
            painter.line_segment(
                [
                    egui::pos2(center.x - 5.0, center.y - 5.0),
                    egui::pos2(center.x + 5.0, center.y + 5.0),
                ],
                stroke,
            );
            painter.line_segment(
                [
                    egui::pos2(center.x + 5.0, center.y - 5.0),
                    egui::pos2(center.x - 5.0, center.y + 5.0),
                ],
                stroke,
            );
        }
        FindGlyph::Replace | FindGlyph::ReplaceAll => {
            let front = egui::Rect::from_center_size(
                egui::pos2(center.x, center.y + 2.0),
                egui::vec2(12.0, 8.0),
            );
            if matches!(glyph, FindGlyph::ReplaceAll) {
                painter.rect_stroke(
                    front.translate(egui::vec2(2.0, -2.0)),
                    CornerRadius::same(2),
                    stroke,
                    egui::StrokeKind::Inside,
                );
                painter.rect_filled(front, CornerRadius::same(2), theme::PANEL_RAISED);
            }
            painter.rect_stroke(
                front,
                CornerRadius::same(2),
                stroke,
                egui::StrokeKind::Inside,
            );
            let elbow = egui::pos2(center.x - 1.0, center.y - 3.0);
            painter.add(egui::Shape::line(
                vec![
                    egui::pos2(center.x - 7.0, center.y - 6.0),
                    egui::pos2(center.x - 7.0, center.y - 3.0),
                    elbow,
                ],
                stroke,
            ));
            painter.line_segment([egui::pos2(elbow.x - 2.5, elbow.y - 2.0), elbow], stroke);
            painter.line_segment([egui::pos2(elbow.x - 2.5, elbow.y + 2.0), elbow], stroke);
        }
    }
}

fn leading_indent_columns(line: &str, tab_width: usize) -> usize {
    let tab_width = tab_width.max(1);
    let mut columns = 0usize;
    for ch in line.chars() {
        match ch {
            ' ' => columns += 1,
            '\t' => columns += tab_width - (columns % tab_width),
            _ => break,
        }
    }
    columns
}

fn paint_indent_guides(
    painter: &egui::Painter,
    galley: &egui::Galley,
    galley_pos: egui::Pos2,
    viewport: egui::Rect,
    indent_width: usize,
    active_level: usize,
) {
    if indent_width == 0 || galley.rows.is_empty() {
        return;
    }
    let first_visible = galley
        .rows
        .partition_point(|row| galley_pos.y + row.pos.y + row.row.height() < viewport.top());
    let mut start = first_visible.min(galley.rows.len());
    while start > 0 && !galley.rows[start - 1].ends_with_newline {
        start -= 1;
    }
    let mut guides: Option<Vec<(usize, f32)>> = None;
    for row in galley.rows.iter().skip(start) {
        let top = galley_pos.y + row.pos.y;
        let bottom = top + row.row.height();
        if top > viewport.bottom() {
            break;
        }
        if guides.is_none() {
            let mut levels = Vec::new();
            let mut columns = 0usize;
            let mut level = 1usize;
            let base_x = galley_pos.x + row.pos.x;
            for (index, glyph) in row.row.glyphs.iter().enumerate() {
                match glyph.chr {
                    ' ' => columns += 1,
                    '\t' => columns += indent_width - (columns % indent_width),
                    _ => break,
                }
                while level * indent_width <= columns {
                    levels.push((level, base_x + row.row.x_offset(index + 1)));
                    level += 1;
                }
            }
            if columns > 0 {
                levels.insert(0, (0, base_x));
            }
            guides = Some(levels);
        }
        if let Some(levels) = &guides {
            for (level, x) in levels {
                if *x <= viewport.left() + 2.0 || *x >= viewport.right() - 4.0 {
                    continue;
                }
                let color = if *level == active_level && active_level > 0 {
                    Color32::from_rgba_unmultiplied(88, 196, 221, 155)
                } else {
                    Color32::from_rgba_unmultiplied(130, 145, 165, 72)
                };
                painter.line_segment(
                    [
                        egui::pos2(*x, top.max(viewport.top())),
                        egui::pos2(*x, bottom.min(viewport.bottom())),
                    ],
                    Stroke::new(1.0_f32, color),
                );
            }
        }
        if row.ends_with_newline {
            guides = None;
        }
    }
}

fn paint_bottom_corners(ui: &egui::Ui, rect: egui::Rect, radius: f32, color: Color32) {
    let radius = radius
        .min(rect.width() * 0.5)
        .min(rect.height() * 0.5)
        .max(0.0);
    if radius <= 0.0 {
        return;
    }
    let painter = ui.painter().with_clip_rect(rect);
    let steps = 8usize;
    let mut left = Vec::with_capacity(steps + 2);
    left.push(egui::pos2(rect.left(), rect.bottom()));
    for step in 0..=steps {
        let t = step as f32 / steps as f32;
        let angle = std::f32::consts::PI - t * std::f32::consts::FRAC_PI_2;
        left.push(egui::pos2(
            rect.left() + radius + radius * angle.cos(),
            rect.bottom() - radius + radius * angle.sin(),
        ));
    }
    painter.add(egui::Shape::convex_polygon(left, color, Stroke::NONE));

    let mut right = Vec::with_capacity(steps + 2);
    right.push(egui::pos2(rect.right(), rect.bottom()));
    for step in 0..=steps {
        let t = step as f32 / steps as f32;
        let angle = std::f32::consts::FRAC_PI_2 - t * std::f32::consts::FRAC_PI_2;
        right.push(egui::pos2(
            rect.right() - radius + radius * angle.cos(),
            rect.bottom() - radius + radius * angle.sin(),
        ));
    }
    painter.add(egui::Shape::convex_polygon(right, color, Stroke::NONE));
}
