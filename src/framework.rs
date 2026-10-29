//! Reader for the `framework.md` produced by the stm32-embedded-dev skill.
//!
//! AI mode deliberately does not guess the architecture from regexes when this
//! document is absent. The skill owns the document format (layers, software
//! architecture, runtime view); this module only parses its headings and the
//! Mermaid Gantt block so the architecture panel can render the same facts.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use crate::util::read_text_file;

const MAX_DOC_BYTES: u64 = 4 * 1024 * 1024;
const MAX_SECTION_BYTES: usize = 400_000;

#[derive(Clone, Debug, Default)]
pub struct FrameworkDoc {
    pub title: String,
    pub sections: Vec<FrameworkSection>,
    pub gantt: Option<GanttChart>,
}

#[derive(Clone, Debug, Default)]
pub struct FrameworkSection {
    pub level: u8,
    pub heading: String,
    pub body: String,
}

impl FrameworkDoc {
    pub fn section(&self, needle: &str) -> Option<&FrameworkSection> {
        self.sections
            .iter()
            .find(|section| section.heading.contains(needle) && section.level >= 2)
    }

    pub fn section_any(&self, needles: &[&str]) -> Option<&FrameworkSection> {
        needles.iter().find_map(|needle| self.section(needle))
    }

    pub fn missing_sections(&self) -> Vec<&'static str> {
        let mut missing = Vec::new();
        if self.section("层级表").is_none() {
            missing.push("1.1 层级表");
        }
        if self.section("执行范式").is_none() {
            missing.push("2.1 执行范式");
        }
        if self.section("调度模式").is_none() {
            missing.push("2.2 调度模式");
        }
        if self.section_any(&["设计模式清单", "设计模式"]).is_none() {
            missing.push("2.3 设计模式");
        }
        if self.section("启动调用链").is_none() {
            missing.push("3.1 启动调用链");
        }
        if self.section("调度时间线").is_none() {
            missing.push("3.2 主循环调度时间线");
        }
        if self.section("中断来源与抢占关系").is_none() {
            missing.push("3.3 中断来源与抢占关系表");
        }
        missing
    }
}

#[derive(Clone, Debug, Default)]
pub struct GanttChart {
    pub title: String,
    pub rows: Vec<GanttRow>,
    pub note: String,
}

#[derive(Clone, Debug, Default)]
pub struct GanttRow {
    pub label: String,
    pub section: String,
    pub kind: GanttKind,
    pub segments: Vec<GanttSegment>,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum GanttKind {
    #[default]
    Task,
    Main,
    Tick,
    Interrupt,
    Event,
}

#[derive(Clone, Debug, Default)]
pub struct GanttSegment {
    pub start: f32,
    pub end: f32,
}

pub fn find(root: &Path) -> Option<PathBuf> {
    // The skill places the document at the project root or in a docs-like
    // directory. Prefer those locations before a bounded recursive search.
    for candidate in [
        root.join("framework.md"),
        root.join("docs").join("framework.md"),
        root.join("doc").join("framework.md"),
        root.join("Doc").join("framework.md"),
        root.join("Documentation").join("framework.md"),
    ] {
        if candidate.is_file() {
            return Some(candidate);
        }
    }
    find_recursive(root, 0)
}

fn find_recursive(directory: &Path, depth: usize) -> Option<PathBuf> {
    if depth > 3 {
        return None;
    }
    let entries = std::fs::read_dir(directory).ok()?;
    let mut directories = Vec::new();
    for entry in entries.flatten() {
        let path = entry.path();
        let Ok(file_type) = entry.file_type() else {
            continue;
        };
        if file_type.is_file()
            && path
                .file_name()
                .is_some_and(|name| name.eq_ignore_ascii_case("framework.md"))
        {
            return Some(path);
        }
        if file_type.is_dir() {
            let name = path
                .file_name()
                .map(|name| name.to_string_lossy().to_ascii_lowercase())
                .unwrap_or_default();
            if !matches!(
                name.as_str(),
                "build"
                    | "debug"
                    | "release"
                    | "out"
                    | "target"
                    | "objects"
                    | "listings"
                    | "tools"
                    | "dist"
                    | "node_modules"
                    | "drivers"
                    | "cmsis"
                    | "middlewares"
            ) {
                directories.push(path);
            }
        }
    }
    directories.sort();
    for directory in directories {
        if let Some(path) = find_recursive(&directory, depth + 1) {
            return Some(path);
        }
    }
    None
}

pub fn load(path: &Path) -> Result<FrameworkDoc, String> {
    let metadata =
        std::fs::metadata(path).map_err(|err| format!("无法读取 framework.md 信息：{err}"))?;
    if metadata.len() > MAX_DOC_BYTES {
        return Err("framework.md 超过 4 MB，拒绝加载".to_owned());
    }
    let loaded = read_text_file(path)?;
    let raw = loaded.text;
    let title = raw
        .lines()
        .find_map(|line| line.strip_prefix("# ").map(str::trim))
        .filter(|title| !title.is_empty())
        .unwrap_or("Framework 设计说明")
        .to_owned();
    let sections = parse_sections(&raw);
    let gantt = parse_gantt(&raw);
    Ok(FrameworkDoc {
        title,
        sections,
        gantt,
    })
}

fn parse_sections(text: &str) -> Vec<FrameworkSection> {
    let mut sections = Vec::new();
    let mut current: Option<FrameworkSection> = None;
    let mut in_fence = false;
    for line in text.lines() {
        let trimmed = line.trim_start();
        if trimmed.starts_with("```") {
            in_fence = !in_fence;
        }
        let heading = if in_fence {
            None
        } else if let Some(rest) = line.strip_prefix("### ") {
            Some((3u8, rest.trim()))
        } else if let Some(rest) = line.strip_prefix("## ") {
            Some((2u8, rest.trim()))
        } else {
            line.strip_prefix("# ").map(|rest| (1u8, rest.trim()))
        };
        if let Some((level, heading)) = heading {
            if let Some(mut section) = current.take() {
                trim_section(&mut section);
                sections.push(section);
            }
            current = Some(FrameworkSection {
                level,
                heading: heading.to_owned(),
                body: String::new(),
            });
            continue;
        }
        if let Some(section) = &mut current {
            if section.body.len() < MAX_SECTION_BYTES {
                section.body.push_str(line);
                section.body.push('\n');
            }
        }
    }
    if let Some(mut section) = current {
        trim_section(&mut section);
        sections.push(section);
    }
    sections
}

fn trim_section(section: &mut FrameworkSection) {
    section.body = section.body.trim().to_owned();
}

fn parse_gantt(raw: &str) -> Option<GanttChart> {
    let sections = parse_sections(raw);
    let body = sections
        .iter()
        .find(|section| {
            section.heading.contains("主循环调度时间线")
                || section.heading.contains("调度时间线")
                || section.heading.contains("时间线")
        })
        .map(|section| section.body.as_str())
        .unwrap_or(raw);
    let block = extract_mermaid_gantt(body)?;
    parse_mermaid_gantt(&block)
}

fn extract_mermaid_gantt(body: &str) -> Option<String> {
    let mut in_mermaid = false;
    let mut block = String::new();
    for line in body.lines() {
        let trimmed = line.trim();
        if !in_mermaid {
            if trimmed.starts_with("```") && trimmed.to_ascii_lowercase().contains("mermaid") {
                in_mermaid = true;
            }
            continue;
        }
        if trimmed.starts_with("```") {
            if block.contains("gantt") {
                return Some(block);
            }
            in_mermaid = false;
            block.clear();
            continue;
        }
        block.push_str(line);
        block.push('\n');
    }
    None
}

fn parse_mermaid_gantt(block: &str) -> Option<GanttChart> {
    #[derive(Clone, Debug)]
    struct Task {
        label: String,
        section: String,
        start: f64,
        end: f64,
        milestone: bool,
    }

    let mut title = String::new();
    let mut section = String::new();
    let mut tasks = Vec::new();
    let mut ends: BTreeMap<String, f64> = BTreeMap::new();
    let mut index = 0usize;
    for line in block.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() || trimmed == "gantt" {
            continue;
        }
        if let Some(value) = trimmed.strip_prefix("title ") {
            title = value.trim().to_owned();
            continue;
        }
        if let Some(value) = trimmed.strip_prefix("section ") {
            section = value.trim().to_owned();
            continue;
        }
        if trimmed.starts_with("dateFormat")
            || trimmed.starts_with("axisFormat")
            || trimmed.starts_with("todayMarker")
            || trimmed.starts_with("excludes")
        {
            continue;
        }
        let Some((label, spec)) = trimmed.split_once(':') else {
            continue;
        };
        let label = label.trim();
        if label.is_empty() {
            continue;
        }
        let mut parts = spec
            .split(',')
            .map(str::trim)
            .filter(|part| !part.is_empty())
            .collect::<Vec<_>>();
        let mut milestone = false;
        parts.retain(|part| {
            if matches!(*part, "milestone") {
                milestone = true;
                return false;
            }
            !matches!(*part, "active" | "crit" | "done")
        });
        if parts.is_empty() {
            continue;
        }
        let id = if parts[0].chars().next().is_some_and(|c| c.is_ascii_digit())
            || parts[0].starts_with("after")
        {
            index += 1;
            format!("task{index}")
        } else {
            parts.remove(0).to_owned()
        };
        let (start, end) = if parts.first().is_some_and(|part| part.starts_with("after")) {
            let reference = parts
                .first()
                .map(|part| part.trim_start_matches("after").trim())
                .filter(|part| !part.is_empty())
                .map(str::to_owned)
                .or_else(|| parts.get(1).map(|part| (*part).to_owned()))
                .unwrap_or_default();
            let start = ends.get(&reference).copied().unwrap_or(0.0);
            let duration = parts
                .get(2)
                .or_else(|| parts.get(1))
                .and_then(|part| part.parse::<f64>().ok())
                .unwrap_or(1.0);
            (start, start + duration.max(0.0))
        } else {
            let start = parts
                .first()
                .and_then(|part| part.parse::<f64>().ok())
                .unwrap_or(0.0);
            let end = parts
                .get(1)
                .and_then(|part| part.parse::<f64>().ok())
                .unwrap_or(start + 1.0);
            (start, end.max(start))
        };
        ends.insert(id.clone(), end);
        tasks.push(Task {
            label: label.to_owned(),
            section: section.clone(),
            start,
            end,
            milestone,
        });
    }
    if tasks.is_empty() {
        return None;
    }
    let max_end = tasks
        .iter()
        .map(|task| task.end)
        .fold(0.0_f64, f64::max)
        .max(1.0);
    let rows = tasks
        .into_iter()
        .map(|task| {
            let start = (task.start / max_end).clamp(0.0, 1.0) as f32;
            let end = if task.milestone {
                (start + 0.006).min(1.0)
            } else {
                (task.end / max_end).clamp(start as f64, 1.0) as f32
            };
            let kind = if task.milestone
                || task.label.to_ascii_lowercase().contains("systick")
                || task.section.contains("中断")
            {
                if task.label.to_ascii_lowercase().contains("systick") {
                    GanttKind::Tick
                } else {
                    GanttKind::Interrupt
                }
            } else if task.section.contains("循环") {
                GanttKind::Main
            } else if task.section.contains("事件") || task.label.contains("事件") {
                GanttKind::Event
            } else {
                GanttKind::Task
            };
            GanttRow {
                label: task.label,
                section: task.section,
                kind,
                segments: vec![GanttSegment { start, end }],
            }
        })
        .collect();
    Some(GanttChart {
        title,
        rows,
        note: "来源：framework.md 第 3.2 节 Mermaid 甘特图（静态规划视图）。".to_owned(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::util::{write_text_file, LineEnding, TextEncoding};

    fn write(path: &Path, text: &str) {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).unwrap();
        }
        write_text_file(path, text, LineEnding::Lf, false, TextEncoding::Utf8).unwrap();
    }

    #[test]
    fn reads_sections_and_gantt() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("framework.md");
        write(
            &path,
            "# Demo Framework\n\n## 2. 软件架构\n### 2.1 执行范式\n- 前后台\n\n### 2.2 调度模式\n- 时间片\n\n## 3. 运行视图\n### 3.2 主循环调度时间线\n```mermaid\ngantt\ntitle 调度\nsection 调度线程\n菜单任务 :active, a1, 0, 5\n采样任务 :a2, after a1, 10\nsection 中断\nSysTick 1ms :milestone, i1, 0, 0\n```\n",
        );
        let doc = load(&path).unwrap();
        assert_eq!(doc.title, "Demo Framework");
        assert!(doc.section("执行范式").unwrap().body.contains("前后台"));
        let gantt = doc.gantt.unwrap();
        assert_eq!(gantt.rows.len(), 3);
        assert!(gantt.rows.iter().any(|row| row.kind == GanttKind::Tick));
        assert!(gantt.rows.iter().any(|row| row.label == "采样任务"));
    }

    #[test]
    fn finds_doc_in_project_or_docs_folder() {
        let dir = tempfile::tempdir().unwrap();
        write(&dir.path().join("docs/framework.md"), "# Doc\n");
        assert_eq!(
            find(dir.path()).unwrap(),
            dir.path().join("docs/framework.md")
        );
    }

    #[test]
    fn parses_after_reference_as_sequential_start() {
        let chart = parse_mermaid_gantt(
            "gantt\ntitle t\nsection A\nfirst :a1, 0, 5\nsecond :a2, after a1, 5\n",
        )
        .unwrap();
        assert_eq!(chart.rows.len(), 2);
        assert!(chart.rows[1].segments[0].start >= chart.rows[0].segments[0].end);
    }
}
