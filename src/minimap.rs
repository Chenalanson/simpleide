//! Lightweight code minimap painted at the right edge of the editor.
//!
//! The cache stores one compact row per sampled source line. Rendering
//! downsamples those rows to the available height, so large files never
//! require a full mini-layout or extra syntect pass.

use eframe::egui::{self, Color32, CornerRadius, Rect, Sense, Stroke};

use crate::theme;

pub const WIDTH: f32 = 76.0;

const COMMENT: Color32 = Color32::from_rgb(106, 153, 85);
const PREPROC: Color32 = Color32::from_rgb(197, 134, 192);
const STRING: Color32 = Color32::from_rgb(206, 145, 120);
const KEYWORD: Color32 = Color32::from_rgb(86, 156, 214);
const NORMAL: Color32 = Color32::from_rgb(96, 108, 124);
const MAX_ROWS: usize = 20_000;

#[derive(Clone, Copy)]
struct Row {
    indent: u16,
    len: u16,
    color: Color32,
}

pub struct Cache {
    revision: u64,
    rows: Vec<Row>,
}

pub struct View<'a> {
    pub text: &'a str,
    pub revision: u64,
    pub scroll: &'a egui::scroll_area::ScrollAreaOutput<()>,
    pub width: f32,
    pub height: f32,
    pub line_count: usize,
}

pub fn show(ui: &mut egui::Ui, view: View<'_>, cache: &mut Option<Cache>) {
    if cache
        .as_ref()
        .is_none_or(|cached| cached.revision != view.revision)
    {
        *cache = Some(build(view.text, view.revision, view.line_count));
    }
    let width = view.width;
    let height = view.height;
    let scroll = view.scroll;
    let rows = cache
        .as_ref()
        .map(|cached| cached.rows.as_slice())
        .unwrap_or(&[]);

    let height = height.max(20.0);
    let (rect, response) =
        ui.allocate_exact_size(egui::vec2(width, height), Sense::click_and_drag());
    let response = response
        .on_hover_cursor(egui::CursorIcon::PointingHand)
        .on_hover_text("代码预览：点击或拖动跳转");
    let painter = ui.painter_at(rect);

    painter.rect_filled(rect, CornerRadius::ZERO, theme::BG);
    painter.rect_filled(
        Rect::from_min_size(rect.min, egui::vec2(1.0, rect.height())),
        CornerRadius::ZERO,
        theme::GUTTER,
    );

    let map_top = rect.top() + 4.0;
    let map_height = (rect.height() - 8.0).max(10.0);
    let target_rows = ((map_height / 1.3).round() as usize).max(8);
    let char_width = (width - 10.0) / 110.0;
    if !rows.is_empty() {
        for row_index in 0..target_rows {
            let source_index = (row_index * rows.len()) / target_rows;
            let row = rows[source_index.min(rows.len() - 1)];
            if row.len == 0 {
                continue;
            }
            let y = map_top + row_index as f32 * map_height / target_rows as f32;
            let x0 = rect.left() + 5.0 + row.indent as f32 * char_width;
            let x1 = (x0 + row.len as f32 * char_width).min(rect.right() - 5.0);
            let row_height = (map_height / target_rows as f32).max(1.0) * 0.68;
            if x1 <= x0 {
                continue;
            }
            painter.rect_filled(
                Rect::from_min_max(egui::pos2(x0, y), egui::pos2(x1, y + row_height)),
                CornerRadius::ZERO,
                row.color,
            );
        }
    }

    let viewport_height = scroll.inner_rect.height().max(1.0);
    let content_height = scroll.content_size.y.max(viewport_height);
    let max_offset = (content_height - viewport_height).max(1.0);
    let ratio = (scroll.state.offset.y / max_offset).clamp(0.0, 1.0);
    let thumb_height =
        (viewport_height / content_height * map_height).clamp(24.0_f32.min(map_height), map_height);
    let thumb_top = map_top + ratio * (map_height - thumb_height);
    let thumb = Rect::from_min_size(
        egui::pos2(rect.left() + 1.0, thumb_top),
        egui::vec2(rect.width() - 2.0, thumb_height),
    );
    painter.rect_filled(thumb, CornerRadius::same(2), Color32::from_white_alpha(10));
    let bar = Rect::from_min_size(
        egui::pos2(rect.right() - 5.0, thumb_top),
        egui::vec2(3.0, thumb_height),
    );
    let bar_color = if response.hovered() || response.dragged() {
        theme::TEXT_DIM
    } else {
        theme::TEXT_FAINT.gamma_multiply(0.65)
    };
    painter.rect_filled(bar, CornerRadius::same(2), bar_color);
    painter.line_segment(
        [rect.right_top(), rect.right_bottom()],
        Stroke::new(1.0_f32, theme::GUTTER),
    );

    if response.clicked() || response.dragged() {
        if let Some(pointer) = response.interact_pointer_pos() {
            let fraction = ((pointer.y - map_top) / map_height).clamp(0.0, 1.0);
            let mut state = egui::scroll_area::State::load(ui.ctx(), scroll.id).unwrap_or_default();
            state.offset.y = fraction * max_offset;
            state.store(ui.ctx(), scroll.id);
            ui.ctx().request_repaint();
        }
    }
}

fn build(text: &str, revision: u64, line_count: usize) -> Cache {
    let step = line_count.div_ceil(MAX_ROWS).max(1);
    let mut rows = Vec::with_capacity(line_count.div_ceil(step).min(MAX_ROWS));
    for (index, line) in text.split('\n').enumerate() {
        if index % step != 0 {
            continue;
        }
        let trimmed = line.trim_start();
        let mut indent = 0u16;
        for ch in line.chars() {
            match ch {
                ' ' => indent = indent.saturating_add(1),
                '\t' => indent = indent.saturating_add(4),
                _ => break,
            }
        }
        let len = line
            .trim_end()
            .chars()
            .count()
            .saturating_sub(indent as usize);
        let len = len.min(u16::MAX as usize) as u16;
        let color = line_color(trimmed);
        rows.push(Row { indent, len, color });
    }
    Cache { revision, rows }
}

fn line_color(trimmed: &str) -> Color32 {
    if trimmed.is_empty() {
        return NORMAL;
    }
    if trimmed.starts_with("//")
        || trimmed.starts_with("/*")
        || trimmed.starts_with('*')
        || trimmed.starts_with("*/")
    {
        return COMMENT;
    }
    if trimmed.starts_with('#') {
        return PREPROC;
    }
    if trimmed.contains('"') || trimmed.contains('\'') {
        return STRING;
    }
    let first = trimmed
        .split(|ch: char| !(ch.is_ascii_alphanumeric() || ch == '_'))
        .next()
        .unwrap_or("");
    if is_keyword(first) {
        KEYWORD
    } else {
        NORMAL
    }
}

fn is_keyword(word: &str) -> bool {
    matches!(
        word,
        "if" | "else"
            | "for"
            | "while"
            | "do"
            | "switch"
            | "case"
            | "default"
            | "break"
            | "continue"
            | "return"
            | "goto"
            | "typedef"
            | "struct"
            | "union"
            | "enum"
            | "static"
            | "extern"
            | "const"
            | "volatile"
            | "inline"
            | "void"
            | "char"
            | "short"
            | "int"
            | "long"
            | "float"
            | "double"
            | "signed"
            | "unsigned"
            | "sizeof"
            | "class"
            | "public"
            | "private"
            | "protected"
            | "template"
            | "namespace"
            | "using"
            | "auto"
            | "bool"
            | "true"
            | "false"
            | "let"
            | "mut"
            | "fn"
            | "impl"
            | "match"
            | "pub"
            | "use"
            | "mod"
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn builds_compact_rows_and_preserves_indentation() {
        let text = "#include <stdint.h>\n\nint main(void) {\n    return 0;\n}\n// end";
        let cache = build(text, 7, 6);
        assert_eq!(cache.revision, 7);
        assert_eq!(cache.rows.len(), 6);
        assert_eq!(cache.rows[0].color, PREPROC);
        assert_eq!(cache.rows[2].color, KEYWORD);
        assert_eq!(cache.rows[3].indent, 4);
        assert_eq!(cache.rows[5].color, COMMENT);
    }

    #[test]
    fn samples_long_files() {
        let text = (0..50_000)
            .map(|index| format!("int value_{index};"))
            .collect::<Vec<_>>()
            .join("\n");
        let cache = build(&text, 1, 50_000);
        assert!(cache.rows.len() <= MAX_ROWS + 1);
        assert!(cache.rows.len() >= 2);
    }
}
