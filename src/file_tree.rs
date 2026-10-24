//! Lazy, cached project file tree with custom-painted rows.

use std::collections::{BTreeMap, HashMap, HashSet};
use std::path::{Path, PathBuf};

use eframe::egui::{self, Align2, Color32, FontId, Sense, Shape, Stroke};

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
    Reveal(PathBuf),
    OpenInVscode(PathBuf),
    HexView(PathBuf),
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

    pub fn show(&mut self, ui: &mut egui::Ui, active: Option<&Path>) -> Vec<TreeAction> {
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
        let row = row(ui, &root_name, true, expanded, 0, false, None);

        if row.response.clicked() {
            if expanded {
                self.expanded.remove(&root);
            } else {
                self.expanded.insert(root.clone());
                let _ = self.children_for(&root);
            }
        }

        let mut context_action = None;
        row.response.context_menu(|ui| {
            if ui.button("在资源管理器中显示").clicked() {
                context_action = Some(TreeAction::Reveal(root.clone()));
                ui.close();
            }
            if ui.button("用 VS Code 打开").clicked() {
                context_action = Some(TreeAction::OpenInVscode(root.clone()));
                ui.close();
            }
        });
        if let Some(action) = context_action {
            actions.push(action);
        }
        row.response.on_hover_text(crate::util::display_path(&root));

        if expanded {
            self.show_dir(ui, &root, 1, active, &mut actions);
        }
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
    ) {
        let entries = self.children_for(dir).to_vec();
        for entry in entries {
            let selected = active == Some(entry.path.as_path());
            let is_expanded = entry.is_dir && self.expanded.contains(&entry.path);
            let badge = self
                .git_badges
                .get(&crate::util::project_key(&entry.path))
                .copied();
            let row = row(
                ui,
                &entry.name,
                entry.is_dir,
                is_expanded,
                depth,
                selected,
                badge,
            );

            if row.response.clicked() {
                if entry.is_dir {
                    if is_expanded {
                        self.expanded.remove(&entry.path);
                    } else {
                        self.expanded.insert(entry.path.clone());
                        let _ = self.children_for(&entry.path);
                    }
                } else {
                    actions.push(TreeAction::OpenFile(entry.path.clone()));
                }
            }

            let mut context_action = None;
            row.response.context_menu(|ui| {
                if ui.button("在资源管理器中显示").clicked() {
                    context_action = Some(TreeAction::Reveal(entry.path.clone()));
                    ui.close();
                }
                if ui.button("用 VS Code 打开").clicked() {
                    context_action = Some(TreeAction::OpenInVscode(entry.path.clone()));
                    ui.close();
                }
                if !entry.is_dir && ui.button("十六进制查看").clicked() {
                    context_action = Some(TreeAction::HexView(entry.path.clone()));
                    ui.close();
                }
            });
            if let Some(action) = context_action {
                actions.push(action);
            }
            row.response
                .on_hover_text(crate::util::display_path(&entry.path));

            if entry.is_dir && is_expanded && depth < 64 {
                self.show_dir(ui, &entry.path, depth + 1, active, actions);
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

struct RowResponse {
    response: egui::Response,
}

fn row(
    ui: &mut egui::Ui,
    name: &str,
    is_dir: bool,
    is_expanded: bool,
    depth: usize,
    selected: bool,
    badge: Option<char>,
) -> RowResponse {
    let height = 22.0;
    let (rect, response) =
        ui.allocate_exact_size(egui::vec2(ui.available_width(), height), Sense::click());

    if ui.is_rect_visible(rect) {
        let painter = ui.painter_at(rect);
        let hovered = response.hovered();
        if selected {
            painter.rect_filled(rect, egui::CornerRadius::same(4), theme::ACCENT_SOFT);
            painter.rect_filled(
                egui::Rect::from_min_size(rect.min, egui::vec2(2.0, rect.height())),
                egui::CornerRadius::same(1),
                theme::ACCENT,
            );
        } else if hovered {
            painter.rect_filled(rect, egui::CornerRadius::same(4), theme::HOVER);
        }

        let indent = 7.0 + depth as f32 * 14.0;
        let arrow_center = egui::pos2(rect.left() + indent + 5.0, rect.center().y);
        if is_dir {
            let points = if is_expanded {
                vec![
                    egui::pos2(arrow_center.x - 3.5, arrow_center.y - 1.5),
                    egui::pos2(arrow_center.x + 3.5, arrow_center.y - 1.5),
                    egui::pos2(arrow_center.x, arrow_center.y + 2.5),
                ]
            } else {
                vec![
                    egui::pos2(arrow_center.x - 1.5, arrow_center.y - 3.5),
                    egui::pos2(arrow_center.x - 1.5, arrow_center.y + 3.5),
                    egui::pos2(arrow_center.x + 2.5, arrow_center.y),
                ]
            };
            painter.add(Shape::convex_polygon(
                points,
                if selected {
                    theme::TEXT
                } else {
                    theme::TEXT_FAINT
                },
                Stroke::NONE,
            ));
        }

        let icon_x = rect.left() + indent + 15.0;
        if is_dir {
            paint_folder(&painter, icon_x, rect.center().y);
        } else {
            paint_file(&painter, icon_x, rect.center().y, color_for_name(name));
        }

        let text_x = icon_x + 16.0;
        let text_color = if selected {
            theme::TEXT
        } else if let Some(badge) = badge {
            crate::git::badge_color(badge)
        } else {
            theme::TEXT_DIM
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
        }
    }

    RowResponse { response }
}

fn paint_folder(painter: &egui::Painter, x: f32, center_y: f32) {
    let color = theme::DIR.gamma_multiply(0.9);
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
        "rs" => Color32::from_rgb(224, 151, 102),
        "c" | "h" => Color32::from_rgb(111, 166, 220),
        "cpp" | "cc" | "cxx" | "hpp" | "hh" => Color32::from_rgb(132, 182, 228),
        "s" | "asm" | "ld" => Color32::from_rgb(206, 158, 220),
        "toml" | "json" | "yaml" | "yml" => Color32::from_rgb(219, 183, 112),
        "md" | "txt" => Color32::from_rgb(142, 160, 180),
        "mk" => Color32::from_rgb(207, 175, 100),
        _ => Color32::from_rgb(139, 149, 164),
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
