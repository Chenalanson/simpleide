//! Lazy, cached project file tree with custom-painted rows.

use std::collections::{BTreeMap, HashMap, HashSet};
use std::path::{Path, PathBuf};

use eframe::egui::{self, Align2, Color32, FontId, Sense, Stroke};

use crate::git::{status_badges, GitStatus};
use crate::ignore::IgnoreRules;
use crate::theme;
use crate::util::file_name;

#[derive(Clone, Debug)]
struct Entry {
    path: PathBuf,
    name: String,
    is_dir: bool,
}

pub enum TreeAction {
    OpenFile(PathBuf),
    OpenToSide(PathBuf),
    Reveal(PathBuf),
    OpenInVscode(PathBuf),
    HexView(PathBuf),
    PreviewImage(PathBuf),
    NewFile(PathBuf),
    NewFolder(PathBuf),
    CopyPath(PathBuf),
    CopyRelativePath(PathBuf),
    Cut(PathBuf),
    Copy(PathBuf),
    Paste(PathBuf),
    Rename(PathBuf),
    Delete(PathBuf),
}

pub struct FileTree {
    root: PathBuf,
    children: HashMap<PathBuf, Vec<Entry>>,
    expanded: HashSet<PathBuf>,
    show_hidden: bool,
    respect_gitignore: bool,
    extra_ignore: String,
    ignore: IgnoreRules,
    git_badges: BTreeMap<String, char>,
}

impl FileTree {
    pub fn new(
        root: PathBuf,
        show_hidden: bool,
        respect_gitignore: bool,
        extra_ignore: &str,
    ) -> Self {
        let ignore = IgnoreRules::load(&root, respect_gitignore, extra_ignore);
        let mut tree = Self {
            root,
            children: HashMap::new(),
            expanded: HashSet::new(),
            show_hidden,
            respect_gitignore,
            extra_ignore: extra_ignore.to_owned(),
            ignore,
            git_badges: BTreeMap::new(),
        };
        tree.expanded.insert(tree.root.clone());
        tree.warm_root();
        tree
    }

    pub fn refresh(&mut self) {
        self.children.clear();
        self.ignore = IgnoreRules::load(&self.root, self.respect_gitignore, &self.extra_ignore);
        self.warm_root();
    }

    pub fn set_git_status(&mut self, status: &GitStatus) {
        self.git_badges = status_badges(status);
    }

    pub fn set_ignore_rules(&mut self, respect_gitignore: bool, extra_ignore: &str) {
        if self.respect_gitignore == respect_gitignore && self.extra_ignore == extra_ignore {
            return;
        }
        self.respect_gitignore = respect_gitignore;
        self.extra_ignore = extra_ignore.to_owned();
        self.children.clear();
        self.refresh();
    }

    /// Expand all parent folders so an already-open file becomes visible.
    pub fn reveal(&mut self, path: &Path) {
        if !path.starts_with(&self.root) {
            return;
        }
        let mut ancestors: Vec<PathBuf> = path
            .ancestors()
            .take_while(|ancestor| ancestor.starts_with(&self.root))
            .map(Path::to_path_buf)
            .collect();
        ancestors.reverse();
        for ancestor in ancestors {
            if ancestor.is_dir() {
                self.expanded.insert(ancestor.clone());
                let _ = self.children_for(&ancestor);
            }
        }
    }

    pub fn set_show_hidden(&mut self, show_hidden: bool) {
        if self.show_hidden == show_hidden {
            return;
        }
        self.show_hidden = show_hidden;
        self.children.clear();
        self.warm_root();
    }

    pub fn show(
        &mut self,
        ui: &mut egui::Ui,
        active: Option<&Path>,
        can_paste: bool,
    ) -> Vec<TreeAction> {
        let mut actions = Vec::new();
        ui.spacing_mut().item_spacing.y = 1.0;
        let root = self.root.clone();
        let root_name = {
            let name = file_name(&root);
            if name.is_empty() {
                crate::util::display_path(&root)
            } else {
                name
            }
        };
        let expanded = self.expanded.contains(&root);
        let mut state = collapsing_state(ui, &root, expanded);
        let openness = state.openness(ui.ctx());
        let row = row(ui, &root_name, true, openness, 0, false, None);

        if row.response.clicked() {
            if expanded {
                self.expanded.remove(&root);
                state.set_open(false);
            } else {
                self.expanded.insert(root.clone());
                let _ = self.children_for(&root);
                state.set_open(true);
            }
            ui.ctx().request_repaint();
        }

        let mut context_action = None;
        row.response.context_menu(|ui| {
            compact_context_menu(ui);
            if ui.button("新建文件...").clicked() {
                context_action = Some(TreeAction::NewFile(root.clone()));
                ui.close();
            }
            if ui.button("新建文件夹...").clicked() {
                context_action = Some(TreeAction::NewFolder(root.clone()));
                ui.close();
            }
            if can_paste && ui.button("粘贴").clicked() {
                context_action = Some(TreeAction::Paste(root.clone()));
                ui.close();
            }
            ui.separator();
            if ui.button("在文件资源管理器中显示").clicked() {
                context_action = Some(TreeAction::Reveal(root.clone()));
                ui.close();
            }
            ui.separator();
            if ui.button("复制路径").clicked() {
                context_action = Some(TreeAction::CopyPath(root.clone()));
                ui.close();
            }
            if ui.button("复制相对路径").clicked() {
                context_action = Some(TreeAction::CopyRelativePath(root.clone()));
                ui.close();
            }
            ui.separator();
            if ui.button("用 VS Code 打开").clicked() {
                context_action = Some(TreeAction::OpenInVscode(root.clone()));
                ui.close();
            }
        });
        if let Some(action) = context_action {
            actions.push(action);
        }
        row.response.on_hover_text(crate::util::display_path(&root));

        state.show_body_unindented(ui, |ui| {
            self.show_dir(ui, &root, 1, active, &mut actions, can_paste);
        });
        actions
    }

    fn warm_root(&mut self) {
        let _ = self.children_for(&self.root.clone());
    }

    fn show_dir(
        &mut self,
        ui: &mut egui::Ui,
        dir: &Path,
        depth: usize,
        active: Option<&Path>,
        actions: &mut Vec<TreeAction>,
        can_paste: bool,
    ) {
        ui.add_space(4.0);
        let entries = self.children_for(dir).to_vec();
        for entry in entries {
            let selected = active == Some(entry.path.as_path());
            let is_expanded = entry.is_dir && self.expanded.contains(&entry.path);
            let mut state = entry
                .is_dir
                .then(|| collapsing_state(ui, &entry.path, is_expanded));
            let openness = state
                .as_ref()
                .map(|state| state.openness(ui.ctx()))
                .unwrap_or(0.0);
            let badge = self
                .git_badges
                .get(&crate::util::project_key(&entry.path))
                .copied();
            let row = row(
                ui,
                &entry.name,
                entry.is_dir,
                openness,
                depth,
                selected,
                badge,
            );

            if row.response.clicked() {
                if entry.is_dir {
                    if let Some(state) = state.as_mut() {
                        if is_expanded {
                            self.expanded.remove(&entry.path);
                            state.set_open(false);
                        } else {
                            self.expanded.insert(entry.path.clone());
                            let _ = self.children_for(&entry.path);
                            state.set_open(true);
                        }
                        ui.ctx().request_repaint();
                    }
                } else {
                    actions.push(TreeAction::OpenFile(entry.path.clone()));
                }
            }

            let mut context_action = None;
            row.response.context_menu(|ui| {
                compact_context_menu(ui);
                if entry.is_dir {
                    if ui.button("新建文件...").clicked() {
                        context_action = Some(TreeAction::NewFile(entry.path.clone()));
                        ui.close();
                    }
                    if ui.button("新建文件夹...").clicked() {
                        context_action = Some(TreeAction::NewFolder(entry.path.clone()));
                        ui.close();
                    }
                    if can_paste && ui.button("粘贴").clicked() {
                        context_action = Some(TreeAction::Paste(entry.path.clone()));
                        ui.close();
                    }
                    ui.separator();
                } else if ui.button("在侧边打开").clicked() {
                    context_action = Some(TreeAction::OpenToSide(entry.path.clone()));
                    ui.close();
                }
                if ui.button("剪切").clicked() {
                    context_action = Some(TreeAction::Cut(entry.path.clone()));
                    ui.close();
                }
                if ui.button("复制").clicked() {
                    context_action = Some(TreeAction::Copy(entry.path.clone()));
                    ui.close();
                }
                ui.separator();
                if ui.button("复制路径").clicked() {
                    context_action = Some(TreeAction::CopyPath(entry.path.clone()));
                    ui.close();
                }
                if ui.button("复制相对路径").clicked() {
                    context_action = Some(TreeAction::CopyRelativePath(entry.path.clone()));
                    ui.close();
                }
                ui.separator();
                if ui.button("重命名...").clicked() {
                    context_action = Some(TreeAction::Rename(entry.path.clone()));
                    ui.close();
                }
                if ui.button("删除").clicked() {
                    context_action = Some(TreeAction::Delete(entry.path.clone()));
                    ui.close();
                }
                ui.separator();
                if ui.button("在文件资源管理器中显示").clicked() {
                    context_action = Some(TreeAction::Reveal(entry.path.clone()));
                    ui.close();
                }
                if !entry.is_dir
                    && is_image_path(&entry.path)
                    && ui.button("在图像预览中打开").clicked()
                {
                    context_action = Some(TreeAction::PreviewImage(entry.path.clone()));
                    ui.close();
                }
                if !entry.is_dir && ui.button("十六进制查看").clicked() {
                    context_action = Some(TreeAction::HexView(entry.path.clone()));
                    ui.close();
                }
                ui.separator();
                if ui.button("用 VS Code 打开").clicked() {
                    context_action = Some(TreeAction::OpenInVscode(entry.path.clone()));
                    ui.close();
                }
            });
            if let Some(action) = context_action {
                actions.push(action);
            }
            row.response
                .on_hover_text(crate::util::display_path(&entry.path));

            if entry.is_dir && depth < 64 {
                if let Some(state) = state.as_mut() {
                    state.show_body_unindented(ui, |ui| {
                        self.show_dir(ui, &entry.path, depth + 1, active, actions, can_paste);
                    });
                }
            }
        }
    }

    fn children_for(&mut self, dir: &Path) -> &[Entry] {
        if !self.children.contains_key(dir) {
            let mut entries = Vec::new();
            if let Ok(read_dir) = std::fs::read_dir(dir) {
                for item in read_dir.flatten() {
                    let path = item.path();
                    let name = file_name(&path);
                    if !self.show_hidden && name.starts_with('.') {
                        continue;
                    }
                    let is_dir = path.is_dir();
                    let relative = path.strip_prefix(&self.root).unwrap_or(path.as_path());
                    if self.ignore.is_ignored(relative, is_dir) {
                        continue;
                    }
                    entries.push(Entry { path, name, is_dir });
                }
            }
            entries.sort_by(|a, b| {
                b.is_dir
                    .cmp(&a.is_dir)
                    .then_with(|| a.name.to_lowercase().cmp(&b.name.to_lowercase()))
            });
            self.children.insert(dir.to_path_buf(), entries);
        }
        self.children
            .get(dir)
            .map(Vec::as_slice)
            .unwrap_or_default()
    }
}

pub(crate) struct RowResponse {
    pub(crate) response: egui::Response,
}

fn is_image_path(path: &Path) -> bool {
    path.extension()
        .and_then(|extension| extension.to_str())
        .is_some_and(|extension| {
            matches!(
                extension.to_ascii_lowercase().as_str(),
                "png" | "jpg" | "jpeg" | "gif" | "bmp" | "webp" | "ico" | "tif" | "tiff"
            )
        })
}

fn compact_context_menu(ui: &mut egui::Ui) {
    ui.spacing_mut().item_spacing.y = 2.0;
    ui.spacing_mut().button_padding = egui::vec2(8.0, 3.0);
    ui.spacing_mut().interact_size.y = 22.0;
}

fn collapsing_state(
    ui: &egui::Ui,
    path: &Path,
    open: bool,
) -> egui::collapsing_header::CollapsingState {
    let id = ui.make_persistent_id(("simple_embedded_file_tree", path));
    let mut state =
        egui::collapsing_header::CollapsingState::load_with_default_open(ui.ctx(), id, open);
    if state.is_open() != open {
        state.set_open(open);
    }
    let openness = state.openness(ui.ctx());
    if openness > 0.0 && openness < 1.0 {
        ui.ctx().request_repaint();
    }
    state
}

fn row(
    ui: &mut egui::Ui,
    name: &str,
    is_dir: bool,
    openness: f32,
    depth: usize,
    selected: bool,
    badge: Option<char>,
) -> RowResponse {
    RowResponse {
        response: row_impl(ui, name, is_dir, openness, depth, selected, badge, None),
    }
}

/// Explorer-style row used by the chip file browser. It shares the file tree
/// icons, indentation guides and hover highlight, with an optional right-aligned
/// size for files.
pub(crate) fn board_row(
    ui: &mut egui::Ui,
    name: &str,
    is_dir: bool,
    openness: f32,
    depth: usize,
    selected: bool,
    size: Option<&str>,
) -> RowResponse {
    RowResponse {
        response: row_impl(ui, name, is_dir, openness, depth, selected, None, size),
    }
}

#[allow(clippy::too_many_arguments)]
fn row_impl(
    ui: &mut egui::Ui,
    name: &str,
    is_dir: bool,
    openness: f32,
    depth: usize,
    selected: bool,
    badge: Option<char>,
    size: Option<&str>,
) -> egui::Response {
    let height = 22.0;
    let (rect, response) =
        ui.allocate_exact_size(egui::vec2(ui.available_width(), height), Sense::click());

    if ui.is_rect_visible(rect) {
        let painter = ui.painter_at(rect);
        let hovered = response.hovered();
        if selected {
            painter.rect_filled(rect, egui::CornerRadius::same(4), theme::accent_soft());
            painter.rect_filled(
                egui::Rect::from_min_size(rect.min, egui::vec2(2.0, rect.height())),
                egui::CornerRadius::same(1),
                theme::accent(),
            );
        } else if hovered {
            painter.rect_filled(rect, egui::CornerRadius::same(4), theme::hover());
        }

        // The tree is a child of the "资源管理器" section header, so the
        // whole body starts one indent level in from the panel edge.
        let base = ui.spacing().indent;
        let indent = base + 7.0 + depth as f32 * 14.0;
        let arrow_center = egui::pos2(rect.left() + indent + 5.0, rect.center().y);
        for level in 0..depth {
            // Keep each guide directly under the chevron of its parent row.
            let x = rect.left() + base + 12.0 + level as f32 * 14.0;
            painter.line_segment(
                [
                    egui::pos2(x, rect.top()),
                    egui::pos2(x, rect.bottom() + 1.0),
                ],
                Stroke::new(1.0_f32, theme::border()),
            );
        }
        if is_dir {
            paint_chevron(&painter, arrow_center, openness);
        }

        let icon_x = rect.left() + indent + 15.0;
        if is_dir {
            paint_folder(&painter, icon_x, rect.center().y);
        } else {
            paint_file(&painter, icon_x, rect.center().y, color_for_name(name));
        }

        let text_x = icon_x + 16.0;
        let text_color = if selected {
            theme::text()
        } else if let Some(badge) = badge {
            crate::git::badge_color(badge)
        } else {
            theme::text_dim()
        };
        painter.text(
            egui::pos2(text_x, rect.center().y),
            Align2::LEFT_CENTER,
            name,
            FontId::proportional(13.0),
            text_color,
        );
        if let Some(badge) = badge {
            painter.text(
                egui::pos2(rect.right() - 8.0, rect.center().y),
                Align2::RIGHT_CENTER,
                badge,
                FontId::monospace(10.0),
                crate::git::badge_color(badge),
            );
        } else if let Some(size) = size {
            painter.text(
                egui::pos2(rect.right() - 8.0, rect.center().y),
                Align2::RIGHT_CENTER,
                size,
                FontId::monospace(10.0),
                theme::text_faint(),
            );
        }
    }

    response
}

fn paint_chevron(painter: &egui::Painter, center: egui::Pos2, openness: f32) {
    let stroke = Stroke::new(1.6_f32, theme::accent());
    let angle = openness.clamp(0.0, 1.0) * std::f32::consts::FRAC_PI_2;
    let (sin, cos) = angle.sin_cos();
    let rotate = |x: f32, y: f32| egui::vec2(x * cos - y * sin, x * sin + y * cos);
    let first = center + rotate(-2.0, -4.0);
    let middle = center + rotate(2.0, 0.0);
    let last = center + rotate(-2.0, 4.0);
    painter.line_segment([first, middle], stroke);
    painter.line_segment([middle, last], stroke);
}

fn paint_folder(painter: &egui::Painter, x: f32, center_y: f32) {
    let color = theme::dir().gamma_multiply(0.9);
    let body = egui::Rect::from_min_size(egui::pos2(x, center_y - 4.0), egui::vec2(11.0, 8.0));
    let tab = egui::Rect::from_min_size(egui::pos2(x, center_y - 5.5), egui::vec2(5.0, 3.0));
    painter.rect_filled(tab, egui::CornerRadius::same(1), color.gamma_multiply(0.7));
    painter.rect_filled(body, egui::CornerRadius::same(1), color);
}

fn paint_file(painter: &egui::Painter, x: f32, center_y: f32, color: Color32) {
    let body =
        egui::Rect::from_min_size(egui::pos2(x + 2.0, center_y - 5.5), egui::vec2(8.0, 11.0));
    painter.rect_filled(
        body,
        egui::CornerRadius::same(1),
        color.gamma_multiply(0.22),
    );
    painter.rect_filled(
        egui::Rect::from_min_size(body.min + egui::vec2(2.0, 2.0), egui::vec2(4.0, 1.0)),
        egui::CornerRadius::ZERO,
        color,
    );
    painter.rect_filled(
        egui::Rect::from_min_size(body.min + egui::vec2(2.0, 4.5), egui::vec2(3.0, 1.0)),
        egui::CornerRadius::ZERO,
        color.gamma_multiply(0.6),
    );
}

fn color_for_name(name: &str) -> Color32 {
    let extension = Path::new(name)
        .extension()
        .map(|extension| extension.to_string_lossy().to_lowercase())
        .unwrap_or_default();
    match extension.as_str() {
        "rs" => theme::warning(),
        "c" | "h" => theme::accent(),
        "cpp" | "cc" | "cxx" | "hpp" | "hh" => theme::info(),
        "s" | "asm" | "ld" => theme::dir(),
        "toml" | "json" | "yaml" | "yml" => theme::warning(),
        "md" | "txt" => theme::text_dim(),
        "mk" => theme::dir(),
        _ => theme::text_faint(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reveal_expands_all_parent_folders() {
        let dir = tempfile::tempdir().unwrap();
        let nested = dir.path().join("a").join("b");
        std::fs::create_dir_all(&nested).unwrap();
        let file = nested.join("main.c");
        std::fs::write(&file, "int main(void) {}\n").unwrap();

        let mut tree = FileTree::new(dir.path().to_path_buf(), false, false, "");
        tree.reveal(&file);

        assert!(tree.expanded.contains(&dir.path().to_path_buf()));
        assert!(tree.expanded.contains(&dir.path().join("a")));
        assert!(tree.expanded.contains(&dir.path().join("a").join("b")));
    }
}
