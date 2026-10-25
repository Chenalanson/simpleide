//! Workspace UI: explorer, tabs and the text editor surface.

use super::*;
use crate::config::autodetect;
use crate::makefile;
use eframe::egui::{
    self, Align, Align2, Button, Color32, FontId, Frame, Id, Label, Layout, Margin, RichText,
    ScrollArea, Sense, Stroke, TextEdit,
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
            .exact_size(32.0)
            .frame(
                Frame::new()
                    .fill(theme::BG)
                    .inner_margin(Margin::symmetric(8, 4)),
            )
            .show_inside(ui, |ui| {
                ui.painter().rect(
                    ui.max_rect(),
                    CornerRadius::same(7),
                    theme::PANEL,
                    Stroke::NONE,
                    egui::StrokeKind::Inside,
                );
                ui.horizontal_centered(|ui| {
                    ui.spacing_mut().item_spacing.x = 8.0;
                    let text_color = theme::TEXT_FAINT;
                    if let Some((line, column, encoding, newline, language, dirty, read_only)) =
                        info
                    {
                        ui.label(
                            RichText::new(format!("行 {line}，列 {column}"))
                                .size(10.5)
                                .color(text_color),
                        );
                        ui.add_space(6.0);
                        let indent = if self.settings.use_spaces {
                            format!("缩进: 空格 {}", self.settings.indent_width)
                        } else {
                            format!("缩进: Tab {}", self.settings.indent_width)
                        };
                        ui.label(RichText::new(indent).size(10.5).color(text_color));
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
                        ui.label(
                            RichText::new(format!("行尾: {newline}"))
                                .size(10.5)
                                .color(text_color),
                        );
                        ui.add_space(6.0);
                        ui.label(RichText::new(language).size(10.5).color(theme::INFO));
                        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
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
                        });
                    } else {
                        ui.label(RichText::new("就绪").size(10.5).color(text_color));
                    }
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
            .resizable(true)
            .default_size(272.0)
            .size_range(230.0..=440.0)
            .frame(Frame::new().fill(theme::BG).inner_margin(Margin::same(3)))
            .show_inside(ui, |ui| {
                ui.painter().rect(
                    ui.max_rect(),
                    CornerRadius::same(8),
                    theme::PANEL,
                    Stroke::NONE,
                    egui::StrokeKind::Inside,
                );
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

                let mut resource_open = self.resource_open;
                let mut info_open = self.info_open;

                ui.allocate_ui_with_layout(
                    egui::vec2(ui.available_width(), ui.available_height()),
                    Layout::top_down(Align::Min),
                    |ui| {
                        ui.add_space(10.0);
                        Frame::new()
                            .inner_margin(Margin::symmetric(11, 7))
                            .show(ui, |ui| {
                                ui.horizontal(|ui| {
                                    if section_chevron(ui, resource_open).clicked() {
                                        resource_open = !resource_open;
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

                        if resource_open {
                            ui.add_space(6.0);
                            let tree_budget = if has_project {
                                (ui.available_height() - 50.0).max(80.0)
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
                                                            TreeAction::HexView(path) => {
                                                                Action::OpenHex(path)
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
                                    split_response.on_hover_cursor(egui::CursorIcon::ResizeVertical)
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
                        } else if has_project {
                            ui.add_space(6.0);
                        }

                        if has_project {
                            ui.add_space(4.0);
                            info_header_row(ui, &mut info_open);
                            ui.add_space(4.0);
                            if info_open {
                                Frame::new().inner_margin(Margin::symmetric(11, 0)).show(
                                    ui,
                                    |ui| {
                                        let height = (ui.available_height() - 2.0).max(80.0);
                                        ScrollArea::vertical()
                                            .id_salt("simple_embedded_project_meta")
                                            .max_height(height)
                                            .auto_shrink([false, false])
                                            .show(ui, |ui| self.show_project_meta(ui, actions));
                                    },
                                );
                            }
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
            .frame(Frame::new().fill(theme::BG).inner_margin(Margin::same(4)))
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
                            SplitDirection::Right => {
                                ui.columns(2, |columns| {
                                    self.show_code_editor(&mut columns[0], primary, Some(0));
                                    self.show_code_editor(&mut columns[1], secondary, Some(1));
                                });
                            }
                            SplitDirection::Up => {
                                let available = ui.available_height();
                                let top =
                                    (available * 0.5).clamp(120.0, (available - 80.0).max(120.0));
                                ui.allocate_ui_with_layout(
                                    egui::vec2(ui.available_width(), top),
                                    Layout::top_down(Align::Min),
                                    |ui| {
                                        self.show_code_editor(ui, primary, Some(0));
                                    },
                                );
                                ui.separator();
                                self.show_code_editor(ui, secondary, Some(1));
                            }
                        }
                    } else {
                        self.split_path = None;
                        self.split_primary = None;
                        self.show_code_editor(ui, active_index, None);
                    }
                } else {
                    if self.split_path.is_some() || self.split_primary.is_some() {
                        self.split_path = None;
                        self.split_primary = None;
                    }
                    self.show_code_editor(ui, active_index, None);
                }
                let ctx = ui.ctx().clone();
                if self.find.open {
                    self.show_find_overlay(&ctx, editor_rect);
                }
                if self.goto_line.open {
                    self.show_goto_overlay(&ctx, editor_rect);
                }
            });
    }

    fn show_goto_overlay(&mut self, ctx: &egui::Context, editor_rect: egui::Rect) {
        let line_count = self
            .active_document()
            .map(|doc| doc.buffer.as_string().split('\n').count().max(1))
            .unwrap_or(1);
        let width = 420.0_f32.min((editor_rect.width() - 24.0).max(260.0));
        let position = egui::pos2(editor_rect.left() + 48.0, editor_rect.top() + 8.0);
        let mut apply = false;
        egui::Area::new(Id::new("simple_embedded_goto_line"))
            .order(egui::Order::Foreground)
            .fixed_pos(position)
            .show(ctx, |ui| {
                Frame::new()
                    .fill(theme::PANEL_RAISED)
                    .stroke(Stroke::new(1.0_f32, theme::BORDER))
                    .corner_radius(5)
                    .inner_margin(Margin::same(6))
                    .show(ui, |ui| {
                        ui.set_width(width);
                        let response = ui.add(
                            TextEdit::singleline(&mut self.goto_line.input)
                                .id(Id::new("simple_embedded_goto_line_input"))
                                .font(FontId::monospace(12.0))
                                .text_color(theme::TEXT)
                                .background_color(theme::BG)
                                .desired_width(width)
                                .hint_text(String::new()),
                        );
                        if self.goto_line.focus_input {
                            response.request_focus();
                            self.goto_line.focus_input = false;
                        }
                        if response.lost_focus()
                            && ui.input(|input| input.key_pressed(egui::Key::Enter))
                        {
                            apply = true;
                        }
                        ui.add_space(4.0);
                        if let Some(error) = &self.goto_line.error {
                            ui.label(RichText::new(error).size(10.5).color(theme::ERROR));
                        } else {
                            Frame::new()
                                .fill(theme::ACCENT_SOFT)
                                .corner_radius(3)
                                .inner_margin(Margin::symmetric(7, 4))
                                .show(ui, |ui| {
                                    ui.label(
                                        RichText::new(format!(
                                            "键入要跳转的行号(从 1 到 {line_count})。"
                                        ))
                                        .size(10.5)
                                        .color(theme::TEXT),
                                    );
                                });
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
        let width = (editor_rect.width() - 24.0).max(300.0);
        let position = egui::pos2(editor_rect.right() - width - 12.0, editor_rect.top() + 8.0);
        egui::Area::new(Id::new("simple_embedded_find_overlay"))
            .order(egui::Order::Foreground)
            .fixed_pos(position)
            .show(ctx, |ui| {
                Frame::new()
                    .fill(theme::PANEL_RAISED)
                    .stroke(Stroke::new(1.0_f32, theme::BORDER))
                    .corner_radius(6)
                    .inner_margin(Margin::symmetric(10, 7))
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
                                if response.secondary_clicked() {
                                    actions.push(Action::SelectTab(index));
                                }
                                response.context_menu(|ui| {
                                    if ui.button("关闭    Ctrl+F4").clicked() {
                                        actions.push(Action::CloseTab(index));
                                        ui.close();
                                    }
                                    if ui.button("关闭其他").clicked() {
                                        actions.push(Action::CloseOtherTabs(index));
                                        ui.close();
                                    }
                                    if ui.button("关闭右侧标签页").clicked() {
                                        actions.push(Action::CloseTabsToRight(index));
                                        ui.close();
                                    }
                                    if ui.button("关闭已保存    Ctrl+K U").clicked() {
                                        actions.push(Action::CloseSavedTabs);
                                        ui.close();
                                    }
                                    if ui.button("全部关闭    Ctrl+K W").clicked() {
                                        actions.push(Action::CloseAllTabs);
                                        ui.close();
                                    }
                                    ui.separator();
                                    if ui.button("向右拆分    Ctrl+\\").clicked() {
                                        actions.push(Action::SplitTab(
                                            doc.path.clone(),
                                            SplitDirection::Right,
                                        ));
                                        ui.close();
                                    }
                                    ui.menu_button("拆分和移动", |ui| {
                                        if ui.button("向上拆分    Ctrl+K Ctrl+\\").clicked() {
                                            actions.push(Action::SplitTab(
                                                doc.path.clone(),
                                                SplitDirection::Up,
                                            ));
                                            ui.close();
                                        }
                                    });
                                    if self.split_primary.is_some()
                                        && ui.button("取消拆分").clicked()
                                    {
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
                    let response = ui.add_sized(
                        [ui.available_width().min(900.0), 22.0],
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
        ui.spacing_mut().item_spacing.x = 4.0;
        ui.spacing_mut().interact_size.y = 30.0;
        let available = ui.available_width();
        ui.horizontal(|ui| {
            let (chevron_rect, chevron_response) =
                ui.allocate_exact_size(egui::vec2(28.0, 30.0), Sense::click());
            let chevron_fill = if self.find.replace_open {
                theme::ACCENT_SOFT
            } else if chevron_response.hovered() {
                theme::SURFACE
            } else {
                theme::PANEL_RAISED
            };
            ui.painter().rect(
                chevron_rect,
                CornerRadius::same(4),
                chevron_fill,
                Stroke::new(1.0_f32, theme::ACCENT),
                egui::StrokeKind::Inside,
            );
            let center = chevron_rect.center();
            let (from_y, to_y) = if self.find.replace_open {
                (2.0, -2.0)
            } else {
                (-2.0, 2.0)
            };
            ui.painter().line_segment(
                [
                    egui::pos2(center.x - 4.0, center.y + from_y),
                    egui::pos2(center.x, center.y + to_y),
                ],
                Stroke::new(1.4_f32, theme::ACCENT),
            );
            ui.painter().line_segment(
                [
                    egui::pos2(center.x, center.y + to_y),
                    egui::pos2(center.x + 4.0, center.y + from_y),
                ],
                Stroke::new(1.4_f32, theme::ACCENT),
            );
            if chevron_response
                .on_hover_text(if self.find.replace_open {
                    "收起替换"
                } else {
                    "展开替换"
                })
                .clicked()
            {
                self.find.replace_open = !self.find.replace_open;
            }
            let query_width = (available - 260.0).clamp(140.0, 420.0);
            let query = ui.add(
                TextEdit::singleline(&mut self.find.query)
                    .id(Id::new("simple_embedded_find_query"))
                    .font(FontId::monospace(12.0))
                    .text_color(theme::TEXT)
                    .background_color(theme::BG)
                    .desired_width(query_width)
                    .hint_text(RichText::new("查找").color(theme::TEXT_FAINT)),
            );
            if self.find.focus_query {
                query.request_focus();
                self.find.focus_query = false;
            }
            if query.changed() {
                self.find.dirty = true;
            }
            if query.lost_focus() && ui.input(|input| input.key_pressed(egui::Key::Enter)) {
                next = true;
            }

            if ui
                .selectable_label(self.find.case_sensitive, RichText::new("Aa").size(11.0))
                .on_hover_text("区分大小写")
                .clicked()
            {
                self.find.case_sensitive = !self.find.case_sensitive;
                self.find.dirty = true;
            }
            if ui
                .selectable_label(self.find.whole_word, RichText::new("ab").size(11.0))
                .on_hover_text("全字匹配")
                .clicked()
            {
                self.find.whole_word = !self.find.whole_word;
                self.find.dirty = true;
            }
            if ui
                .selectable_label(self.find.use_regex, RichText::new(".*").size(11.0))
                .on_hover_text("正则表达式")
                .clicked()
            {
                self.find.use_regex = !self.find.use_regex;
                self.find.dirty = true;
            }

            if let Some(active) = self.active {
                if let Some(doc) = self.tabs.get(active) {
                    self.find
                        .ensure_matches(doc.buffer.as_string(), doc.buffer.revision());
                }
            }

            let status = if let Some(error) = &self.find.error {
                error.clone()
            } else if self.find.query.is_empty() {
                String::new()
            } else if self.find.matches.is_empty() {
                "无结果".to_owned()
            } else {
                format!("{}/{}", self.find.active + 1, self.find.matches.len())
            };
            let status_color = if self.find.error.is_some() {
                theme::ERROR
            } else {
                theme::TEXT_FAINT
            };
            ui.label(RichText::new(status).size(10.5).color(status_color));

            if ui
                .add(small_flat_button("↑", theme::TEXT_DIM))
                .on_hover_text("上一个匹配")
                .clicked()
            {
                previous = true;
            }
            if ui
                .add(small_flat_button("↓", theme::TEXT_DIM))
                .on_hover_text("下一个匹配")
                .clicked()
            {
                next = true;
            }
            if ui
                .add(small_flat_button("≡", theme::TEXT_DIM))
                .on_hover_text("全工程搜索 (Ctrl+Shift+F)")
                .clicked()
            {
                self.project_search.open = true;
                self.project_search.focus_query = true;
            }
            if ui
                .add(small_flat_button("×", theme::TEXT_DIM))
                .on_hover_text("关闭查找")
                .clicked()
            {
                self.find.open = false;
                self.focus_editor = true;
            }
        });

        if self.find.replace_open {
            ui.add_space(3.0);
            ui.horizontal(|ui| {
                ui.add_space(28.0);
                let replace_width = (available - 150.0).clamp(140.0, 520.0);
                let replace = ui.add(
                    TextEdit::singleline(&mut self.find.replacement)
                        .id(Id::new("simple_embedded_find_replace"))
                        .font(FontId::monospace(12.0))
                        .text_color(theme::TEXT)
                        .background_color(theme::BG)
                        .desired_width(replace_width)
                        .hint_text(RichText::new("替换").color(theme::TEXT_FAINT)),
                );
                if replace.lost_focus() && ui.input(|input| input.key_pressed(egui::Key::Enter)) {
                    replace_current = true;
                }
                if ui
                    .selectable_label(self.find.preserve_case, RichText::new("AB").size(11.0))
                    .on_hover_text("保留大小写")
                    .clicked()
                {
                    self.find.preserve_case = !self.find.preserve_case;
                }
                if ui
                    .add(small_flat_button("替换", theme::TEXT_DIM))
                    .on_hover_text("替换当前匹配")
                    .clicked()
                {
                    replace_current = true;
                }
                if ui
                    .add(small_flat_button("全部", theme::TEXT_DIM))
                    .on_hover_text("全部替换")
                    .clicked()
                {
                    replace_all = true;
                }
            });
        }

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
                        if ui
                            .add(small_flat_button("×", theme::TEXT_DIM))
                            .on_hover_text("关闭")
                            .clicked()
                        {
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
                        if ui
                            .add(small_flat_button("×", theme::TEXT_DIM))
                            .on_hover_text("关闭")
                            .clicked()
                        {
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
                        if ui
                            .add(small_flat_button("×", theme::TEXT_DIM))
                            .on_hover_text("关闭")
                            .clicked()
                        {
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
                        if ui
                            .add(small_flat_button("×", theme::TEXT_DIM))
                            .on_hover_text("关闭")
                            .clicked()
                        {
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
                        if ui
                            .add(small_flat_button("×", theme::TEXT_DIM))
                            .on_hover_text("关闭")
                            .clicked()
                        {
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
                        if ui
                            .add(small_flat_button("×", theme::TEXT_DIM))
                            .on_hover_text("关闭")
                            .clicked()
                        {
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
                        if ui
                            .add(small_flat_button("×", theme::TEXT_DIM))
                            .on_hover_text("关闭")
                            .clicked()
                        {
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
                                    .on_hover_text(util::display_path(&item.path))
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
            actions.push(Action::OpenAt(
                item.path.to_string_lossy().into_owned(),
                item.line,
                1,
            ));
        }
    }

    pub(super) fn show_debug_window(&mut self, ctx: &egui::Context, actions: &mut Vec<Action>) {
        let mut open = self.debug.open;
        let mut action = None;
        let mut clear_log = false;
        let mut send = false;
        let running = self
            .debug
            .session
            .as_mut()
            .map(|session| session.is_running())
            .unwrap_or(false);
        let started = self
            .debug
            .session
            .as_ref()
            .is_some_and(|session| session.started);
        let target = self
            .debug
            .target
            .as_ref()
            .map(|path| util::display_path(path))
            .unwrap_or_else(|| "未选择调试程序".to_owned());
        let log = self
            .debug
            .session
            .as_ref()
            .map(|session| session.log.clone())
            .unwrap_or_else(|| self.debug.log.clone());
        egui::Window::new("调试")
            .id(Id::new("simple_embedded_debug"))
            .title_bar(false)
            .collapsible(false)
            .resizable(true)
            .default_size([820.0, 520.0])
            .min_height(420.0)
            .frame(
                Frame::new()
                    .fill(theme::PANEL)
                    .stroke(Stroke::new(1.0_f32, theme::BORDER))
                    .inner_margin(Margin::same(12))
                    .corner_radius(6),
            )
            .show(ctx, |ui| {
                ui.set_min_width(780.0);
                ui.set_min_height(390.0);
                ui.visuals_mut().override_text_color = Some(theme::TEXT);
                ui.horizontal(|ui| {
                    ui.label(RichText::new("调试").size(16.0).strong().color(theme::TEXT));
                    let (status_color, status) = if self.debug.error.is_some() {
                        (theme::ERROR, "错误")
                    } else if self.debug.session.is_some() {
                        if running {
                            (theme::SUCCESS, "调试器运行中")
                        } else {
                            (theme::TEXT_FAINT, "调试器已停止")
                        }
                    } else {
                        (theme::TEXT_FAINT, "未启动")
                    };
                    ui.label(RichText::new(status).size(11.0).color(status_color));
                    ui.label(RichText::new(&target).size(10.5).color(theme::TEXT_FAINT));
                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        if ui
                            .add(small_flat_button("×", theme::TEXT_DIM))
                            .on_hover_text("关闭窗口（不会停止调试器）")
                            .clicked()
                        {
                            open = false;
                        }
                    });
                });
                if let Some(error) = &self.debug.error {
                    ui.label(RichText::new(error).size(11.0).color(theme::ERROR));
                }
                ui.add_space(4.0);
                ui.horizontal(|ui| {
                    let start_label = if self.debug.session.is_none() {
                        "启动"
                    } else if started {
                        "继续"
                    } else {
                        "运行"
                    };
                    if ui.add(theme::primary_button(start_label)).clicked() {
                        action = Some(Action::DebugContinue);
                    }
                    if ui
                        .add(theme::secondary_button("下一步"))
                        .on_hover_text("F10")
                        .clicked()
                    {
                        action = Some(Action::DebugStepOver);
                    }
                    if ui
                        .add(theme::secondary_button("单步"))
                        .on_hover_text("F11")
                        .clicked()
                    {
                        action = Some(Action::DebugStepInto);
                    }
                    if ui
                        .add(theme::secondary_button("跳出"))
                        .on_hover_text("Shift+F11")
                        .clicked()
                    {
                        action = Some(Action::DebugFinish);
                    }
                    if ui
                        .add(theme::secondary_button("停止"))
                        .on_hover_text("Shift+F5")
                        .clicked()
                    {
                        action = Some(Action::DebugStop);
                    }
                    if ui.add(theme::secondary_button("清空")).clicked() {
                        clear_log = true;
                    }
                    ui.label(
                        RichText::new(format!("{} 个断点", self.debug.breakpoints.len()))
                            .size(10.5)
                            .color(theme::TEXT_FAINT),
                    );
                });
                ui.add_space(4.0);
                ui.horizontal(|ui| {
                    let response = ui.add(
                        TextEdit::singleline(&mut self.debug.input)
                            .id(Id::new("simple_embedded_debug_input"))
                            .font(FontId::monospace(12.0))
                            .text_color(theme::TEXT)
                            .background_color(theme::BG)
                            .desired_width(420.0)
                            .hint_text(
                                RichText::new("gdb 命令，例如 info locals")
                                    .color(theme::TEXT_FAINT),
                            ),
                    );
                    if (response.lost_focus()
                        && ui.input(|input| input.key_pressed(egui::Key::Enter)))
                        || ui.button("发送").clicked()
                    {
                        send = true;
                    }
                });
                ui.separator();
                ScrollArea::vertical()
                    .id_salt("simple_embedded_debug_console")
                    .stick_to_bottom(true)
                    .max_height(330.0)
                    .auto_shrink([false, false])
                    .show(ui, |ui| {
                        ui.spacing_mut().item_spacing.y = 0.0;
                        if log.is_empty() {
                            ui.label(
                                RichText::new("启动 gdb 后，这里显示调试输出")
                                    .size(11.0)
                                    .color(theme::TEXT_FAINT),
                            );
                        }
                        for line in &log {
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
            });
        self.debug.open = open;
        if clear_log {
            if let Some(session) = &mut self.debug.session {
                session.log.clear();
            } else {
                self.debug.log.clear();
            }
        }
        if send {
            let command = self.debug.input.trim().to_owned();
            if !command.is_empty() {
                self.debug.input.clear();
                action = Some(Action::DebugSend(command));
            }
        }
        if let Some(action) = action {
            actions.push(action);
        }
    }

    pub(super) fn show_completion(&mut self, ctx: &egui::Context) {
        let mut open = self.completion.open;
        let mut chosen = None;
        let query = self.completion.query.to_lowercase();
        let filtered = self
            .completion
            .items
            .iter()
            .filter(|name| name.to_lowercase().contains(&query))
            .take(120)
            .cloned()
            .collect::<Vec<_>>();
        egui::Window::new("符号补全")
            .id(Id::new("simple_embedded_completion"))
            .title_bar(false)
            .collapsible(false)
            .resizable(true)
            .default_size([420.0, 320.0])
            .min_height(240.0)
            .frame(
                Frame::new()
                    .fill(theme::PANEL)
                    .stroke(Stroke::new(1.0_f32, theme::BORDER))
                    .inner_margin(Margin::same(10))
                    .corner_radius(6),
            )
            .show(ctx, |ui| {
                ui.set_min_width(380.0);
                ui.set_min_height(210.0);
                ui.visuals_mut().override_text_color = Some(theme::TEXT);
                ui.horizontal(|ui| {
                    ui.label(
                        RichText::new("符号补全")
                            .size(14.0)
                            .strong()
                            .color(theme::TEXT),
                    );
                    ui.label(
                        RichText::new(format!("{} 项", filtered.len()))
                            .size(10.5)
                            .color(theme::TEXT_FAINT),
                    );
                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        if ui
                            .add(small_flat_button("×", theme::TEXT_DIM))
                            .on_hover_text("关闭")
                            .clicked()
                        {
                            open = false;
                        }
                    });
                });
                let query_response = ui.add(
                    TextEdit::singleline(&mut self.completion.query)
                        .id(Id::new("simple_embedded_completion_filter"))
                        .font(FontId::monospace(12.0))
                        .text_color(theme::TEXT)
                        .background_color(theme::BG)
                        .desired_width(360.0)
                        .hint_text(RichText::new("过滤符号").color(theme::TEXT_FAINT)),
                );
                if query_response.lost_focus()
                    && ui.input(|input| input.key_pressed(egui::Key::Enter))
                {
                    chosen = filtered.first().cloned();
                }
                ui.separator();
                ScrollArea::vertical()
                    .id_salt("simple_embedded_completion_list")
                    .max_height(220.0)
                    .auto_shrink([false, false])
                    .show(ui, |ui| {
                        for name in &filtered {
                            if ui
                                .selectable_label(
                                    false,
                                    RichText::new(name)
                                        .monospace()
                                        .size(12.0)
                                        .color(theme::INFO),
                                )
                                .clicked()
                            {
                                chosen = Some(name.clone());
                            }
                        }
                    });
            });
        self.completion.open = open;
        if let Some(name) = chosen {
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
                        if ui
                            .add(small_flat_button("×", theme::TEXT_DIM))
                            .on_hover_text("关闭")
                            .clicked()
                        {
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

    fn show_code_editor(&mut self, ui: &mut egui::Ui, pane_index: usize, pane_slot: Option<usize>) {
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
        let viewport_width = ui.available_width();
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
        let scroll_output = ScrollArea::both()
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
                        state
                            .cursor
                            .set_char_range(Some(egui::text::CCursorRange::one(
                                cursor_range.primary,
                            )));
                        state.store(ui.ctx(), output.response.response.id);
                    }
                }
                if scroll_cursor {
                    let caret = output
                        .galley
                        .pos_from_cursor(egui::text::CCursor::new(doc.cursor_index))
                        .translate(output.galley_pos.to_vec2());
                    ui.scroll_to_rect(caret, None);
                }
                gutter = Some((Arc::clone(&output.galley), output.galley_pos));
                response = Some(output.response.response);
            });

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
                    "Make 工程保存时会按差异同步到 Makefile；Keil 工程只保存到本机设置，不修改 .uvprojx。",
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
                    ui.horizontal(|ui| {
                        ui.label(
                            RichText::new("调试程序")
                                .size(11.5)
                                .color(theme::TEXT_DIM),
                        );
                        ui.add(
                            TextEdit::singleline(&mut draft.debug_program)
                                .desired_width(300.0)
                                .hint_text("可执行文件，留空自动查找 build/*.elf/exe"),
                        );
                    });
                    ui.horizontal(|ui| {
                        ui.label(
                            RichText::new("gdb 路径")
                                .size(11.5)
                                .color(theme::TEXT_DIM),
                        );
                        ui.add(
                            TextEdit::singleline(&mut draft.gdb_path)
                                .desired_width(300.0)
                                .hint_text("留空自动查找 gdb"),
                        );
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
            self.settings.debug_program = draft.debug_program.trim().to_owned();
            self.settings.gdb_path = draft.gdb_path.trim().to_owned();
            self.settings.auto_scroll_output = draft.auto_scroll_output;
            self.settings.show_hidden = draft.show_hidden;
            self.settings.respect_gitignore = draft.respect_gitignore;
            self.settings.ignore_patterns = draft.ignore_patterns.trim().to_owned();
            self.settings.save();

            if let Some(root) = &project_root {
                self.build_config = draft.config.clone();
                self.settings.set_build_config(root, &draft.config);
                self.settings.save();
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

fn info_header_row(ui: &mut egui::Ui, info_open: &mut bool) {
    Frame::new()
        .inner_margin(Margin::symmetric(11, 5))
        .show(ui, |ui| {
            ui.horizontal(|ui| {
                if section_chevron(ui, *info_open).clicked() {
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

fn section_chevron(ui: &mut egui::Ui, open: bool) -> egui::Response {
    let (rect, response) = ui.allocate_exact_size(egui::vec2(16.0, 18.0), Sense::click());
    paint_chevron(ui.painter(), rect, open, theme::ACCENT);
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
