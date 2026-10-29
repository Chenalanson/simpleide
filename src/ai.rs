//! OpenAI-compatible chat client and STM32 skill/context loader for AI mode.

use std::collections::HashSet;
use std::fmt::Write as _;
use std::io::{BufRead, BufReader};
use std::path::{Path, PathBuf};
use std::sync::mpsc;
use std::time::Duration;

use serde_json::{json, Value};

use crate::config::{BuildConfig, ProjectDebugSettings};
use crate::ignore::IgnoreRules;
use crate::memory::ResolvedMemory;
use crate::project_meta::ProjectMeta;
use crate::util::{display_path, read_text_file};

const SKILL_ID: &str = "stm32-embedded-dev";
const MAX_SKILL_BYTES: usize = 160_000;
const MAX_SYSTEM_PROMPT_BYTES: usize = 220_000;
const MAX_TREE_FILES: usize = 400;

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
pub fn build_system_prompt(context: &Context<'_>) -> String {
    let mut prompt = String::new();
    prompt.push_str(
        "你是 Simple embedded 的 AI 助手，正在协助 STM32 嵌入式开发。\n\
         必须优先遵循已加载的 stm32-embedded-dev skill：先按第 1 节判定 T1–T5 任务类型，再按第 3 节对应固定模板的小节编号、标题和表格输出；\
         模板中的代码实现/代码修改节用下方的 se-file / se-replace 承载，不要自创分点或省略不适用的 N/A 行。\
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

    let mut appended = Vec::new();
    append_file(
        &mut prompt,
        context.root,
        &context.root.join("hardware.dts"),
        60_000,
        "dts",
        &mut appended,
    );
    if let Some(framework) = crate::framework::find(context.root) {
        append_file(
            &mut prompt,
            context.root,
            &framework,
            80_000,
            "markdown",
            &mut appended,
        );
    }
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
    prompt.push_str("\n### 工程主要源文件\n");
    let mut source_bytes = 0usize;
    for path in source_candidates(context.root) {
        if prompt.len() > MAX_SYSTEM_PROMPT_BYTES - 24_000
            || source_bytes > 110_000
            || appended.len() > 40
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
        "\n\n## 文件修改协议（重要）\n\n\
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
         规则：路径必须相对工程根目录，禁止绝对路径、`..`、`build` / `Debug` / `Release` / `out` / `target` 等生成目录；一次最多修改 20 个文件；SEARCH 必须能在当前文件中唯一匹配；不要修改 `.simple-embedded` 工程配置。IDE 会生成差异，只有用户点击“应用全部”后才会写入，写入前会备份到用户目录。skill 中要求输出代码块的步骤在 AI 模式下改用上述格式，解释文字照常输出。\n",
    );
    prompt.push_str(
        "\n\n## 输出要求（与 skill 固定模板的关系）\n\n\
         1. 正文结构以 skill 第 1 节任务判定和第 3 节对应模板为准：保留模板的小节编号、标题、表格维度和 N/A 行，不得自创分点。\n\
         2. skill 模板中的代码实现/代码修改节仍保留编号，但实际写入工程的内容用 se-file / se-replace 块承载；执行器会显示差异卡片，不要在普通代码块里重复同一份完整代码。\n\
         3. 涉及硬件资源变化时，先按模板输出 hardware.dts 台账；手册级事实不确定时放在待核对表/TODO 中。\n\
         4. S4 核对表和 S5 构建/排查是必输出项；不要用本提示词的通用分点替代 skill 模板。\n\
         5. 若模型无法完整遵循模板，至少给出任务判定、台账/无变更说明、代码修改、核对表和验证步骤，并明确缺失项。\n",
    );

    if prompt.len() > MAX_SYSTEM_PROMPT_BYTES {
        prompt.truncate(MAX_SYSTEM_PROMPT_BYTES);
        prompt.push_str("\n\n[工程上下文已截断，请向用户询问缺失文件]\n");
    }
    prompt
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

fn source_candidates(root: &Path) -> Vec<PathBuf> {
    let mut files = Vec::new();
    for directory in ["Core/Src", "Core/Inc", "src", "include", "Inc", "Src"] {
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
    files.truncate(30);
    files
}

fn collect_sources(root: &Path, directory: &Path, files: &mut Vec<PathBuf>, depth: usize) {
    if depth > 2 || files.len() >= 30 {
        return;
    }
    let Ok(entries) = std::fs::read_dir(directory) else {
        return;
    };
    let mut entries = entries.flatten().collect::<Vec<_>>();
    entries.sort_by_key(|entry| entry.file_name());
    for entry in entries {
        if files.len() >= 30 {
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
        assert!(bundle.files.len() >= 10);
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
        };
        let prompt = build_system_prompt(&context);
        assert!(prompt.contains("STM32G431CBTx"));
        assert!(prompt.contains("hardware.dts"));
        assert!(prompt.contains("int main(void)"));
        assert!(prompt.contains("Core/Src/main.c"));
        assert!(prompt.contains("gpio_marker"));
        assert!(prompt.contains("se-file"));
        assert!(prompt.contains("se-replace"));
        assert!(prompt.contains("用户导入的额外 Skill"));
        assert!(prompt.contains("extra skill marker"));
        assert!(prompt.contains("用户导入的参考资料"));
        assert!(prompt.contains("reference marker"));
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
}
