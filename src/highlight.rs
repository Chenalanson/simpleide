//! Syntect-backed syntax highlighting with a tiny per-document galley cache.

use std::cell::RefCell;
use std::ops::Range;
use std::path::Path;
use std::rc::Rc;
use std::str::FromStr;
use std::sync::{Arc, RwLock};

use eframe::egui::{self, Color32, FontId, Galley, TextFormat};
use syntect::easy::HighlightLines;
use syntect::highlighting::{
    Color, ScopeSelectors, StyleModifier, Theme, ThemeItem, ThemeSettings,
};
use syntect::parsing::SyntaxSet;
use syntect::util::LinesWithEndings;

use crate::codetheme::{self, CodeTheme};

pub struct Highlighter {
    syntax_set: SyntaxSet,
    theme: RwLock<Theme>,
    code_theme: RwLock<CodeTheme>,
}

impl Highlighter {
    pub fn new() -> Self {
        let syntax_set = SyntaxSet::load_defaults_newlines();
        let code_theme = *codetheme::default_theme();
        Self {
            syntax_set,
            theme: RwLock::new(build_theme(&code_theme)),
            code_theme: RwLock::new(code_theme),
        }
    }

    /// Switch the editor palette. Existing documents clear their layout caches
    /// in `App::apply_editor_theme`.
    pub fn set_code_theme(&self, code_theme: &CodeTheme) {
        if let Ok(mut theme) = self.theme.write() {
            *theme = build_theme(code_theme);
        }
        if let Ok(mut current) = self.code_theme.write() {
            *current = *code_theme;
        }
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
        search_ranges: &[(Range<usize>, bool)],
    ) -> egui::text::LayoutJob {
        let mut job = egui::text::LayoutJob::default();
        job.wrap.max_width = wrap_width;
        job.wrap.break_anywhere = false;
        job.break_on_newline = true;
        job.halign = egui::Align::LEFT;

        let code_theme = self
            .code_theme
            .read()
            .map(|theme| *theme)
            .unwrap_or(*codetheme::default_theme());
        let default_format = TextFormat {
            font_id: font_id.clone(),
            color: code_theme.text,
            ..Default::default()
        };

        let Some(syntax_name) = syntax_name else {
            job.append(text, 0.0, default_format);
            apply_search_highlights(&mut job, search_ranges, &code_theme);
            return job;
        };
        // Syntect's Makefile grammar produces a section per flag/path, which
        // makes egui re-layout long `C_INCLUDES`/`CFLAGS` lines very slowly.
        // A small line-oriented highlighter keeps variable names, comments and
        // targets colored without that cost.
        if matches!(syntax_name, "Makefile" | "CMake") {
            return build_make_like_job(text, &font_id, &code_theme, wrap_width, search_ranges);
        }
        let Some(syntax) = self.syntax_set.find_syntax_by_name(syntax_name) else {
            job.append(text, 0.0, default_format);
            return job;
        };

        let theme = self
            .theme
            .read()
            .expect("highlighter theme lock is not poisoned");
        let mut highlighter = HighlightLines::new(syntax, &theme);
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
        apply_search_highlights(&mut job, search_ranges, &code_theme);
        job
    }
}

fn build_theme(code: &CodeTheme) -> Theme {
    let mut theme = Theme {
        name: Some(code.name.to_owned()),
        author: None,
        settings: ThemeSettings {
            foreground: Some(syntect_color(code.text)),
            background: Some(syntect_color(code.bg)),
            caret: Some(syntect_color(code.text)),
            line_highlight: Some(syntect_color(code.line_highlight)),
            ..Default::default()
        },
        scopes: Vec::new(),
    };
    add_scope(
        &mut theme,
        "comment, punctuation.definition.comment",
        code.comment,
    );
    add_scope(
        &mut theme,
        "keyword, storage, storage.type, storage.modifier",
        code.keyword,
    );
    add_scope(
        &mut theme,
        "string, constant.character, constant.other",
        code.string,
    );
    add_scope(
        &mut theme,
        "constant.numeric, constant.language, constant",
        code.number,
    );
    add_scope(
        &mut theme,
        "entity.name.function, support.function, variable.function, meta.function-call",
        code.function,
    );
    add_scope(&mut theme, "meta.preprocessor", code.keyword);
    theme
}

fn add_scope(theme: &mut Theme, scope: &str, color: Color32) {
    let Ok(selectors) = ScopeSelectors::from_str(scope) else {
        return;
    };
    theme.scopes.push(ThemeItem {
        scope: selectors,
        style: StyleModifier {
            foreground: Some(syntect_color(color)),
            background: None,
            font_style: None,
        },
    });
}

fn syntect_color(color: Color32) -> Color {
    Color {
        r: color.r(),
        g: color.g(),
        b: color.b(),
        a: 255,
    }
}

fn build_make_like_job(
    text: &str,
    font_id: &FontId,
    code_theme: &CodeTheme,
    wrap_width: f32,
    search_ranges: &[(Range<usize>, bool)],
) -> egui::text::LayoutJob {
    let mut job = egui::text::LayoutJob::default();
    job.wrap.max_width = wrap_width;
    job.wrap.break_anywhere = false;
    job.break_on_newline = true;
    job.halign = egui::Align::LEFT;

    let format = |color: Color32| TextFormat {
        font_id: font_id.clone(),
        color,
        ..Default::default()
    };
    let plain = format(code_theme.text);
    let comment = format(code_theme.comment);
    let variable = format(code_theme.keyword);
    let target = format(code_theme.function);

    for line in text.split_inclusive('\n') {
        let (body, ending) = line
            .strip_suffix('\n')
            .map(|body| (body, "\n"))
            .unwrap_or((line, ""));
        if body.starts_with('\t') {
            job.append(body, 0.0, plain.clone());
            job.append(ending, 0.0, plain.clone());
            continue;
        }
        let leading = body.len() - body.trim_start().len();
        let (indent, rest) = body.split_at(leading);
        if !indent.is_empty() {
            job.append(indent, 0.0, plain.clone());
        }
        if rest.is_empty() {
            job.append(ending, 0.0, plain.clone());
            continue;
        }
        if rest.starts_with('#') {
            job.append(rest, 0.0, comment.clone());
            job.append(ending, 0.0, comment.clone());
            continue;
        }
        let equals = rest.find('=');
        let colon = rest.find(':').filter(|_| !rest.starts_with('-'));
        let assignment = equals.is_some_and(|equals| colon.is_none_or(|colon| equals < colon));
        if assignment {
            let equals = equals.expect("assignment has an equals sign");
            job.append(&rest[..equals], 0.0, variable.clone());
            job.append(&rest[equals..], 0.0, plain.clone());
        } else if let Some(colon) = colon {
            job.append(&rest[..colon], 0.0, target.clone());
            job.append(&rest[colon..], 0.0, plain.clone());
        } else {
            job.append(rest, 0.0, plain.clone());
        }
        if !ending.is_empty() {
            job.append(ending, 0.0, plain.clone());
        }
    }
    apply_search_highlights(&mut job, search_ranges, code_theme);
    job
}

fn apply_search_highlights(
    job: &mut egui::text::LayoutJob,
    search_ranges: &[(Range<usize>, bool)],
    code_theme: &CodeTheme,
) {
    if search_ranges.is_empty() {
        return;
    }
    let mut sections = Vec::with_capacity(job.sections.len() + search_ranges.len());
    for section in job.sections.drain(..) {
        let mut cursor = section.byte_range.start;
        let end = section.byte_range.end;
        let mut first = true;
        for (range, active) in search_ranges {
            if range.end <= cursor || range.start >= end {
                continue;
            }
            let start = range.start.max(cursor);
            let stop = range.end.min(end);
            if cursor < start {
                let mut plain = section.clone();
                plain.byte_range = cursor..start;
                if !first {
                    plain.leading_space = 0.0;
                }
                sections.push(plain);
                first = false;
            }
            if start < stop {
                let mut highlighted = section.clone();
                highlighted.byte_range = start..stop;
                highlighted.leading_space = if first { section.leading_space } else { 0.0 };
                highlighted.format.background = if *active {
                    code_theme.selection
                } else {
                    code_theme.line_highlight
                };
                sections.push(highlighted);
                first = false;
                cursor = stop;
            }
        }
        if cursor < end || (first && !section.byte_range.is_empty()) {
            let mut plain = section;
            plain.byte_range = cursor..end;
            if !first {
                plain.leading_space = 0.0;
            }
            sections.push(plain);
        }
    }
    job.sections = sections;
}

#[derive(Default)]
pub struct LayoutCache {
    revision: u64,
    wrap_width: f32,
    font_size: f32,
    decorations_key: u64,
    theme_key: u64,
    galley: Option<Arc<Galley>>,
}

pub type SharedLayoutCache = Rc<RefCell<LayoutCache>>;

pub struct LayoutSpec<'a> {
    pub syntax_name: Option<&'a str>,
    pub revision: u64,
    pub wrap_width: f32,
    pub font_id: FontId,
    pub decorations_key: u64,
    pub theme_key: u64,
    pub search_ranges: &'a [(Range<usize>, bool)],
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
                && cache_borrow.decorations_key == spec.decorations_key
                && cache_borrow.theme_key == spec.theme_key
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
        spec.search_ranges,
    );
    let galley = ui.fonts_mut(|fonts| fonts.layout_job(job));

    let mut cache = cache.borrow_mut();
    cache.revision = spec.revision;
    cache.wrap_width = normalized_width;
    cache.font_size = spec.font_id.size;
    cache.decorations_key = spec.decorations_key;
    cache.theme_key = spec.theme_key;
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
