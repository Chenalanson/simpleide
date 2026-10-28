//! Project architecture overview for the AI mode middle panel.
//!
//! The report is a deliberately static, text-level analysis. It reuses the
//! project call index when available, then applies conservative regexes for
//! interrupts, RTOS usage, main-loop scheduling and common embedded design
//! patterns. It answers "how is this firmware put together?" at a glance; it
//! is not a compiler-grade call-graph analyzer.

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

use regex::Regex;

use crate::ignore::IgnoreRules;
use crate::relations::RelationIndex;
use crate::util::read_text_file;

const MAX_FILES: usize = 200;
const MAX_FILE_BYTES: u64 = 2 * 1024 * 1024;
const MAX_INTERRUPTS: usize = 16;
const MAX_TASKS: usize = 8;
const MAX_PATTERNS: usize = 8;
const MAX_FLOW_NODES: usize = 7;

#[derive(Clone, Debug, Default)]
pub struct ArchitectureReport {
    pub file_count: usize,
    pub function_count: usize,
    pub edge_count: usize,
    pub layers: Vec<LayerInfo>,
    pub schedule: ScheduleInfo,
    pub framework: Vec<FrameworkInfo>,
    pub patterns: Vec<PatternInfo>,
    pub interrupts: Vec<InterruptInfo>,
    pub tasks: Vec<TaskInfo>,
    pub flow: Vec<FlowStage>,
    pub roots: Vec<RootInfo>,
    pub notes: Vec<String>,
}

#[derive(Clone, Debug, Default)]
pub struct LayerInfo {
    pub key: String,
    pub name: String,
    pub role: String,
    pub files: usize,
    pub functions: usize,
    pub examples: Vec<String>,
}

#[derive(Clone, Debug, Default)]
pub struct ScheduleInfo {
    pub kind: String,
    pub summary: String,
    pub evidence: Vec<String>,
}

#[derive(Clone, Debug, Default)]
pub struct FrameworkInfo {
    pub name: String,
    pub evidence: String,
}

#[derive(Clone, Debug, Default)]
pub struct PatternInfo {
    pub name: String,
    pub confidence: String,
    pub evidence: String,
}

#[derive(Clone, Debug, Default)]
pub struct InterruptInfo {
    pub handler: String,
    pub peripheral: String,
    pub file: String,
    pub line: usize,
    pub priority: Option<String>,
    pub calls: Vec<String>,
}

#[derive(Clone, Debug, Default)]
pub struct TaskInfo {
    pub name: String,
    pub api: String,
    pub file: String,
    pub line: usize,
    pub detail: String,
}

#[derive(Clone, Debug, Default)]
pub struct FlowStage {
    pub title: String,
    pub nodes: Vec<String>,
}

#[derive(Clone, Debug, Default)]
pub struct RootInfo {
    pub name: String,
    pub kind: String,
    pub count: usize,
    pub callees: Vec<String>,
}

#[derive(Clone, Debug)]
struct SourceFile {
    rel: String,
    text: String,
}

#[derive(Clone, Debug)]
struct FunctionDef {
    name: String,
    rel: String,
    line: usize,
}

#[derive(Clone, Debug, Default)]
struct MainLoop {
    file: String,
    line: usize,
    init_calls: Vec<String>,
    loop_calls: Vec<String>,
}

pub fn analyze(
    root: &Path,
    relation: Option<&RelationIndex>,
    ignore: &IgnoreRules,
) -> Result<ArchitectureReport, String> {
    let files = collect_sources(root, ignore);
    if files.is_empty() {
        return Err("未找到可分析的 C/C++ 源文件".to_owned());
    }
    let functions = relation
        .map(|index| {
            index
                .functions
                .iter()
                .map(|(name, info)| FunctionDef {
                    name: name.clone(),
                    rel: relative(root, &info.path),
                    line: info.line,
                })
                .collect::<Vec<_>>()
        })
        .unwrap_or_else(|| extract_functions(&files));
    let user_functions = functions
        .iter()
        .filter(|function| !is_vendor_path(&function.rel))
        .cloned()
        .collect::<Vec<_>>();
    let function_set = user_functions
        .iter()
        .map(|function| function.name.clone())
        .collect::<BTreeSet<_>>();
    let callbacks = user_functions
        .iter()
        .filter(|function| is_callback(&function.name))
        .cloned()
        .collect::<Vec<_>>();

    let main = detect_main_loop(&files);
    let tasks = detect_tasks(&files);
    let interrupts = detect_interrupts(&files, &user_functions, relation, &callbacks);
    let state_machine = detect_state_machine(&files);
    let ring = detect_ring_buffer(&files);
    let dma = first_match(
        files.as_slice(),
        r"\b(?:HAL_[A-Za-z0-9_]*_Start_DMA|HAL_DMA_Start_IT)\b",
    );
    let low_power = first_match(files.as_slice(), r"\b(?:__WFI|HAL_PWR_EnterSLEEPMode)\b");
    let event_flag = detect_event_flag(&files, callbacks.len(), main.is_some());

    let schedule = detect_schedule(&files, &interrupts, &tasks, &main, &callbacks, &event_flag);
    let layers = detect_layers(root, &user_functions);
    let framework = detect_framework(
        &schedule,
        &interrupts,
        &callbacks,
        &tasks,
        main.as_ref(),
        state_machine.is_some(),
        dma.is_some(),
        low_power.is_some(),
        event_flag.is_some(),
    );
    let patterns = detect_patterns(
        &files,
        &interrupts,
        &callbacks,
        &tasks,
        &layers,
        state_machine.as_ref(),
        ring.as_ref(),
        event_flag.as_ref(),
    );
    let flow = build_flow(&interrupts, &tasks, main.as_ref(), relation, &function_set);
    let roots = build_roots(
        &user_functions,
        relation,
        &tasks,
        &interrupts,
        &function_set,
    );

    let mut report = ArchitectureReport {
        file_count: files.len(),
        function_count: user_functions.len(),
        edge_count: relation.map_or(0, |index| {
            index
                .calls
                .values()
                .map(|callees| callees.len())
                .sum::<usize>()
        }),
        layers,
        schedule,
        framework,
        patterns,
        interrupts,
        tasks,
        flow,
        roots,
        notes: Vec::new(),
    };

    if relation.is_none() {
        report
            .notes
            .push("调用索引尚未就绪，函数调用关系可能不完整；索引完成后会自动刷新。".to_owned());
    }
    if report.interrupts.len() >= MAX_INTERRUPTS {
        report
            .notes
            .push(format!("中断较多，仅展示前 {MAX_INTERRUPTS} 个。"));
    }
    if report.file_count >= MAX_FILES {
        report
            .notes
            .push(format!("源文件较多，仅分析前 {MAX_FILES} 个。"));
    }
    report.notes.push(
        "静态启发式分析：只读取已保存的 C/H 文件，不展开宏、函数指针、条件编译和汇编。".to_owned(),
    );
    Ok(report)
}

fn collect_sources(root: &Path, ignore: &IgnoreRules) -> Vec<SourceFile> {
    let mut paths = Vec::new();
    collect_files(root, root, &mut paths, 0);
    let mut files = Vec::new();
    let mut seen = BTreeSet::new();
    for path in paths {
        if files.len() >= MAX_FILES {
            break;
        }
        let rel_path = path.strip_prefix(root).unwrap_or(&path);
        if ignore.is_ignored(rel_path, false) || is_generated(rel_path) || is_vendor_dir(rel_path) {
            continue;
        }
        let Ok(metadata) = std::fs::metadata(&path) else {
            continue;
        };
        if metadata.len() > MAX_FILE_BYTES {
            continue;
        }
        let rel = relative(root, &path);
        if !seen.insert(rel.clone()) {
            continue;
        }
        let Ok(loaded) = read_text_file(&path) else {
            continue;
        };
        let text = crate::relations::sanitize_source(&loaded.text);
        files.push(SourceFile { rel, text });
    }
    files
}

fn collect_files(root: &Path, directory: &Path, paths: &mut Vec<PathBuf>, depth: usize) {
    if depth > 5 || paths.len() >= MAX_FILES * 3 {
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
        if file_type.is_dir() {
            let relative = path.strip_prefix(root).unwrap_or(&path);
            if !is_generated(relative) && !is_vendor_dir(relative) {
                collect_files(root, &path, paths, depth + 1);
            }
        } else if file_type.is_file() && is_source(&path) {
            paths.push(path);
        }
    }
}

fn is_source(path: &Path) -> bool {
    path.extension().is_some_and(|extension| {
        matches!(
            extension.to_string_lossy().to_ascii_lowercase().as_str(),
            "c" | "h" | "cc" | "cpp" | "cxx" | "hpp" | "hh"
        )
    })
}

fn is_generated(relative: &Path) -> bool {
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
                | "dist"
                | "tools"
        )
    })
}

fn is_vendor_dir(relative: &Path) -> bool {
    relative.components().any(|component| {
        matches!(
            component
                .as_os_str()
                .to_string_lossy()
                .to_ascii_lowercase()
                .as_str(),
            "drivers"
                | "cmsis"
                | "middlewares"
                | "third_party"
                | "third-party"
                | "vendor"
                | "hal_driver"
                | "hal_drivers"
        )
    })
}

fn is_vendor_path(relative: &str) -> bool {
    let lower = relative.to_ascii_lowercase();
    lower.starts_with("drivers/")
        || lower.contains("/drivers/")
        || lower.contains("cmsis")
        || lower.starts_with("middlewares/")
        || lower.contains("/middlewares/")
        || lower.contains("third_party")
        || lower.contains("vendor")
}

fn relative(root: &Path, path: &Path) -> String {
    path.strip_prefix(root)
        .unwrap_or(path)
        .to_string_lossy()
        .replace('\\', "/")
}

fn extract_functions(files: &[SourceFile]) -> Vec<FunctionDef> {
    let Ok(regex) =
        Regex::new(r"(?m)^[ \t]*(?:[A-Za-z_]\w*[\s\*]+)+([A-Za-z_]\w*)\s*\([^;{}]*\)\s*\{")
    else {
        return Vec::new();
    };
    let mut functions = Vec::new();
    for file in files {
        for captures in regex.captures_iter(&file.text) {
            let Some(name) = captures.get(1) else {
                continue;
            };
            functions.push(FunctionDef {
                name: name.as_str().to_owned(),
                rel: file.rel.clone(),
                line: line_of(&file.text, name.start()),
            });
        }
    }
    functions
}

fn detect_interrupts(
    files: &[SourceFile],
    functions: &[FunctionDef],
    relation: Option<&RelationIndex>,
    callbacks: &[FunctionDef],
) -> Vec<InterruptInfo> {
    let mut priorities = BTreeMap::new();
    if let Ok(regex) = Regex::new(
        r"(?:HAL_)?NVIC_SetPriority\s*\(\s*([A-Za-z_]\w*)\s*,\s*([^,)]+)\s*,\s*([^)]+)\)",
    ) {
        for file in files {
            for captures in regex.captures_iter(&file.text) {
                let Some(irq) = captures.get(1) else {
                    continue;
                };
                let irq = irq.as_str().trim_end_matches("_IRQn").to_owned();
                let value = format!("{} / {}", captures[2].trim(), captures[3].trim());
                priorities.insert(irq, value);
            }
        }
    }

    let mut handlers = Vec::new();
    let mut seen = BTreeSet::new();
    for function in functions {
        if !is_interrupt_handler(&function.name) || !seen.insert(function.name.clone()) {
            continue;
        }
        let peripheral = interrupt_peripheral(&function.name);
        let mut calls = relation
            .and_then(|index| index.calls.get(&function.name))
            .map(|callees| {
                callees
                    .iter()
                    .filter(|name| {
                        functions.iter().any(|function| &function.name == *name)
                            || name.contains("Callback")
                    })
                    .take(5)
                    .cloned()
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default();
        for callback in callbacks {
            if callback_matches(&peripheral, &callback.name) && !calls.contains(&callback.name) {
                calls.push(callback.name.clone());
            }
        }
        calls.truncate(6);
        handlers.push(InterruptInfo {
            handler: function.name.clone(),
            peripheral: peripheral.clone(),
            file: function.rel.clone(),
            line: function.line,
            priority: priorities
                .get(&function.name)
                .or_else(|| priorities.get(&peripheral))
                .cloned(),
            calls,
        });
    }
    handlers.sort_by(|left, right| {
        interrupt_rank(&left.handler)
            .cmp(&interrupt_rank(&right.handler))
            .then_with(|| left.handler.cmp(&right.handler))
    });
    handlers.truncate(MAX_INTERRUPTS);
    handlers
}

/// Peripheral IRQs first, then the system tick, then fault/exception vectors.
/// The architecture panel is about scheduling, so faults should not push the
/// actual peripheral interrupts out of view.
fn interrupt_rank(handler: &str) -> u8 {
    if handler.ends_with("_IRQHandler") {
        0
    } else if handler == "SysTick_Handler" {
        1
    } else {
        2
    }
}

fn is_interrupt_handler(name: &str) -> bool {
    name.ends_with("_IRQHandler")
        || matches!(
            name,
            "SysTick_Handler"
                | "HardFault_Handler"
                | "NMI_Handler"
                | "SVC_Handler"
                | "PendSV_Handler"
                | "MemManage_Handler"
                | "BusFault_Handler"
                | "UsageFault_Handler"
        )
}

fn interrupt_peripheral(handler: &str) -> String {
    if let Some(prefix) = handler.strip_suffix("_IRQHandler") {
        return prefix.to_owned();
    }
    if let Some(prefix) = handler.strip_suffix("_Handler") {
        return prefix.to_owned();
    }
    handler.to_owned()
}

fn is_callback(name: &str) -> bool {
    name.ends_with("Callback")
        || name.ends_with("callback")
        || name.ends_with("_cb")
        || name.ends_with("_CB")
}

fn callback_matches(peripheral: &str, callback: &str) -> bool {
    let peripheral = peripheral.to_ascii_lowercase();
    let callback = callback.to_ascii_lowercase();
    [
        ("usart", "uart"),
        ("uart", "uart"),
        ("spi", "spi"),
        ("i2c", "i2c"),
        ("tim", "tim"),
        ("adc", "adc"),
        ("can", "can"),
        ("gpio", "gpio"),
        ("exti", "exti"),
        ("dma", "dma"),
        ("rtc", "rtc"),
    ]
    .iter()
    .any(|(token, callback_token)| peripheral.contains(token) && callback.contains(callback_token))
}

fn detect_main_loop(files: &[SourceFile]) -> Option<MainLoop> {
    let function = find_function_body(files, "main")?;
    let loop_regex =
        Regex::new(r"\b(?:while\s*\(\s*(?:1|true)\s*\)|for\s*\(\s*;\s*;\s*\))").ok()?;
    let loop_match = loop_regex.find(&function.body)?;
    let before = &function.body[..loop_match.start()];
    let after = &function.body[loop_match.start()..];
    let loop_body = after
        .find('{')
        .and_then(|open| block_end(after, open).map(|close| &after[open + 1..close]))
        .unwrap_or(after);
    let init_calls = extract_calls(before)
        .into_iter()
        .filter(|name| {
            let lower = name.to_ascii_lowercase();
            lower.contains("init") || lower.contains("systemclock") || lower.starts_with("mx_")
        })
        .take(MAX_FLOW_NODES)
        .collect::<Vec<_>>();
    let loop_calls = extract_calls(loop_body)
        .into_iter()
        .take(10)
        .collect::<Vec<_>>();
    Some(MainLoop {
        file: function.file,
        line: function.line,
        init_calls,
        loop_calls,
    })
}

fn detect_tasks(files: &[SourceFile]) -> Vec<TaskInfo> {
    let Ok(regex) = Regex::new(
        r"\b(xTaskCreate(?:Static)?|osThreadNew)\s*\(\s*(?:\([^)]*\)\s*)?([A-Za-z_]\w*)",
    ) else {
        return Vec::new();
    };
    let name_regex = Regex::new(r#""([^"]*)""#).ok();
    let number_regex = Regex::new(r"\b(\d+)\b").ok();
    let mut tasks = Vec::new();
    let mut seen = BTreeSet::new();
    for file in files {
        for captures in regex.captures_iter(&file.text) {
            let api = captures.get(1).map(|value| value.as_str()).unwrap_or("");
            let name = captures.get(2).map(|value| value.as_str()).unwrap_or("");
            if name.is_empty() || !seen.insert((api.to_owned(), name.to_owned())) {
                continue;
            }
            let detail = call_snippet(&file.text, captures.get(0).map_or(0, |value| value.start()))
                .map(|snippet| {
                    let quoted = name_regex
                        .as_ref()
                        .and_then(|regex| regex.captures(&snippet))
                        .and_then(|captures| captures.get(1))
                        .map(|value| value.as_str().to_owned());
                    let numbers = number_regex
                        .as_ref()
                        .map(|regex| {
                            regex
                                .captures_iter(&snippet)
                                .filter_map(|captures| captures.get(1))
                                .map(|value| value.as_str())
                                .take(3)
                                .collect::<Vec<_>>()
                                .join("/")
                        })
                        .unwrap_or_default();
                    match quoted {
                        Some(name) if !numbers.is_empty() => format!("{name} · {numbers}"),
                        Some(name) => name,
                        None => numbers,
                    }
                })
                .unwrap_or_default();
            tasks.push(TaskInfo {
                name: name.to_owned(),
                api: api.to_owned(),
                file: file.rel.clone(),
                line: line_of(&file.text, captures.get(0).map_or(0, |value| value.start())),
                detail,
            });
            if tasks.len() >= MAX_TASKS {
                return tasks;
            }
        }
    }
    tasks
}

fn detect_schedule(
    files: &[SourceFile],
    interrupts: &[InterruptInfo],
    tasks: &[TaskInfo],
    main: &Option<MainLoop>,
    callbacks: &[FunctionDef],
    event_flag: &Option<(String, usize, String)>,
) -> ScheduleInfo {
    let mut evidence = Vec::new();
    let rtos = !tasks.is_empty()
        || has_any(
            files,
            &[
                "vTaskStartScheduler",
                "osKernelStart",
                "xTaskCreate",
                "osThreadNew",
            ],
        );
    let queues = has_any(
        files,
        &[
            "xQueueCreate",
            "osMessageQueueNew",
            "xEventGroupCreate",
            "osEventFlagsNew",
            "xSemaphoreCreate",
            "osSemaphoreNew",
        ],
    );
    if rtos {
        evidence.push(format!(
            "RTOS API：任务 {} 个{}",
            tasks.len(),
            if queues {
                "，使用队列/信号量/事件"
            } else {
                ""
            }
        ));
    }
    if let Some(main) = main {
        if rtos {
            evidence.push(format!("main 初始化：{}:{}", main.file, main.line));
        } else {
            evidence.push(format!("main 主循环：{}:{}", main.file, main.line));
            if !main.loop_calls.is_empty() {
                evidence.push(format!(
                    "循环调用：{}",
                    main.loop_calls
                        .iter()
                        .take(5)
                        .cloned()
                        .collect::<Vec<_>>()
                        .join("、")
                ));
            }
        }
    }
    if !interrupts.is_empty() {
        evidence.push(format!("IRQHandler：{} 个", interrupts.len()));
    }
    if !callbacks.is_empty() {
        evidence.push(format!("HAL/外设回调：{} 个", callbacks.len()));
    }
    if event_flag.is_some() {
        evidence.push("中断/回调置标志，任务或主循环中处理".to_owned());
    }
    if has_any(files, &["HAL_Delay", "osDelay", "vTaskDelay"]) {
        evidence.push("延时/节拍 API：HAL_Delay / osDelay / vTaskDelay".to_owned());
    }
    if has_any(files, &["HAL_TIM_PeriodElapsedCallback", "SysTick_Handler"]) {
        evidence.push("周期节拍：SysTick / 定时器回调".to_owned());
    }

    let (kind, summary) = if rtos {
        let kind = if interrupts.is_empty() {
            "RTOS 任务调度"
        } else {
            "RTOS 任务 + 中断"
        };
        let summary = if let Some(main) = main {
            format!(
                "RTOS 任务框架。main 负责初始化（{}:{}），启动调度器后由任务接管，中断/队列与任务交互。",
                main.file, main.line
            )
        } else {
            "RTOS 任务框架。启动调度器后由任务接管，中断/队列与任务交互。".to_owned()
        };
        (kind.to_owned(), summary)
    } else if let Some(main) = main {
        let kind = if interrupts.is_empty() {
            "裸机超级循环"
        } else if event_flag.is_some() {
            "前后台 + 事件标志"
        } else {
            "裸机超级循环 + 中断"
        };
        let summary = format!(
            "main（{}:{}）中的 while(1)/for(;;) 前后台循环：初始化后轮询业务，外设通过中断/回调或标志位参与。",
            main.file, main.line
        );
        (kind.to_owned(), summary)
    } else if !interrupts.is_empty() {
        (
            "中断驱动".to_owned(),
            "未发现 main 主循环，主要逻辑由 IRQHandler / 回调驱动。".to_owned(),
        )
    } else {
        (
            "未识别".to_owned(),
            "未发现 main、RTOS 任务或中断处理函数。".to_owned(),
        )
    };
    ScheduleInfo {
        kind,
        summary,
        evidence,
    }
}

#[allow(clippy::too_many_arguments)]
fn detect_framework(
    schedule: &ScheduleInfo,
    interrupts: &[InterruptInfo],
    callbacks: &[FunctionDef],
    tasks: &[TaskInfo],
    main: Option<&MainLoop>,
    state_machine: bool,
    dma: bool,
    low_power: bool,
    event_flag: bool,
) -> Vec<FrameworkInfo> {
    let mut values = Vec::new();
    if schedule.kind.contains("RTOS") {
        values.push(FrameworkInfo {
            name: "RTOS 任务框架".to_owned(),
            evidence: format!("{} 个任务，调度器接管执行", tasks.len()),
        });
    }
    if main.is_some() && !interrupts.is_empty() && !schedule.kind.contains("RTOS") {
        values.push(FrameworkInfo {
            name: "前后台框架".to_owned(),
            evidence: format!("main 超级循环 + {} 个中断处理函数", interrupts.len()),
        });
    }
    if event_flag {
        values.push(FrameworkInfo {
            name: "事件驱动".to_owned(),
            evidence: "中断/回调置位，任务或主循环查询标志后处理".to_owned(),
        });
    }
    if !callbacks.is_empty() {
        values.push(FrameworkInfo {
            name: "回调分发".to_owned(),
            evidence: callbacks
                .iter()
                .take(3)
                .map(|callback| callback.name.clone())
                .collect::<Vec<_>>()
                .join("、"),
        });
    }
    if state_machine {
        values.push(FrameworkInfo {
            name: "状态机执行".to_owned(),
            evidence: "使用 enum/switch 管理运行状态或模式".to_owned(),
        });
    }
    if dma {
        values.push(FrameworkInfo {
            name: "DMA + 中断".to_owned(),
            evidence: "外设通过 DMA 搬运数据，完成/半完成中断通知".to_owned(),
        });
    }
    if low_power {
        values.push(FrameworkInfo {
            name: "低功耗空闲".to_owned(),
            evidence: "使用 __WFI / HAL_PWR_EnterSLEEPMode 进入睡眠".to_owned(),
        });
    }
    if values.is_empty() {
        values.push(FrameworkInfo {
            name: "未识别".to_owned(),
            evidence: "未匹配到常见执行框架特征".to_owned(),
        });
    }
    values.dedup_by(|left, right| left.name == right.name);
    values
}

type LayerAccumulator = (BTreeSet<String>, usize, Vec<(u8, String)>);

fn detect_layers(root: &Path, functions: &[FunctionDef]) -> Vec<LayerInfo> {
    let mut layers: BTreeMap<&'static str, LayerAccumulator> = BTreeMap::new();
    for function in functions {
        let key = layer_of(&function.rel);
        let entry = layers
            .entry(key)
            .or_insert_with(|| (BTreeSet::new(), 0, Vec::new()));
        entry.0.insert(function.rel.clone());
        entry.1 += 1;
        if !function.name.ends_with("Handler")
            && !entry.2.iter().any(|(_, name)| name == &function.name)
        {
            entry
                .2
                .push((example_priority(&function.name), function.name.clone()));
        }
    }

    let mut values = Vec::new();
    for key in ["app", "bsp"] {
        if let Some((files, functions, mut examples)) = layers.remove(key) {
            examples.sort();
            examples.truncate(3);
            values.push(LayerInfo {
                key: key.to_owned(),
                name: layer_label(key).to_owned(),
                role: layer_role(key).to_owned(),
                files: files.len(),
                functions,
                examples: examples.into_iter().map(|(_, name)| name).collect(),
            });
        }
    }
    for (key, directory, name, role) in [
        ("hal", "Drivers", "HAL 驱动", "STM32 HAL 外设驱动"),
        ("cmsis", "Drivers/CMSIS", "CMSIS", "内核、启动与寄存器定义"),
        ("middleware", "Middlewares", "中间件", "FreeRTOS、协议栈等"),
    ] {
        let path = root.join(directory);
        if !path.is_dir() {
            continue;
        }
        let count_root = if key == "hal" {
            first_vendor_path(root, "Drivers", "stm32")
                .map(|vendor| root.join(vendor))
                .unwrap_or_else(|| path.clone())
        } else {
            path.clone()
        };
        let files = count_source_files(&count_root, 2_000);
        if files == 0 {
            continue;
        }
        let examples = if key == "hal" {
            first_vendor_path(root, "Drivers", "stm32")
                .into_iter()
                .collect::<Vec<_>>()
        } else {
            vec![format!("{directory}/")]
        };
        values.push(LayerInfo {
            key: key.to_owned(),
            name: name.to_owned(),
            role: role.to_owned(),
            files,
            functions: 0,
            examples,
        });
    }
    values
}

fn example_priority(name: &str) -> u8 {
    if name == "main" {
        0
    } else if name.starts_with("app_") || name.starts_with("APP_") {
        1
    } else if name.starts_with("MX_") {
        2
    } else if name.contains("task")
        || name.contains("Task")
        || name.contains("poll")
        || name.contains("process")
        || name.contains("dispatch")
    {
        3
    } else if name.ends_with("Callback") {
        5
    } else {
        4
    }
}

fn layer_of(relative: &str) -> &'static str {
    let lower = relative.to_ascii_lowercase();
    if lower.starts_with("drivers/cmsis") || lower.contains("/cmsis/") {
        "cmsis"
    } else if lower.starts_with("drivers/") || lower.contains("/hal_driver/") {
        "hal"
    } else if lower.starts_with("middlewares/") || lower.contains("freertos") {
        "middleware"
    } else if lower.starts_with("bsp")
        || lower.starts_with("board")
        || lower.starts_with("hardware")
        || lower.contains("/bsp/")
        || lower.contains("/board/")
    {
        "bsp"
    } else {
        "app"
    }
}

fn layer_label(key: &str) -> &'static str {
    match key {
        "cmsis" => "CMSIS",
        "hal" => "HAL 驱动",
        "middleware" => "中间件",
        "bsp" => "板级",
        _ => "应用",
    }
}

fn layer_role(key: &str) -> &'static str {
    match key {
        "cmsis" => "内核、启动与寄存器定义",
        "hal" => "STM32 HAL 外设驱动",
        "middleware" => "FreeRTOS、协议栈等",
        "bsp" => "板级引脚与外设封装",
        _ => "业务逻辑、调度与状态机",
    }
}

fn count_source_files(directory: &Path, cap: usize) -> usize {
    let mut count = 0usize;
    let mut stack = vec![(directory.to_path_buf(), 0usize)];
    while let Some((path, depth)) = stack.pop() {
        if depth > 4 || count >= cap {
            continue;
        }
        let Ok(entries) = std::fs::read_dir(&path) else {
            continue;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            let Ok(file_type) = entry.file_type() else {
                continue;
            };
            if file_type.is_dir() {
                stack.push((path, depth + 1));
            } else if file_type.is_file() && is_source(&path) {
                count += 1;
                if count >= cap {
                    break;
                }
            }
        }
    }
    count
}

fn first_vendor_path(root: &Path, directory: &str, prefix: &str) -> Option<String> {
    let entries = std::fs::read_dir(root.join(directory)).ok()?;
    for entry in entries.flatten() {
        let name = entry.file_name().to_string_lossy().into_owned();
        if name.to_ascii_lowercase().starts_with(prefix) {
            return Some(format!("{directory}/{name}/"));
        }
    }
    None
}

#[allow(clippy::too_many_arguments)]
fn detect_patterns(
    files: &[SourceFile],
    interrupts: &[InterruptInfo],
    callbacks: &[FunctionDef],
    tasks: &[TaskInfo],
    layers: &[LayerInfo],
    state_machine: Option<&(String, usize, String)>,
    ring: Option<&(String, usize, String)>,
    event_flag: Option<&(String, usize, String)>,
) -> Vec<PatternInfo> {
    let mut patterns = Vec::new();
    if !callbacks.is_empty() {
        patterns.push(PatternInfo {
            name: "回调 / 观察者".to_owned(),
            confidence: if callbacks.len() >= 2 { "高" } else { "中" }.to_owned(),
            evidence: format!(
                "{} · 共 {} 个回调",
                callbacks
                    .iter()
                    .take(2)
                    .map(|callback| callback.name.clone())
                    .collect::<Vec<_>>()
                    .join("、"),
                callbacks.len()
            ),
        });
    }
    if !interrupts.is_empty() && !callbacks.is_empty() {
        patterns.push(PatternInfo {
            name: "中断 → 回调分发".to_owned(),
            confidence: "高".to_owned(),
            evidence: format!(
                "{} 个 IRQHandler，回调 {} 个",
                interrupts.len(),
                callbacks.len()
            ),
        });
    }
    if let Some((file, line, text)) = state_machine {
        patterns.push(PatternInfo {
            name: "状态机".to_owned(),
            confidence: if text.contains("enum") { "高" } else { "中" }.to_owned(),
            evidence: format!("{file}:{line} · {text}"),
        });
    }
    if let Some((file, line, text)) = ring {
        patterns.push(PatternInfo {
            name: "环形缓冲 / FIFO".to_owned(),
            confidence: "中".to_owned(),
            evidence: format!("{file}:{line} · {text}"),
        });
    }
    if !tasks.is_empty() {
        patterns.push(PatternInfo {
            name: if has_any(
                files,
                &[
                    "xQueueCreate",
                    "osMessageQueueNew",
                    "xEventGroupCreate",
                    "osEventFlagsNew",
                    "xSemaphoreCreate",
                ],
            ) {
                "生产者 / 消费者"
            } else {
                "多任务并行"
            }
            .to_owned(),
            confidence: "中".to_owned(),
            evidence: format!("{} 个 RTOS 任务", tasks.len()),
        });
    }
    if let Some((file, line, text)) = event_flag {
        patterns.push(PatternInfo {
            name: "中断置标志 / 主循环处理".to_owned(),
            confidence: "中".to_owned(),
            evidence: format!("{file}:{line} · {text}"),
        });
    }
    if has_any(
        files,
        &[
            "taskENTER_CRITICAL",
            "taskEXIT_CRITICAL",
            "__disable_irq",
            "osMutex",
            "xSemaphoreTake",
        ],
    ) {
        let evidence = first_match(
            files,
            r"\b(?:taskENTER_CRITICAL|__disable_irq|osMutex\w*|xSemaphoreTake)\b",
        )
        .map(|(file, line, text)| format!("{file}:{line} · {text}"))
        .unwrap_or_default();
        patterns.push(PatternInfo {
            name: "临界区 / 互斥".to_owned(),
            confidence: "中".to_owned(),
            evidence,
        });
    }
    if let Some((file, line, text)) =
        first_match(files, r"\b(?:get_instance|[A-Za-z_]\w*_instance)\b")
    {
        patterns.push(PatternInfo {
            name: "单例 / 全局实例".to_owned(),
            confidence: "低".to_owned(),
            evidence: format!("{file}:{line} · {text}"),
        });
    }
    if let Some((file, line, text)) =
        first_match(files, r"\b[A-Za-z_]\w*_(?:create|new|alloc)\s*\(")
    {
        patterns.push(PatternInfo {
            name: "工厂 / 创建函数".to_owned(),
            confidence: "低".to_owned(),
            evidence: format!("{file}:{line} · {text}"),
        });
    }
    if first_match(files, r"\[\s*\]\s*=\s*\{").is_some() {
        if let Some((file, line, text)) = first_match(files, r"\(\s*\*\s*[A-Za-z_]\w*\s*\)\s*\(") {
            patterns.push(PatternInfo {
                name: "表驱动 / 函数指针表".to_owned(),
                confidence: "中".to_owned(),
                evidence: format!("{file}:{line} · {text}"),
            });
        }
    }
    if has_any(
        files,
        &[
            "switch (cmd",
            "switch (command",
            "switch (event",
            "switch (evt",
        ],
    ) {
        let evidence = first_match(
            files,
            r"(?i)\bswitch\s*\(\s*\w*(?:cmd|command|event|evt|msg|type)\w*\s*\)",
        )
        .map(|(file, line, text)| format!("{file}:{line} · {text}"))
        .unwrap_or_default();
        patterns.push(PatternInfo {
            name: "命令 / 事件分发".to_owned(),
            confidence: "中".to_owned(),
            evidence,
        });
    }
    if layers
        .iter()
        .any(|layer| layer.key == "hal" || layer.key == "cmsis" || layer.key == "middleware")
    {
        patterns.push(PatternInfo {
            name: "分层 / HAL 抽象".to_owned(),
            confidence: "高".to_owned(),
            evidence: "应用层与 CMSIS/HAL/中间件目录分离".to_owned(),
        });
    }
    let rank = |confidence: &str| match confidence {
        "高" => 0,
        "中" => 1,
        _ => 2,
    };
    patterns.sort_by_key(|pattern| rank(&pattern.confidence));
    patterns.truncate(MAX_PATTERNS);
    patterns
}

fn detect_state_machine(files: &[SourceFile]) -> Option<(String, usize, String)> {
    let switch = first_match(
        files,
        r"(?i)\bswitch\s*\(\s*\w*(?:state|mode|step|fsm|status)\w*\s*\)",
    );
    let enumeration = first_match(files, r"(?i)\benum\s+\w*(?:state|mode|step|fsm|status)\w*");
    match (enumeration, switch) {
        (Some((file, line, text)), _) => Some((file, line, text)),
        (_, Some(found)) => Some(found),
        _ => None,
    }
}

fn detect_ring_buffer(files: &[SourceFile]) -> Option<(String, usize, String)> {
    if let Some(found) = first_match(files, r"(?i)\b(?:ring|fifo|circular)[A-Za-z0-9_]*\b") {
        return Some(found);
    }
    let head_tail = first_match(files, r"(?i)\b(?:head|tail|read_idx|write_idx)\b");
    if head_tail.is_some() && has_any(files, &["buffer", "Buffer"]) {
        return head_tail;
    }
    None
}

fn detect_event_flag(
    files: &[SourceFile],
    callback_count: usize,
    has_main_loop: bool,
) -> Option<(String, usize, String)> {
    if callback_count == 0 || !has_main_loop {
        return None;
    }
    first_match(files, r"\bvolatile\b")
}

fn build_flow(
    interrupts: &[InterruptInfo],
    tasks: &[TaskInfo],
    main: Option<&MainLoop>,
    relation: Option<&RelationIndex>,
    function_set: &BTreeSet<String>,
) -> Vec<FlowStage> {
    let mut stages = Vec::new();
    if let Some(main) = main {
        let mut startup = vec!["复位 / 启动".to_owned(), "main".to_owned()];
        startup.extend(main.init_calls.iter().take(4).cloned());
        stages.push(FlowStage {
            title: "启动".to_owned(),
            nodes: startup,
        });
        if !tasks.is_empty() {
            stages.push(FlowStage {
                title: "RTOS 任务".to_owned(),
                nodes: tasks
                    .iter()
                    .take(MAX_FLOW_NODES)
                    .map(|task| format!("{} · {}", task.name, task.api))
                    .collect(),
            });
        } else if !main.loop_calls.is_empty() {
            stages.push(FlowStage {
                title: "主循环".to_owned(),
                nodes: main
                    .loop_calls
                    .iter()
                    .take(MAX_FLOW_NODES)
                    .map(|name| {
                        let mut node = name.clone();
                        if let Some(relation) = relation {
                            if let Some(callees) = relation.calls.get(name) {
                                let children = callees
                                    .iter()
                                    .filter(|callee| function_set.contains(*callee))
                                    .take(3)
                                    .cloned()
                                    .collect::<Vec<_>>();
                                if !children.is_empty() {
                                    node.push_str(" → ");
                                    node.push_str(&children.join("、"));
                                }
                            }
                        }
                        node
                    })
                    .collect(),
            });
        }
    } else if !tasks.is_empty() {
        stages.push(FlowStage {
            title: "RTOS 任务".to_owned(),
            nodes: tasks
                .iter()
                .take(MAX_FLOW_NODES)
                .map(|task| format!("{} · {}", task.name, task.api))
                .collect(),
        });
    }
    if !interrupts.is_empty() {
        stages.push(FlowStage {
            title: "中断".to_owned(),
            nodes: interrupts
                .iter()
                .take(5)
                .map(|interrupt| {
                    let mut node = interrupt.handler.clone();
                    for call in interrupt.calls.iter().take(2) {
                        node.push_str(" → ");
                        node.push_str(call);
                    }
                    node
                })
                .collect(),
        });
    }
    stages
}

fn build_roots(
    functions: &[FunctionDef],
    relation: Option<&RelationIndex>,
    tasks: &[TaskInfo],
    interrupts: &[InterruptInfo],
    function_set: &BTreeSet<String>,
) -> Vec<RootInfo> {
    let mut roots = Vec::new();
    let mut names = Vec::new();
    if functions.iter().any(|function| function.name == "main") {
        names.push(("main".to_owned(), "入口"));
    }
    for interrupt in interrupts.iter().take(6) {
        names.push((interrupt.handler.clone(), "中断"));
    }
    for task in tasks.iter().take(4) {
        names.push((task.name.clone(), "任务"));
    }
    let mut seen = BTreeSet::new();
    for (name, kind) in names {
        if !seen.insert(name.clone()) {
            continue;
        }
        let Some(index) = relation else {
            roots.push(RootInfo {
                name,
                kind: kind.to_owned(),
                count: 0,
                callees: Vec::new(),
            });
            continue;
        };
        let callees = index
            .calls
            .get(&name)
            .map(|callees| {
                callees
                    .iter()
                    .filter(|callee| function_set.contains(*callee))
                    .take(5)
                    .cloned()
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default();
        let count = index.calls.get(&name).map_or(0, |callees| callees.len());
        roots.push(RootInfo {
            name,
            kind: kind.to_owned(),
            count,
            callees,
        });
    }
    roots
}

fn has_any(files: &[SourceFile], needles: &[&str]) -> bool {
    files
        .iter()
        .any(|file| needles.iter().any(|needle| file.text.contains(needle)))
}

fn first_match(files: &[SourceFile], pattern: &str) -> Option<(String, usize, String)> {
    let regex = Regex::new(pattern).ok()?;
    for file in files {
        if let Some(found) = regex.find(&file.text) {
            return Some((
                file.rel.clone(),
                line_of(&file.text, found.start()),
                found.as_str().trim().to_owned(),
            ));
        }
    }
    None
}

fn line_of(text: &str, offset: usize) -> usize {
    text[..offset.min(text.len())]
        .bytes()
        .filter(|byte| *byte == b'\n')
        .count()
        + 1
}

#[derive(Clone, Debug, Default)]
struct FunctionBody {
    file: String,
    line: usize,
    body: String,
}

fn find_function_body(files: &[SourceFile], name: &str) -> Option<FunctionBody> {
    let pattern = format!(r"\b{}\s*\([^;{{}}]*\)\s*\{{", regex::escape(name));
    let regex = Regex::new(&pattern).ok()?;
    for file in files {
        let Some(found) = regex.find(&file.text) else {
            continue;
        };
        let start = found.start();
        let Some(open) = file.text[start..].find('{').map(|offset| offset + start) else {
            continue;
        };
        let Some(close) = block_end(&file.text, open) else {
            continue;
        };
        return Some(FunctionBody {
            file: file.rel.clone(),
            line: line_of(&file.text, start),
            body: file.text[open + 1..close].to_owned(),
        });
    }
    None
}

fn block_end(text: &str, open: usize) -> Option<usize> {
    let mut depth = 0usize;
    for (offset, character) in text[open..].char_indices() {
        match character {
            '{' => depth += 1,
            '}' => {
                depth = depth.saturating_sub(1);
                if depth == 0 {
                    return Some(open + offset);
                }
            }
            _ => {}
        }
    }
    None
}

fn extract_calls(text: &str) -> Vec<String> {
    let Ok(regex) = Regex::new(r"\b([A-Za-z_]\w*)\s*\(") else {
        return Vec::new();
    };
    let mut calls = Vec::new();
    for captures in regex.captures_iter(text) {
        let Some(name) = captures.get(1).map(|value| value.as_str()) else {
            continue;
        };
        if matches!(
            name,
            "if" | "for"
                | "while"
                | "switch"
                | "sizeof"
                | "return"
                | "do"
                | "else"
                | "case"
                | "defined"
                | "typeof"
                | "__attribute__"
        ) {
            continue;
        }
        if !calls.iter().any(|existing| existing == name) {
            calls.push(name.to_owned());
        }
    }
    calls
}

fn call_snippet(text: &str, start: usize) -> Option<String> {
    let open = text[start..].find('(')? + start;
    let mut depth = 0usize;
    for (offset, character) in text[open..].char_indices() {
        match character {
            '(' => depth += 1,
            ')' => {
                depth = depth.saturating_sub(1);
                if depth == 0 {
                    return Some(text[start..open + offset + 1].to_owned());
                }
            }
            _ => {}
        }
    }
    None
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

    fn analyze_fixture(text: &str) -> ArchitectureReport {
        let dir = tempfile::tempdir().unwrap();
        write(&dir.path().join("Core/Src/main.c"), text);
        write(
            &dir.path().join("Core/Src/stm32f1xx_it.c"),
            "void USART1_IRQHandler(void) { HAL_UART_IRQHandler(0); }\n",
        );
        analyze(dir.path(), None, &IgnoreRules::default()).unwrap()
    }

    #[test]
    fn detects_bare_metal_loop_and_interrupts() {
        let report = analyze_fixture(
            "int main(void) {\n  HAL_Init();\n  MX_GPIO_Init();\n  while (1) {\n    app_echo_poll();\n    HAL_Delay(10);\n  }\n}\n",
        );
        assert!(report.schedule.kind.contains("裸机"));
        assert_eq!(report.interrupts.len(), 1);
        assert!(report
            .framework
            .iter()
            .any(|item| item.name.contains("前后台")));
        assert!(report.flow.iter().any(|stage| stage.title == "主循环"));
    }

    #[test]
    fn detects_rtos_tasks_and_queues() {
        let report = analyze_fixture(
            "void app_task(void *arg) { (void)arg; for (;;) { app_echo_poll(); } }\nint main(void) {\n  osThreadNew(app_task, 0, 0);\n  osKernelStart();\n}\n",
        );
        assert!(report.schedule.kind.contains("RTOS"));
        assert_eq!(report.tasks.len(), 1);
        assert!(report
            .patterns
            .iter()
            .any(|pattern| pattern.name.contains("多任务")));
        assert!(report
            .framework
            .iter()
            .any(|item| item.name == "RTOS 任务框架"));
        assert!(!report
            .framework
            .iter()
            .any(|item| item.name == "前后台框架"));
    }

    #[test]
    fn associates_hal_callback_and_event_driven_framework() {
        let report = analyze_fixture(
            "static volatile int uart_flag;\nvoid HAL_UART_RxCpltCallback(void *huart) { (void)huart; uart_flag = 1; }\nint main(void) {\n  while (1) { if (uart_flag) { uart_flag = 0; } }\n}\n",
        );
        assert!(report.framework.iter().any(|item| item.name == "事件驱动"));
        assert!(report.framework.iter().any(|item| item.name == "回调分发"));
        assert!(report
            .patterns
            .iter()
            .any(|pattern| pattern.name.contains("回调")));
        let interrupt = report
            .interrupts
            .iter()
            .find(|interrupt| interrupt.handler == "USART1_IRQHandler")
            .unwrap();
        assert!(interrupt
            .calls
            .iter()
            .any(|call| call == "HAL_UART_RxCpltCallback"));
    }

    #[test]
    fn detects_state_machine_and_layers() {
        let report = analyze_fixture(
            "typedef enum { IDLE, RUN } app_state_t;\nstatic app_state_t state;\nvoid app_task(void) {\n  switch (state) { case IDLE: state = RUN; break; default: break; }\n}\nint main(void) { while (1) { app_task(); } }\n",
        );
        assert!(report
            .patterns
            .iter()
            .any(|pattern| pattern.name == "状态机"));
        assert!(report.layers.iter().any(|layer| layer.name == "应用"));
    }
}
