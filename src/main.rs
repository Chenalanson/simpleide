#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod app;
mod arraygen;
mod brackets;
mod build;
mod column;
mod column_select;
mod config;
mod debugger;
mod editor;
mod file_tree;
mod formatting;
mod git;
mod hexview;
mod highlight;
mod ignore;
mod indent;
mod log;
mod makefile;
mod minimap;
mod navigation;
mod project_meta;
mod registers;
mod search;
mod symbols;
mod theme;
mod todo;
mod util;

use std::path::PathBuf;
use std::sync::Arc;

use eframe::egui;

fn main() -> eframe::Result<()> {
    let mut args = std::env::args_os().skip(1);
    let initial_project = args.next().map(PathBuf::from).filter(|path| path.is_dir());
    let initial_file = args.next().map(PathBuf::from).filter(|path| path.is_file());

    let viewport = egui::ViewportBuilder::default()
        .with_inner_size([1360.0, 850.0])
        .with_min_inner_size([1000.0, 640.0])
        .with_title("Simple embedded · 作者 smallYYY")
        .with_icon(Arc::new(app_icon(128)));

    let native_options = eframe::NativeOptions {
        viewport,
        renderer: eframe::Renderer::Glow,
        ..Default::default()
    };

    eframe::run_native(
        "Simple embedded",
        native_options,
        Box::new(move |cc| {
            configure_fonts(&cc.egui_ctx);
            theme::install(&cc.egui_ctx);
            Ok(Box::new(app::App::new(
                cc,
                initial_project.clone(),
                initial_file.clone(),
            )))
        }),
    )
}

fn configure_fonts(ctx: &egui::Context) {
    let mut fonts = egui::FontDefinitions::default();

    let candidates = [
        r"C:\Windows\Fonts\msyh.ttc",
        r"C:\Windows\Fonts\Deng.ttf",
        r"C:\Windows\Fonts\NotoSansSC-VF.ttf",
        r"C:\Windows\Fonts\simhei.ttf",
        "/usr/share/fonts/opentype/noto/NotoSansCJK-Regular.ttc",
        "/usr/share/fonts/opentype/noto/NotoSansCJKsc-Regular.otf",
        "/usr/share/fonts/truetype/wqy/wqy-microhei.ttc",
        "/System/Library/Fonts/PingFang.ttc",
    ];

    if let Some(bytes) = candidates.iter().find_map(|path| std::fs::read(path).ok()) {
        let mut data = egui::FontData::from_owned(bytes);
        data.index = 0;
        fonts
            .font_data
            .insert("cjk_fallback".to_owned(), data.into());
        for family in [egui::FontFamily::Proportional, egui::FontFamily::Monospace] {
            fonts
                .families
                .entry(family)
                .or_default()
                .push("cjk_fallback".to_owned());
        }
    }

    ctx.set_fonts(fonts);
}

fn app_icon(size: u32) -> egui::IconData {
    let mut rgba = vec![0u8; (size * size * 4) as usize];
    let s = size as f32;
    let radius = s * 0.22;
    let chevron = egui::Stroke::new(s * 0.055, egui::Color32::from_rgb(88, 196, 221));
    let left = s * 0.34;
    let right = s * 0.62;
    let top = s * 0.35;
    let bottom = s * 0.65;
    let center = s * 0.5;

    for y in 0..size {
        for x in 0..size {
            let px = x as f32 + 0.5;
            let py = y as f32 + 0.5;
            if !inside_rounded_rect(px, py, s, radius) {
                continue;
            }

            let edge = distance_to_rounded_rect_edge(px, py, s, radius);
            let mut color = if edge < s * 0.018 {
                egui::Color32::from_rgb(42, 53, 67)
            } else {
                egui::Color32::from_rgb(18, 22, 29)
            };

            let lines = [
                (egui::pos2(left + s * 0.13, top), egui::pos2(left, center)),
                (
                    egui::pos2(left, center),
                    egui::pos2(left + s * 0.13, bottom),
                ),
                (egui::pos2(right - s * 0.13, top), egui::pos2(right, center)),
                (
                    egui::pos2(right, center),
                    egui::pos2(right - s * 0.13, bottom),
                ),
            ];
            if lines
                .iter()
                .any(|(a, b)| distance_to_segment(px, py, *a, *b) < chevron.width)
            {
                color = chevron.color;
            }
            if (px - center).hypot(py - center) < s * 0.035 {
                color = egui::Color32::from_rgb(114, 203, 139);
            }

            let index = ((y * size + x) * 4) as usize;
            rgba[index] = color.r();
            rgba[index + 1] = color.g();
            rgba[index + 2] = color.b();
            rgba[index + 3] = 255;
        }
    }

    egui::IconData {
        rgba,
        width: size,
        height: size,
    }
}

fn inside_rounded_rect(x: f32, y: f32, size: f32, radius: f32) -> bool {
    let min = radius;
    let max = size - radius;
    if x >= min && x <= max || y >= min && y <= max {
        return true;
    }
    let cx = if x < min { min } else { max };
    let cy = if y < min { min } else { max };
    (x - cx).hypot(y - cy) <= radius
}

fn distance_to_rounded_rect_edge(x: f32, y: f32, size: f32, radius: f32) -> f32 {
    let center = size * 0.5;
    let dx = (x - center).abs();
    let dy = (y - center).abs();
    let qx = dx - (center - radius);
    let qy = dy - (center - radius);
    let outside = qx.max(0.0).hypot(qy.max(0.0));
    let inside = qx.max(qy).min(0.0);
    (outside + inside - radius).abs()
}

fn distance_to_segment(px: f32, py: f32, a: egui::Pos2, b: egui::Pos2) -> f32 {
    let abx = b.x - a.x;
    let aby = b.y - a.y;
    let apx = px - a.x;
    let apy = py - a.y;
    let length_sq = abx * abx + aby * aby;
    let t = if length_sq > 0.0 {
        ((apx * abx + apy * aby) / length_sq).clamp(0.0, 1.0)
    } else {
        0.0
    };
    let dx = apx - abx * t;
    let dy = apy - aby * t;
    (dx * dx + dy * dy).sqrt()
}
