//! Build-output storage: ANSI SGR parsing, severity classification and line caps.

use std::collections::VecDeque;

use eframe::egui::Color32;

use crate::theme;

const MAX_LINES: usize = 20_000;
const MAX_BYTES: usize = 8 * 1024 * 1024;
const MAX_LINE_CHARS: usize = 12_000;
const MAX_DIAGNOSTICS: usize = 1000;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Severity {
    Normal,
    Info,
    Command,
    Success,
    Warning,
    Error,
}

impl Severity {
    pub fn color(self) -> Color32 {
        match self {
            Self::Normal => theme::text(),
            Self::Info => theme::info(),
            Self::Command => theme::accent(),
            Self::Success => theme::success(),
            Self::Warning => theme::warning(),
            Self::Error => theme::error(),
        }
    }

    pub fn is_problem(self) -> bool {
        matches!(self, Self::Warning | Self::Error)
    }
}

#[derive(Clone, Debug)]
pub struct LogSpan {
    pub text: String,
    pub color: Option<Color32>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SourceLocation {
    pub path: String,
    pub line: usize,
    pub column: usize,
}

#[derive(Clone, Debug)]
pub struct LogDiagnostic {
    pub location: SourceLocation,
    pub severity: Severity,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LogFilter {
    All,
    Errors,
    Warnings,
}

impl LogFilter {
    pub fn matches(self, severity: Severity) -> bool {
        match self {
            Self::All => true,
            Self::Errors => severity == Severity::Error,
            Self::Warnings => severity == Severity::Warning,
        }
    }
}

#[derive(Clone, Debug)]
pub struct LogLine {
    pub spans: Vec<LogSpan>,
    pub severity: Severity,
    pub plain: String,
    pub location: Option<SourceLocation>,
}

#[derive(Default)]
pub struct LogStore {
    lines: VecDeque<LogLine>,
    bytes: usize,
    diagnostics: Vec<LogDiagnostic>,
    diagnostics_revision: u64,
}

impl LogStore {
    pub fn clear(&mut self) {
        self.lines.clear();
        self.bytes = 0;
        self.diagnostics.clear();
        self.diagnostics_revision = self.diagnostics_revision.wrapping_add(1);
    }

    pub fn clear_diagnostics(&mut self) {
        self.diagnostics.clear();
        self.diagnostics_revision = self.diagnostics_revision.wrapping_add(1);
    }

    pub fn diagnostics(&self) -> &[LogDiagnostic] {
        &self.diagnostics
    }

    pub fn diagnostics_revision(&self) -> u64 {
        self.diagnostics_revision
    }

    pub fn len(&self) -> usize {
        self.lines.len()
    }

    pub fn is_empty(&self) -> bool {
        self.lines.is_empty()
    }

    pub fn line_at(&self, index: usize) -> Option<&LogLine> {
        self.lines.get(index)
    }

    pub fn push_raw(&mut self, raw: &str) {
        let raw = truncate_line(raw);
        let (segments, plain) = parse_ansi(&raw);
        let severity = classify(&plain);
        let location = parse_location(&plain);
        if severity.is_problem() {
            if let Some(location) = &location {
                self.diagnostics.push(LogDiagnostic {
                    location: location.clone(),
                    severity,
                });
                if self.diagnostics.len() > MAX_DIAGNOSTICS {
                    self.diagnostics.remove(0);
                }
                self.diagnostics_revision = self.diagnostics_revision.wrapping_add(1);
            }
        }
        self.push_line(LogLine {
            spans: segments,
            severity,
            plain,
            location,
        });
    }

    pub fn push_message(&mut self, text: impl Into<String>, severity: Severity) {
        let text = text.into();
        if text.is_empty() {
            return;
        }
        self.push_line(LogLine {
            spans: vec![LogSpan {
                text: text.clone(),
                color: Some(severity.color()),
            }],
            severity,
            plain: text,
            location: None,
        });
    }

    pub fn push_blank(&mut self) {
        self.push_line(LogLine {
            spans: Vec::new(),
            severity: Severity::Normal,
            plain: String::new(),
            location: None,
        });
    }

    fn push_line(&mut self, line: LogLine) {
        self.bytes += line.plain.len() + 1;
        self.lines.push_back(line);
        while self.lines.len() > MAX_LINES || self.bytes > MAX_BYTES {
            if let Some(removed) = self.lines.pop_front() {
                self.bytes = self.bytes.saturating_sub(removed.plain.len() + 1);
            } else {
                break;
            }
        }
    }

    pub fn all_text(&self) -> String {
        let mut text = String::new();
        for line in &self.lines {
            text.push_str(&line.plain);
            text.push('\n');
        }
        text
    }

    pub fn problem_count(&self) -> usize {
        self.lines
            .iter()
            .filter(|line| line.severity.is_problem())
            .count()
    }
}

/// Extract the missing symbol from a linker `undefined reference to ...` line.
pub fn undefined_reference(line: &str) -> Option<String> {
    let rest = line.split("undefined reference to ").nth(1)?;
    let symbol = rest
        .trim()
        .trim_matches(|ch| ch == '`' || ch == '\'' || ch == '"')
        .trim_end_matches(':')
        .trim();
    if symbol.is_empty() || symbol.len() > 160 {
        return None;
    }
    Some(symbol.to_owned())
}

pub fn parse_location(line: &str) -> Option<SourceLocation> {
    let trimmed = line.trim_start();
    let parts = trimmed.split(':').collect::<Vec<_>>();
    for index in 1..parts.len() {
        let candidate = parts[index].trim();
        if candidate.is_empty() || !candidate.chars().all(|ch| ch.is_ascii_digit()) {
            continue;
        }
        let Ok(line_no) = candidate.parse::<usize>() else {
            continue;
        };
        if line_no == 0 {
            continue;
        }
        let path = parts[..index].join(":");
        if !looks_like_path(&path) {
            continue;
        }
        let column = parts
            .get(index + 1)
            .and_then(|part| part.trim().parse::<usize>().ok())
            .filter(|column| *column > 0)
            .unwrap_or(1);
        return Some(SourceLocation {
            path: path.trim_matches('"').to_owned(),
            line: line_no,
            column,
        });
    }
    None
}

fn looks_like_path(path: &str) -> bool {
    let path = path.trim();
    !path.is_empty()
        && (path.contains('.')
            || path.contains('/')
            || path.contains('\\')
            || matches!(path, "Makefile" | "makefile" | "GNUmakefile"))
}

fn truncate_line(raw: &str) -> String {
    let raw = raw.trim_end_matches(['\r', '\n']);
    if raw.chars().count() <= MAX_LINE_CHARS {
        return raw.to_owned();
    }
    let mut value: String = raw.chars().take(MAX_LINE_CHARS).collect();
    value.push_str(" … [输出过长，已截断]");
    value
}

fn classify(text: &str) -> Severity {
    let lower = text.to_ascii_lowercase();
    let error = lower.contains("error:")
        || lower.contains("error[")
        || lower.contains("error c")
        || lower.contains("fatal error")
        || lower.contains("undefined reference")
        || lower.contains("cannot find -l")
        || lower.contains("link failed")
        || lower.contains("linker command failed")
        || lower.contains("failed:")
        || lower.contains("make: ***")
        || lower.contains("ninja: build stopped")
        || text.contains("错误")
        || text.contains("失败");

    if error {
        Severity::Error
    } else if lower.contains("warning:")
        || lower.contains("warning[")
        || lower.contains("warning c")
        || text.contains("警告")
    {
        Severity::Warning
    } else {
        Severity::Normal
    }
}

fn parse_ansi(input: &str) -> (Vec<LogSpan>, String) {
    let bytes = input.as_bytes();
    let mut spans = Vec::new();
    let mut plain = String::with_capacity(input.len());
    let mut color: Option<Color32> = None;
    let mut bold = false;
    let mut span_start = 0usize;
    let mut i = 0usize;

    while i < bytes.len() {
        if bytes[i] != 0x1B {
            i += 1;
            continue;
        }

        push_span(&mut spans, &mut plain, &input[span_start..i], color);
        i += 1;
        if i >= bytes.len() {
            span_start = i;
            break;
        }

        match bytes[i] {
            b'[' => {
                i += 1;
                let params_start = i;
                while i < bytes.len() && !(0x40..=0x7E).contains(&bytes[i]) {
                    i += 1;
                }
                if i < bytes.len() {
                    let is_sgr = bytes[i] == b'm';
                    if is_sgr {
                        let params = &input[params_start..i];
                        apply_sgr(params, &mut color, &mut bold);
                    }
                    i += 1;
                }
            }
            b']' => {
                // OSC: drop until BEL or ST.
                i += 1;
                while i < bytes.len() {
                    if bytes[i] == 0x07 {
                        i += 1;
                        break;
                    }
                    if bytes[i] == 0x1B && i + 1 < bytes.len() && bytes[i + 1] == b'\\' {
                        i += 2;
                        break;
                    }
                    i += 1;
                }
            }
            _ => {
                i += 1;
            }
        }
        span_start = i;
    }

    if span_start < input.len() {
        push_span(&mut spans, &mut plain, &input[span_start..], color);
    }

    if spans.is_empty() {
        spans.push(LogSpan {
            text: String::new(),
            color: None,
        });
    }
    let _ = bold;
    (spans, plain)
}

fn push_span(spans: &mut Vec<LogSpan>, plain: &mut String, raw: &str, color: Option<Color32>) {
    if raw.is_empty() {
        return;
    }
    let cleaned: String = raw
        .chars()
        .filter(|ch| *ch == '\t' || !ch.is_control())
        .collect();
    if cleaned.is_empty() {
        return;
    }
    plain.push_str(&cleaned);
    spans.push(LogSpan {
        text: cleaned,
        color,
    });
}

fn apply_sgr(params: &str, color: &mut Option<Color32>, bold: &mut bool) {
    let values: Vec<u16> = if params.is_empty() {
        vec![0]
    } else {
        params
            .split(';')
            .map(|value| value.parse::<u16>().unwrap_or(0))
            .collect()
    };

    let mut index = 0usize;
    while index < values.len() {
        let value = values[index];
        match value {
            0 => {
                *color = None;
                *bold = false;
            }
            1 => *bold = true,
            2 | 22 => *bold = false,
            30..=37 => {
                *color = Some(basic_color(value - 30, *bold));
            }
            39 => *color = None,
            40..=47 | 49 => {}
            90..=97 => {
                *color = Some(basic_color(value - 90, true));
            }
            38 | 48 => {
                let foreground = value == 38;
                let mut parsed: Option<Color32> = None;
                if index + 1 < values.len() {
                    match values[index + 1] {
                        5 if index + 2 < values.len() => {
                            parsed = Some(color_256(values[index + 2]));
                            index += 2;
                        }
                        2 if index + 4 < values.len() => {
                            parsed = Some(adapt_light(Color32::from_rgb(
                                values[index + 2].min(255) as u8,
                                values[index + 3].min(255) as u8,
                                values[index + 4].min(255) as u8,
                            )));
                            index += 4;
                        }
                        _ => {}
                    }
                }
                if foreground {
                    *color = parsed;
                }
            }
            _ => {}
        }
        index += 1;
    }
}

fn basic_color(index: u16, bright: bool) -> Color32 {
    if theme::is_dark() {
        match (index, bright) {
            (0, false) => Color32::from_rgb(83, 94, 108),
            (0, true) => Color32::from_rgb(118, 130, 145),
            (1, false) => Color32::from_rgb(214, 103, 110),
            (1, true) => Color32::from_rgb(245, 132, 138),
            (2, false) => Color32::from_rgb(113, 190, 135),
            (2, true) => Color32::from_rgb(139, 220, 159),
            (3, false) => Color32::from_rgb(220, 175, 103),
            (3, true) => Color32::from_rgb(245, 201, 128),
            (4, false) => Color32::from_rgb(111, 159, 211),
            (4, true) => Color32::from_rgb(136, 184, 236),
            (5, false) => Color32::from_rgb(188, 140, 205),
            (5, true) => Color32::from_rgb(213, 164, 229),
            (6, false) => Color32::from_rgb(92, 187, 202),
            (6, true) => Color32::from_rgb(122, 216, 230),
            (7, false) => Color32::from_rgb(205, 212, 222),
            (7, true) => Color32::from_rgb(240, 244, 249),
            _ => theme::text(),
        }
    } else {
        match (index, bright) {
            (0, false) => Color32::from_rgb(92, 102, 114),
            (0, true) => Color32::from_rgb(66, 75, 85),
            (1, false) => Color32::from_rgb(178, 59, 69),
            (1, true) => Color32::from_rgb(143, 46, 55),
            (2, false) => Color32::from_rgb(47, 125, 79),
            (2, true) => Color32::from_rgb(37, 102, 63),
            (3, false) => Color32::from_rgb(138, 90, 18),
            (3, true) => Color32::from_rgb(109, 70, 13),
            (4, false) => Color32::from_rgb(61, 111, 158),
            (4, true) => Color32::from_rgb(49, 90, 130),
            (5, false) => Color32::from_rgb(120, 84, 155),
            (5, true) => Color32::from_rgb(95, 65, 128),
            (6, false) => Color32::from_rgb(47, 125, 134),
            (6, true) => Color32::from_rgb(37, 100, 107),
            (7, false) => Color32::from_rgb(44, 51, 58),
            (7, true) => Color32::from_rgb(31, 36, 40),
            _ => theme::text(),
        }
    }
}

fn adapt_light(color: Color32) -> Color32 {
    if theme::is_dark() {
        return color;
    }
    let luminance = (0.2126 * f32::from(color.r())
        + 0.7152 * f32::from(color.g())
        + 0.0722 * f32::from(color.b()))
        / 255.0;
    if luminance > 0.68 {
        theme::blend(color, theme::text(), (luminance - 0.68) / 0.32)
    } else {
        color
    }
}

fn color_256(index: u16) -> Color32 {
    if index < 16 {
        return basic_color(index % 8, index >= 8);
    }
    if index < 232 {
        let value = index - 16;
        let r = value / 36;
        let g = (value % 36) / 6;
        let b = value % 6;
        let channel = |component: u16| -> u8 {
            if component == 0 {
                0
            } else {
                (55 + component * 40).min(255) as u8
            }
        };
        return adapt_light(Color32::from_rgb(channel(r), channel(g), channel(b)));
    }
    let gray = (8 + (index.saturating_sub(232)) * 10).min(255) as u8;
    adapt_light(Color32::from_rgb(gray, gray, gray))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_ansi_sgr() {
        let (spans, plain) = parse_ansi("\x1b[31merror:\x1b[0m missing file\n");
        assert_eq!(plain, "error: missing file");
        assert_eq!(spans.len(), 2);
        assert_eq!(spans[0].text, "error:");
        assert!(spans[0].color.is_some());
        assert_eq!(spans[1].text, " missing file");
    }

    #[test]
    fn strips_osc_sequences() {
        let (_, plain) = parse_ansi("\x1b]0;title\x07hello");
        assert_eq!(plain, "hello");
    }

    #[test]
    fn classifies_gcc_errors() {
        assert_eq!(
            classify("src/main.c:4:2: error: expected ';'"),
            Severity::Error
        );
        assert_eq!(
            classify("src/main.c:4:2: warning: unused variable"),
            Severity::Warning
        );
        assert_eq!(
            classify("Finished release [optimized] target(s)"),
            Severity::Normal
        );
    }

    #[test]
    fn extracts_undefined_reference_symbols() {
        assert_eq!(
            undefined_reference("main.c:100: undefined reference to `usart_test'"),
            Some("usart_test".to_owned())
        );
        assert_eq!(
            undefined_reference("ld: undefined reference to `Serial_Send(char*)'"),
            Some("Serial_Send(char*)".to_owned())
        );
        assert_eq!(
            undefined_reference("collect2.exe: error: ld returned 1"),
            None
        );
    }

    #[test]
    fn parses_compiler_locations() {
        let location = parse_location("Core/Src/main.c:12:5: error: expected ';'").unwrap();
        assert_eq!(location.path, "Core/Src/main.c");
        assert_eq!(location.line, 12);
        assert_eq!(location.column, 5);

        let location = parse_location(r"D:\proj\main.c:7: warning: unused").unwrap();
        assert_eq!(location.path, r"D:\proj\main.c");
        assert_eq!(location.line, 7);
        assert_eq!(location.column, 1);

        assert_eq!(parse_location("ld: undefined reference to `foo'"), None);
    }

    #[test]
    fn collects_diagnostics_only_for_problem_lines() {
        let mut store = LogStore::default();
        store.push_raw("Core/Src/main.c:4:2: error: bad");
        store.push_raw("Core/Src/main.c:5:1: warning: maybe");
        store.push_raw("finished");
        assert_eq!(store.diagnostics().len(), 2);
        assert_eq!(store.line_at(0).unwrap().location.as_ref().unwrap().line, 4);
        store.clear_diagnostics();
        assert!(store.diagnostics().is_empty());
    }

    #[test]
    fn caps_line_length() {
        let raw = "x".repeat(MAX_LINE_CHARS + 10);
        let store = {
            let mut store = LogStore::default();
            store.push_raw(&raw);
            store
        };
        assert!(store.line_at(0).unwrap().plain.contains("已截断"));
    }
}
