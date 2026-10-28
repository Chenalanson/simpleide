//! Static project resource report (hardware, software and application info).
//!
//! The report is intentionally file-based: it never changes project settings
//! and writes one JSON document to the project root. It combines the project
//! metadata already used by the IDE with a bounded source-tree inventory, SVD
//! registers, `.ioc` pin assignments and optional firmware statistics.

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};
use std::sync::Arc;

use serde::Serialize;

use crate::config::{BuildConfig, BuildSystem, ProjectDebugSettings};
use crate::ignore::IgnoreRules;
use crate::memory::{self, FirmwareAnalysis, MemoryUsage, ResolvedMemory};
use crate::project_meta::ProjectMeta;
use crate::registers::{self, SvdPeripheral};
use crate::relations::RelationIndex;
use crate::support_pack::SupportPack;
use crate::symbols::{self, Symbol, SymbolKind};
use crate::util::{display_path, file_name, read_text_file};

const SCHEMA: &str = "simple-embedded.resources";
const SCHEMA_VERSION: u32 = 1;
const MAX_SCAN_FILES: usize = 20_000;
const MAX_TEXT_FILE_BYTES: u64 = 2 * 1024 * 1024;
const MAX_SOURCE_LIST: usize = 500;
const MAX_LIBRARY_LIST: usize = 200;
const MAX_SCRIPT_LIST: usize = 50;
const MAX_IOC_FILES: usize = 20;
const MAX_PIN_ROWS: usize = 500;

pub struct ReportRequest {
    pub root: PathBuf,
    pub project_name: String,
    pub meta: ProjectMeta,
    pub build: BuildConfig,
    pub debug: ProjectDebugSettings,
    pub memory_layout: Option<ResolvedMemory>,
    pub memory_usage: Option<MemoryUsage>,
    pub support_packs: Vec<SupportPack>,
    pub registers: Option<Vec<SvdPeripheral>>,
    pub register_source: Option<String>,
    pub registers_truncated: bool,
    pub symbols: Option<Vec<Symbol>>,
    pub symbols_truncated: bool,
    pub relations: Option<Arc<RelationIndex>>,
    pub firmware_path: Option<PathBuf>,
    pub firmware_analysis: Option<FirmwareAnalysis>,
    pub include_dirs: Vec<String>,
    pub library_dirs: Vec<String>,
    pub defines: Vec<String>,
    pub respect_gitignore: bool,
    pub ignore_patterns: String,
}

#[derive(Serialize)]
pub struct ResourceReport {
    pub schema: &'static str,
    pub schema_version: u32,
    pub generated_at: String,
    pub application: ApplicationInfo,
    pub project: ProjectInfo,
    pub hardware: HardwareInfo,
    pub software: SoftwareInfo,
    pub system: SystemInfo,
    pub notes: Vec<String>,
    pub warnings: Vec<String>,
}

#[derive(Serialize)]
pub struct ApplicationInfo {
    pub name: String,
    pub version: String,
    pub author: String,
    pub executable: String,
    pub profile: String,
}

#[derive(Serialize)]
pub struct ProjectInfo {
    pub name: String,
    pub root: String,
    pub settings_file: String,
    pub settings_exists: bool,
    pub workspace_file: Option<String>,
    pub build_system: String,
    pub toolchain: String,
    pub target: String,
    pub target_name: Option<String>,
    pub device: Option<String>,
    pub compiler: Option<String>,
    pub optimization: Option<String>,
    pub output_dir: Option<String>,
    pub artifact: Option<String>,
    pub include_dirs: Vec<String>,
    pub library_dirs: Vec<String>,
    pub defines: Vec<String>,
    pub build: BuildConfig,
    pub debug: ProjectDebugSettings,
}

#[derive(Serialize)]
pub struct HardwareInfo {
    pub device: Option<String>,
    pub cpu: Option<String>,
    pub fpu: Option<String>,
    pub memory_layout: Option<String>,
    pub linker_script: Option<String>,
    pub memory: MemoryInfo,
    pub firmware: Option<FirmwareInfo>,
    pub peripherals: Vec<PeripheralInfo>,
    pub peripheral_count: usize,
    pub register_count: usize,
    pub register_source: Option<String>,
    pub registers_truncated: bool,
    pub used_components: Vec<String>,
    pub ioc: Option<IocInfo>,
}

#[derive(Serialize)]
pub struct MemoryInfo {
    pub flash: Option<MemoryRegionInfo>,
    pub ram: Option<MemoryRegionInfo>,
    pub source: Option<String>,
    pub usage: Option<MemoryUsageInfo>,
    pub regions: Vec<crate::config::MemoryRegionConfig>,
}

#[derive(Serialize)]
pub struct MemoryRegionInfo {
    pub name: String,
    pub start: String,
    pub size: String,
    pub bytes: u64,
}

#[derive(Serialize)]
pub struct MemoryUsageInfo {
    pub flash_used: u64,
    pub flash_total: u64,
    pub flash_percent: f32,
    pub ram_used: u64,
    pub ram_total: u64,
    pub ram_percent: f32,
}

#[derive(Serialize)]
pub struct FirmwareInfo {
    pub path: String,
    pub format: String,
    pub file_size: u64,
    pub modified_unix: u64,
    pub data_size: u64,
    pub bss_size: u64,
    pub stack_top: Option<String>,
    pub stack_size: u64,
    pub heap_size: u64,
    pub heap_kind: Option<String>,
    pub has_symbols: bool,
    pub globals: Vec<GlobalInfo>,
    pub analysis_error: Option<String>,
}

#[derive(Serialize)]
pub struct GlobalInfo {
    pub name: String,
    pub address: String,
    pub size: u64,
    pub section: String,
}

#[derive(Serialize)]
pub struct PeripheralInfo {
    pub name: String,
    pub base: String,
    pub description: String,
    pub register_count: usize,
    pub registers: Vec<RegisterInfo>,
}

#[derive(Serialize)]
pub struct RegisterInfo {
    pub name: String,
    pub address: String,
    pub offset: String,
    pub size: u32,
    pub access: String,
    pub description: String,
    pub field_count: usize,
}

#[derive(Serialize)]
pub struct IocInfo {
    pub files: Vec<String>,
    pub device: Option<String>,
    pub components: Vec<String>,
    pub clock: BTreeMap<String, String>,
    pub pins: Vec<PinInfo>,
}

#[derive(Serialize)]
pub struct PinInfo {
    pub pin: String,
    pub signal: String,
    pub label: Option<String>,
    pub source: String,
}

#[derive(Serialize)]
pub struct SoftwareInfo {
    pub languages: Vec<String>,
    pub source_files: Vec<String>,
    pub header_files: Vec<String>,
    pub python_files: Vec<String>,
    pub linker_scripts: Vec<String>,
    pub startup_files: Vec<String>,
    pub libraries: Vec<String>,
    pub components: Vec<ComponentInfo>,
    pub counts: FileCounts,
    pub symbols: Option<SymbolSummary>,
    pub call_graph: Option<CallGraphInfo>,
    pub support_packs: Vec<SupportPackInfo>,
}

#[derive(Serialize)]
pub struct FileCounts {
    pub source_files: usize,
    pub header_files: usize,
    pub total_files: usize,
    pub total_bytes: u64,
    pub source_lines: u64,
    pub header_lines: u64,
    pub truncated: bool,
}

#[derive(Serialize)]
pub struct ComponentInfo {
    pub name: String,
    pub category: String,
    pub evidence: Vec<String>,
}

#[derive(Serialize)]
pub struct SymbolSummary {
    pub functions: usize,
    pub structs: usize,
    pub macros: usize,
    pub labels: usize,
    pub total: usize,
    pub truncated: bool,
}

#[derive(Serialize)]
pub struct CallGraphInfo {
    pub functions: usize,
    pub call_edges: usize,
    pub caller_links: usize,
    pub reference_symbols: usize,
    pub top_functions: Vec<CallRank>,
}

#[derive(Serialize)]
pub struct CallRank {
    pub name: String,
    pub callers: usize,
    pub callees: usize,
}

#[derive(Serialize)]
pub struct SupportPackInfo {
    pub name: String,
    pub vendor: Option<String>,
    pub version: Option<String>,
    pub file: String,
    pub device_count: usize,
    pub devices: Vec<String>,
    pub svd_count: usize,
    pub error: Option<String>,
}

#[derive(Serialize)]
pub struct SystemInfo {
    pub os: String,
    pub arch: String,
    pub profile: String,
    pub executable: String,
    pub report_file: String,
    pub scan: ScanInfo,
}

#[derive(Serialize)]
pub struct ScanInfo {
    pub files: usize,
    pub total_bytes: u64,
    pub truncated: bool,
    pub source_lines: u64,
    pub header_lines: u64,
}

/// Analyze the project and write `<project>.resources.json` in its root.
pub fn generate_and_write(request: ReportRequest) -> Result<PathBuf, String> {
    let ignore = IgnoreRules::load(
        &request.root,
        request.respect_gitignore,
        &request.ignore_patterns,
    );
    let inventory = scan_inventory(&request.root, &ignore);
    let mut warnings = Vec::new();

    let ioc = analyze_ioc(&inventory.ioc_files, &request.root);
    if ioc.is_none() {
        warnings.push("未找到 .ioc 文件，未分析 CubeMX 引脚分配".to_owned());
    }
    let device = request
        .meta
        .device
        .clone()
        .or_else(|| ioc.as_ref().and_then(|ioc| ioc.device.clone()));
    let register_data = load_registers(&request, &ignore, device.as_deref(), &mut warnings);
    let symbol_summary = load_symbols(&request, &ignore, &mut warnings);
    let call_graph = request
        .relations
        .as_ref()
        .map(|index| call_graph_info(index));
    if call_graph.is_none() {
        warnings.push("调用 / 引用索引尚未就绪，报告未包含调用图统计".to_owned());
    }
    let memory = build_memory_info(&request);
    let firmware = build_firmware_info(&request, &mut warnings);
    let software = build_software_info(
        &request,
        &inventory,
        symbol_summary,
        call_graph,
        &mut warnings,
    );

    if device.is_none() {
        warnings.push("未识别芯片型号，硬件信息可能不完整".to_owned());
    }
    if request.memory_layout.is_none() {
        warnings.push("未解析 Flash / RAM 布局".to_owned());
    }
    if register_data.source.is_none() {
        warnings.push("未找到 SVD / 寄存器定义".to_owned());
    }
    if register_data.peripheral_count == 0 {
        warnings.push("未找到可用于报告的 SVD 外设 / 寄存器信息".to_owned());
    }
    if firmware.is_none() {
        warnings.push("未找到可分析的固件产物（ELF / AXF / EXE）".to_owned());
    }
    if inventory.truncated {
        warnings.push("文件清单超过扫描上限，列表已截断".to_owned());
    }
    if inventory.unreadable_files > 0 {
        warnings.push(format!(
            "有 {} 个源码文件无法读取，行数统计可能偏小",
            inventory.unreadable_files
        ));
    }
    for pack in &request.support_packs {
        if let Some(error) = &pack.error {
            warnings.push(format!("支持包 {} 解析失败：{error}", pack.name));
        }
    }

    let report_file = format!("{}.resources.json", safe_file_name(&request.project_name));
    let generated_at = crate::util::local_datetime();
    let output = request.root.join(&report_file);
    let application = ApplicationInfo {
        name: crate::config::APP_NAME.to_owned(),
        version: env!("CARGO_PKG_VERSION").to_owned(),
        author: crate::config::APP_AUTHOR.to_owned(),
        executable: std::env::current_exe()
            .map(|path| display_path(&path))
            .unwrap_or_default(),
        profile: if cfg!(debug_assertions) {
            "debug".to_owned()
        } else {
            "release".to_owned()
        },
    };
    let project = build_project_info(&request);
    let hardware = HardwareInfo {
        device,
        cpu: request.meta.cpu.clone(),
        fpu: request.meta.fpu.clone(),
        memory_layout: request.meta.memory_layout.clone(),
        linker_script: request.meta.linker_script.clone(),
        memory,
        firmware,
        peripherals: register_data.peripherals,
        peripheral_count: register_data.peripheral_count,
        register_count: register_data.register_count,
        register_source: register_data.source,
        registers_truncated: register_data.truncated,
        used_components: ioc
            .as_ref()
            .map(|ioc| ioc.components.clone())
            .unwrap_or_default(),
        ioc,
    };
    let system = SystemInfo {
        os: std::env::consts::OS.to_owned(),
        arch: std::env::consts::ARCH.to_owned(),
        profile: if cfg!(debug_assertions) {
            "debug".to_owned()
        } else {
            "release".to_owned()
        },
        executable: std::env::current_exe()
            .map(|path| display_path(&path))
            .unwrap_or_default(),
        report_file,
        scan: ScanInfo {
            files: inventory.scanned_files,
            total_bytes: inventory.total_bytes,
            truncated: inventory.truncated,
            source_lines: inventory.source_lines,
            header_lines: inventory.header_lines,
        },
    };

    let report = ResourceReport {
        schema: SCHEMA,
        schema_version: SCHEMA_VERSION,
        generated_at,
        application,
        project,
        hardware,
        software,
        system,
        notes: vec![
            "本报告由工程文件、Makefile / Keil 元数据、SVD、.ioc 和可选固件静态分析生成。"
                .to_owned(),
            "引脚和组件信息代表工程配置，不等同于实际电路连接或物料清单。".to_owned(),
            "报告读取已保存文件和当前索引；未保存的编辑不会进入源码清单或符号统计。".to_owned(),
            "扫描跳过 build / Debug / Release / out / target / Objects / Listings / tools / dist 等生成或工具目录。"
                .to_owned(),
        ],
        warnings,
    };
    let mut bytes =
        serde_json::to_vec_pretty(&report).map_err(|err| format!("无法序列化资源报告：{err}"))?;
    bytes.push(b'\n');
    std::fs::write(&output, bytes)
        .map_err(|err| format!("无法写入 {}：{err}", display_path(&output)))?;
    Ok(output)
}

fn build_project_info(request: &ReportRequest) -> ProjectInfo {
    let settings = request
        .root
        .join(crate::config::PROJECT_CONFIG_DIR)
        .join(crate::config::PROJECT_CONFIG_FILE);
    let workspace_file = [
        request
            .root
            .join(format!("{}.se-workspace", request.project_name)),
        request
            .root
            .join(format!("{}.code-workspace", request.project_name)),
    ]
    .into_iter()
    .find(|path| path.is_file())
    .map(|path| display_path(&path));
    let build_system = match request.build.effective_build_system() {
        BuildSystem::Make => "Make",
        BuildSystem::Keil => "Keil MDK",
        BuildSystem::Cargo => "Cargo",
        BuildSystem::CMake => "CMake",
        BuildSystem::Other => "Other",
    };
    ProjectInfo {
        name: request.project_name.clone(),
        root: display_path(&request.root),
        settings_file: display_path(&settings),
        settings_exists: settings.is_file(),
        workspace_file,
        build_system: build_system.to_owned(),
        toolchain: request
            .build
            .effective_toolchain(&request.meta)
            .label()
            .to_owned(),
        target: match request.build.target {
            crate::config::BuildTarget::Local => "local",
            crate::config::BuildTarget::Ssh => "ssh",
        }
        .to_owned(),
        target_name: request.meta.target.clone(),
        device: request.meta.device.clone(),
        compiler: request.meta.compiler.clone(),
        optimization: request.meta.optimization.clone(),
        output_dir: request.meta.output_dir.clone(),
        artifact: request.meta.artifact.clone(),
        include_dirs: request.include_dirs.clone(),
        library_dirs: request.library_dirs.clone(),
        defines: request.defines.clone(),
        build: request.build.clone(),
        debug: request.debug.clone(),
    }
}

fn build_memory_info(request: &ReportRequest) -> MemoryInfo {
    let layout = request.memory_layout.as_ref();
    let flash = layout
        .map(|memory| memory.layout.flash)
        .filter(|region| region.is_valid())
        .map(|region| MemoryRegionInfo {
            name: "Flash".to_owned(),
            start: memory::format_hex(region.start),
            size: memory::format_capacity(region.size),
            bytes: region.size,
        });
    let ram = layout
        .map(|memory| memory.layout.ram)
        .filter(|region| region.is_valid())
        .map(|region| MemoryRegionInfo {
            name: "RAM".to_owned(),
            start: memory::format_hex(region.start),
            size: memory::format_capacity(region.size),
            bytes: region.size,
        });
    let usage = request.memory_usage.map(|usage| {
        let flash_total = flash.as_ref().map_or(0, |region| region.bytes);
        let ram_total = ram.as_ref().map_or(0, |region| region.bytes);
        MemoryUsageInfo {
            flash_used: usage.flash_used,
            flash_total,
            flash_percent: percentage(usage.flash_used, flash_total),
            ram_used: usage.ram_used,
            ram_total,
            ram_percent: percentage(usage.ram_used, ram_total),
        }
    });
    let regions = if request.build.memory.regions.is_empty() {
        memory::default_regions(request.memory_layout.as_ref().map(|memory| memory.layout))
    } else {
        request.build.memory.regions.clone()
    };
    MemoryInfo {
        flash,
        ram,
        source: layout.map(|memory| memory.source.clone()),
        usage,
        regions,
    }
}

fn percentage(used: u64, total: u64) -> f32 {
    if total == 0 {
        0.0
    } else {
        (used as f64 * 100.0 / total as f64) as f32
    }
}

fn build_firmware_info(
    request: &ReportRequest,
    warnings: &mut Vec<String>,
) -> Option<FirmwareInfo> {
    let path = request.firmware_path.as_ref()?;
    let metadata = std::fs::metadata(path).ok();
    let modified_unix = metadata
        .as_ref()
        .and_then(|metadata| metadata.modified().ok())
        .and_then(|time| time.duration_since(std::time::UNIX_EPOCH).ok())
        .map(|duration| duration.as_secs())
        .unwrap_or(0);
    let format = path
        .extension()
        .map(|extension| extension.to_string_lossy().to_ascii_uppercase())
        .unwrap_or_else(|| "BIN".to_owned());
    let analysis = request
        .firmware_analysis
        .clone()
        .or_else(|| memory::analyze_elf(path));
    let analysis_error = analysis.is_none().then(|| {
        warnings.push(format!("无法解析固件 {} 的 ELF 符号", display_path(path)));
        "ELF 符号解析失败".to_owned()
    });
    let analysis = analysis.unwrap_or_default();
    Some(FirmwareInfo {
        path: display_path(path),
        format,
        file_size: metadata.map(|metadata| metadata.len()).unwrap_or(0),
        modified_unix,
        data_size: analysis.data_size,
        bss_size: analysis.bss_size,
        stack_top: analysis.stack_top.map(memory::format_hex),
        stack_size: analysis.stack_size.unwrap_or(0),
        heap_size: analysis.heap_size.unwrap_or(0),
        heap_kind: analysis.heap_kind,
        has_symbols: analysis.has_symbols,
        globals: analysis
            .globals
            .into_iter()
            .take(20)
            .map(|global| GlobalInfo {
                name: global.name,
                address: memory::format_hex(global.address),
                size: global.size,
                section: global.section,
            })
            .collect(),
        analysis_error,
    })
}

fn load_registers(
    request: &ReportRequest,
    ignore: &IgnoreRules,
    device: Option<&str>,
    warnings: &mut Vec<String>,
) -> RegisterData {
    let (peripherals, source, truncated) = if let Some(peripherals) = &request.registers {
        (
            peripherals.clone(),
            request.register_source.clone(),
            request.registers_truncated,
        )
    } else {
        match registers::scan_project(&request.root, ignore, device) {
            Ok(index) => (index.peripherals, Some(index.source), index.truncated),
            Err(err) => {
                warnings.push(format!("寄存器索引失败：{err}"));
                (Vec::new(), None, false)
            }
        }
    };
    let register_count = peripherals
        .iter()
        .map(|peripheral| peripheral.registers.len())
        .sum();
    RegisterData {
        peripheral_count: peripherals.len(),
        peripherals: peripherals
            .into_iter()
            .map(|peripheral| {
                let base = peripheral.base;
                PeripheralInfo {
                    name: peripheral.name,
                    base: memory::format_hex(base),
                    description: peripheral.description,
                    register_count: peripheral.registers.len(),
                    registers: peripheral
                        .registers
                        .into_iter()
                        .map(|register| RegisterInfo {
                            name: register.name,
                            address: memory::format_hex(base.wrapping_add(register.offset)),
                            offset: format!("0x{:X}", register.offset),
                            size: register.size,
                            access: register.access,
                            description: register.description,
                            field_count: register.fields.len(),
                        })
                        .collect(),
                }
            })
            .collect(),
        register_count,
        source,
        truncated,
    }
}

struct RegisterData {
    peripherals: Vec<PeripheralInfo>,
    peripheral_count: usize,
    register_count: usize,
    source: Option<String>,
    truncated: bool,
}

fn load_symbols(
    request: &ReportRequest,
    ignore: &IgnoreRules,
    warnings: &mut Vec<String>,
) -> Option<SymbolSummary> {
    if let Some(symbols) = &request.symbols {
        return Some(summarize_symbols(symbols, request.symbols_truncated));
    }
    match symbols::scan_project(&request.root, ignore) {
        Ok(outcome) => Some(summarize_symbols(&outcome.symbols, outcome.truncated)),
        Err(err) => {
            warnings.push(format!("符号索引失败：{err}"));
            None
        }
    }
}

fn summarize_symbols(symbols: &[Symbol], truncated: bool) -> SymbolSummary {
    let mut summary = SymbolSummary {
        functions: 0,
        structs: 0,
        macros: 0,
        labels: 0,
        total: symbols.len(),
        truncated,
    };
    for symbol in symbols {
        match symbol.kind {
            SymbolKind::Function => summary.functions += 1,
            SymbolKind::Struct => summary.structs += 1,
            SymbolKind::Macro => summary.macros += 1,
            SymbolKind::Label => summary.labels += 1,
        }
    }
    summary
}

fn call_graph_info(index: &RelationIndex) -> CallGraphInfo {
    let mut ranked = index
        .functions
        .keys()
        .map(|name| CallRank {
            name: name.clone(),
            callers: index.callers.get(name).map_or(0, |set| set.len()),
            callees: index.calls.get(name).map_or(0, |set| set.len()),
        })
        .collect::<Vec<_>>();
    ranked.sort_by(|left, right| {
        (right.callers + right.callees)
            .cmp(&(left.callers + left.callees))
            .then_with(|| left.name.cmp(&right.name))
    });
    ranked.truncate(10);
    CallGraphInfo {
        functions: index.functions.len(),
        call_edges: index.calls.values().map(|set| set.len()).sum(),
        caller_links: index.callers.len(),
        reference_symbols: index.references.len(),
        top_functions: ranked,
    }
}

fn build_software_info(
    request: &ReportRequest,
    inventory: &Inventory,
    symbols: Option<SymbolSummary>,
    call_graph: Option<CallGraphInfo>,
    warnings: &mut Vec<String>,
) -> SoftwareInfo {
    let components = detect_components(inventory);
    let support_packs = request
        .support_packs
        .iter()
        .map(|pack| SupportPackInfo {
            name: pack.name.clone(),
            vendor: pack.vendor.clone(),
            version: pack.version.clone(),
            file: display_path(&pack.path),
            device_count: pack.devices.len(),
            devices: pack.devices.iter().take(10).cloned().collect(),
            svd_count: pack.svd_count,
            error: pack.error.clone(),
        })
        .collect();
    if symbols.is_none() {
        warnings.push("未生成软件符号统计".to_owned());
    }
    SoftwareInfo {
        languages: inventory.languages.iter().cloned().collect(),
        source_files: inventory.source_files.clone(),
        header_files: inventory.header_files.clone(),
        python_files: inventory.python_files.clone(),
        linker_scripts: inventory.linker_scripts.clone(),
        startup_files: inventory.startup_files.clone(),
        libraries: inventory.libraries.clone(),
        components,
        counts: FileCounts {
            source_files: inventory.source_file_total,
            header_files: inventory.header_file_total,
            total_files: inventory.scanned_files,
            total_bytes: inventory.total_bytes,
            source_lines: inventory.source_lines,
            header_lines: inventory.header_lines,
            truncated: inventory.truncated,
        },
        symbols,
        call_graph,
        support_packs,
    }
}

fn detect_components(inventory: &Inventory) -> Vec<ComponentInfo> {
    const SPECS: &[(&str, &str, &[&str])] = &[
        ("FreeRTOS", "RTOS", &["freertos", "tasks.c", "queue.c"]),
        ("CMSIS", "Framework", &["cmsis", "core_cm"]),
        ("STM32 HAL", "HAL", &["_hal.c", "_hal.h", "hal_driver"]),
        ("STM32 LL", "HAL", &["_ll.c", "_ll.h", "ll_driver"]),
        ("STM32CubeMX", "Tooling", &[".ioc"]),
        ("Keil MDK", "Build", &[".uvprojx"]),
        ("Make", "Build", &["makefile"]),
        ("CMake", "Build", &["cmakelists.txt"]),
        ("Cargo", "Build", &["cargo.toml"]),
        ("USB", "Middleware", &["usb", "tinyusb", "usbd"]),
        ("FatFS", "Middleware", &["fatfs", "ff.c", "ffconf.h"]),
        ("LittleFS", "Middleware", &["littlefs"]),
        ("LwIP", "Middleware", &["lwip"]),
        ("mbedTLS", "Security", &["mbedtls"]),
        ("FreeModbus", "Protocol", &["freemodbus", "modbus"]),
        ("SEGGER RTT", "Debug", &["segger_rtt", "rtt/"]),
        ("LVGL", "GUI", &["lvgl"]),
        ("u8g2", "GUI", &["u8g2"]),
        ("Arduino", "Framework", &["arduino"]),
        ("ESP-IDF", "Framework", &["esp-idf", "esp_idf"]),
        ("Zephyr", "RTOS", &["zephyr"]),
        ("RT-Thread", "RTOS", &["rt-thread", "rtthread"]),
        ("ChibiOS", "RTOS", &["chibios"]),
        ("ThreadX", "RTOS", &["threadx", "tx_api.h"]),
        ("MicroPython", "Runtime", &["main.py", "boot.py"]),
    ];
    let mut found: BTreeMap<&str, (&str, Vec<String>)> = BTreeMap::new();
    for (lower, original) in &inventory.paths {
        for (name, category, keywords) in SPECS {
            if !keywords.iter().any(|keyword| lower.contains(keyword)) {
                continue;
            }
            let entry = found.entry(name).or_insert_with(|| (category, Vec::new()));
            if entry.1.len() < 5 && !entry.1.iter().any(|item| item == original) {
                entry.1.push(original.clone());
            }
        }
    }
    found
        .into_iter()
        .map(|(name, (category, evidence))| ComponentInfo {
            name: name.to_owned(),
            category: category.to_owned(),
            evidence,
        })
        .collect()
}

fn analyze_ioc(files: &[PathBuf], root: &Path) -> Option<IocInfo> {
    if files.is_empty() {
        return None;
    }
    let mut ioc_files = Vec::new();
    let mut device = None;
    let mut components = BTreeSet::new();
    let mut clock = BTreeMap::new();
    let mut pins = BTreeMap::new();
    for path in files.iter().take(MAX_IOC_FILES) {
        let Ok(loaded) = read_text_file(path) else {
            continue;
        };
        let relative = path
            .strip_prefix(root)
            .unwrap_or(path)
            .to_string_lossy()
            .replace('\\', "/");
        ioc_files.push(relative.clone());
        let mut labels = BTreeMap::new();
        for line in loaded.text.lines() {
            let line = line.trim();
            if line.is_empty() || line.starts_with('#') {
                continue;
            }
            let Some((key, value)) = line.split_once('=') else {
                continue;
            };
            let key = key.trim();
            let value = value.trim();
            if key.starts_with("Mcu.IP")
                && !value.is_empty()
                && value
                    .chars()
                    .next()
                    .is_some_and(|character| character.is_ascii_alphabetic() || character == '_')
            {
                components.insert(value.to_owned());
            }
            if matches!(key, "ProjectManager.DeviceId" | "Mcu.DeviceId") && !value.is_empty() {
                device = Some(value.to_owned());
            }
            if key.starts_with("RCC.") && !value.is_empty() {
                clock.insert(key.to_owned(), value.to_owned());
            }
            if let Some(pin) = key.strip_suffix(".GPIO_Label") {
                labels.insert(pin.to_owned(), value.to_owned());
            }
        }
        for line in loaded.text.lines() {
            let line = line.trim();
            if line.is_empty() || line.starts_with('#') {
                continue;
            }
            let Some((key, value)) = line.split_once('=') else {
                continue;
            };
            let key = key.trim();
            let Some(pin) = key.strip_suffix(".Signal") else {
                continue;
            };
            let value = value.trim();
            if value.is_empty() {
                continue;
            }
            pins.insert(
                pin.to_owned(),
                PinInfo {
                    pin: pin.to_owned(),
                    signal: value.to_owned(),
                    label: labels.get(pin).cloned(),
                    source: relative.clone(),
                },
            );
        }
    }
    Some(IocInfo {
        files: ioc_files,
        device,
        components: components.into_iter().collect(),
        clock,
        pins: pins.into_values().take(MAX_PIN_ROWS).collect(),
    })
}

struct Inventory {
    source_files: Vec<String>,
    header_files: Vec<String>,
    python_files: Vec<String>,
    linker_scripts: Vec<String>,
    startup_files: Vec<String>,
    libraries: Vec<String>,
    ioc_files: Vec<PathBuf>,
    languages: BTreeSet<String>,
    paths: Vec<(String, String)>,
    source_file_total: usize,
    header_file_total: usize,
    scanned_files: usize,
    total_bytes: u64,
    source_lines: u64,
    header_lines: u64,
    truncated: bool,
    unreadable_files: usize,
}

fn scan_inventory(root: &Path, ignore: &IgnoreRules) -> Inventory {
    let mut inventory = Inventory {
        source_files: Vec::new(),
        header_files: Vec::new(),
        python_files: Vec::new(),
        linker_scripts: Vec::new(),
        startup_files: Vec::new(),
        libraries: Vec::new(),
        ioc_files: Vec::new(),
        languages: BTreeSet::new(),
        paths: Vec::new(),
        source_file_total: 0,
        header_file_total: 0,
        scanned_files: 0,
        total_bytes: 0,
        source_lines: 0,
        header_lines: 0,
        truncated: false,
        unreadable_files: 0,
    };
    let mut stack = vec![root.to_path_buf()];
    while let Some(directory) = stack.pop() {
        let Ok(read_dir) = std::fs::read_dir(&directory) else {
            continue;
        };
        let mut entries = read_dir.flatten().collect::<Vec<_>>();
        entries.sort_by_key(|entry| entry.file_name());
        for entry in entries {
            let path = entry.path();
            let Ok(file_type) = entry.file_type() else {
                continue;
            };
            if file_type.is_symlink() {
                continue;
            }
            let relative = path.strip_prefix(root).unwrap_or(path.as_path());
            if file_type.is_dir() {
                if !ignore.is_ignored(relative, true) && !skip_directory(relative) {
                    stack.push(path);
                }
                continue;
            }
            if !file_type.is_file() || ignore.is_ignored(relative, false) {
                continue;
            }
            if inventory.scanned_files >= MAX_SCAN_FILES {
                inventory.truncated = true;
                break;
            }
            inventory.scanned_files += 1;
            let Ok(metadata) = entry.metadata() else {
                continue;
            };
            inventory.total_bytes = inventory.total_bytes.saturating_add(metadata.len());
            let relative = relative.to_string_lossy().replace('\\', "/");
            let lower = relative.to_ascii_lowercase();
            inventory.paths.push((lower.clone(), relative.clone()));
            let extension = path
                .extension()
                .map(|value| value.to_string_lossy().to_ascii_lowercase())
                .unwrap_or_default();
            let file_name = file_name(&path).to_ascii_lowercase();
            let is_header = matches!(extension.as_str(), "h" | "hh" | "hpp" | "hxx");
            let is_source = matches!(
                extension.as_str(),
                "c" | "cc" | "cpp" | "cxx" | "ino" | "s" | "asm"
            );
            let is_python = extension == "py";
            if is_header {
                inventory.header_file_total += 1;
                inventory.languages.insert("C/C++".to_owned());
                inventory.header_lines += count_lines(&path, &mut inventory.unreadable_files);
                if inventory.header_files.len() < MAX_SOURCE_LIST {
                    inventory.header_files.push(relative.clone());
                } else {
                    inventory.truncated = true;
                }
            } else if is_source {
                inventory.source_file_total += 1;
                inventory
                    .languages
                    .insert(if matches!(extension.as_str(), "s" | "asm") {
                        "Assembly".to_owned()
                    } else if matches!(extension.as_str(), "c" | "ino") {
                        "C".to_owned()
                    } else {
                        "C++".to_owned()
                    });
                inventory.source_lines += count_lines(&path, &mut inventory.unreadable_files);
                if inventory.source_files.len() < MAX_SOURCE_LIST {
                    inventory.source_files.push(relative.clone());
                } else {
                    inventory.truncated = true;
                }
                if file_name.contains("startup") || file_name.starts_with("system_") {
                    if inventory.startup_files.len() < MAX_SCRIPT_LIST {
                        inventory.startup_files.push(relative.clone());
                    } else {
                        inventory.truncated = true;
                    }
                }
            } else if is_python {
                inventory.languages.insert("Python".to_owned());
                if inventory.python_files.len() < MAX_SOURCE_LIST {
                    inventory.python_files.push(relative.clone());
                } else {
                    inventory.truncated = true;
                }
            } else if matches!(extension.as_str(), "ld" | "lds" | "sct" | "icf") {
                if inventory.linker_scripts.len() < MAX_SCRIPT_LIST {
                    inventory.linker_scripts.push(relative.clone());
                } else {
                    inventory.truncated = true;
                }
            } else if matches!(extension.as_str(), "a" | "lib" | "so" | "dll") {
                if inventory.libraries.len() < MAX_LIBRARY_LIST {
                    inventory.libraries.push(relative.clone());
                } else {
                    inventory.truncated = true;
                }
            } else if extension == "ioc" {
                inventory.ioc_files.push(path);
            }
        }
        if inventory.scanned_files >= MAX_SCAN_FILES {
            break;
        }
    }
    inventory.source_files.sort();
    inventory.header_files.sort();
    inventory.python_files.sort();
    inventory.linker_scripts.sort();
    inventory.startup_files.sort();
    inventory.libraries.sort();
    inventory
}

fn count_lines(path: &Path, unreadable: &mut usize) -> u64 {
    if std::fs::metadata(path)
        .map(|metadata| metadata.len() > MAX_TEXT_FILE_BYTES)
        .unwrap_or(true)
    {
        return 0;
    }
    match read_text_file(path) {
        Ok(loaded) => loaded.text.lines().count() as u64,
        Err(_) => {
            *unreadable += 1;
            0
        }
    }
}

fn skip_directory(relative: &Path) -> bool {
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

fn safe_file_name(name: &str) -> String {
    let mut result = name
        .chars()
        .map(|character| {
            if character.is_control()
                || matches!(
                    character,
                    '<' | '>' | ':' | '"' | '/' | '\\' | '|' | '?' | '*'
                )
            {
                '_'
            } else {
                character
            }
        })
        .collect::<String>();
    result = result
        .trim_matches(|character| character == ' ' || character == '.')
        .to_owned();
    if result.is_empty() {
        result = "project".to_owned();
    }
    result.chars().take(80).collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::BuildConfig;
    use crate::util::{write_text_file, LineEnding, TextEncoding};

    fn write(path: &Path, text: &str) {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).unwrap();
        }
        write_text_file(path, text, LineEnding::Lf, false, TextEncoding::Utf8).unwrap();
    }

    fn request(root: &Path) -> ReportRequest {
        let meta = ProjectMeta {
            device: Some("STM32G431CBTx".to_owned()),
            cpu: Some("Cortex-M4".to_owned()),
            fpu: Some("单精度".to_owned()),
            memory_layout: Some("Flash 128K / RAM 32K".to_owned()),
            ..ProjectMeta::default()
        };
        ReportRequest {
            root: root.to_path_buf(),
            project_name: "demo".to_owned(),
            meta,
            build: BuildConfig::default(),
            debug: ProjectDebugSettings::default(),
            memory_layout: None,
            memory_usage: None,
            support_packs: Vec::new(),
            registers: Some(Vec::new()),
            register_source: None,
            registers_truncated: false,
            symbols: None,
            symbols_truncated: false,
            relations: None,
            firmware_path: None,
            firmware_analysis: None,
            include_dirs: Vec::new(),
            library_dirs: Vec::new(),
            defines: Vec::new(),
            respect_gitignore: true,
            ignore_patterns: String::new(),
        }
    }

    #[test]
    fn writes_hardware_and_software_inventory() {
        let dir = tempfile::tempdir().unwrap();
        write(
            &dir.path().join("Core/Src/main.c"),
            "int main(void) { return 0; }\n",
        );
        write(&dir.path().join("Core/Inc/main.h"), "#define LED_PIN 1\n");
        write(
            &dir.path()
                .join("Drivers/STM32G4xx_HAL_Driver/Src/stm32g4xx_hal.c"),
            "void HAL_Init(void) {}\n",
        );
        write(
            &dir.path()
                .join("Middlewares/Third_Party/FreeRTOS/Source/tasks.c"),
            "void vTaskDelay(void) {}\n",
        );
        write(
            &dir.path().join("STM32G431CBTx_FLASH.ld"),
            "MEMORY { FLASH : ORIGIN = 0x08000000, LENGTH = 128K }\n",
        );
        write(
            &dir.path().join("demo.ioc"),
            "Mcu.IP0=ADC1\nMcu.IP1=USART2\nMcu.IP9=3\nProjectManager.DeviceId=STM32G431CBTx\nRCC.SYSCLKFreq_VALUE=170000000\nPA0.Signal=GPIO_Output\nPA0.GPIO_Label=LED\n",
        );

        let output = generate_and_write(request(dir.path())).unwrap();
        let value: serde_json::Value =
            serde_json::from_slice(&std::fs::read(&output).unwrap()).unwrap();
        assert_eq!(value["schema"], SCHEMA);
        assert_eq!(value["hardware"]["device"], "STM32G431CBTx");
        assert_eq!(value["hardware"]["ioc"]["device"], "STM32G431CBTx");
        assert!(value["hardware"]["ioc"]["components"]
            .as_array()
            .unwrap()
            .iter()
            .any(|component| component == "ADC1"));
        assert!(!value["hardware"]["ioc"]["components"]
            .as_array()
            .unwrap()
            .iter()
            .any(|component| component == "3"));
        assert!(value["hardware"]["ioc"]["pins"]
            .as_array()
            .unwrap()
            .iter()
            .any(|pin| pin["pin"] == "PA0" && pin["label"] == "LED"));
        assert_eq!(
            value["hardware"]["ioc"]["clock"]["RCC.SYSCLKFreq_VALUE"],
            "170000000"
        );
        assert_eq!(value["software"]["counts"]["source_files"], 3);
        assert_eq!(value["software"]["counts"]["header_files"], 1);
        assert_eq!(
            value["software"]["linker_scripts"][0],
            "STM32G431CBTx_FLASH.ld"
        );
        assert!(value["software"]["components"]
            .as_array()
            .unwrap()
            .iter()
            .any(|component| component["name"] == "FreeRTOS"));
        assert!(value["software"]["components"]
            .as_array()
            .unwrap()
            .iter()
            .any(|component| component["name"] == "STM32 HAL"));
        assert!(value["software"]["symbols"]["total"].as_u64().unwrap() >= 1);
    }

    #[test]
    fn writes_peripheral_and_register_inventory() {
        let dir = tempfile::tempdir().unwrap();
        let mut request = request(dir.path());
        request.registers = Some(vec![crate::registers::SvdPeripheral {
            name: "ADC1".to_owned(),
            base: 0x4001_2400,
            description: "Analog to digital converter".to_owned(),
            registers: vec![crate::registers::SvdRegister {
                name: "ISR".to_owned(),
                offset: 0,
                description: "interrupt and status register".to_owned(),
                size: 32,
                access: "read-write".to_owned(),
                fields: Vec::new(),
                line: 1,
            }],
        }]);
        request.register_source = Some("SVD · STM32G431.svd".to_owned());

        let output = generate_and_write(request).unwrap();
        let value: serde_json::Value =
            serde_json::from_slice(&std::fs::read(&output).unwrap()).unwrap();
        assert_eq!(value["hardware"]["peripheral_count"], 1);
        assert_eq!(value["hardware"]["register_count"], 1);
        assert_eq!(value["hardware"]["peripherals"][0]["name"], "ADC1");
        assert_eq!(
            value["hardware"]["peripherals"][0]["registers"][0]["description"],
            "interrupt and status register"
        );
    }

    #[test]
    fn sanitizes_report_file_name() {
        assert_eq!(safe_file_name("my:project/name"), "my_project_name");
        assert_eq!(safe_file_name(".."), "project");
    }
}
