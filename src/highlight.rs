//! Syntect-backed syntax highlighting with a tiny per-document galley cache.

use std::cell::RefCell;
use std::path::Path;
use std::rc::Rc;
use std::sync::Arc;

use eframe::egui::{self, Color32, FontId, Galley, TextFormat};
use syntect::easy::HighlightLines;
use syntect::highlighting::{Theme, ThemeSet};
use syntect::parsing::SyntaxSet;
use syntect::util::LinesWithEndings;

use crate::theme;

pub struct Highlighter {
    syntax_set: SyntaxSet,
    theme: Theme,
}

impl Highlighter {
    pub fn new() -> Self {
        let syntax_set = SyntaxSet::load_defaults_newlines();
        let theme_set = ThemeSet::load_defaults();
        let theme = theme_set
            .themes
            .get("base16-ocean.dark")
            .or_else(|| theme_set.themes.values().next())
            .expect("syntect ships with default themes")
            .clone();

        Self { syntax_set, theme }
    }

    /// Returns `(syntect syntax name, display label)`.
    pub fn detect(&self, path: &Path) -> (Option<String>, String) {
        let file_name = path
            .file_name()
            .map(|name| name.to_string_lossy())
            .unwrap_or_default();

        let special = match file_name.as_ref() {
            "Makefile" | "makefile" | "GNUmakefile" => Some("Makefile"),
            "CMakeLists.txt" => Some("CMake"),
            _ => None,
        };

        let syntax = special
            .and_then(|name| self.syntax_set.find_syntax_by_name(name))
            .or_else(|| {
                path.extension()
                    .map(|extension| extension.to_string_lossy())
                    .and_then(|extension| self.syntax_set.find_syntax_by_extension(&extension))
            })
            .or_else(|| {
                path.extension()
                    .map(|extension| extension.to_string_lossy().to_lowercase())
                    .and_then(|extension| self.syntax_set.find_syntax_by_extension(&extension))
            });

        if let Some(syntax) = syntax {
            (Some(syntax.name.clone()), syntax.name.clone())
        } else {
            let label = path
                .extension()
                .map(|extension| extension.to_string_lossy().to_uppercase())
                .filter(|extension| !extension.is_empty())
                .unwrap_or_else(|| "Plain Text".to_owned());
            (None, label)
        }
    }

    fn build_job(
        &self,
        text: &str,
        syntax_name: Option<&str>,
        font_id: FontId,
        wrap_width: f32,
    ) -> egui::text::LayoutJob {
        let mut job = egui::text::LayoutJob::default();
        job.wrap.max_width = wrap_width;
        job.wrap.break_anywhere = false;
        job.break_on_newline = true;
        job.halign = egui::Align::LEFT;

        let default_format = TextFormat {
            font_id: font_id.clone(),
            color: theme::TEXT,
            ..Default::default()
        };

        let Some(syntax_name) = syntax_name else {
            job.append(text, 0.0, default_format);
            return job;
        };
        let Some(syntax) = self.syntax_set.find_syntax_by_name(syntax_name) else {
            job.append(text, 0.0, default_format);
            return job;
        };

        let mut highlighter = HighlightLines::new(syntax, &self.theme);
        let mut appended = 0usize;
        for line in LinesWithEndings::from(text) {
            match highlighter.highlight_line(line, &self.syntax_set) {
                Ok(ranges) => {
                    for (style, piece) in ranges {
                        if piece.is_empty() {
                            continue;
                        }
                        let color = Color32::from_rgb(
                            style.foreground.r,
                            style.foreground.g,
                            style.foreground.b,
                        );
                        job.append(
                            piece,
                            0.0,
                            TextFormat {
                                font_id: font_id.clone(),
                                color,
                                ..Default::default()
                            },
                        );
                        appended += piece.len();
                    }
                }
                Err(_) => {
                    job.append(line, 0.0, default_format.clone());
                    appended += line.len();
                }
            }
        }

        // Syntect should cover the entire input, but a parse error must never
        // make the buffer and its galley disagree.
        if appended < text.len() {
            job.append(&text[appended..], 0.0, default_format);
        }
        job
    }
}

#[derive(Default)]
pub struct LayoutCache {
    revision: u64,
    wrap_width: f32,
    font_size: f32,
    galley: Option<Arc<Galley>>,
}

pub type SharedLayoutCache = Rc<RefCell<LayoutCache>>;

pub struct LayoutSpec<'a> {
    pub syntax_name: Option<&'a str>,
    pub revision: u64,
    pub wrap_width: f32,
    pub font_id: FontId,
}

pub fn new_cache() -> SharedLayoutCache {
    Rc::new(RefCell::new(LayoutCache::default()))
}

pub fn layout_cached(
    highlighter: &Highlighter,
    ui: &egui::Ui,
    text: &str,
    spec: &LayoutSpec<'_>,
    cache: &SharedLayoutCache,
) -> Arc<Galley> {
    let normalized_width = if spec.wrap_width.is_finite() {
        (spec.wrap_width * 2.0).round() / 2.0
    } else {
        spec.wrap_width
    };
    {
        let cache_borrow = cache.borrow();
        if let Some(galley) = &cache_borrow.galley {
            if cache_borrow.revision == spec.revision
                && (cache_borrow.wrap_width - normalized_width).abs() < 0.25
                && (cache_borrow.font_size - spec.font_id.size).abs() < f32::EPSILON
            {
                return Arc::clone(galley);
            }
        }
    }

    let job = highlighter.build_job(
        text,
        spec.syntax_name,
        spec.font_id.clone(),
        normalized_width,
    );
    let galley = ui.fonts_mut(|fonts| fonts.layout_job(job));

    let mut cache = cache.borrow_mut();
    cache.revision = spec.revision;
    cache.wrap_width = normalized_width;
    cache.font_size = spec.font_id.size;
    cache.galley = Some(Arc::clone(&galley));
    galley
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detects_common_embedded_languages() {
        let highlighter = Highlighter::new();
        for file in [
            "main.c",
            "board.h",
            "main.cpp",
            "startup.s",
            "main.rs",
            "Makefile",
            "CMakeLists.txt",
        ] {
            let (syntax, label) = highlighter.detect(Path::new(file));
            assert!(
                syntax.is_some(),
                "{file} was not mapped to a syntax (label {label})"
            );
        }
    }
}
