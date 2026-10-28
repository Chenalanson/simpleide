//! OpenAI-compatible chat client and STM32 skill/context loader for AI mode.

use std::collections::HashSet;
use std::fmt::Write as _;
use std::path::{Path, PathBuf};
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
         新建或整体覆盖文件，使用：\n\n\
         ```se-file path=相对路径\n\
         <完整文件内容>\n\
         ```\n\n\
         局部修改（优先使用），使用：\n\n\
         ```se-replace path=相对路径\n\
         <<<<<<< SEARCH\n\
         <当前文件中完全一致的原文，包含缩进和换行>\n\
         =======\n\
         <替换后的文本>\n\
         >>>>>>> REPLACE\n\
         ```\n\n\
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
            let message = serde_json::from_str::<Value>(&body)
                .ok()
                .and_then(|value| {
                    value
                        .pointer("/error/message")
                        .and_then(Value::as_str)
                        .map(ToOwned::to_owned)
                })
                .unwrap_or_else(|| truncate_chars(body.trim(), 800));
            Err(format!("API 请求失败（HTTP {code}）：{message}"))
        }
        Err(err) => Err(format!("AI 网络请求失败：{err}")),
    }
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
        assert!(bundle.files.len() >= 6);
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
        };
        let prompt = build_system_prompt(&context);
        assert!(prompt.contains("STM32G431CBTx"));
        assert!(prompt.contains("hardware.dts"));
        assert!(prompt.contains("int main(void)"));
        assert!(prompt.contains("Core/Src/main.c"));
        assert!(prompt.contains("gpio_marker"));
        assert!(prompt.contains("se-file"));
        assert!(prompt.contains("se-replace"));
    }
}
