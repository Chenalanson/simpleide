//! OpenAI-compatible chat client and STM32 skill/context loader for AI mode.

use std::collections::HashSet;
use std::fmt::Write as _;
use std::io::{BufRead, BufReader};
use std::path::{Path, PathBuf};
use std::sync::mpsc;
use std::sync::OnceLock;
use std::time::Duration;

use regex::Regex;
use serde_json::{json, Value};

use crate::config::{BuildConfig, ProjectDebugSettings};
use crate::ignore::IgnoreRules;
use crate::memory::ResolvedMemory;
use crate::project_meta::ProjectMeta;
use crate::util::{display_path, read_text_file};

const SKILL_ID: &str = "stm32-embedded-dev";
const MAX_SKILL_BYTES: usize = 200_000;
const MAX_SYSTEM_PROMPT_BYTES: usize = 240_000;
const MAX_TREE_FILES: usize = 400;
/// Reserve enough room for the edit protocol and output requirements at the end
/// of the system prompt. Project files must not crowd them out.
const PROMPT_PROTOCOL_RESERVE: usize = 16_000;
const MAX_FRAMEWORK_BYTES: usize = 120_000;
const MAX_FRAMEWORK_FILES: usize = 32;
const MAX_FRAMEWORK_FILE_BYTES: usize = 60_000;
/// Files explicitly requested with `se-list` / `se-read` are inserted before
/// the rest of the project context and capped so they cannot crowd out the
/// edit protocol.
const MAX_RETRIEVAL_BYTES: usize = 48_000;

#[derive(Clone, Debug, Default)]
pub struct AiConfig {
    pub api_key: String,
    pub base_url: String,
    pub model: String,
}

impl AiConfig {
    pub fn is_ready(&self) -> bool {
        !self.api_key.trim().is_empty()
            && !self.base_url.trim().is_empty()
            && !self.model.trim().is_empty()
    }
}

#[derive(Clone, Debug)]
pub struct ChatMessage {
    pub role: String,
    pub content: String,
}

/// Incremental updates from an SSE chat completion. The UI appends these to
/// the live assistant message so the user sees reasoning/content as it arrives.
#[derive(Clone, Debug)]
pub enum AiStreamEvent {
    Reasoning(String),
    Content(String),
    /// `truncated` is true when the provider hit its output limit and the
    /// continuation retries were exhausted. The UI must not apply edits then.
    Done {
        truncated: bool,
    },
    Error(String),
}

#[derive(Clone, Debug)]
pub struct SkillBundle {
    pub root: PathBuf,
    pub files: Vec<PathBuf>,
    pub text: String,
}

const EMBEDDED_SKILL_FILES: &[(&str, &str)] = &[
    (
        "SKILL.md",
        include_str!("../.opencode/skills/stm32-embedded-dev/SKILL.md"),
    ),
    (
        "evolution.md",
        include_str!("../.opencode/skills/stm32-embedded-dev/evolution.md"),
    ),
    (
        "references/checklist.md",
        include_str!("../.opencode/skills/stm32-embedded-dev/references/checklist.md"),
    ),
    (
        "references/code-rules.md",
        include_str!("../.opencode/skills/stm32-embedded-dev/references/code-rules.md"),
    ),
    (
        "references/debug-playbook.md",
        include_str!("../.opencode/skills/stm32-embedded-dev/references/debug-playbook.md"),
    ),
    (
        "references/field-map.md",
        include_str!("../.opencode/skills/stm32-embedded-dev/references/field-map.md"),
    ),
    (
        "references/ledger-schema.md",
        include_str!("../.opencode/skills/stm32-embedded-dev/references/ledger-schema.md"),
    ),
    (
        "references/architecture.md",
        include_str!("../.opencode/skills/stm32-embedded-dev/references/architecture.md"),
    ),
    (
        "references/framework-template.md",
        include_str!("../.opencode/skills/stm32-embedded-dev/references/framework-template.md"),
    ),
    (
        "references/module-convention.md",
        include_str!("../.opencode/skills/stm32-embedded-dev/references/module-convention.md"),
    ),
    (
        "references/software-arch.md",
        include_str!("../.opencode/skills/stm32-embedded-dev/references/software-arch.md"),
    ),
    (
        "references/other-operations.md",
        include_str!("../.opencode/skills/stm32-embedded-dev/references/other-operations.md"),
    ),
    (
        "references/serial-cli.md",
        include_str!("../.opencode/skills/stm32-embedded-dev/references/serial-cli.md"),
    ),
    (
        "references/code-style.md",
        include_str!("../.opencode/skills/stm32-embedded-dev/references/code-style.md"),
    ),
    (
        "references/diagnostics.md",
        include_str!("../.opencode/skills/stm32-embedded-dev/references/diagnostics.md"),
    ),
    (
        "references/consulting-design.md",
        include_str!("../.opencode/skills/stm32-embedded-dev/references/consulting-design.md"),
    ),
    (
        "references/system-perception.md",
        include_str!("../.opencode/skills/stm32-embedded-dev/references/system-perception.md"),
    ),
];

/// Skill copy compiled into the executable. Used when the opened project (and
/// the usual global skill folders) do not contain stm32-embedded-dev, so AI
/// mode still follows the same ledger-first workflow after installation.
pub fn embedded_skill() -> SkillBundle {
    let mut files = Vec::with_capacity(EMBEDDED_SKILL_FILES.len());
    let mut text = String::new();
    for (name, body) in EMBEDDED_SKILL_FILES {
        files.push(PathBuf::from(name));
        text.push_str("\n\n### skill file: ");
        text.push_str(name);
        text.push('\n');
        text.push_str(body);
        if text.len() > MAX_SKILL_BYTES {
            text.truncate(MAX_SKILL_BYTES);
            text.push_str("\n\n[skill 内容已截断]");
            break;
        }
    }
    SkillBundle {
        root: PathBuf::from("内置 stm32-embedded-dev"),
        files,
        text,
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AiResourceKind {
    Skill,
    Reference,
}

impl AiResourceKind {
    pub fn directory(self) -> PathBuf {
        match self {
            Self::Skill => crate::config::ai_skills_dir(),
            Self::Reference => crate::config::ai_references_dir(),
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Self::Skill => "Skill",
            Self::Reference => "参考资料",
        }
    }
}

#[derive(Clone, Debug, Default)]
pub struct ResourceEntry {
    pub name: String,
    pub path: PathBuf,
    pub files: usize,
    pub bytes: u64,
    pub note: String,
}

#[derive(Clone, Debug, Default)]
pub struct ResourceBundle {
    pub text: String,
    pub files: usize,
    pub warnings: Vec<String>,
}

#[derive(Clone, Debug, Default)]
pub struct ImportOutcome {
    pub imported: usize,
    pub extracted_pdfs: usize,
    pub warnings: Vec<String>,
}

const MAX_EXTRA_SKILL_BYTES: usize = 48_000;
const MAX_REFERENCE_BYTES: usize = 96_000;
const MAX_RESOURCE_FILE_BYTES: usize = 32_000;
const MAX_RESOURCE_FILES: usize = 80;
const MAX_PDF_BYTES: u64 = 64 * 1024 * 1024;

/// List the immediate children of a user resource directory for the settings
/// dialog. Folders are counted recursively but only their summary is returned.
pub fn scan_resources(kind: AiResourceKind) -> Vec<ResourceEntry> {
    let directory = kind.directory();
    let Ok(entries) = std::fs::read_dir(&directory) else {
        return Vec::new();
    };
    let mut result = Vec::new();
    for entry in entries.flatten() {
        let path = entry.path();
        let Some(name) = path
            .file_name()
            .map(|name| name.to_string_lossy().into_owned())
        else {
            continue;
        };
        if name.starts_with('.') {
            continue;
        }
        let Ok(file_type) = entry.file_type() else {
            continue;
        };
        if file_type.is_dir() {
            let (files, bytes) = count_resource_files(&path, 0, 5_000);
            if files == 0 {
                continue;
            }
            let has_skill = path.join("SKILL.md").is_file();
            result.push(ResourceEntry {
                name,
                path,
                files,
                bytes,
                note: if has_skill {
                    "含 SKILL.md".to_owned()
                } else {
                    "文件夹".to_owned()
                },
            });
        } else if file_type.is_file() {
            if name.to_ascii_lowercase().ends_with(".pdf.txt") {
                let original = path.with_file_name(&name[..name.len() - 4]);
                if original.is_file() {
                    // The sidecar is an implementation detail; list the PDF.
                    continue;
                }
            }
            let bytes = entry.metadata().map(|meta| meta.len()).unwrap_or(0);
            result.push(ResourceEntry {
                name: name.clone(),
                path,
                files: 1,
                bytes,
                note: resource_note(&name),
            });
        }
    }
    result.sort_by_key(|entry| entry.name.to_ascii_lowercase());
    result
}

/// Load imported skills for the system prompt. The result is bounded so a
/// large imported document cannot push the project context out of the model.
pub fn load_extra_skills() -> ResourceBundle {
    load_resource_bundle(AiResourceKind::Skill, MAX_EXTRA_SKILL_BYTES)
}

pub fn load_references() -> ResourceBundle {
    load_resource_bundle(AiResourceKind::Reference, MAX_REFERENCE_BYTES)
}

const MAX_USER_EVOLUTION_BYTES: usize = 32_000;

/// User-level rules learned from corrections. These are deliberately kept
/// outside the project and the executable so the built-in skill can evolve
/// without rebuilding the application.
pub fn load_user_evolution() -> String {
    let path = crate::config::ai_evolution_file();
    let Ok(loaded) = read_text_file(&path) else {
        return String::new();
    };
    truncate_chars(&loaded.text, MAX_USER_EVOLUTION_BYTES)
}

/// Global evolution rules plus an optional project-local `evolution-user.md`.
/// The project file is user-editable and is created by double-clicking the AI
/// orb in AI mode.
pub fn load_user_evolution_for(root: &Path) -> String {
    let mut text = String::new();
    let global = load_user_evolution();
    if !global.trim().is_empty() {
        text.push_str("## 全局用户规则\n\n");
        text.push_str(global.trim());
        text.push_str("\n\n");
    }
    let local = root.join("evolution-user.md");
    if let Ok(loaded) = read_text_file(&local) {
        if !loaded.text.trim().is_empty() {
            text.push_str("## 项目用户规则（evolution-user.md）\n\n");
            text.push_str(loaded.text.trim());
            text.push('\n');
        }
    }
    truncate_chars(&text, MAX_USER_EVOLUTION_BYTES)
}

pub fn append_user_evolution(rules: &[String]) -> Result<(PathBuf, usize), String> {
    let path = crate::config::ai_evolution_file();
    let added = append_evolution_rules(&path, rules)?;
    Ok((path, added))
}

fn append_evolution_rules(path: &Path, rules: &[String]) -> Result<usize, String> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)
            .map_err(|err| format!("无法创建 {}：{err}", display_path(parent)))?;
    }
    let mut text = std::fs::read_to_string(path).unwrap_or_default();
    if text.trim().is_empty() {
        text.push_str("# stm32-embedded-dev 用户进化规则\n\n");
        text.push_str("> 由 Simple embedded 的 AI 技能进化卡片追加；仅保存长期可复用偏好。\n\n");
    }
    let mut added = 0usize;
    for rule in rules {
        let rule = rule
            .trim()
            .trim_start_matches(['-', '*', '•'])
            .trim()
            .replace('\n', " ");
        if rule.is_empty() || rule.chars().count() > 500 {
            continue;
        }
        let line = format!("- {rule}");
        if text.lines().any(|existing| existing.trim() == line) {
            continue;
        }
        if text.len() + line.len() + 1 > MAX_USER_EVOLUTION_BYTES {
            return Err("用户进化规则超过 32KB，请先清理旧规则".to_owned());
        }
        text.push_str(&line);
        text.push('\n');
        added += 1;
    }
    if added > 0 {
        crate::util::write_text_file(
            path,
            &text,
            crate::util::LineEnding::Lf,
            false,
            crate::util::TextEncoding::Utf8,
        )?;
    }
    Ok(added)
}

/// Export the currently loaded skill (project/global if present, otherwise the
/// embedded copy) plus the user evolution overlay into a new folder.
pub fn export_skill_bundle(
    project_root: &Path,
    destination_parent: &Path,
) -> Result<PathBuf, String> {
    let evolution = load_user_evolution_for(project_root);
    export_skill_bundle_with_evolution(project_root, destination_parent, &evolution)
}

fn export_skill_bundle_with_evolution(
    project_root: &Path,
    destination_parent: &Path,
    evolution: &str,
) -> Result<PathBuf, String> {
    std::fs::create_dir_all(destination_parent).map_err(|err| {
        format!(
            "无法创建导出目录 {}：{err}",
            display_path(destination_parent)
        )
    })?;
    let source = find_skill(project_root)
        .and_then(|path| path.parent().map(Path::to_path_buf))
        .filter(|path| path.is_dir());
    if let Some(source) = &source {
        let src = source.canonicalize().unwrap_or_else(|_| source.clone());
        let dst = destination_parent
            .canonicalize()
            .unwrap_or_else(|_| destination_parent.to_path_buf());
        if dst.starts_with(&src) {
            return Err("导出目录不能位于 Skill 目录内部，请选择其他位置".to_owned());
        }
    }
    let target = unique_export_dir(destination_parent);
    if let Some(source) = source {
        copy_skill_tree(&source, &target)?;
    } else {
        std::fs::create_dir_all(target.join("references"))
            .map_err(|err| format!("无法创建 {}：{err}", display_path(&target)))?;
        for (name, body) in EMBEDDED_SKILL_FILES {
            let path = target.join(name);
            if let Some(parent) = path.parent() {
                std::fs::create_dir_all(parent)
                    .map_err(|err| format!("无法创建 {}：{err}", display_path(parent)))?;
            }
            std::fs::write(&path, body)
                .map_err(|err| format!("写入 {} 失败：{err}", display_path(&path)))?;
        }
    }
    if !evolution.trim().is_empty() {
        let evolution_path = target.join("references").join("evolution-user.md");
        if let Some(parent) = evolution_path.parent() {
            std::fs::create_dir_all(parent)
                .map_err(|err| format!("无法创建 {}：{err}", display_path(parent)))?;
        }
        crate::util::write_text_file(
            &evolution_path,
            &format!("> 从 Simple embedded 导出；导入后作为用户偏好参考。\n\n{evolution}"),
            crate::util::LineEnding::Lf,
            false,
            crate::util::TextEncoding::Utf8,
        )?;
    }
    Ok(target)
}

fn unique_export_dir(parent: &Path) -> PathBuf {
    let base = "stm32-embedded-dev-export";
    let first = parent.join(base);
    if !first.exists() {
        return first;
    }
    for index in 2..10_000 {
        let candidate = parent.join(format!("{base}-{index}"));
        if !candidate.exists() {
            return candidate;
        }
    }
    parent.join(format!("{base}-{}", std::process::id()))
}

fn copy_skill_tree(source: &Path, destination: &Path) -> Result<(), String> {
    std::fs::create_dir_all(destination)
        .map_err(|err| format!("无法创建 {}：{err}", display_path(destination)))?;
    let entries = std::fs::read_dir(source)
        .map_err(|err| format!("读取 {} 失败：{err}", display_path(source)))?;
    for entry in entries.flatten() {
        let path = entry.path();
        let Ok(file_type) = entry.file_type() else {
            continue;
        };
        if file_type.is_symlink() {
            continue;
        }
        let target = destination.join(entry.file_name());
        if file_type.is_dir() {
            copy_skill_tree(&path, &target)?;
        } else if file_type.is_file() {
            std::fs::copy(&path, &target)
                .map_err(|err| format!("复制 {} 失败：{err}", display_path(&path)))?;
        }
    }
    Ok(())
}

fn load_resource_bundle(kind: AiResourceKind, max_bytes: usize) -> ResourceBundle {
    load_resource_dir(&kind.directory(), kind, max_bytes)
}

fn load_resource_dir(directory: &Path, kind: AiResourceKind, max_bytes: usize) -> ResourceBundle {
    let mut files = Vec::new();
    collect_resource_files(directory, &mut files, 0);
    files.sort();
    files.truncate(MAX_RESOURCE_FILES);
    let mut bundle = ResourceBundle::default();
    for file in files {
        if bundle.text.len() >= max_bytes {
            break;
        }
        let name = file
            .strip_prefix(directory)
            .unwrap_or(&file)
            .to_string_lossy()
            .replace('\\', "/");
        if file
            .extension()
            .is_some_and(|extension| extension.eq_ignore_ascii_case("pdf"))
        {
            if !pdf_sidecar(&file).is_file() {
                bundle.warnings.push(format!(
                    "{name}：PDF 尚未提取文本，请重新导入或转换为 Markdown"
                ));
            }
            continue;
        }
        if !is_resource_text(&file) {
            continue;
        }
        let Ok(loaded) = read_text_file(&file) else {
            bundle.warnings.push(format!("{name}：无法读取，已跳过"));
            continue;
        };
        let body = truncate_chars(&loaded.text, MAX_RESOURCE_FILE_BYTES);
        bundle.text.push_str("\n\n### ");
        bundle.text.push_str(if kind == AiResourceKind::Skill {
            "额外 skill"
        } else {
            "参考资料"
        });
        bundle.text.push_str(": ");
        bundle.text.push_str(&name);
        bundle.text.push('\n');
        bundle.text.push_str(&body);
        bundle.files += 1;
        if bundle.text.len() > max_bytes {
            truncate_to_boundary(&mut bundle.text, max_bytes);
            bundle.text.push_str("\n\n[用户导入资料过长，已截断]");
            break;
        }
    }
    bundle
}

fn collect_resource_files(directory: &Path, files: &mut Vec<PathBuf>, depth: usize) {
    if depth > 5 || files.len() >= MAX_RESOURCE_FILES {
        return;
    }
    let Ok(entries) = std::fs::read_dir(directory) else {
        return;
    };
    let mut entries = entries.flatten().collect::<Vec<_>>();
    entries.sort_by_key(|entry| entry.file_name());
    for entry in entries {
        let path = entry.path();
        let Ok(file_type) = entry.file_type() else {
            continue;
        };
        if file_type.is_symlink() {
            continue;
        }
        if file_type.is_dir() {
            collect_resource_files(&path, files, depth + 1);
        } else if file_type.is_file() {
            files.push(path);
            if files.len() >= MAX_RESOURCE_FILES {
                return;
            }
        }
    }
}

pub fn is_resource_text(path: &Path) -> bool {
    path.extension().is_some_and(|extension| {
        matches!(
            extension.to_string_lossy().to_ascii_lowercase().as_str(),
            "md" | "markdown"
                | "txt"
                | "rst"
                | "c"
                | "h"
                | "cc"
                | "cpp"
                | "cxx"
                | "hpp"
                | "hh"
                | "hxx"
                | "json"
                | "csv"
                | "yaml"
                | "yml"
                | "toml"
                | "ini"
                | "log"
                | "s"
                | "asm"
                | "ld"
                | "dts"
                | "dtsi"
                | "ioc"
        )
    })
}

fn resource_note(name: &str) -> String {
    let lower = name.to_ascii_lowercase();
    if lower.ends_with(".pdf") {
        "PDF".to_owned()
    } else if lower.ends_with(".md") || lower.ends_with(".markdown") {
        "Markdown".to_owned()
    } else if lower.ends_with(".txt") {
        "文本".to_owned()
    } else {
        "文件".to_owned()
    }
}

fn pdf_sidecar(path: &Path) -> PathBuf {
    let name = path
        .file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .unwrap_or_else(|| "document.pdf".to_owned());
    path.with_file_name(format!("{name}.txt"))
}

fn count_resource_files(directory: &Path, depth: usize, cap: usize) -> (usize, u64) {
    if depth > 5 {
        return (0, 0);
    }
    let mut count = 0usize;
    let mut bytes = 0u64;
    let Ok(entries) = std::fs::read_dir(directory) else {
        return (0, 0);
    };
    for entry in entries.flatten() {
        if count >= cap {
            break;
        }
        let path = entry.path();
        let Ok(file_type) = entry.file_type() else {
            continue;
        };
        if file_type.is_symlink() {
            continue;
        }
        if file_type.is_dir() {
            let (nested_files, nested_bytes) = count_resource_files(&path, depth + 1, cap - count);
            count += nested_files;
            bytes += nested_bytes;
        } else if file_type.is_file() {
            count += 1;
            bytes += entry.metadata().map(|meta| meta.len()).unwrap_or(0);
        }
    }
    (count, bytes)
}

/// Copy imported files/folders into the user resource directory and extract
/// PDF text into sidecar `.txt` files. Returning warnings instead of failing
/// lets the caller import several documents when one of them is unsupported.
pub fn import_resources(
    kind: AiResourceKind,
    sources: &[PathBuf],
) -> Result<ImportOutcome, String> {
    import_resources_into(&kind.directory(), sources)
}

fn import_resources_into(directory: &Path, sources: &[PathBuf]) -> Result<ImportOutcome, String> {
    std::fs::create_dir_all(directory)
        .map_err(|err| format!("无法创建 {}：{err}", display_path(directory)))?;
    let mut outcome = ImportOutcome::default();
    for source in sources {
        if !source.exists() {
            outcome
                .warnings
                .push(format!("{} 不存在，已跳过", display_path(source)));
            continue;
        }
        let destination = unique_resource_path(directory, source);
        if source.is_dir() {
            copy_resource_dir(source, &destination, &mut outcome)?;
        } else {
            if let Some(parent) = destination.parent() {
                std::fs::create_dir_all(parent)
                    .map_err(|err| format!("无法创建 {}：{err}", display_path(parent)))?;
            }
            std::fs::copy(source, &destination)
                .map_err(|err| format!("复制 {} 失败：{err}", display_path(source)))?;
        }
        outcome.imported += 1;
        extract_pdfs(&destination, &mut outcome);
    }
    Ok(outcome)
}

fn copy_resource_dir(
    source: &Path,
    destination: &Path,
    outcome: &mut ImportOutcome,
) -> Result<(), String> {
    std::fs::create_dir_all(destination)
        .map_err(|err| format!("无法创建 {}：{err}", display_path(destination)))?;
    let Ok(entries) = std::fs::read_dir(source) else {
        return Ok(());
    };
    for entry in entries.flatten() {
        let path = entry.path();
        let Ok(file_type) = entry.file_type() else {
            continue;
        };
        if file_type.is_symlink() {
            outcome
                .warnings
                .push(format!("跳过符号链接 {}", display_path(&path)));
            continue;
        }
        let target = destination.join(entry.file_name());
        if file_type.is_dir() {
            copy_resource_dir(&path, &target, outcome)?;
        } else if file_type.is_file() {
            std::fs::copy(&path, &target)
                .map_err(|err| format!("复制 {} 失败：{err}", display_path(&path)))?;
        }
    }
    Ok(())
}

fn extract_pdfs(path: &Path, outcome: &mut ImportOutcome) {
    if path.is_dir() {
        let Ok(entries) = std::fs::read_dir(path) else {
            return;
        };
        for entry in entries.flatten() {
            extract_pdfs(&entry.path(), outcome);
        }
        return;
    }
    if !path
        .extension()
        .is_some_and(|extension| extension.eq_ignore_ascii_case("pdf"))
    {
        return;
    }
    let sidecar = pdf_sidecar(path);
    if sidecar.is_file() {
        return;
    }
    let bytes = match std::fs::read(path) {
        Ok(bytes) if bytes.len() as u64 <= MAX_PDF_BYTES => bytes,
        Ok(_) => {
            outcome.warnings.push(format!(
                "{}：PDF 超过 64 MB，未提取文本",
                display_path(path)
            ));
            return;
        }
        Err(err) => {
            outcome
                .warnings
                .push(format!("{}：读取 PDF 失败：{err}", display_path(path)));
            return;
        }
    };
    let extracted = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        pdf_extract::extract_text_from_mem(&bytes)
    }));
    match extracted {
        Ok(Ok(text)) if !text.trim().is_empty() => {
            let text = truncate_chars(&text, 400_000);
            if let Err(err) = crate::util::write_text_file(
                &sidecar,
                &text,
                crate::util::LineEnding::Lf,
                false,
                crate::util::TextEncoding::Utf8,
            ) {
                outcome
                    .warnings
                    .push(format!("{}：写入提取文本失败：{err}", display_path(path)));
            } else {
                outcome.extracted_pdfs += 1;
            }
        }
        Ok(Ok(_)) => outcome.warnings.push(format!(
            "{}：PDF 没有可提取文本（可能是扫描件）",
            display_path(path)
        )),
        Ok(Err(err)) => outcome
            .warnings
            .push(format!("{}：PDF 文本提取失败：{err}", display_path(path))),
        Err(_) => outcome.warnings.push(format!(
            "{}：PDF 解析异常，已保留原文件",
            display_path(path)
        )),
    }
}

fn unique_resource_path(directory: &Path, source: &Path) -> PathBuf {
    let name = source
        .file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .unwrap_or_else(|| "resource".to_owned());
    let candidate = directory.join(&name);
    if !candidate.exists() {
        return candidate;
    }
    let (stem, extension) = if source.is_dir() {
        (name.clone(), String::new())
    } else {
        match name.rsplit_once('.') {
            Some((stem, extension)) => (stem.to_owned(), format!(".{extension}")),
            None => (name.clone(), String::new()),
        }
    };
    for index in 1..10_000 {
        let suffix = if index == 1 {
            " - 副本".to_owned()
        } else {
            format!(" - 副本 ({index})")
        };
        let candidate = directory.join(format!("{stem}{suffix}{extension}"));
        if !candidate.exists() {
            return candidate;
        }
    }
    directory.join(format!("{stem} - 副本{extension}"))
}

fn truncate_to_boundary(text: &mut String, max: usize) {
    if text.len() <= max {
        return;
    }
    let mut end = max;
    while end > 0 && !text.is_char_boundary(end) {
        end -= 1;
    }
    text.truncate(end);
}

pub struct Context<'a> {
    pub root: &'a Path,
    pub project_name: &'a str,
    pub meta: &'a ProjectMeta,
    pub build: &'a BuildConfig,
    pub debug: &'a ProjectDebugSettings,
    pub memory_layout: Option<&'a ResolvedMemory>,
    pub active_file: Option<&'a Path>,
    pub include_dirs: &'a [String],
    pub library_dirs: &'a [String],
    pub defines: &'a [String],
    pub skill: &'a SkillBundle,
    pub extra_skills: &'a ResourceBundle,
    pub references: &'a ResourceBundle,
    pub evolution: &'a str,
    /// The user's current request; used to retrieve focused chunks from the
    /// local context index instead of attaching every source file.
    pub query: &'a str,
    pub index: Option<&'a crate::context_index::ContextIndex>,
    /// Real file contents returned by the previous `se-list` / `se-read`
    /// request. This is deliberately separate from chat history so the 12k
    /// per-message truncation cannot drop the files the model asked for.
    pub retrieval: &'a str,
}

#[derive(Clone, Copy, Debug, Default)]
pub struct ContextBuildStats {
    pub indexed_files: usize,
    pub indexed_chunks: usize,
    pub selected_chunks: usize,
    pub selected_bytes: usize,
    pub prompt_bytes: usize,
}

/// Find `.opencode/skills/stm32-embedded-dev/SKILL.md` in the project, an
/// ancestor, the working directory, or the user's global skill directories.
pub fn find_skill(project_root: &Path) -> Option<PathBuf> {
    let mut starts = vec![project_root.to_path_buf()];
    if let Ok(current) = std::env::current_dir() {
        starts.push(current);
    }
    if let Ok(exe) = std::env::current_exe() {
        if let Some(parent) = exe.parent() {
            starts.push(parent.to_path_buf());
        }
    }
    let mut seen = HashSet::new();
    for start in starts {
        for ancestor in start.ancestors().take(8) {
            for base in [".opencode/skills", ".claude/skills", ".agents/skills"] {
                let path = ancestor.join(base).join(SKILL_ID).join("SKILL.md");
                if path.is_file() {
                    return Some(path);
                }
                seen.insert(path.to_string_lossy().to_ascii_lowercase());
            }
        }
    }
    let home = std::env::var_os("USERPROFILE")
        .or_else(|| std::env::var_os("HOME"))
        .map(PathBuf::from);
    if let Some(home) = home {
        for base in [
            ".config/opencode/skills",
            ".claude/skills",
            ".agents/skills",
        ] {
            let path = home.join(base).join(SKILL_ID).join("SKILL.md");
            if path.is_file() {
                return Some(path);
            }
        }
    }
    if let Some(appdata) = std::env::var_os("APPDATA") {
        let path = PathBuf::from(appdata)
            .join("opencode")
            .join("skills")
            .join(SKILL_ID)
            .join("SKILL.md");
        if path.is_file() {
            return Some(path);
        }
    }
    let _ = seen;
    None
}

/// Read the skill body and its Markdown references. The result is intentionally
/// bounded so a very large skill directory cannot blow up the API request.
pub fn load_skill(skill_md: &Path) -> Result<SkillBundle, String> {
    let root = skill_md
        .parent()
        .ok_or_else(|| "无法确定 skill 目录".to_owned())?
        .to_path_buf();
    let mut files = Vec::new();
    collect_markdown(&root, &root, &mut files, 0)?;
    files.sort();
    let mut text = String::new();
    for file in &files {
        let loaded = read_text_file(file)
            .map_err(|err| format!("无法读取 skill 文件 {}：{err}", display_path(file)))?;
        let relative = file
            .strip_prefix(&root)
            .unwrap_or(file)
            .to_string_lossy()
            .replace('\\', "/");
        text.push_str("\n\n### skill file: ");
        text.push_str(&relative);
        text.push('\n');
        text.push_str(&loaded.text);
        if text.len() > MAX_SKILL_BYTES {
            text.truncate(MAX_SKILL_BYTES);
            text.push_str("\n\n[skill 内容已截断]");
            break;
        }
    }
    Ok(SkillBundle { root, files, text })
}

fn collect_markdown(
    root: &Path,
    directory: &Path,
    files: &mut Vec<PathBuf>,
    depth: usize,
) -> Result<(), String> {
    if depth > 3 || files.len() >= 64 {
        return Ok(());
    }
    let Ok(entries) = std::fs::read_dir(directory) else {
        return Ok(());
    };
    let mut entries = entries.flatten().collect::<Vec<_>>();
    entries.sort_by_key(|entry| entry.file_name());
    for entry in entries {
        let path = entry.path();
        let Ok(file_type) = entry.file_type() else {
            continue;
        };
        if file_type.is_symlink() {
            continue;
        }
        if file_type.is_dir() {
            collect_markdown(root, &path, files, depth + 1)?;
        } else if file_type.is_file()
            && path
                .extension()
                .is_some_and(|extension| extension.eq_ignore_ascii_case("md"))
            && (path == root.join("SKILL.md") || !files.contains(&path))
        {
            files.push(path);
        }
    }
    Ok(())
}

fn append_resource_section(prompt: &mut String, title: &str, note: &str, text: &str, max: usize) {
    if text.trim().is_empty() {
        return;
    }
    // Keep enough room for the project context and the file-edit protocol.
    let available = MAX_SYSTEM_PROMPT_BYTES.saturating_sub(prompt.len() + 80_000);
    let budget = max.min(available);
    if budget < 1_000 {
        return;
    }
    prompt.push_str(title);
    prompt.push_str(note);
    if text.len() <= budget {
        prompt.push_str(text);
        return;
    }
    let mut end = budget;
    while end > 0 && !text.is_char_boundary(end) {
        end -= 1;
    }
    prompt.push_str(&text[..end]);
    prompt.push_str("\n\n[用户导入资料过长，已截断]");
}

/// Build the system prompt sent with every AI request.
pub fn build_system_prompt_with_stats(context: &Context<'_>) -> (String, ContextBuildStats) {
    let mut stats = ContextBuildStats::default();
    if let Some(index) = context.index {
        let index_stats = index.stats();
        stats.indexed_files = index_stats.files;
        stats.indexed_chunks = index_stats.chunks;
    }
    let mut prompt = String::new();
    prompt.push_str(
        "你是 Simple embedded 的 AI 助手，正在协助 STM32 嵌入式开发。\n\
         必须优先遵循已加载的 stm32-embedded-dev skill：先按第 1 节判定 T1–T8 任务类型，再按第 3 节对应固定模板的小节编号、标题和表格输出；\
         T1–T5 使用模板 A–E，T6 其他操作使用模板 F，T7 代码风格使用模板 G，T8 系统整体感知使用模板 H。模板中的代码实现/代码修改节用下方的 se-file / se-replace 承载，删除文件用 se-delete 承载；\
         用户要求阅读/理解新导入或别人的工程、分析架构、重新生成或替换 framework.md，或工程明显不是默认 APP/MID/BSP 结构时，先按 T8 盘点实际目录/构建/入口/RTOS/厂商代码和现有文档，判断“默认/兼容/外来/混合”，不要直接套默认架构；T8 默认不改代码/不移动目录，替换或重构前用 se-ask 确认；\
         凡是需要用户选择、确认、授权或回答的内容，必须输出四反引号的 se-ask 卡片（问题 + 2–8 个选项或自由回答），禁止只在正文末尾写问句等待用户回复；只读的 se-list / se-read 不需要用户授权，缺文件时直接读取，禁止问“请允许我读取”；\
         用户提到代码风格、优化、code_style.md、模块封装或代码审查时，先读取工程根目录 code_style.md 并优先遵守；T4 必须先给诊断假设和可观测验证（串口或 LED/GPIO/watch），不得跳过验证；\
         缺少某个目录或文件正文时，先用 se-list / se-read 请求 IDE 读取，不要声称无法遍历磁盘，也不要猜测未读取的代码；\
         不要自创分点或省略不适用的 N/A 行。\
         硬件资源变化先更新/新建 hardware.dts 台账；现有工程只做最小改动，不重写已有初始化，不修改已验证的引脚、时钟和句柄。\
         手册级事实（AF、IRQn、DMA 映射、地址、电气极限）不确定时用 TODO/待核对表标注，不要编造。\n\n",
    );
    prompt.push_str("## 已加载 Skill\n\n");
    prompt.push_str(&context.skill.text);
    append_resource_section(
        &mut prompt,
        "\n\n## 用户导入的额外 Skill\n\n",
        "以下 Skill 由用户额外导入，用于补充领域知识；与 stm32-embedded-dev 冲突时，以专用 STM32 skill 和用户当前指令为准。\n",
        &context.extra_skills.text,
        MAX_EXTRA_SKILL_BYTES,
    );
    append_resource_section(
        &mut prompt,
        "\n\n## 用户导入的参考资料\n\n",
        "以下资料包括芯片手册、HAL 使用说明、例程等，仅作为事实参考；与工程代码冲突时以工程现状和用户确认为准，不确定时标注待核对。\n",
        &context.references.text,
        MAX_REFERENCE_BYTES,
    );
    if !context.evolution.trim().is_empty() {
        prompt.push_str("\n\n## 用户技能进化规则（优先遵守）\n\n");
        prompt.push_str(
            "以下是用户通过历史纠正确认的长期规则，优先于 SKILL.md 默认写法；但仍以用户当前指令、工程现状和安全/硬件事实为准。\n\n",
        );
        prompt.push_str(context.evolution);
    }
    prompt.push_str("\n\n## 工程上下文\n\n");
    let _ = writeln!(prompt, "- 工程名：{}", context.project_name);
    let _ = writeln!(prompt, "- 根目录：{}", display_path(context.root));
    if let Some(device) = &context.meta.device {
        let _ = writeln!(prompt, "- 芯片：{device}");
    }
    if let Some(cpu) = &context.meta.cpu {
        let _ = writeln!(prompt, "- CPU：{cpu}");
    }
    if let Some(fpu) = &context.meta.fpu {
        let _ = writeln!(prompt, "- FPU：{fpu}");
    }
    let _ = writeln!(
        prompt,
        "- 构建系统：{:?} / 工具链：{}",
        context.build.effective_build_system(),
        context.build.effective_toolchain(context.meta).label()
    );
    if !context.build.build_command.trim().is_empty() {
        let _ = writeln!(
            prompt,
            "- 构建命令：`{}`；Clean：`{}`；Rebuild：`{}`",
            context.build.build_command, context.build.clean_command, context.build.rebuild_command
        );
    }
    if let Some(memory) = context.memory_layout {
        let layout = memory.layout;
        let _ = writeln!(
            prompt,
            "- 内存：Flash 0x{:08X} / {}，RAM 0x{:08X} / {}（{}）",
            layout.flash.start,
            crate::memory::format_size(layout.flash.size),
            layout.ram.start,
            crate::memory::format_size(layout.ram.size),
            memory.source
        );
    }
    if !context.include_dirs.is_empty() {
        let _ = writeln!(prompt, "- 包含目录：{}", context.include_dirs.join("; "));
    }
    if !context.defines.is_empty() {
        let _ = writeln!(prompt, "- 预处理器定义：{}", context.defines.join("; "));
    }
    if !context.library_dirs.is_empty() {
        let _ = writeln!(prompt, "- 库目录：{}", context.library_dirs.join("; "));
    }
    if let Some(active) = context.active_file {
        let _ = writeln!(prompt, "- 用户当前打开的文件：{}", display_path(active));
    }
    if !context.debug.program.trim().is_empty() {
        let _ = writeln!(prompt, "- 调试程序：{}", context.debug.program);
    }

    prompt.push_str("\n## 工程文件树（已跳过生成目录）\n\n```text\n");
    for path in scan_files(context.root) {
        prompt.push_str(&path);
        prompt.push('\n');
    }
    prompt.push_str("```\n\n## 关键文件\n");
    prompt.push_str(
        "> framework.md 及其列出的工程文件会尽量自动附带；当前打开的文件优先于 framework.md。\
         不要要求用户手动附加这些文件，也不要只根据用户附加的一个文件回答。\
         先读 framework.md 的分层、调用链、调度和文件表，再定位实际代码。\
         如果某个文件没有出现在关键文件或自动引用文件的正文中，不要猜测其内容；\
         用 se-read 请求 IDE 读取它，或明确说明还需要哪个文件。\n",
    );

    let mut appended = Vec::new();
    // The file the user is looking at is the most important context. Attach it
    // before framework.md so a large architecture document cannot crowd it out.
    if let Some(active) = context.active_file {
        append_file(
            &mut prompt,
            context.root,
            active,
            60_000,
            fence_language(active),
            &mut appended,
        );
    }
    append_file(
        &mut prompt,
        context.root,
        &context.root.join("hardware.dts"),
        60_000,
        "dts",
        &mut appended,
    );
    // Keep the entry file before framework.md for the same reason: it is the
    // file most likely to contain the function the user is asking about.
    for candidate in ["Core/Src/main.c", "src/main.c", "main.c"] {
        append_file(
            &mut prompt,
            context.root,
            &context.root.join(candidate),
            50_000,
            "c",
            &mut appended,
        );
        if appended.iter().any(|path| path.ends_with(candidate)) {
            break;
        }
    }
    // Put files requested by the previous se-list / se-read after the active
    // file and entry file, but before framework.md. The model asked for them,
    // yet they must not crowd out the file the user is looking at.
    if !context.retrieval.trim().is_empty() {
        prompt.push_str("\n\n## IDE 文件检索结果（上一轮 se-list / se-read 的真实返回）\n\n");
        prompt.push_str(
            "> 以下内容由 IDE 从工程磁盘读取，不是模型猜测。请直接基于这些正文继续分析；需要更多文件时继续输出 se-list / se-read，不要编造未读取的文件内容。\n\n",
        );
        let limit = MAX_SYSTEM_PROMPT_BYTES.saturating_sub(PROMPT_PROTOCOL_RESERVE);
        let available = limit.saturating_sub(prompt.len());
        let budget = MAX_RETRIEVAL_BYTES.min(available);
        if budget > 512 {
            let mut end = context.retrieval.len().min(budget);
            while end > 0 && !context.retrieval.is_char_boundary(end) {
                end -= 1;
            }
            prompt.push_str(&context.retrieval[..end]);
            if end < context.retrieval.len() {
                prompt.push_str("\n\n[检索结果过长，已截断；请分轮读取剩余部分]\n");
            }
        } else {
            prompt.push_str("[上下文预算不足，未插入检索结果；请减少 Skill 或参考资料后重试]\n");
        }
    }
    let mut framework_text = None;
    if let Some(framework) = crate::framework::find(context.root) {
        framework_text = read_text_file(&framework).ok().map(|loaded| loaded.text);
        // Keep room for framework-referenced source files and the issue log.
        let framework_budget = MAX_FRAMEWORK_BYTES.min(
            MAX_SYSTEM_PROMPT_BYTES
                .saturating_sub(PROMPT_PROTOCOL_RESERVE)
                .saturating_sub(prompt.len())
                .saturating_sub(32_000),
        );
        append_file_clipped(
            &mut prompt,
            context.root,
            &framework,
            framework_budget,
            "markdown",
            &mut appended,
        );
    }
    let referenced = framework_text
        .as_deref()
        .map(|text| framework_context_files(context.root, text));
    if let Some(referenced) = referenced {
        prompt.push_str("\n### framework.md 自动引用文件\n");
        if referenced.is_empty() {
            prompt.push_str("（未在 framework.md 中发现可解析的工程文件路径）\n");
        } else {
            prompt.push_str(
                "以下文件由 IDE 按 framework.md 中的路径自动读取，不需要用户手动附加；\
                 如与文件表冲突，以文件表路径为准。受上下文预算限制，部分文件可能未附全文，\
                 请以实际出现的 ### 文件名为准：\n",
            );
            for path in &referenced {
                let relative = path
                    .strip_prefix(context.root)
                    .unwrap_or(path)
                    .to_string_lossy()
                    .replace('\\', "/");
                let _ = writeln!(prompt, "- {relative}");
            }
        }
        for path in &referenced {
            append_file_clipped(
                &mut prompt,
                context.root,
                path,
                MAX_FRAMEWORK_FILE_BYTES,
                fence_language(path),
                &mut appended,
            );
        }
    }
    let issue_log = crate::ai_issue::issue_path(context.root, context.project_name);
    if issue_log.is_file() {
        append_file(
            &mut prompt,
            context.root,
            &issue_log,
            40_000,
            "markdown",
            &mut appended,
        );
    }
    // User code style has a higher priority than the default code rules for
    // this project. Always attach it when present.
    let code_style = context.root.join("code_style.md");
    if code_style.is_file() {
        prompt.push_str("\n### 用户代码风格（最高优先级）\n");
        prompt.push_str(
            "> 编码、模块封装和优化必须优先遵守 code_style.md；与默认 code-rules 冲突时以用户风格为准，但不得违反安全、硬件事实和当前指令。\n",
        );
        append_file_clipped(
            &mut prompt,
            context.root,
            &code_style,
            60_000,
            "markdown",
            &mut appended,
        );
    }
    if style_requested(context.query) {
        let samples = style_sample_files(context.root);
        if !samples.is_empty() {
            prompt.push_str("\n### 代码风格样例（用于学习，不要直接复制进业务目录）\n");
            for path in samples {
                if prompt.len() > MAX_SYSTEM_PROMPT_BYTES - PROMPT_PROTOCOL_RESERVE {
                    break;
                }
                append_file(
                    &mut prompt,
                    context.root,
                    &path,
                    40_000,
                    fence_language(&path),
                    &mut appended,
                );
            }
        }
    }
    let index_hits = context
        .index
        .map(|index| index.search(context.query, 24))
        .unwrap_or_default();
    let mut selected_chunks = 0usize;
    let mut selected_bytes = 0usize;
    let mut section_chunks = 0usize;
    if !index_hits.is_empty() {
        prompt.push_str("\n## 上下文索引命中（按当前需求检索，不是全量文件）\n");
        prompt.push_str(
            "> 下列完整文件/片段由本地上下文索引按本次需求检索，只保留相关代码；\
             修改时 SEARCH 必须来自当前磁盘文件，必要时先打开完整文件核对。\n",
        );
        // Keep the top two matched files complete so edits have exact context;
        // the rest are focused chunks from the local index.
        let mut full_files = HashSet::new();
        for hit in &index_hits {
            if full_files.len() >= 2 {
                break;
            }
            if !full_files.insert(hit.path.clone()) {
                continue;
            }
            let before = appended.len();
            append_file(
                &mut prompt,
                context.root,
                &context.root.join(&hit.path),
                40_000,
                fence_language(Path::new(&hit.path)),
                &mut appended,
            );
            if appended.len() > before {
                selected_chunks += 1;
                selected_bytes += std::fs::metadata(context.root.join(&hit.path))
                    .map(|metadata| metadata.len() as usize)
                    .unwrap_or(0);
            }
        }
        let mut section = String::new();
        for hit in &index_hits {
            if appended
                .iter()
                .any(|path| path.eq_ignore_ascii_case(&hit.path))
            {
                continue;
            }
            if section_chunks >= 20 || selected_bytes >= 64_000 {
                break;
            }
            if prompt.len() + section.len() + hit.text.len() + 512
                > MAX_SYSTEM_PROMPT_BYTES - PROMPT_PROTOCOL_RESERVE
            {
                break;
            }
            section.push_str(&format!(
                "\n### {}（第 {}-{} 行）\n\n```{}\n",
                hit.path,
                hit.start_line,
                hit.end_line,
                fence_language(Path::new(&hit.path))
            ));
            section.push_str(&hit.text);
            if !hit.text.ends_with('\n') {
                section.push('\n');
            }
            section.push_str("```\n");
            section_chunks += 1;
            selected_chunks += 1;
            selected_bytes += hit.text.len();
        }
        if section_chunks > 0 {
            prompt.push_str(&section);
        }
    }
    // Fall back to broad source files only when the index produced no hits;
    // with an index we deliberately avoid attaching every layer file.
    if index_hits.is_empty() {
        prompt.push_str("\n### 工程主要源文件\n");
        let mut source_bytes = 0usize;
        for path in source_candidates(context.root) {
            if prompt.len() > MAX_SYSTEM_PROMPT_BYTES - PROMPT_PROTOCOL_RESERVE
                || source_bytes > 140_000
                || appended.len() > 48
            {
                break;
            }
            let before = prompt.len();
            append_file(
                &mut prompt,
                context.root,
                &path,
                36_000,
                fence_language(&path),
                &mut appended,
            );
            source_bytes += prompt.len().saturating_sub(before);
        }
    }
    if let Some(ioc) = first_file_with_extension(context.root, "ioc") {
        append_file(
            &mut prompt,
            context.root,
            &ioc,
            40_000,
            "ini",
            &mut appended,
        );
    }
    for candidate in ["Makefile", "CMakeLists.txt", "Cargo.toml"] {
        append_file(
            &mut prompt,
            context.root,
            &context.root.join(candidate),
            20_000,
            "text",
            &mut appended,
        );
    }
    if let Some(summary) = report_summary(context.root, context.project_name) {
        prompt.push_str("\n### 资源报告摘要\n\n");
        prompt.push_str(&summary);
        prompt.push('\n');
    }

    prompt.push_str(
        "\n\n## 工程文件检索协议（重要）\n\n\
         如果当前上下文缺少某个目录或文件的正文，不要声称“无法主动遍历磁盘”，也不要根据 framework.md 的文件表猜测代码内容。先输出四反引号的 se-list / se-read，IDE 会从工程根目录真实读取，并把结果放进下一轮上下文自动继续：\n\n\
         ````se-list path=Core/Src depth=2\n\
         ````\n\n\
         ````se-read path=Core/Src/main.c lines=1-200\n\
         ````\n\n\
         se-list：列出目录，path=. 表示工程根目录，depth 为 1–3，默认 1；只列真实文件和目录。\n\
         se-read：读取一个文本文件，lines 可省略；一次最多 24KB，单文件上限 2MB；需要后续内容时用 lines 继续读。\n\
         se-list / se-read 是只读操作，不需要用户授权；缺文件时直接读取，禁止在正文问“请允许我读取”。只有写文件、删除、重构、烧录、串口连接等有副作用的动作才用 se-ask 确认。\n\
         一次最多 12 个检索请求，IDE 最多自动继续 6 轮；路径必须相对工程根目录，禁止绝对路径、`..`、build/Debug/Release/out/target、`.simple-embedded`、`.git`、`.opencode` 等生成或配置目录，禁止符号链接和二进制文件。\n\
         检索回复只输出 se-list / se-read 和简短说明，不要在同一回复里输出 se-file / se-replace / se-delete；拿到正文后再生成修改。优先按 framework.md 的文件表定位，再 list 目录、read 关键文件，不要重复读取已经出现过的全文。\n\n\
         ## 文件修改协议（重要）\n\n\
         你运行在 Simple embedded 的 AI 模式中，IDE 会执行你给出的文件修改。不要声称无法写入磁盘，也不要把完整代码只放在普通 Markdown 代码块里让用户手动复制。\n\n\
         新建或整体覆盖文件，使用四个反引号（内容里可以安全包含普通的三反引号 Markdown 代码块，例如 framework.md 的 Mermaid 图）：\n\n\
         ````se-file path=相对路径\n\
         <完整文件内容>\n\
         ````\n\n\
         局部修改（优先使用），使用四个反引号：\n\n\
         ````se-replace path=相对路径\n\
         <<<<<<< SEARCH\n\
         <当前文件中完全一致的原文，包含缩进和换行>\n\
         =======\n\
         <替换后的文本>\n\
         >>>>>>> REPLACE\n\
         ````\n\n\
         删除工程内文件，使用四个反引号；IDE 会显示独立的“AI 文件删除”卡片，只有用户再次确认后才会删除，并在删除前备份到用户目录：\n\n\
         ````se-delete path=相对路径\n\
         <为什么删除；可省略>\n\
         ````\n\n\
         删除规则：只删除用户明确要求删除的普通文件；一次最多 10 个删除；禁止删除目录、工程根目录、`.simple-embedded`、`.git`、构建/生成目录和 `.se-workspace`；\
         禁止用空文件、占位注释、“请手动删除”或 se-file 覆盖来假装删除。文件已迁移时，直接对旧文件发 se-delete，不要在新文件里写“请从工程中删除旧文件”。\
         se-delete 是真实执行请求，不是示例；展示删除命令或演示路径时使用普通 Markdown 代码块。\n\n\
         需要用户确认关键信息才能继续时，使用四个反引号的 se-ask 块；IDE 会显示“AI 需要确认”卡片，用户可以选择选项或填写自定义回答，回答会作为下一条用户消息发给你。每个块只放一个问题，最多 6 个；能通过工程事实推断的不要问：\n\n\
         ````se-ask\n\
         使用哪个串口？\n\
         - USART1\n\
         - USART2\n\
         ````\n\n\
         需要多选时在开头写 multiple：\n\n\
         ````se-ask multiple\n\
         需要保留哪些功能？\n\
         - 串口回显\n\
         - DMA\n\
         - 空闲中断\n\
         ````\n\n\
         也可以只写问题、不写选项，让用户自由回答：\n\n\
         ````se-ask\n\
         目标芯片的具体型号是什么？\n\
         ````\n\n\
         se-ask 只用于真正阻塞的关键信息，不要把问题重复写在正文里，也不要把 se-ask 放进普通代码块；已经有工程事实或用户已说明的内容不要再问。\n\n\
         完成代码修改后，最后一步使用四个反引号的 se-build 请求 IDE 执行真实构建验证：\n\n\
         ````se-build\n\
         ````\n\n\
         IDE 会运行当前工程配置的 Build 命令，并把真实构建输出自动作为下一条消息发回。如果同一回复还包含 se-file / se-replace / se-delete，IDE 会等用户应用修改后再构建。\
         构建失败时必须根据真实错误继续修复，并再次输出 se-build，直到没有 error；构建通过后才能结束。禁止伪造“编译通过”，禁止只写“请用户自行构建”，也不要把 se-build 放进普通代码块。\n\n\
         构建通过后，如果固件已实现串口 CLI 且开发板已连接，用 se-cli 让 IDE 通过串口执行命令并回传真实输出：\n\n\
         ````se-cli\n\
         help\n\
         info\n\
         adc read\n\
         ````\n\n\
         se-cli 每行一条命令（最多 32 条），IDE 按工程 CLI 配置的端口、波特率和行尾逐条发送，每条等待超时后把真实输出自动发回。串口未连接时 IDE 会提示用户打开 CLI 窗口选择端口。禁止编造串口输出；设备无响应时用 se-ask 询问端口/接线，或用 se-issue 记录。本轮修改过固件时，构建通过后先用 se-ask 请用户烧录并确认开发板已运行新固件，再发 se-cli。固件还没有 CLI 时，按 skill 的 references/serial-cli.md 生成 BSP/PERIPHERAL/bsp_cli.c/.h，并在主循环调用 cli_poll()。\n\n\
         遇到编译错误、链接错误、HardFault、串口/外设调试问题时，用 se-issue 记录到工程根目录的 .ai-issues.md；问题未解决时 status=open，解决后用同一标题和 status=resolved 补充原因与解决方案：\n\n\
         ````se-issue status=open\n\
         标题：编译错误：bsp_usart_echo.c\n\
         现象：unknown type name 'usart_echo_t'\n\
         原因：头文件和源文件使用的类型名不一致\n\
         相关文件：BSP/PERIPHERAL/bsp_usart_echo.c\n\
         ````\n\n\
         解决后：\n\n\
         ````se-issue status=resolved\n\
         标题：编译错误：bsp_usart_echo.c\n\
         解决方案：统一改为 usart_echo_handle_t 并同步头文件\n\
         ````\n\n\
         同标题会更新已有记录；只记录真实遇到并解决/待解决的问题，不要编造。\n\n\
         当用户提出长期可复用的偏好或纠正（目录归属、命名、分层、调度、流程、代码风格等）时，在回答末尾追加一个四反引号的 se-evolve 块；IDE 会显示“AI 技能进化”卡片，用户确认后才写入用户级 evolution.md：\n\n\
         ````se-evolve\n\
         - 目录：应用任务放 APP/tasks/，不要在 Core/Src 新增业务文件。\n\
         - 命名：外设驱动统一 bsp_<外设>.c/.h。\n\
         ````\n\n\
         se-evolve 只记录长期规则，不记录一次性需求、密钥、客户隐私或大段源码，一次最多 5 条；不确定是否长期有效时先询问用户。\n\n\
         规则：路径必须相对工程根目录，禁止绝对路径、`..`、`build` / `Debug` / `Release` / `out` / `target` 等生成目录；一次最多修改 20 个文件、删除 10 个文件；SEARCH 必须能在当前文件中唯一匹配；不要修改 `.simple-embedded` 工程配置。IDE 会生成修改差异和独立删除卡片：用户点击“应用全部”只写入修改，删除必须再点“删除文件”确认；写入/删除前都会备份到用户目录。skill 中要求输出代码块的步骤在 AI 模式下改用上述格式，解释文字照常输出。\n",
    );
    prompt.push_str(
        "\n\n## 输出要求（与 skill 固定模板的关系）\n\n\
         1. 正文结构以 skill 第 1 节任务判定和第 3 节对应模板为准：T1–T5 使用模板 A–E，T6 使用模板 F，T7 使用模板 G，T8 使用模板 H；保留模板的小节编号、标题、表格维度和 N/A 行，不得自创分点。\n\
         2. skill 模板中的代码实现/代码修改节仍保留编号，但实际写入工程的内容用 se-file / se-replace 块承载，删除用 se-delete，确认问题用 se-ask，真实构建验证用 se-build，串口调试用 se-cli，文件检索用 se-list / se-read，问题/解决方案记录用 se-issue；执行器会显示差异卡片、独立删除卡片、确认卡片、构建卡片、串口调试卡片和文件检索卡片，并把问题写入 .ai-issues.md，不要在普通代码块里重复同一份完整代码。\n\
         3. 涉及硬件资源变化时，先按模板输出 hardware.dts 台账；手册级事实不确定时放在待核对表/TODO 中。\n\
         4. S4 核对表、S5 构建/排查和 S7 串口 Debug 是 T1–T5 的必输出项；S5 必须用 se-build 实际构建并引用 IDE 回传输出，串口可用时 S7 必须用 se-cli 实际收发，不能只给命令或伪造结果；T6 按模板 F 给出影响分析、执行预览、结果、验证与回退；T8 按模板 H 先给工程盘点、归属判断、framework.md 处理和用户确认项，不得在未确认时改写/替换 framework.md。\n\
         5. 若模型无法完整遵循模板，至少给出任务判定、台账/无变更说明、代码修改或删除预览、核对表和验证步骤，并明确缺失项。\n",
    );

    if prompt.len() > MAX_SYSTEM_PROMPT_BYTES {
        let mut end = MAX_SYSTEM_PROMPT_BYTES;
        while end > 0 && !prompt.is_char_boundary(end) {
            end -= 1;
        }
        prompt.truncate(end);
        prompt.push_str("\n\n[工程上下文已截断，请向用户询问缺失文件]\n");
    }
    stats.selected_chunks = selected_chunks;
    stats.selected_bytes = selected_bytes;
    stats.prompt_bytes = prompt.len();
    (prompt, stats)
}

fn append_file(
    prompt: &mut String,
    root: &Path,
    path: &Path,
    max_bytes: usize,
    language: &str,
    appended: &mut Vec<String>,
) {
    if !path.is_file() {
        return;
    }
    let relative = path
        .strip_prefix(root)
        .unwrap_or(path)
        .to_string_lossy()
        .replace('\\', "/");
    if appended.iter().any(|item| item == &relative) {
        return;
    }
    let Ok(loaded) = read_text_file(path) else {
        return;
    };
    let limit = MAX_SYSTEM_PROMPT_BYTES.saturating_sub(PROMPT_PROTOCOL_RESERVE);
    if prompt.len() + loaded.text.len() + 256 > limit {
        return;
    }
    if loaded.text.len() > max_bytes {
        return;
    }
    appended.push(relative.clone());
    prompt.push_str("\n### ");
    prompt.push_str(&relative);
    prompt.push_str("\n\n```");
    prompt.push_str(language);
    prompt.push('\n');
    prompt.push_str(&loaded.text);
    if !loaded.text.ends_with('\n') {
        prompt.push('\n');
    }
    prompt.push_str("```\n");
}

/// Append a file even when it is larger than the per-file budget. The body is
/// clipped at a character boundary and marked, keeping the edit protocol at the
/// end of the system prompt intact.
fn append_file_clipped(
    prompt: &mut String,
    root: &Path,
    path: &Path,
    max_bytes: usize,
    language: &str,
    appended: &mut Vec<String>,
) {
    if !path.is_file() {
        return;
    }
    let relative = path
        .strip_prefix(root)
        .unwrap_or(path)
        .to_string_lossy()
        .replace('\\', "/");
    if appended.iter().any(|item| item == &relative) {
        return;
    }
    let Ok(loaded) = read_text_file(path) else {
        return;
    };
    let limit = MAX_SYSTEM_PROMPT_BYTES
        .saturating_sub(PROMPT_PROTOCOL_RESERVE)
        .saturating_sub(512);
    let available = limit.saturating_sub(prompt.len());
    if available < 2_000 {
        return;
    }
    let budget = max_bytes.min(available);
    let mut body = loaded.text;
    let mut clipped = false;
    if body.len() > budget {
        let mut end = budget;
        while end > 0 && !body.is_char_boundary(end) {
            end -= 1;
        }
        body.truncate(end);
        clipped = true;
    }
    appended.push(relative.clone());
    prompt.push_str("\n### ");
    prompt.push_str(&relative);
    prompt.push_str("\n\n```");
    prompt.push_str(language);
    prompt.push('\n');
    prompt.push_str(&body);
    if !body.ends_with('\n') {
        prompt.push('\n');
    }
    if clipped {
        prompt.push_str("\n…（文件较长，已按上下文预算截断；需要完整内容时请让用户打开该文件）\n");
    }
    prompt.push_str("```\n");
}

fn framework_path_regex() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| {
        Regex::new(
            r"(?i)((?:[A-Za-z0-9_.\-]+[\\/])*[A-Za-z0-9_.\-]+\.(?:cc|cpp|cxx|hpp|hh|hxx|asm|dtsi|dts|c|h|s|ld|ioc|json))(?:[^A-Za-z0-9_]|$)",
        )
        .expect("framework path regex")
    })
}

fn framework_referenced_paths(text: &str) -> Vec<String> {
    let mut seen = HashSet::new();
    let mut paths = Vec::new();
    for found in framework_path_regex().captures_iter(text) {
        let Some(found) = found.get(1) else {
            continue;
        };
        let candidate = found.as_str().trim_start_matches("./").to_owned();
        if candidate.is_empty() || candidate.contains("..") {
            continue;
        }
        let key = candidate.replace('\\', "/").to_ascii_lowercase();
        if seen.insert(key) {
            paths.push(candidate);
        }
        if paths.len() >= 128 {
            break;
        }
    }
    paths
}

/// Resolve a path mentioned in `framework.md` against the project. Direct
/// matches win; a case-insensitive component walk handles Windows/Linux path
/// case differences; a bounded suffix search handles bare file names.
fn resolve_framework_path(root: &Path, candidate: &str) -> Option<PathBuf> {
    let candidate = candidate.replace('\\', "/");
    let candidate = candidate.trim_start_matches("./");
    if candidate.is_empty()
        || candidate.starts_with('/')
        || candidate.contains("..")
        || candidate.contains(':')
    {
        return None;
    }
    let mut current = root.to_path_buf();
    for part in candidate.split('/').filter(|part| !part.is_empty()) {
        let entries = std::fs::read_dir(&current).ok()?;
        let mut found = None;
        for entry in entries.flatten() {
            if entry
                .file_name()
                .to_string_lossy()
                .eq_ignore_ascii_case(part)
            {
                found = Some(entry.path());
                break;
            }
        }
        current = found?;
    }
    current.is_file().then_some(current)
}

fn find_file_by_suffix(root: &Path, candidate: &str) -> Option<PathBuf> {
    let lower = candidate
        .replace('\\', "/")
        .trim_start_matches("./")
        .to_ascii_lowercase();
    if lower.is_empty() {
        return None;
    }
    let mut stack = vec![(root.to_path_buf(), 0usize)];
    let mut visited = 0usize;
    while let Some((directory, depth)) = stack.pop() {
        if depth > 8 || visited > 20_000 {
            break;
        }
        let Ok(entries) = std::fs::read_dir(&directory) else {
            continue;
        };
        for entry in entries.flatten() {
            visited += 1;
            let path = entry.path();
            let Ok(file_type) = entry.file_type() else {
                continue;
            };
            let relative = path
                .strip_prefix(root)
                .unwrap_or(&path)
                .to_string_lossy()
                .replace('\\', "/");
            if file_type.is_dir() {
                if !skip_source_dir(Path::new(&relative))
                    && !relative
                        .split('/')
                        .any(|part| part.starts_with('.') && part.len() > 1)
                {
                    stack.push((path, depth + 1));
                }
            } else if file_type.is_file() {
                let relative_lower = relative.to_ascii_lowercase();
                if relative_lower == lower || relative_lower.ends_with(&format!("/{lower}")) {
                    return Some(path);
                }
            }
        }
    }
    None
}

fn framework_context_files(root: &Path, text: &str) -> Vec<PathBuf> {
    let mut seen = HashSet::new();
    let mut files = Vec::new();
    for candidate in framework_referenced_paths(text) {
        let Some(path) = resolve_framework_path(root, &candidate)
            .or_else(|| find_file_by_suffix(root, &candidate))
        else {
            continue;
        };
        let relative = path
            .strip_prefix(root)
            .unwrap_or(&path)
            .to_string_lossy()
            .replace('\\', "/")
            .to_ascii_lowercase();
        if relative.ends_with("framework.md")
            || relative.ends_with(".ai-chat.md")
            || relative.ends_with(".ai-issues.md")
            || skip_source_dir(Path::new(&relative).parent().unwrap_or(Path::new("")))
        {
            continue;
        }
        if seen.insert(relative) {
            files.push(path);
        }
        if files.len() >= MAX_FRAMEWORK_FILES {
            break;
        }
    }
    files
}

fn fence_language(path: &Path) -> &'static str {
    match path
        .extension()
        .map(|extension| extension.to_string_lossy().to_ascii_lowercase())
        .as_deref()
    {
        Some("c") | Some("h") => "c",
        Some("cc") | Some("cpp") | Some("cxx") | Some("hpp") => "cpp",
        Some("py") => "python",
        Some("s") | Some("asm") => "asm",
        Some("json") => "json",
        Some("dts") | Some("dtsi") => "dts",
        _ => "text",
    }
}

fn first_file_with_extension(root: &Path, extension: &str) -> Option<PathBuf> {
    let entries = std::fs::read_dir(root).ok()?;
    entries.flatten().map(|entry| entry.path()).find(|path| {
        path.is_file()
            && path
                .extension()
                .is_some_and(|value| value.eq_ignore_ascii_case(extension))
    })
}

fn style_requested(query: &str) -> bool {
    let lower = query.to_ascii_lowercase();
    query.contains("代码风格")
        || query.contains("风格")
        || query.contains("优化")
        || lower.contains("code_style")
        || lower.contains("code style")
        || lower.contains("style")
}

/// Sample files for T7. Convention: put uploads in `code_style_samples/` at the
/// project root (or drop `.c`/`.h` samples in the root itself). Only attached
/// when the user is asking about style, so normal requests stay small.
fn style_sample_files(root: &Path) -> Vec<PathBuf> {
    let mut files = Vec::new();
    for directory in ["code_style_samples", "style-samples", "style_samples"] {
        collect_style_files(root, &root.join(directory), 0, &mut files);
    }
    if let Ok(entries) = std::fs::read_dir(root) {
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_file() && is_style_source(&path) {
                files.push(path);
            }
        }
    }
    files.sort();
    files.dedup();
    files.truncate(32);
    files
}

fn collect_style_files(root: &Path, directory: &Path, depth: usize, files: &mut Vec<PathBuf>) {
    if depth > 3 || files.len() >= 64 || !directory.is_dir() {
        return;
    }
    let Ok(entries) = std::fs::read_dir(directory) else {
        return;
    };
    let mut entries = entries.flatten().collect::<Vec<_>>();
    entries.sort_by_key(|entry| entry.file_name());
    for entry in entries {
        if files.len() >= 64 {
            break;
        }
        let path = entry.path();
        let Ok(file_type) = entry.file_type() else {
            continue;
        };
        if file_type.is_dir() {
            let relative = path.strip_prefix(root).unwrap_or(&path);
            if !skip_source_dir(relative) {
                collect_style_files(root, &path, depth + 1, files);
            }
        } else if file_type.is_file() && is_style_source(&path) {
            files.push(path);
        }
    }
}

fn is_style_source(path: &Path) -> bool {
    path.extension().is_some_and(|extension| {
        matches!(
            extension.to_string_lossy().to_ascii_lowercase().as_str(),
            "c" | "h" | "cc" | "cpp" | "cxx" | "hpp"
        )
    })
}

fn source_candidates(root: &Path) -> Vec<PathBuf> {
    let mut files = Vec::new();
    for directory in [
        "APP",
        "MID",
        "BSP",
        "BSP/MODULES",
        "BSP/PERIPHERAL",
        "LL",
        "HAL",
        "Core/Src",
        "Core/Inc",
        "src",
        "include",
        "Inc",
        "Src",
        "Application",
        "User",
    ] {
        collect_sources(root, &root.join(directory), &mut files, 0);
    }
    if let Ok(entries) = std::fs::read_dir(root) {
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_file() && is_source_file(&path) {
                files.push(path);
            }
        }
    }
    files.sort();
    files.dedup();
    files.truncate(48);
    files
}

fn collect_sources(root: &Path, directory: &Path, files: &mut Vec<PathBuf>, depth: usize) {
    if depth > 2 || files.len() >= 64 {
        return;
    }
    let Ok(entries) = std::fs::read_dir(directory) else {
        return;
    };
    let mut entries = entries.flatten().collect::<Vec<_>>();
    entries.sort_by_key(|entry| entry.file_name());
    for entry in entries {
        if files.len() >= 64 {
            break;
        }
        let path = entry.path();
        let Ok(file_type) = entry.file_type() else {
            continue;
        };
        if file_type.is_dir() {
            let relative = path.strip_prefix(root).unwrap_or(&path);
            if !skip_source_dir(relative) {
                collect_sources(root, &path, files, depth + 1);
            }
        } else if file_type.is_file() && is_source_file(&path) {
            files.push(path);
        }
    }
}

fn is_source_file(path: &Path) -> bool {
    path.extension().is_some_and(|extension| {
        matches!(
            extension.to_string_lossy().to_ascii_lowercase().as_str(),
            "c" | "h" | "cc" | "cpp" | "cxx" | "hpp"
        )
    })
}

fn skip_source_dir(relative: &Path) -> bool {
    relative.components().any(|component| {
        matches!(
            component
                .as_os_str()
                .to_string_lossy()
                .to_ascii_lowercase()
                .as_str(),
            "build"
                | "debug"
                | "release"
                | "out"
                | "target"
                | "objects"
                | "listings"
                | "drivers"
                | "middlewares"
                | "cmsis"
                | "third_party"
                | "vendor"
                | "node_modules"
                | "tools"
                | "dist"
                | ".git"
                | ".simple-embedded"
        )
    })
}

fn scan_files(root: &Path) -> Vec<String> {
    let ignore = IgnoreRules::load(root, true, "");
    let mut files = Vec::new();
    let mut stack = vec![root.to_path_buf()];
    while let Some(directory) = stack.pop() {
        if files.len() >= MAX_TREE_FILES {
            break;
        }
        let Ok(entries) = std::fs::read_dir(&directory) else {
            continue;
        };
        let mut entries = entries.flatten().collect::<Vec<_>>();
        entries.sort_by_key(|entry| entry.file_name());
        for entry in entries {
            if files.len() >= MAX_TREE_FILES {
                break;
            }
            let path = entry.path();
            let Ok(file_type) = entry.file_type() else {
                continue;
            };
            if file_type.is_symlink() {
                continue;
            }
            let relative = path.strip_prefix(root).unwrap_or(path.as_path());
            if file_type.is_dir() {
                if !ignore.is_ignored(relative, true) && !skip_dir(relative) {
                    stack.push(path);
                }
            } else if file_type.is_file() && !ignore.is_ignored(relative, false) {
                files.push(relative.to_string_lossy().replace('\\', "/"));
            }
        }
    }
    files.sort();
    files
}

fn skip_dir(relative: &Path) -> bool {
    relative.components().any(|component| {
        matches!(
            component
                .as_os_str()
                .to_string_lossy()
                .to_ascii_lowercase()
                .as_str(),
            ".git"
                | ".svn"
                | ".simple-embedded"
                | "build"
                | "debug"
                | "release"
                | "out"
                | "target"
                | "objects"
                | "listings"
                | "node_modules"
                | "tools"
                | "dist"
        )
    })
}

fn report_summary(root: &Path, project_name: &str) -> Option<String> {
    let path = root.join(format!("{project_name}.resources.json"));
    if !path.is_file() {
        return None;
    }
    let text = std::fs::read_to_string(path).ok()?;
    let value: Value = serde_json::from_str(&text).ok()?;
    let mut summary = String::new();
    if let Some(hardware) = value.get("hardware") {
        let _ = writeln!(
            summary,
            "- hardware: device={:?}, cpu={:?}, fpu={:?}, peripherals={}, registers={}",
            hardware.get("device").and_then(Value::as_str),
            hardware.get("cpu").and_then(Value::as_str),
            hardware.get("fpu").and_then(Value::as_str),
            hardware
                .get("peripheral_count")
                .and_then(Value::as_u64)
                .unwrap_or(0),
            hardware
                .get("register_count")
                .and_then(Value::as_u64)
                .unwrap_or(0)
        );
        if let Some(components) = hardware.get("used_components").and_then(Value::as_array) {
            let names = components
                .iter()
                .filter_map(Value::as_str)
                .collect::<Vec<_>>();
            if !names.is_empty() {
                let _ = writeln!(summary, "- used components: {}", names.join(", "));
            }
        }
        if let Some(pins) = hardware.pointer("/ioc/pins").and_then(Value::as_array) {
            let names = pins
                .iter()
                .take(40)
                .map(|pin| {
                    format!(
                        "{}={}{}",
                        pin.get("pin").and_then(Value::as_str).unwrap_or("?"),
                        pin.get("signal").and_then(Value::as_str).unwrap_or("?"),
                        pin.get("label")
                            .and_then(Value::as_str)
                            .map(|label| format!(" ({label})"))
                            .unwrap_or_default()
                    )
                })
                .collect::<Vec<_>>();
            if !names.is_empty() {
                let _ = writeln!(summary, "- pins: {}", names.join(", "));
            }
        }
    }
    if let Some(software) = value.get("software") {
        if let Some(counts) = software.get("counts") {
            let _ = writeln!(
                summary,
                "- source files={}, headers={}, source_lines={}, header_lines={}",
                counts
                    .get("source_files")
                    .and_then(Value::as_u64)
                    .unwrap_or(0),
                counts
                    .get("header_files")
                    .and_then(Value::as_u64)
                    .unwrap_or(0),
                counts
                    .get("source_lines")
                    .and_then(Value::as_u64)
                    .unwrap_or(0),
                counts
                    .get("header_lines")
                    .and_then(Value::as_u64)
                    .unwrap_or(0)
            );
        }
        if let Some(components) = software.get("components").and_then(Value::as_array) {
            let names = components
                .iter()
                .filter_map(|component| component.get("name").and_then(Value::as_str))
                .collect::<Vec<_>>();
            if !names.is_empty() {
                let _ = writeln!(summary, "- software components: {}", names.join(", "));
            }
        }
    }
    if let Some(warnings) = value.get("warnings").and_then(Value::as_array) {
        let names = warnings
            .iter()
            .filter_map(Value::as_str)
            .collect::<Vec<_>>();
        if !names.is_empty() {
            let _ = writeln!(summary, "- warnings: {}", names.join("; "));
        }
    }
    Some(summary)
}

/// Send a non-streaming OpenAI-compatible chat completion request.
pub fn chat(
    config: &AiConfig,
    system_prompt: &str,
    messages: &[ChatMessage],
) -> Result<String, String> {
    if !config.is_ready() {
        return Err("请先在 AI 设置中填写 API Key、API 地址和模型名称".to_owned());
    }
    let base = config.base_url.trim().trim_end_matches('/');
    let url = if base.ends_with("/chat/completions") {
        base.to_owned()
    } else {
        format!("{base}/chat/completions")
    };
    let mut api_messages = Vec::with_capacity(messages.len() + 1);
    api_messages.push(json!({ "role": "system", "content": system_prompt }));
    for message in messages {
        if message.content.trim().is_empty() {
            continue;
        }
        api_messages.push(json!({
            "role": message.role,
            "content": message.content,
        }));
    }
    let body = json!({
        "model": config.model.trim(),
        "messages": api_messages,
        "temperature": 0.2,
        "stream": false,
    });
    let agent = ureq::AgentBuilder::new()
        .timeout(Duration::from_secs(180))
        .build();
    let response = agent
        .post(&url)
        .set("Accept", "application/json")
        .set("Content-Type", "application/json")
        .set(
            "Authorization",
            &format!("Bearer {}", config.api_key.trim()),
        )
        .send_json(body);
    match response {
        Ok(response) => {
            let value: Value = response
                .into_json()
                .map_err(|err| format!("API 返回内容无法解析：{err}"))?;
            let content = value
                .pointer("/choices/0/message/content")
                .and_then(Value::as_str)
                .or_else(|| value.pointer("/choices/0/text").and_then(Value::as_str))
                .unwrap_or("");
            if content.trim().is_empty() {
                let reasoning = value
                    .pointer("/choices/0/message/reasoning_content")
                    .and_then(Value::as_str)
                    .unwrap_or("");
                return Err(if reasoning.trim().is_empty() {
                    "模型返回了空内容".to_owned()
                } else {
                    format!("模型只返回了推理内容：\n{reasoning}")
                });
            }
            Ok(content.trim().to_owned())
        }
        Err(ureq::Error::Status(code, response)) => {
            let body = response.into_string().unwrap_or_default();
            let message = api_error_message(&body);
            Err(format!("API 请求失败（HTTP {code}）：{message}"))
        }
        Err(err) => Err(format!("AI 网络请求失败：{err}")),
    }
}

/// Stream an OpenAI-compatible chat completion over SSE. Reasoning deltas
/// (`reasoning_content` / `reasoning`) and normal content deltas are forwarded
/// to the UI as they arrive. If a provider explicitly rejects `stream:true`,
/// fall back to the existing non-streaming request.
pub fn chat_stream(
    config: &AiConfig,
    system_prompt: &str,
    messages: &[ChatMessage],
    sender: mpsc::Sender<AiStreamEvent>,
) {
    if !config.is_ready() {
        let _ = sender.send(AiStreamEvent::Error(
            "请先在 AI 设置中填写 API Key、API 地址和模型名称".to_owned(),
        ));
        return;
    }
    let base = config.base_url.trim().trim_end_matches('/');
    let url = if base.ends_with("/chat/completions") {
        base.to_owned()
    } else {
        format!("{base}/chat/completions")
    };
    let mut api_messages = Vec::with_capacity(messages.len() + 1);
    api_messages.push(json!({ "role": "system", "content": system_prompt }));
    for message in messages {
        if message.content.trim().is_empty() {
            continue;
        }
        api_messages.push(json!({
            "role": message.role,
            "content": message.content,
        }));
    }
    let agent = ureq::AgentBuilder::new()
        .timeout(Duration::from_secs(600))
        .build();
    let mut continuation = 0usize;
    loop {
        let body = json!({
            "model": config.model.trim(),
            "messages": &api_messages,
            "temperature": 0.2,
            "stream": true,
        });
        let response = agent
            .post(&url)
            .set("Accept", "text/event-stream")
            .set("Content-Type", "application/json")
            .set(
                "Authorization",
                &format!("Bearer {}", config.api_key.trim()),
            )
            .send_json(body);
        match response {
            Ok(response) => {
                let content_type = response
                    .header("Content-Type")
                    .unwrap_or("")
                    .to_ascii_lowercase();
                if content_type.contains("text/event-stream") {
                    let mut finish_reason = None;
                    let mut response_content = String::new();
                    let mut saw_done = false;
                    let reader = BufReader::new(response.into_reader());
                    for line in reader.lines() {
                        let line = match line {
                            Ok(line) => line,
                            Err(err) => {
                                let _ = sender
                                    .send(AiStreamEvent::Error(format!("读取 AI 流失败：{err}")));
                                return;
                            }
                        };
                        let Some(data) = line.trim_start().strip_prefix("data:") else {
                            continue;
                        };
                        let data = data.trim();
                        if data == "[DONE]" {
                            saw_done = true;
                            break;
                        }
                        if data.is_empty() {
                            continue;
                        }
                        let value: Value = match serde_json::from_str(data) {
                            Ok(value) => value,
                            Err(_) => continue,
                        };
                        if let Some(error) = value.get("error") {
                            let message = error
                                .pointer("/message")
                                .and_then(Value::as_str)
                                .unwrap_or("AI 流返回错误");
                            let _ = sender.send(AiStreamEvent::Error(message.to_owned()));
                            return;
                        }
                        if let Some(reason) = stream_finish_reason(&value) {
                            finish_reason = Some(reason);
                        }
                        let (reasoning, content) = stream_parts(&value);
                        if let Some(reasoning) = reasoning {
                            if sender.send(AiStreamEvent::Reasoning(reasoning)).is_err() {
                                return;
                            }
                        }
                        if let Some(content) = content {
                            response_content.push_str(&content);
                            if sender.send(AiStreamEvent::Content(content)).is_err() {
                                return;
                            }
                        }
                    }
                    if matches!(
                        finish_reason.as_deref(),
                        Some("length") | Some("max_tokens")
                    ) {
                        if continuation < MAX_STREAM_CONTINUATIONS {
                            continuation += 1;
                            if response_content.is_empty() {
                                let _ = sender.send(AiStreamEvent::Done { truncated: true });
                                return;
                            }
                            api_messages.push(json!({
                                "role": "assistant",
                                "content": response_content,
                            }));
                            api_messages.push(json!({
                                "role": "user",
                                "content": STREAM_CONTINUE_PROMPT,
                            }));
                            continue;
                        }
                        let _ = sender.send(AiStreamEvent::Done { truncated: true });
                        return;
                    }
                    let truncated = match finish_reason.as_deref() {
                        Some("stop") => false,
                        Some(_) => true,
                        None => !saw_done,
                    };
                    let _ = sender.send(AiStreamEvent::Done { truncated });
                    return;
                } else {
                    match response.into_json::<Value>() {
                        Ok(value) => {
                            let (reasoning, content) = stream_parts(&value);
                            if let Some(reasoning) = reasoning {
                                let _ = sender.send(AiStreamEvent::Reasoning(reasoning));
                            }
                            if let Some(content) = content {
                                let _ = sender.send(AiStreamEvent::Content(content));
                            }
                            let _ = sender.send(AiStreamEvent::Done { truncated: false });
                        }
                        Err(err) => {
                            let _ = sender
                                .send(AiStreamEvent::Error(format!("API 返回内容无法解析：{err}")));
                        }
                    }
                    return;
                }
            }
            Err(ureq::Error::Status(code, response)) => {
                let body = response.into_string().unwrap_or_default();
                let message = api_error_message(&body);
                if (code == 400 || code == 422) && message.to_ascii_lowercase().contains("stream") {
                    match chat(config, system_prompt, messages) {
                        Ok(content) => {
                            let _ = sender.send(AiStreamEvent::Content(content));
                            let _ = sender.send(AiStreamEvent::Done { truncated: false });
                        }
                        Err(err) => {
                            let _ = sender.send(AiStreamEvent::Error(err));
                        }
                    }
                } else {
                    let _ = sender.send(AiStreamEvent::Error(format!(
                        "API 请求失败（HTTP {code}）：{message}"
                    )));
                }
                return;
            }
            Err(err) => {
                let _ = sender.send(AiStreamEvent::Error(format!("AI 网络请求失败：{err}")));
                return;
            }
        }
    }
}

const MAX_STREAM_CONTINUATIONS: usize = 8;
const STREAM_CONTINUE_PROMPT: &str =
    "上一条回复因输出长度限制被截断。请严格从中断处继续输出剩余内容，不要重复已经输出的字符或代码块，不要重新开始文件，不要添加解释；如果正在写 se-file 代码块，请先补全代码和结束围栏。";

fn stream_finish_reason(value: &Value) -> Option<String> {
    value
        .pointer("/choices/0/finish_reason")
        .and_then(Value::as_str)
        .filter(|reason| !reason.is_empty())
        .map(ToOwned::to_owned)
}

fn stream_parts(value: &Value) -> (Option<String>, Option<String>) {
    let choice = value.pointer("/choices/0");
    let delta = choice.and_then(|choice| choice.get("delta").or_else(|| choice.get("message")));
    let reasoning = delta
        .and_then(|delta| {
            delta
                .get("reasoning_content")
                .or_else(|| delta.get("reasoning"))
        })
        .and_then(Value::as_str)
        .filter(|text| !text.is_empty())
        .map(ToOwned::to_owned);
    let content = delta
        .and_then(|delta| delta.get("content"))
        .and_then(Value::as_str)
        .or_else(|| {
            choice
                .and_then(|choice| choice.get("text"))
                .and_then(Value::as_str)
        })
        .filter(|text| !text.is_empty())
        .map(ToOwned::to_owned);
    (reasoning, content)
}

fn api_error_message(body: &str) -> String {
    serde_json::from_str::<Value>(body)
        .ok()
        .and_then(|value| {
            value
                .pointer("/error/message")
                .and_then(Value::as_str)
                .map(ToOwned::to_owned)
        })
        .unwrap_or_else(|| truncate_chars(body.trim(), 800))
}

pub(crate) fn truncate_chars(text: &str, max: usize) -> String {
    if text.chars().count() <= max {
        return text.to_owned();
    }
    let mut result = text.chars().take(max).collect::<String>();
    result.push_str("...");
    result
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
    fn finds_skill_in_project_ancestor() {
        let dir = tempfile::tempdir().unwrap();
        write(
            &dir.path()
                .join(".opencode/skills/stm32-embedded-dev/SKILL.md"),
            "# skill\n",
        );
        let project = dir.path().join("project");
        std::fs::create_dir_all(&project).unwrap();
        let found = find_skill(&project).expect("skill");
        assert!(found.ends_with("stm32-embedded-dev/SKILL.md"));
    }

    #[test]
    fn loads_skill_and_references() {
        let dir = tempfile::tempdir().unwrap();
        let skill = dir.path().join("stm32-embedded-dev");
        write(&skill.join("SKILL.md"), "# Skill body\n");
        write(&skill.join("references/ledger.md"), "ledger notes\n");
        let bundle = load_skill(&skill.join("SKILL.md")).unwrap();
        assert!(bundle.text.contains("Skill body"));
        assert!(bundle.text.contains("ledger notes"));
        assert_eq!(bundle.files.len(), 2);
    }

    #[test]
    fn embedded_skill_contains_workflow_and_references() {
        let bundle = embedded_skill();
        assert!(bundle.text.contains("hardware.dts"));
        assert!(bundle.text.contains("references/checklist.md"));
        assert!(bundle.text.contains("references/framework-template.md"));
        assert!(bundle.text.contains("references/software-arch.md"));
        assert!(bundle.text.contains("references/other-operations.md"));
        assert!(bundle.text.contains("references/serial-cli.md"));
        assert!(bundle.text.contains("references/code-style.md"));
        assert!(bundle.text.contains("references/diagnostics.md"));
        assert!(bundle.text.contains("references/consulting-design.md"));
        assert!(bundle.text.contains("references/system-perception.md"));
        assert!(bundle.text.contains("T7 代码风格"));
        assert!(bundle.text.contains("T8 系统整体感知"));
        assert!(!bundle.text.contains("skill 文本已截断"));
        assert!(bundle.text.contains("code_style.md"));
        assert!(bundle.text.contains("se-cli"));
        assert!(bundle.text.contains("evolution.md"));
        assert!(bundle.files.len() >= 17);
    }

    #[test]
    fn collects_code_style_samples_and_detects_style_requests() {
        let dir = tempfile::tempdir().unwrap();
        write(
            &dir.path().join("code_style_samples/app/bsp_led.c"),
            "void bsp_led_init(void) {}\n",
        );
        write(
            &dir.path().join("code_style_samples/app/bsp_led.h"),
            "#pragma once\n",
        );
        write(&dir.path().join("uploaded.c"), "void uploaded(void) {}\n");
        write(
            &dir.path().join("code_style_samples/build/generated.c"),
            "void generated(void) {}\n",
        );
        write(
            &dir.path().join("code_style_samples/readme.md"),
            "not source\n",
        );

        let samples = style_sample_files(dir.path());
        let names = samples
            .iter()
            .map(|path| path.file_name().unwrap().to_string_lossy().into_owned())
            .collect::<Vec<_>>();
        assert!(names.contains(&"bsp_led.c".to_owned()));
        assert!(names.contains(&"bsp_led.h".to_owned()));
        assert!(names.contains(&"uploaded.c".to_owned()));
        assert!(!names.contains(&"generated.c".to_owned()));
        assert!(!names.contains(&"readme.md".to_owned()));

        assert!(style_requested("学习我的代码风格"));
        assert!(style_requested("按 code_style.md 优化"));
        assert!(style_requested("code style review"));
        assert!(!style_requested("配置 USART1 引脚"));
    }

    #[test]
    fn streams_sse_reasoning_and_content() {
        use std::io::{Read, Write};
        use std::net::TcpListener;

        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let address = listener.local_addr().unwrap();
        let server = std::thread::spawn(move || {
            let (mut socket, _) = listener.accept().unwrap();
            let mut request = Vec::new();
            let mut buffer = [0u8; 1024];
            loop {
                let read = socket.read(&mut buffer).unwrap_or(0);
                if read == 0 {
                    break;
                }
                request.extend_from_slice(&buffer[..read]);
                if request.windows(4).any(|window| window == b"\r\n\r\n") {
                    break;
                }
                if request.len() > 16 * 1024 {
                    break;
                }
            }
            // Let the client finish writing its tiny JSON body before the
            // mock server replies and closes the socket.
            std::thread::sleep(Duration::from_millis(5));
            let response = concat!(
                "HTTP/1.1 200 OK\r\n",
                "Content-Type: text/event-stream\r\n",
                "Connection: close\r\n",
                "\r\n",
                "data: {\"choices\":[{\"delta\":{\"reasoning_content\":\"think \"}}]}\n\n",
                "data: {\"choices\":[{\"delta\":{\"content\":\"answer\"}}]}\n\n",
                "data: [DONE]\n\n"
            );
            socket.write_all(response.as_bytes()).unwrap();
            socket.flush().unwrap();
        });
        let config = AiConfig {
            api_key: "test".to_owned(),
            base_url: format!("http://{address}/v1"),
            model: "test".to_owned(),
        };
        let (sender, receiver) = mpsc::channel();
        chat_stream(&config, "system", &[], sender);
        server.join().unwrap();
        let events = receiver.try_iter().collect::<Vec<_>>();
        assert!(
            events.iter().any(|event| {
                matches!(event, AiStreamEvent::Reasoning(text) if text.as_str() == "think ")
            }),
            "events: {events:?}"
        );
        assert!(
            events.iter().any(|event| {
                matches!(event, AiStreamEvent::Content(text) if text.as_str() == "answer")
            }),
            "events: {events:?}"
        );
        assert!(matches!(
            events.last(),
            Some(AiStreamEvent::Done { truncated: false })
        ));
    }

    #[test]
    fn continues_when_provider_reports_length() {
        use std::io::{Read, Write};
        use std::net::TcpListener;

        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let address = listener.local_addr().unwrap();
        let server = std::thread::spawn(move || {
            for attempt in 0..2 {
                let (mut socket, _) = listener.accept().unwrap();
                let mut request = Vec::new();
                let mut buffer = [0u8; 1024];
                loop {
                    let read = socket.read(&mut buffer).unwrap_or(0);
                    if read == 0 {
                        break;
                    }
                    request.extend_from_slice(&buffer[..read]);
                    if request.windows(4).any(|window| window == b"\r\n\r\n") {
                        break;
                    }
                }
                std::thread::sleep(Duration::from_millis(5));
                let body = if attempt == 0 {
                    concat!(
                        "data: {\"choices\":[{\"delta\":{\"content\":\"partial\"},\"finish_reason\":\"length\"}]}\n\n",
                        "data: [DONE]\n\n"
                    )
                } else {
                    concat!(
                        "data: {\"choices\":[{\"delta\":{\"content\":\" rest\"},\"finish_reason\":\"stop\"}]}\n\n",
                        "data: [DONE]\n\n"
                    )
                };
                let response = format!(
                    "HTTP/1.1 200 OK\r\nContent-Type: text/event-stream\r\nConnection: close\r\n\r\n{body}"
                );
                socket.write_all(response.as_bytes()).unwrap();
                socket.flush().unwrap();
            }
        });
        let config = AiConfig {
            api_key: "test".to_owned(),
            base_url: format!("http://{address}/v1"),
            model: "test".to_owned(),
        };
        let (sender, receiver) = mpsc::channel();
        chat_stream(&config, "system", &[], sender);
        server.join().unwrap();
        let events = receiver.try_iter().collect::<Vec<_>>();
        let content = events
            .iter()
            .filter_map(|event| match event {
                AiStreamEvent::Content(text) => Some(text.as_str()),
                _ => None,
            })
            .collect::<String>();
        assert_eq!(content, "partial rest");
        assert!(matches!(
            events.last(),
            Some(AiStreamEvent::Done { truncated: false })
        ));
    }

    #[test]
    fn parses_stream_reasoning_and_content_delta() {
        let value = json!({
            "choices": [{
                "delta": {
                    "reasoning_content": "think",
                    "content": "answer"
                }
            }]
        });
        let (reasoning, content) = stream_parts(&value);
        assert_eq!(reasoning.as_deref(), Some("think"));
        assert_eq!(content.as_deref(), Some("answer"));
    }

    #[test]
    fn parses_non_stream_message_reasoning() {
        let value = json!({
            "choices": [{
                "message": {
                    "reasoning_content": "think",
                    "content": "answer"
                }
            }]
        });
        let (reasoning, content) = stream_parts(&value);
        assert_eq!(reasoning.as_deref(), Some("think"));
        assert_eq!(content.as_deref(), Some("answer"));
    }

    #[test]
    fn framework_references_resolve_automatically() {
        let dir = tempfile::tempdir().unwrap();
        write(
            &dir.path().join("APP/app_init.c"),
            "void app_init_marker(void) {}\n",
        );
        write(
            &dir.path().join("BSP/PERIPHERAL/bsp_led.c"),
            "void led_marker(void) {}\n",
        );
        write(
            &dir.path().join("src/main.c"),
            "int main(void) { return 0; }\n",
        );
        let text = "调用 `APP/app_init.c`、`bsp_led.c` 和 `SRC/MAIN.C`；版本 v1.2.3，不是文件。\n";
        let files = framework_context_files(dir.path(), text);
        assert_eq!(files.len(), 3);
        let relative = |path: &std::path::PathBuf| path.to_string_lossy().replace('\\', "/");
        assert!(files
            .iter()
            .any(|path| relative(path).ends_with("APP/app_init.c")));
        assert!(files
            .iter()
            .any(|path| relative(path).ends_with("BSP/PERIPHERAL/bsp_led.c")));
        assert!(files
            .iter()
            .any(|path| relative(path).ends_with("src/main.c")));
        assert!(!framework_referenced_paths(text)
            .iter()
            .any(|path| path.contains("1.2.3")));
    }

    #[test]
    fn loads_project_evolution_user_file() {
        let dir = tempfile::tempdir().unwrap();
        write(
            &dir.path().join("evolution-user.md"),
            "# 项目规则\n\n- 场景：示例；做法：写项目级规则；原因：共享给团队。\n",
        );
        let text = load_user_evolution_for(dir.path());
        assert!(text.contains("项目用户规则"));
        assert!(text.contains("写项目级规则"));
    }

    #[test]
    fn active_file_survives_large_framework_and_skill() {
        let dir = tempfile::tempdir().unwrap();
        write(
            &dir.path().join("src/main.c"),
            "void active_main_marker(void) {}\n",
        );
        write(
            &dir.path().join("framework.md"),
            &"# Framework\n\n".repeat(12_000),
        );
        let bundle = embedded_skill();
        let extra_skills = ResourceBundle::default();
        let references = ResourceBundle::default();
        let context = Context {
            root: dir.path(),
            project_name: "demo",
            meta: &ProjectMeta::default(),
            build: &BuildConfig::default(),
            debug: &ProjectDebugSettings::default(),
            memory_layout: None,
            active_file: Some(&dir.path().join("src/main.c")),
            include_dirs: &[],
            library_dirs: &[],
            defines: &[],
            skill: &bundle,
            extra_skills: &extra_skills,
            references: &references,
            evolution: "",
            query: "修改 main.c",
            index: None,
            retrieval: "",
        };
        let prompt = build_system_prompt_with_stats(&context).0;
        assert!(prompt.contains("active_main_marker"));
        assert!(prompt.contains("src/main.c"));
    }

    #[test]
    fn prompt_uses_context_index_chunks() {
        let dir = tempfile::tempdir().unwrap();
        let skill = dir.path().join("skill");
        write(&skill.join("SKILL.md"), "skill body");
        let bundle = load_skill(&skill.join("SKILL.md")).unwrap();
        write(
            &dir.path().join("BSP/PERIPHERAL/bsp_uart.c"),
            "void uart_echo_marker(void) {}\n",
        );
        write(
            &dir.path().join("BSP/PERIPHERAL/bsp_led.c"),
            "void led_marker(void) {}\n",
        );
        let index = crate::context_index::ContextIndex::build(dir.path());
        let meta = ProjectMeta::default();
        let extra_skills = ResourceBundle::default();
        let references = ResourceBundle::default();
        let context = Context {
            root: dir.path(),
            project_name: "demo",
            meta: &meta,
            build: &BuildConfig::default(),
            debug: &ProjectDebugSettings::default(),
            memory_layout: None,
            active_file: None,
            include_dirs: &[],
            library_dirs: &[],
            defines: &[],
            skill: &bundle,
            extra_skills: &extra_skills,
            references: &references,
            evolution: "",
            query: "uart echo",
            index: Some(&index),
            retrieval: "",
        };
        let (prompt, stats) = build_system_prompt_with_stats(&context);
        assert!(stats.selected_chunks > 0);
        assert!(prompt.contains("上下文索引命中"));
        assert!(prompt.contains("uart_echo_marker"));
    }

    #[test]
    fn builds_prompt_with_project_context() {
        let dir = tempfile::tempdir().unwrap();
        let skill = dir.path().join("skill");
        write(&skill.join("SKILL.md"), "skill body");
        let bundle = load_skill(&skill.join("SKILL.md")).unwrap();
        write(
            &dir.path().join("hardware.dts"),
            "/ { model = \"board\"; };\n",
        );
        write(
            &dir.path().join("Core/Src/main.c"),
            "int main(void) { return 0; }\n",
        );
        write(
            &dir.path().join("Core/Src/gpio.c"),
            "void gpio_marker(void) {}\n",
        );
        write(
            &dir.path().join("framework.md"),
            "# Demo framework\n\n| APP | `app/app_init.c` |\n",
        );
        write(
            &dir.path().join("APP/app_init.c"),
            "void app_init_marker(void) {}\n",
        );
        write(
            &dir.path().join("code_style.md"),
            "# Style\n\n- code_style_marker: 4 spaces, bsp_ prefix.\n",
        );
        let meta = ProjectMeta {
            device: Some("STM32G431CBTx".to_owned()),
            ..ProjectMeta::default()
        };
        let extra_skills = ResourceBundle {
            text: "extra skill marker".to_owned(),
            ..ResourceBundle::default()
        };
        let references = ResourceBundle {
            text: "reference marker".to_owned(),
            ..ResourceBundle::default()
        };
        let context = Context {
            root: dir.path(),
            project_name: "demo",
            meta: &meta,
            build: &BuildConfig::default(),
            debug: &ProjectDebugSettings::default(),
            memory_layout: None,
            active_file: Some(&dir.path().join("Core/Src/main.c")),
            include_dirs: &[],
            library_dirs: &[],
            defines: &[],
            skill: &bundle,
            extra_skills: &extra_skills,
            references: &references,
            evolution: "user evolution marker",
            query: "uart gpio",
            index: None,
            retrieval: "ide retrieval marker",
        };
        let prompt = build_system_prompt_with_stats(&context).0;
        assert!(prompt.contains("STM32G431CBTx"));
        assert!(prompt.contains("hardware.dts"));
        assert!(prompt.contains("int main(void)"));
        assert!(prompt.contains("Core/Src/main.c"));
        assert!(prompt.contains("gpio_marker"));
        assert!(prompt.contains("framework.md 自动引用文件"));
        assert!(prompt.contains("app_init_marker"));
        assert!(prompt.contains("code_style_marker"));
        assert!(prompt.contains("se-file"));
        assert!(prompt.contains("se-replace"));
        assert!(prompt.contains("se-cli"));
        assert!(prompt.contains("se-list"));
        assert!(prompt.contains("se-read"));
        assert!(prompt.contains("T8 系统整体感知"));
        assert!(prompt.contains("用户导入的额外 Skill"));
        assert!(prompt.contains("extra skill marker"));
        assert!(prompt.contains("用户导入的参考资料"));
        assert!(prompt.contains("reference marker"));
        assert!(prompt.contains("用户技能进化规则"));
        assert!(prompt.contains("user evolution marker"));
        assert!(prompt.contains("IDE 文件检索结果"));
        assert!(prompt.contains("ide retrieval marker"));
    }

    #[test]
    fn loads_imported_skill_and_reference_text() {
        let dir = tempfile::tempdir().unwrap();
        write(
            &dir.path().join("hal-notes/SKILL.md"),
            "# HAL notes\nuse HAL_UART_Transmit\n",
        );
        let bundle = load_resource_dir(dir.path(), AiResourceKind::Skill, MAX_EXTRA_SKILL_BYTES);
        assert_eq!(bundle.files, 1);
        assert!(bundle.text.contains("HAL_UART_Transmit"));
        assert!(bundle.text.contains("hal-notes/SKILL.md"));
    }

    #[test]
    fn imported_pdf_text_sidecar_keeps_original_extension() {
        let path = Path::new("rm0008.pdf");
        assert_eq!(
            pdf_sidecar(path).file_name().unwrap().to_string_lossy(),
            "rm0008.pdf.txt"
        );
    }

    #[test]
    fn unique_resource_names_avoid_overwriting() {
        let dir = tempfile::tempdir().unwrap();
        let source = dir.path().join("manual.pdf");
        write(&source, "pdf placeholder");
        let first = unique_resource_path(&dir.path().join("imports"), &source);
        assert_eq!(first.file_name().unwrap(), "manual.pdf");
        std::fs::create_dir_all(first.parent().unwrap()).unwrap();
        std::fs::write(&first, "existing").unwrap();
        let second = unique_resource_path(&dir.path().join("imports"), &source);
        assert_eq!(second.file_name().unwrap(), "manual - 副本.pdf");
    }

    #[test]
    fn imports_files_and_folders_into_resource_directory() {
        let source = tempfile::tempdir().unwrap();
        write(&source.path().join("hal.md"), "# HAL\n");
        write(&source.path().join("skill/SKILL.md"), "# Skill\n");
        let destination = tempfile::tempdir().unwrap();
        let imported = source.path().join("hal.md");
        let skill = source.path().join("skill");
        let outcome = import_resources_into(destination.path(), &[imported, skill]).unwrap();
        assert_eq!(outcome.imported, 2);
        assert!(destination.path().join("hal.md").is_file());
        assert!(destination.path().join("skill/SKILL.md").is_file());
    }

    #[test]
    fn evolution_rules_are_normalized_and_deduped() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("ai/evolution/rules.md");
        let rules = vec![
            "- 目录：应用任务放 APP/tasks/".to_owned(),
            "目录：应用任务放 APP/tasks/".to_owned(),
            "命名：外设驱动统一 bsp_<外设>.c/.h".to_owned(),
            "   ".to_owned(),
        ];
        assert_eq!(append_evolution_rules(&path, &rules).unwrap(), 2);
        let text = std::fs::read_to_string(&path).unwrap();
        assert_eq!(text.matches("目录：应用任务放 APP/tasks/").count(), 1);
        assert!(text.contains("- 命名：外设驱动统一 bsp_<外设>.c/.h"));
        assert_eq!(append_evolution_rules(&path, &rules).unwrap(), 0);
    }

    #[test]
    fn exports_skill_tree_with_user_evolution() {
        let dir = tempfile::tempdir().unwrap();
        let skill = dir.path().join(".opencode/skills/stm32-embedded-dev");
        write(&skill.join("SKILL.md"), "# Skill\n");
        write(&skill.join("references/architecture.md"), "# Arch\n");
        let exports = dir.path().join("exports");
        let evolution_text = "# stm32-embedded-dev 用户进化规则\n\n- 目录：应用任务放 APP/tasks/\n";
        let exported =
            export_skill_bundle_with_evolution(dir.path(), &exports, evolution_text).unwrap();
        assert_eq!(
            exported.file_name().unwrap().to_string_lossy(),
            "stm32-embedded-dev-export"
        );
        assert!(exported.join("SKILL.md").is_file());
        assert!(exported.join("references/architecture.md").is_file());
        let evolution =
            std::fs::read_to_string(exported.join("references/evolution-user.md")).unwrap();
        assert!(evolution.contains("APP/tasks/"));
        assert!(evolution.contains("用户进化规则"));

        let second = export_skill_bundle_with_evolution(dir.path(), &exports, "").unwrap();
        assert_eq!(
            second.file_name().unwrap().to_string_lossy(),
            "stm32-embedded-dev-export-2"
        );

        let error = export_skill_bundle_with_evolution(dir.path(), &skill, "").unwrap_err();
        assert!(error.contains("Skill 目录内部"));
    }
}
