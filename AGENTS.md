# AGENTS.md

本文件记录 Simple embedded 的项目约定、架构和验证方式，供后续编码代理快速进入状态。

## 项目概览

Simple embedded 是一个 Windows 桌面端、文件夹优先的极简嵌入式 IDE：

- 左侧文件树
- 中间多标签代码编辑 + syntect 语法高亮
- 底部异步构建日志
- 本地 / 远程 SSH 构建
- 可配置的本地 Flash 命令

核心产品约束：**工程文件夹就是工程**。不要引入 `.eide`、`.mini-ide` 等专有工程格式，也不要修改 Keil `.uvprojx`。用户设置默认写到用户目录；工程内的普通 JSON 配置文件 `.simple-embedded/settings.json` 保存该工程的构建/烧录配置，打开工程时优先读取并自动迁移旧的本机配置。允许写入工程的场景：自动创建/保存工程配置、用户主动生成 `<工程名>.se-workspace`、Makefile 工程保存构建选项或 C/C++ 属性时按差异同步（修改前自动备份到用户目录）、用户主动添加芯片支持包时复制 `.pack` / `.pdsc` 到工程根目录。

技术栈：Rust 2021、eframe/egui 0.34.3（glow）、syntect 5.3、serde、zip 9.1（读取 Keil/CMSIS 支持包）、serialport 4（MicroPython / ESP32 串口扫描）、ureq 2（AI 模式的 OpenAI 兼容 HTTP 请求）、OpenSSH。

## 快速命令

```powershell
cargo run --release
cargo run --release -- examples\blink examples\blink\src\main.c
cargo run --release -- --ai examples\blink examples\blink\src\main.c
cargo test
cargo clippy --all-targets -- -D warnings
cargo fmt --all
cargo build --release
```

产物：

```text
target\release\simple-embedded.exe
```

演示工程：

```text
examples\blink
```

## 目录

```text
src/main.rs            窗口、图标（字标 + 小尺寸标记）、CJK 字体、eframe 启动
src/icon_codec.rs      字标/标记 PNG 解码、缩放和 ICO 编码（build.rs 也复用）
src/app.rs             App 状态、快捷键、打开/保存、右侧操作栏、构建输出
src/app/ai_ui.rs       AI 模式四栏界面、项目架构面板、聊天 Markdown、串口 CLI 窗口、设置弹窗、1 秒淡入与输入拖动条
src/framework.rs       读取 framework.md、解析章节与 Mermaid gantt（AI 架构面板数据源；flow/sequence 交给 mermaid.rs）
src/ai.rs              OpenAI 兼容请求、工程上下文、额外 Skill/参考资料与 PDF 提取
src/ai_ask.rs          se-ask 确认问题解析、选项/自定义回答草稿与回答文本构建
src/ai_chat.rs         工程根目录 <工程名>.ai-chat.md 对话记录读写（Markdown + 隐藏标记）
src/ai_issue.rs        <工程名>.ai-issues.md 问题记录：按标题合并现象/原因/解决方案
src/ai_edit.rs         AI 文件修改/删除协议解析、路径校验、diff、base hash 与删除目标校验
src/ai_fs.rs           AI 自主文件检索：se-list/se-read 解析、工程内目录遍历、文本读取和结果压缩
src/app/ui.rs          app 的子模块：文件树、Tab、编辑器、空工程/空标签占位、设置/确认弹窗
src/comment.rs         多行/单行注释切换、按文件类型选择行注释标记
src/build.rs           子进程、stdout/stderr、停止、构建事件与结果
src/config.rs          全局/工程设置、ToolchainKind 选择、自动检测、SSH 命令构造、工作区文件
src/project_meta.rs    按所选构建系统提取 Makefile / Keil .uvprojx 元数据和 AC5/AC6/GCC 编译器
src/project_report.rs  硬件 / 软件 / 系统资源静态分析并写入 <工程名>.resources.json
src/editor.rs          EditorBuffer（String + revision）和 Document
src/file_tree.rs       惰性目录树
src/flash.rs           烧录配置、OpenOCD 接口预设/USB 探针检测、pyOCD 命令构造
src/memory.rs          ELF 符号 / 全局变量 / 栈堆分析、链接脚本 / PDSC 内存区域与固件占用统计
src/memory_check.rs    数组越界、危险字符串函数、重复释放等 C 内存安全静态扫描
src/mermaid.rs        Mermaid flowchart / sequenceDiagram 解析和 egui 绘制（聊天与架构面板共用）
src/macro_check.rs     同名宏重复、宏族相同替换值（可选黄色波浪线）诊断
src/completion.rs      轻量成员索引、补全上下文和候选类型
src/highlight.rs       syntect 高亮和 galley 缓存；Makefile / CMake 行级轻量高亮
src/hover.rs           标识符悬停：声明签名、注释、返回值/类型
src/minimap.rs         编辑器右侧代码预览、缩略行缓存和滚动跳转
src/log.rs             ANSI SGR、错误分级、日志上限、链接错误提示
src/lsp.rs             clangd LSP 客户端、compile_commands、补全/跳转/引用/悬停/重命名
src/serial.rs          MicroPython / ESP32 串口配置、端口扫描与 USB 桥接芯片识别
src/serial_cli.rs      AI 模式串口 CLI：端口/波特率/行尾配置、后台收发线程、行事件
src/context_index.rs   AI 模式本地上下文索引：代码分块、BM25 风格检索、输出压缩
src/micropython.rs     MicroPython raw REPL 上传、软复位、运行结果解析
src/codetheme.rs       50 套编辑器 / 应用主题色板（暗色 40 + 亮色 10）
src/theme.rs           从当前主题派生整套 UI 配色、egui 样式和按钮样式
src/toolchain.rs       系统 / 软件自带工具链检测、PATH 目录和复制安装
src/util.rs            路径、换行、系统集成辅助
src/brackets.rs        注释/字符串感知的括号匹配
src/indent.rs          缩进单元、行缩进/反缩进、回车和粘贴缩进
src/ignore.rs          gitignore 风格匹配
src/install.rs         Windows 用户级安装、卸载、开始菜单和文件夹右键菜单
src/search.rs          全工程文本搜索
src/support_pack.rs    Keil/CMSIS .pack/.pdsc 扫描、导入和元数据解析
src/symbols.rs         C/C++/汇编符号索引
src/navigation.rs      标识符提取与补全替换
src/todo.rs            TODO/FIXME 扫描
src/git.rs             git 状态/修改行
src/registers.rs       SVD / 头文件寄存器索引
src/relations.rs       C/C++ 调用关系与标识符引用索引（悬停调用树/引用树）
src/hexview.rs         十六进制模型
src/arraygen.rs        C 数组生成
src/formatting.rs      clang-format
src/debugger.rs        gdb 会话、ARM GDB server 与目标识别
src/column.rs          旧批量列编辑算法
src/column_select.rs   矩形列选择文本操作
src/multi_cursor.rs    多光标文本编辑和光标位置重映射
src/fold.rs            大括号块折叠和可编辑折叠缓冲
examples/blink/        可编译的演示固件
assets/                应用图标源图：icon-word.png（Simple 字标）、icon-mark.png（S 标记）
build.rs               从 assets 生成 ICO，并用 winres 嵌入 exe 资源
scripts/package-tools.ps1  打包 release exe 和软件自带工具链到 dist
docs/                  README 截图和使用教程
```

## 架构要点

### UI

- 使用 eframe 0.34 的 `eframe::App::ui(&mut self, ui, ...)`，界面挂在 root `Ui` 下。
- 面板使用 `egui::Panel::top/bottom/left(...).show_inside(ui, ...)` 和 `CentralPanel::show_inside`。不要回退到旧的 `update(ctx)` + `.show(ctx)` 写法。
- 事件采用 immediate mode：`App::ui` 收集 `Action`，再在帧末 `apply_actions`。部分明显的即时状态（滚动、输出开关）直接修改。
- `src/app/ui.rs` 是 `app` 的子模块，可以访问父模块私有字段；跨模块公开方法用 `pub(super)`。
- 自定义画线/图标优先用 `Painter`，不要依赖可能没有字形的 Unicode 图标。CJK 字体在 `main.rs` 运行时从系统字体加载。
- 左侧构建卡片顶部显示工程名（粗体）和路径，路径使用 `util::display_path` 去掉 Windows `\\?\` 前缀；下方显示有效工具链 + 构建位置（例如 `Make · GCC · 本地`），按钮行上方左对齐显示从 Makefile / `.uvprojx` 检测到的芯片型号；再下面是三个按钮，不再显示构建命令预览。未导入时按钮为“导入 / 配置 / 工作区”，导入后为“更换 / 配置 / 工作区”。工作区按钮弹出菜单：生成并关联工作区文件、打开工作区文件、打开工程配置 `settings.json`。
- 左侧栏全高；资源管理器（文件树）与工程信息的分界受 `left_split` 控制：文件树内容较短时区域自动收缩，工程信息标题紧跟最后一个可见文件；折叠资源管理器时工程信息标题紧贴资源管理器标题。文件树内容超出时按比例截断；展开时工程信息标题上下各留 6px，折叠且树超出时上下各留 8px，标题自身保留 6px 内边距和最小高度，避免贴住底部或上下间距不一致。中间分割线可拖动。折叠/展开工程信息只影响其内容，不改变标题位置。`left_split`、`resource_open`、`info_open` 都持久化在 `Settings`，下次启动保持位置。左栏使用 `Settings.left_panel_width`（默认 300px）作为初始宽度，可拖动 300–520px，拖动结束后写回设置，下次启动保持用户选择的宽度；不再用补底色掩盖间隙。工程信息内容超高时可滚动，顶部/底部使用与资源管理器一致的渐变遮罩；横向内边距放在 `ScrollArea` 内容内侧，让遮罩横跨整个左栏宽度，而不是被外框内边距截窄。无工程时中间只显示一行提示“请在左上角选择工程并开始”。
- Windows 标题栏通过 DWM 深色属性和 `ViewportCommand::SetTheme(Dark)` 与面板配色保持一致。
- 应用图标是 `assets/icon-word.png`（黑色圆角底 + 白色斜体 `Simple` 字标）和 `assets/icon-mark.png`（同风格 `S` 标记）两张内嵌 PNG。`app_icon(size)` 在 32px 及以上用字标、16/24px 用 `S` 标记；`app_icon_ico()` 生成 16/24/32/48/64/128/256 多尺寸 ICO（16–128 用 32 位 BMP 条目，256 用 PNG）。`build.rs` 用同一套 `icon_codec` 从 assets 生成 ICO，并通过 `winres` + `windres` 把图标资源嵌进 exe，所以文件夹里看到的 `.exe` 图标也是 `Simple`，不再依赖 `.se-workspace` 关联。Windows 标题栏 16px 放不下六个字母，所以窗口初始化时先用 48px 字标让任务栏取到 `Simple`，再由 `apply_pending_icons` 在前几帧把 ICON_SMALL 换成 `S` 标记：标题栏显示 S，任务栏和 Explorer 大图标显示 Simple。
- 沉浸式编码模式由 `Ctrl+Shift+Enter` 切换（`App.immersive`，不持久化）：通过 `ViewportCommand::Fullscreen(true)` 和 `WindowLevel::AlwaysOnTop` 进入无边框全屏，隐藏标题栏、任务栏、`show_status_bar` / `show_action_rail` / `show_side_panel` / `show_output_panel`，只保留 `show_center`；右侧 `show_immersive_explorer` 用可折叠的 `egui::Window` 悬浮显示资源管理器，复用 `show_project_tree`，标题栏三角可收起成一条标题栏，关闭悬浮窗或再次按快捷键退出并恢复普通窗口；调试浮层在该模式下隐藏。

### AI 模式

- 右侧操作栏底部、**设置** 上方是强调色 **AI模式** 按钮，快捷键 `Ctrl+Alt+A`（`Action::EnterAiMode` / `App::enter_ai_mode`）；无工程时只提示先打开工程。启动参数 `--ai` 可在打开工程后直接进入，用于测试和快捷启动；命令行中以 `--` 开头的参数不会当作工程路径。
- `App.ai: AiState` 不持久化，保存对话、输入、发送线程 receiver、skill 状态、上下文文件和转场状态；切换工程时重置。`App.ai_settings_draft` 是独立弹窗，已加入 `has_modal`，Esc 可关闭。
- AI 模式接管整个窗口（不显示状态栏、左栏、右栏、输出面板），由 `src/app/ai_ui.rs::show_ai_mode` 画四个大圆角控件：左上 AI 输出、左下输入、中间项目架构、右侧 AI 工作台。右侧工作台宽度约为窗口的 20%（208–288px），中间架构面板约 29%（260–420px，窗口过窄时自动收窄以避免和左栏重叠）。进入 AI 模式时把 egui 缩放临时设为 `1.2`（`AI_UI_ZOOM`，上限 1.6），退出后恢复进入前的缩放；这样架构面板和聊天文字更大而不影响普通编辑器。右侧面板右下角是 **原始模式** 和 **设置**；设置打开 AI 设置弹窗，AI 模式下 `Ctrl+,` 也打开 AI 设置。
- 进入转场固定 1.0s：不再播放雾气、流星或桌面透明，直接切到 AI 布局，四块面板在约 0.95s 内依次淡入/缩放（`PANEL_START=0`、`PANEL_STAGGER=0.05`、`PANEL_DURATION=0.80`）；`AI_EARLY_FOG_SECS=0`，`ai_window_alpha` 恒 255，避免每帧 DWM 透明度、噪声纹理和贝塞尔流星的卡顿。AI 模式空闲时 33ms、发送中 16ms 请求重绘，避免 AI 图标轨道旋转卡顿。左侧 AI 输出与输入之间有一条 3px 水平拖动条（`ai_output_ratio`，0.20–0.85），向下拖可扩大输入框，方便粘贴和编辑长文本；拖动时使用 `ResizeVertical` 光标。输入 TextEdit 放在垂直 ScrollArea 中，长文本可用鼠标滚轮/触控板上下浏览。
- 中间 **项目架构** 面板的唯一数据源是 skill 生成的 `framework.md`（`src/framework.rs`）：优先在工程根目录和 `docs/` / `doc/` 下查找，找不到再限深递归。没有文档时显示 **等待 framework.md** 和“让 AI 生成”按钮，绝不使用启发式结果或空模板；文档存在但缺少 `1.1 层级表` / `2.1 执行范式` / `2.2 调度模式` / `2.3 设计模式` / `3.1 启动调用链` / `3.2 主循环调度时间线` / `3.3 中断来源与抢占关系` 时标记 **文档不完整**，并显示“让 AI 补全 framework.md”。有文档时按 **00 分层架构 → 01 执行范式 → 02 调度模式 → 03 设计模式 → 函数调度图（调用方向图 + 关键事件上下文链）→ 调度入口（启动调用链）→ 中断与任务（中断/抢占表）→ 04 调度甘特图** 展示。Markdown 表格渲染为可横向滚动的 `egui::Grid`；Mermaid `flowchart` / `graph` 和 `sequenceDiagram` 由 `src/mermaid.rs` 解析成 `egui::Painter` 图形（节点/箭头/虚线/边标签/参与者/Note/alt 标签），内容过宽时显示横向滚动条并提示；时序图会把每条消息的“发送方 → 接收方：内容”左对齐显示，并在窄面板压缩参与者列，避免箭头很长但标签看不到。`关键事件上下文链` 和 `启动调用链` 在面板内改为紧凑程序流程图（消息框 + 向下箭头，alt/else 作为分支标签），每个“程序流程”旁有 **全屏泳道图** 按钮，一次只打开当前这张 Mermaid `sequenceDiagram` 泳道图；大窗口固定大小，右上角 X / **关闭** 或 Esc 退出。不支持的 Mermaid 类型回退为源码块 + 复制按钮；`3.2` 的 Mermaid `gantt` 仍由 `framework.rs` 解析成甘特图，解析失败回退源码。顶部显示文档路径、**打开** 和 **刷新**；每 800ms 检查 mtime，AI 生成/保存/应用修改后自动刷新。`App.ai.framework` 保存文档、路径、mtime、错误和 missing 标记，切换工程时随 `AiState` 重置。面板是文档查看器，不再做启发式架构推断。
- 聊天渲染支持 fenced code block（语言标签 + 复制按钮 + 横向滚动）、行内 `code`、标题、列表、编号列表、引用、分隔线、Markdown 表格、链接、粗体/斜体/删除线和 Mermaid `flowchart` / `sequenceDiagram` 图形；消息卡片带角色/错误色、复制按钮和自动滚底。普通代码块仍然只显示/复制；`se-file` / `se-replace` 特殊块会从正文移除并生成“AI 文件修改”卡片，`se-delete` 生成独立的“AI 文件删除”卡片，`se-ask` 生成“AI 需要确认”卡片，`se-build` 生成“AI 构建验证”卡片，`se-cli` 生成“AI 串口 Debug”卡片，`se-issue` 生成“AI 问题记录”卡片。流式回复在正文前显示 **思考过程** 卡片（默认展开，可收起/复制，最多 300KB），正文在生成期间以纯文本实时预览，完成后切换为 Markdown、文件修改/删除/确认/构建/串口/问题卡片。
- 确认卡片（`se-ask`）：模型每个块问一个问题，可带 2–8 个 `- 选项`，`se-ask multiple` 支持多选，也可以只写问题让用户自由回答。卡片内可用选项按钮选择，也可在“自定义回答”输入框补充；所有问题都有选择或自定义文本后才能点 **发送回答**，回答会拼成一条用户消息（包含问题、选择、自定义）并像普通输入一样发送。一次最多 6 个问题（skill 建议 ≤2），存在解析错误或超限时提交按钮禁用。回答后卡片显示“已回答/已取消”并保留问题。
- 对话记录：进入 AI 模式时读取工程根目录 `<工程名>.ai-chat.md`（`src/ai_chat.rs`），没有则自动创建；发送用户消息、流式完成、清空聊天、退出 AI 模式和切换工程时重写该文件。Markdown 中每个消息用隐藏 HTML 标记分隔，AI 正文和 `<details>` 思考过程都可读；待应用的修改/删除/确认卡片以“历史记录”小节写入，但重新加载后不会恢复成可执行卡片（避免重启后误应用旧操作）。文件超过约 2MB 时保留最近消息并标注截断；`清空` 会重写为空记录，等于清除该工程的对话历史。AI 的 `se-file`/`se-delete` 禁止修改或删除 `*.ai-chat.md`。
- 文件修改协议：系统提示词要求模型用 **四个反引号** 的 ` ````se-file path=相对路径 ` 新建/整体覆盖文件，或用 ` ````se-replace path=相对路径 ` + `<<<<<<< SEARCH` / `=======` / `>>>>>>> REPLACE` 做唯一匹配替换；删除文件用 ` ````se-delete path=相对路径 ` + 可选删除原因；需要用户确认时用 ` ````se-ask [multiple] ` + 问题 + 可选 `- 选项`；真实构建验证用 ` ````se-build `；串口调试用 ` ````se-cli ` + 每行一条命令；问题记录用 ` ````se-issue status=open|resolved ` + 标题/现象/原因/解决方案/相关文件。四个反引号允许文件内容（尤其 `framework.md`）安全包含普通三反引号 Markdown/Mermaid 代码块；`parse_edits` 同时兼容三/四反引号，并按开围栏长度匹配关闭围栏，避免在第一个内部 ``` 处提前截断。`prepare` 读取当前文件、校验路径、计算 `similar` 行级 diff 和 base hash；`AiDelete::prepare/verify` 校验目标存在、是普通文件、不在生成/配置目录、不是符号链接，并用大小/修改时间防止生成后被替换。模型被明确告知 IDE 会执行修改，不要声称无法写盘；优先 `se-replace` 保留无关代码，禁止用空文件或“请手动删除”注释代替 `se-delete`。
- 输出截断保护：`chat_stream` 读取 SSE `finish_reason`；遇到 `length` 时把本轮 assistant 内容追加进消息，再发一轮“从中断处继续”的请求，最多 `MAX_STREAM_CONTINUATIONS`（8）轮，避免单次输出上限把 `framework.md` 写成半份。最终仍截断时发送 `AiStreamEvent::Done { truncated: true }`；`finish_ai_stream` 遇到 truncated、网络中断或 `ai_edit::incomplete_edit_block` 为真时只保留已收到的文字并标记错误，**不解析、不准备、不写入任何文件修改**。普通非流式回退无法获知 finish_reason，但未闭合围栏仍会被 incomplete_edit_block 拦住。
- Skill 自我进化：内置 skill 根目录新增 `evolution.md`（已加入 `EMBEDDED_SKILL_FILES`），定义“场景 + 做法 + 原因”的长期规则和 `se-evolve` 提议协议。模型遇到用户长期偏好/纠正时输出四反引号 `se-evolve` 块；`ai_edit::parse_edits` 把它解析到 `ParsedEdits.evolutions` 并从正文移除，聊天气泡显示 **AI 技能进化** 卡片，用户点 **保存规则** 后 `ai::append_user_evolution` 写入 `%APPDATA%\SimpleEmbedded\ai\evolution\stm32-embedded-dev.md`（去重、单条 500 字、总 32KB），后续请求通过 `load_user_evolution_for` 读取全局规则和项目级 `evolution-user.md` 注入系统提示词并优先于默认 skill 写法（但仍服从当前指令、工程现状和安全/硬件事实）。`evolution_status` 与 `evolution_error` 保存在 `AiChatMessage`。
- 隐藏的 Skill 导出：在右侧 AI 工作台连续点击 AI 图标 5 次（3 秒内）触发 `App::record_ai_orb_click` → `export_ai_skill`；选择目录后 `ai::export_skill_bundle` 优先复制当前加载的目录 skill，找不到时写出内置副本，并把全局 + 项目级 evolution 规则写成 `references/evolution-user.md`，完成后打开导出文件夹。该入口**不在 UI、悬停或设置页提示**，导出目录名为 `stm32-embedded-dev-export`，重名自动加序号。AI 图标不再设置 PointingHand 光标。
- 用户规则导出：**双击** AI 图标触发 `App::export_ai_evolution`，在工程根目录创建（已存在则直接打开）`evolution-user.md`，退出 AI 模式并在编辑器打开；文件是普通 Markdown，模板包含“场景 / 做法 / 原因”说明。下次请求通过 `ai::load_user_evolution_for` 同时读取全局 `%APPDATA%\SimpleEmbedded\ai\evolution\stm32-embedded-dev.md` 和项目级 `evolution-user.md`（总上限 32KB），作为用户自定义规则注入系统提示词。工作台 Skill 卡片下方有静态提示“双击 AI 图标：导出并编辑本项目 evolution-user.md”，不使用悬停手指光标。
- 代码风格（T7）：工作台 **代码风格** 分区显示 `code_style.md` 是否存在、样例目录状态，提供 **学习代码风格** 和 **打开**。点击学习会发送 T7 提示，要求扫描 `code_style_samples/`、根目录上传的 `.c/.h` 和现有 APP/MID/BSP/Core 源码，归纳文件头/命名/缩进/注释/类型宏/错误处理/模块封装/中断调度/优化偏好，用 `se-file` 生成或更新 `code_style.md`，不修改业务代码。`ai::build_system_prompt_with_stats` 只要发现 `code_style.md` 就完整附带并标注最高优先级；查询涉及“风格/优化/code_style”时额外附带 `code_style_samples/` 与根目录样例（最多 32 个）。T7 模板 G 包含风格规则、code_style.md 内容、偏差审计和极致优化方案。
- T6 其他操作：skill 第 1 节新增 T6（模板 F，S6），分为 T6-O 操作（删除/重命名/移动/工程配置/文档/报告/诊断）和 T6-C 咨询/设计（架构、选型、方案对比、评审），规则在 `references/other-operations.md` / `references/consulting-design.md`。T6-C 默认不改文件，给需求、约束、方案对比、推荐、风险和下一步；用户明确要设计文档时才用 `se-file`。当前 IDE 自动执行范围只有 `se-delete`（普通文件、二次确认、先备份、最多 10 个）；重命名/移动/目录创建/批量清理等仍只给预览和影响分析，不谎称已执行。若 T6 同时涉及代码或硬件变更，主任务仍按 T1–T5，T6 附带项在对应模板列出。
- T4 诊断：`references/diagnostics.md` 规定先列可能故障位置和证据，再写可移除的诊断代码；有串口用 `se-cli` 联合测试，无串口用 LED 闪烁码、GPIO/蜂鸣器、调试器 watch/寄存器/断点等替代，模板 D 已加入诊断假设、诊断代码、串口/无串口验证和清理小节。
- T8 系统整体感知：skill 第 1 节新增 T8（模板 H，S9），规则在 `references/system-perception.md`。用于新导入/别人的工程：先盘点构建文件、目录、入口、启动/链接脚本、RTOS/厂商代码和现有 `framework.md`/`hardware.dts`，判断“默认 / 兼容 / 外来 / 混合”，再决定“保留现状并更新文档 / 增量补全 / 替换整份 / 迁移到默认结构”。T8 默认不改业务代码、不移动目录、不覆盖 `framework.md`；替换或重构前必须用 `se-ask` 确认。外来工程可以保留原目录，`framework.md` 记录真实层级 + 默认层映射；实际重构按 T1/T2/T3/T6 执行。AI 模式架构面板的“让 AI 生成/补全 framework.md”按钮改为先走 T8 感知，不再直接按默认结构生成。
- `se-replace` 匹配先精确查找；如果 0 匹配且 SEARCH 以换行结尾，会去掉末尾换行再匹配一次，并从 REPLACE 去掉一个末尾换行以保留原行换行。这解决模型复制行内锚点（如 `表核对：USART1_TX=DMA1_CH4`）时把围栏换行带进 SEARCH、导致 0 匹配的问题。失败信息包含片段序号和预览，仍然不会模糊匹配或猜测。
- 应用流程：`poll_ai` 收到回复后准备 diff，聊天气泡下方显示文件列表、kind（新建/覆盖/替换）、可展开差异、每个文件的 **应用此文件**、**应用全部** / **取消**；只有点击应用才调用 `App::apply_ai_edits`（全部）或 `App::apply_ai_single_edit`（单个）。每个 `AiEdit` 带 `applied` 标记，部分应用后卡片显示 `已应用/总数`，`应用全部` 只处理尚未应用且没有错误的文件；只要还有无法匹配的文件，`应用全部` 禁用，但其他文件仍可单独应用。失败行提供 **分析原因**，卡片顶部还提供 **分析失败原因**（一次最多分析 2 个失败文件，避免超出聊天消息长度）：`ai_edit::edit_failure_report` 把原 SEARCH/REPLACE、当前磁盘文件片段（长文件只截取最相似位置附近的窗口）、哈希是否变化和失败原因拼成一条用户消息发给 AI，AI 只需重新生成该文件；不会自动改写或模糊匹配。应用前统一校验：路径相对工程根、禁止 `..`/绝对路径/生成目录（`build`、`Debug`、`Release`、`out`、`target`、`tools`、`dist`、`.simple-embedded` 等）和二进制扩展；同一文件的多个 `se-replace` 会先合并成一次修改；多个 `se-file` 或 `se-file` + `se-replace` 混用仍视为冲突；校验 base hash，文件在生成后变化会拒绝；打开且未保存的 Tab 会拒绝；打开且干净的 Tab 更新缓冲区并保存，关闭的文件按原编码/行尾/BOM 写回。写入前把已存在文件备份到 `%APPDATA%\SimpleEmbedded\backups\<工程名>-<hash>\ai-<timestamp>\`。应用后刷新文件树、符号/调用索引和查找状态。没有自动应用开关，也没有一键撤销。
- 删除流程：`se-delete` 进入独立的“AI 文件删除”卡片，不随“应用全部”执行；点 **删除文件** 后才调用 `App::apply_ai_deletes`。校验路径、普通文件、符号链接、生成/配置目录、打开且未保存的 Tab、重复/与修改冲突、最多 10 个、文件在生成后是否变化；先把文件复制到 `backups\<工程名>-<hash>\ai-delete-<timestamp>\` 并写同级 manifest，再逐个删除。删除后关闭相关标签页、清理断点/分屏/调试位置、同步 Makefile `C_SOURCES`、刷新文件树/符号/调用关系/架构面板/支持包/Git 状态；部分失败会在卡片和 Toast 中列出。当前只支持删除普通文件，不支持目录、重命名或移动。
- 确认问题流程：`parse_edits` 把 `se-ask` 解析到 `AiAsk`，`finish_ai_stream` 初始化 `AiAskDraft`；卡片用单选/多选按钮和自定义输入收集答案，`ai_ask::answer_text` 把所有问题拼成一条“回答你的确认问题：…”用户消息，`send_ai_text` 直接发送而不占用底部输入框。发送前会检查所有问题已选择或填写；已回答/已取消后卡片保留问题。加载历史时 `se-ask` 只作为“AI 需要确认（历史记录）”文本保留，不恢复成可提交卡片。若模型没有输出 `se-ask`、但在正文写了明确的授权/决策问句（如“请允许我…”“是否继续/替换/重构”），`ai_ask::fallback_question_card` 会把它转成带“允许/不允许”“继续/暂停”“替换/保留”等默认选项的问答卡片；只读 `se-list`/`se-read` 不需要授权，系统提示词要求直接读取。
- 退出 AI 模式不会中止正在生成的回复：`exit_ai_mode` 不再调用 `abort_ai_stream`，`poll_ai` 在普通编辑模式下继续接收 SSE 事件，流式正文/思考过程保留在 `ai.messages`；重新进入时 `load_ai_chat` 因为消息非空不会重载，`se-ask` 卡片和后续文件/构建卡片仍在。用户可以先回编辑器确认寄存器、引脚或代码，再进入 AI 模式回答或应用。退出时构建验证会继续在后台运行但不再自动回传，串口 Debug 会停止；只有明确停止、清空、切换工程或应用退出才会丢弃接收端。
- 构建验证流程：`se-build` 解析为 `ParsedEdits.build`，`finish_ai_stream` 创建 `AiBuildCheck` 并在消息上显示“AI 构建验证”卡片。若同一回复还有待应用的文件修改/删除/确认问题，则等待用户应用/回答；否则在下一帧调用 `App::start_task(TaskKind::Build)`（MicroPython 模式拒绝）。`poll_ai_build` 在 `build.poll()` 后读取 `log_start..log.len()` 和 `BuildOutcome`，把错误/警告优先、末尾输出写入报告，更新卡片状态/摘要/可折叠输出，并把报告作为一条特殊用户消息自动发送给 AI。失败时 AI 修复并再次 `se-build`；成功需 IDE 回传 0 error。自动发送最多 `MAX_AI_BUILD_ROUNDS`（5）轮，超过后只保留报告等待用户手动继续；停止/清空/取消前置卡会中止构建，退出 AI 模式只停止自动回传并让构建继续在后台运行，应用退出会中止。发送普通用户消息会取消当前构建验证并重置轮次。
- 串口 CLI：输入面板发送按钮旁有 **CLI** 按钮，打开独立的“串口 CLI”窗口（`src/serial_cli.rs`），内含端口（可手动输入）、波特率、行尾 `CRLF`/`LF`/`CR`、等待时间、自动检测、连接/断开、输出区和命令输入。配置保存到工程 `cli` 节点。后台线程负责串口读，`poll_cli` 每帧收集行事件并写入输出/捕获缓冲；写线程与读线程分离，关闭窗口或切换工程时断开。左侧 AI 工作台会显示当前工程的 CLI 状态。
- 串口 Debug 验证：`se-cli` 解析为 `ParsedEdits.cli_commands`（每行一条，最多 32 条），`finish_ai_stream` 创建 `AiCliCheck` 并显示“AI 串口 Debug”卡片。若同一回复有待应用的修改/删除/确认或正在构建，则等待；否则自动连接项目配置的串口（未选端口时尝试自动检测），按行尾逐条发送，每条等待 `cli.timeout_ms` 后截取该命令输出，全部结束后把 `[IDE 串口 CLI 执行报告]` 作为用户消息自动发回 AI。自动发送最多 `MAX_AI_CLI_ROUNDS`（5）轮；发送失败/无响应会写入卡片并回传原因。停止/清空/取消前置卡/退出 AI 模式会停止串口任务；发送普通用户消息会取消并重置轮次。
- 问题记录：`se-issue` 解析为 `AiIssue`（标题、状态、现象、原因、解决方案、相关文件），`finish_ai_stream` 立即调用 `ai_issue::record` 写入工程根目录 `<工程名>.ai-issues.md`；同标题（不区分大小写）更新已有记录，`status=resolved` 标记已解决。进入 AI 模式时若文件不存在会自动创建；该文件被禁止通过 `se-file`/`se-delete` 修改，但会加入系统提示词上下文（上限 40KB），让模型看到历史问题。聊天卡片显示条目、状态和解决方案。
- 上下文构建会额外附带 `APP`、`MID`、`BSP`、`BSP/MODULES`、`BSP/PERIPHERAL`、`LL`、`HAL`、`Core/Src`、`Core/Inc`、`src`、`include` 下的主要 `.c/.h` 文件（最多扫描 64 个、实际附加到 48 个或约 140KB，跳过 Drivers/Middlewares/vendor 等），并优先附加 `framework.md` 中引用的文件，方便模型用 `se-replace` 修改 `gpio.c`、`bsp_led.c` 等非当前文件；这些文件同样从磁盘读取，未保存编辑不会进入上下文。
- 上下文索引（内置 context-mode 思路）：`src/context_index.rs` 在打开工程后后台扫描工程（最多 1200 文件、6000 个 48 行片段，跳过 `build` / `Drivers` / `tools` / `.opencode` / `code_style_samples` 等），按代码标识符、snake_case/camelCase、CJK 单字与二元组建立倒排索引。AI 发送时用用户输入 + 当前文件做 BM25 风格检索，只把相关片段（最多 20 个、约 64KB）和最多 2 个命中的完整文件加入 prompt，代替原先的全量源文件；`framework.md`、`hardware.dts`、当前附加文件、`main.c`、framework 引用文件仍完整。工作台“上下文索引”显示文件/片段数、上次检索和更新时间，支持 **重建索引**；保存、应用 AI 修改、删除文件后标记 stale，下次发送重建（本次发送回退到完整文件，避免用旧索引）。构建报告和串口报告经 `context_index::compress_output` 去除 ANSI、折叠重复行、保留错误/警告与头尾，减少 token。索引是词法检索，不理解同义词或类型关系；检索不到时回退到完整源文件。
- 文件自主检索：模型可以输出 `se-list path=相对目录 [depth=1-3]` 和 `se-read path=相对文件 [lines=起-止]`，IDE 在工程根目录内执行只读遍历/读取，把真实正文作为“IDE 文件检索结果”加入下一轮系统提示词并自动继续，最多 6 轮。结果同时显示为“AI 文件检索”卡片（默认折叠读取内容），并保留在 `pending_retrieval` 中，即使同回复还有待确认卡片也会在用户下一次发消息时带上。`se-list`/`se-read` 禁止绝对路径、`..`、生成/配置目录、`.opencode`、符号链接和二进制文件；单文件最多 24KB，单轮报告最多 48KB、12 个请求。当前打开的文件在提示词中优先于 `framework.md` 附加，`framework.md` 只使用剩余预算，避免大架构文档挤掉用户正在看的源码。
- 输入框支持 `Ctrl+Enter` 发送；进入时默认把当前文件作为额外附加文件（输入面板显示“附加 · 文件名”，不是模型唯一上下文），可在输入面板移除。`framework.md`、`hardware.dts`、其中引用的文件和工程源文件始终自动加入，输入面板的“自动 · framework.md + 检索片段”提示可悬停查看。发送前调用 `save_all_dirty` 保存所有未保存标签页（失败则中止发送），保证提示词和文件修改的 base hash 基于最新磁盘内容。AI 请求在后台线程执行，通过 `mpsc` 发送 `AiStreamEvent::Reasoning/Content/Done/Error`；UI 每帧 `poll_ai` 最多处理 256 个事件，发送期间每 30ms 请求重绘，占位 assistant 消息实时追加思考/正文。完成（`Done`）后统一解析 `se-file`/`se-replace`/`se-delete`/`se-ask`/`se-build`/`se-cli`/`se-issue`/`se-evolve`/`se-list`/`se-read` 并生成对应卡片，同时写入工程根目录对话记录；流中断或线程异常会保留已收到的部分内容并标记错误。
- AI 设置弹窗填写 API Key / API 地址 / 模型名称；保存在全局 `%APPDATA%\SimpleEmbedded\settings.json` 的 `ai_api_key` / `ai_base_url` / `ai_model`，不写入工程。默认 `https://api.deepseek.com/v1` + `deepseek-chat`。API Key 明文保存，界面已说明。AI 模式下按 `Ctrl+,` 打开 AI 设置；普通编辑器仍打开应用设置。
- AI 设置还管理用户级 **额外 Skill** 和 **参考资料**：额外 Skill 放在 `%APPDATA%\SimpleEmbedded\ai\skills\`，可导入包含 `SKILL.md`/Markdown 的文件夹或单个文件；参考资料放在 `%APPDATA%\SimpleEmbedded\ai\references\`，可导入 PDF、Markdown、TXT、C/H、JSON、CSV、YAML 等。导入会把文件复制到用户目录（重名自动加“副本”），在后台线程执行；PDF 用 `pdf-extract` 提取文本并写成 `<原名>.pdf.txt` 旁车文件（单文件上限 64MB，扫描件可能无文本并给出提示）。`scan_resources` 扫描目录显示名称、文件数、大小和类型；`load_extra_skills` / `load_references` 在发送时读取文本，额外 Skill 上限 48KB、参考资料上限 96KB，并在提示词里预留 80KB 给工程上下文和文件修改协议。设置弹窗用滚动区容纳 API、Skill、额外 Skill、参考资料；导入状态和警告显示在弹窗内，完成时同时弹 Toast。
- `src/ai.rs` 负责 OpenAI 兼容 `/chat/completions` 的 SSE 流式请求（`stream:true`，600s；解析 `delta.reasoning_content`/`delta.reasoning` 与 `delta.content`，兼容非 SSE JSON 响应；400/422 且错误提到 stream 时回退到非流式 `chat`，180s 超时）、系统提示词和上下文构建：优先从工程或其上级、当前目录、可执行文件目录、用户全局 skills 目录查找 `.opencode/skills/stm32-embedded-dev/SKILL.md`，读取 SKILL.md 和 references 下的 Markdown；按 L0→L3 契约优先顺序加载，放不下的文件整体跳过而不是截成半份，并允许模型用 `se-read` 只读读取 `.opencode/skills/stm32-embedded-dev/` 下的 `.md`；找不到时使用 `include_str!` 编译进 exe 的内置副本（构建依赖 `.opencode/skills/stm32-embedded-dev/` 目录存在）。系统提示词包含 skill 全文（上限 240KB；优先 `SKILL.md`、`contract-map.md`、`task-registry.md`、`orchestration-templates.md`、`templates.md`、`context-schema.md`、`system-perception.md`）、工程名/芯片/CPU/FPU/工具链/内存布局/包含目录/宏/当前文件、文件树（最多 400 个）、当前打开文件（优先附加，避免被 framework.md 挤掉）、`hardware.dts`、`framework.md`（只使用剩余预算，至少给引用文件和问题记录留 32KB）、`framework.md` 中出现的工程文件路径自动解析并读取（大小写不敏感，支持裸文件名，最多 32 个，过长文件按预算截断）、上一轮 `se-list`/`se-read` 的真实检索结果（上限 48KB，插入在当前文件 / main.c 之后、framework.md 之前）、上下文索引检索到的相关代码片段（BM25 风格，最多 20 项；索引不可用或 stale 时回退到 `APP` / `MID` / `BSP` / `Core/Src` / `Core/Inc` / `src` / `include` 等层的主要源文件）、`<工程名>.ai-issues.md`（上限 40KB）、`code_style.md`（存在时最高优先级完整读取）、用户代码风格样例（查询涉及风格时读取 `code_style_samples/` 和根目录样例）、当前附加文件、`main.c`、`.ioc`、`Makefile` / CMake / Cargo，以及 `<工程名>.resources.json` 摘要；这些文件从磁盘读取，未保存的编辑不会进入上下文；总上限 300KB，其中 16KB 预留给文件修改协议；聊天历史最多 20 条、普通消息每条 32k 字符、检索消息 48k 字符、总计 128k 字符；生成中预览最近 48KB，完成后显示全文。SSE 流按 `finish_reason` 判断截断并自动续写最多 8 轮，最终仍截断时 UI 不应用文件修改。
- AI 自主检索是只读协议：`se-list` 只列工程内目录（深度 1–3，最多 300 项），`se-read` 只读文本文件（单文件 24KB、单轮 48KB、最多 12 个请求、最多自动继续 6 轮）；二进制、符号链接、生成/配置目录和工程外路径一律拒绝，唯一例外是 `.opencode/skills/stm32-embedded-dev/` 下的 `.md` 参考文件可只读读取（用于补读未加载的 skill 文件）。检索结果插入在当前文件 / main.c 之后、framework.md 之前；同时写入自动继续用户消息的 `model_content`（界面只显示摘要），保证模型在本轮用户消息里也能看到正文，而不是只看到“已读取 N 个文件”的统计行。聊天里只显示“AI 文件检索”卡片摘要，完整读取内容默认折叠。模型仍可能不主动使用该协议；此时可在输入框直接要求“先 se-read 读取 xxx 再改”。
- 只思考不回复：`chat_stream` 在收到 reasoning 但 `content` 为空时，会追加“停止思考、直接输出正文/承诺的文件”提示并自动重试最多 2 次；仍为空才返回 Error。这样推理模型不会以“（模型只返回了思考过程）”结束；系统提示词和 skill 也禁止只输出思考过程或只承诺“下一轮输出”。
- 读取计划没有可执行块：如果回复正文只写“读取目标/本轮取/下一轮读取”或把 `se-read` 放进普通代码块，`finish_ai_stream` 检测到没有实际 `fs_requests` 时会自动发送“立即输出四反引号 se-read/se-list 块”提示，最多重试 2 次；不会因为只写计划就反复卡在检索。连续两轮仍不输出可执行块时，卡片会提示手动打开文件或说明缺哪个文件。
- 只承诺落盘没有可执行块：如果回复只写“我将落盘/接下来修改”或提到 `se-replace`/`se-file` 却没有实际块，`finish_ai_stream` 会自动要求输出实际修改，最多重试 2 次；O14 同时要求已读文件不得连续 `se-read`，读完后的下一轮必须落盘。新增的“先补齐…正文”等读取计划标记也会触发读取重试。用户催落盘但部分文件缺正文时，Skill 要求先落盘已读文件、明确列出待读项；`se-file` 整份重写已有文件也必须先读，不能用它绕过 O3；删除源文件后 IDE 会自动同步 Makefile `C_SOURCES`，模型不需要先读 Makefile。
- 内置 skill 副本由 `src/ai.rs::EMBEDDED_SKILL_FILES` 的 `include_str!` 固定列出，按四层契约优先顺序加载：`SKILL.md`、`evolution.md`、`contract-map.md`、`task-registry.md`、`orchestration-templates.md`、`templates.md`、`context-schema.md`、`system-perception.md`、`architecture.md`、`code-rules.md`、`ledger-schema.md`、`field-map.md`、`module-convention.md`、`software-arch.md`、`framework-template.md`、`checklist.md`、`debug-playbook.md`、`diagnostics.md`、`serial-cli.md`、`code-style.md`、`other-operations.md`、`consulting-design.md`。skill 增删或重命名文件时必须同步该列表，否则编译失败；`load_skill` 按同一优先级排序，超出 240KB 时整体跳过放不下的文件并在末尾列出，模型可用 `se-read` 读取 `.opencode/skills/stm32-embedded-dev/` 下的 `.md`。系统提示词要求按 L0 总则 O1–O14、L1 任务判定、L2 任务注册表、L3 模板 A–H 输出；修改已有文件用 `se-replace`，新建/整份重写用 `se-file`，删除用 `se-delete`。O14 明确“用户确认执行后不得只写计划或重复读取同一文件，已拿到正文必须直接落盘”。
- 已知限制：推理/正文通过 SSE 流式显示，但速度取决于服务商；没有请求取消（清空/退出只丢弃接收端，后台线程可能持续到读超时）；没有 function calling、自动应用、一键撤销；文件修改使用文本协议，模型不遵循 `se-file`/`se-replace`/`se-delete` 时只会显示普通代码块，需要用户手动复制；修改支持新建/覆盖/局部替换，删除只支持工程内普通文件（需再次确认、先备份，不支持目录、重命名或移动）；上下文可能超过小模型窗口，错误会显示在聊天里；目录 skill 不存在时用内置副本，修改 skill 后需要重新进入或点击“重新加载”，请求时始终重新读取目录 skill。项目架构面板只读 skill 生成的 `framework.md`：文档不存在时保持等待，不提供启发式替代；文档不完整时提示缺失小节并拒绝用猜测补内容。章节解析按固定标题关键字（1.1/2.1/2.2/2.3/3.1/3.2/3.3）匹配，自定义标题需包含这些关键字；Mermaid 只支持 `flowchart` / `graph`（TD/LR）和 `sequenceDiagram` 的常用语法，支持节点形状、实线/虚线/粗线、边标签、participant/Note/alt/else/loop 标签；不支持 subgraph、classDiagram、stateDiagram、timeline、主题样式和复杂语法，解析失败或类型不支持时回退显示源码。过宽的图需要横向拖动滚动条；甘特图仍只渲染 3.2 的简单 `gantt`，复杂 `after`/`dateFormat` 组合可能显示不完整，甘特图是文档规划视图而非实测时间轴。框架文档需要由 skill 生成/维护，AI 输出截断时不会写入半份文档，但用户仍需重新生成或补全。额外 Skill/参考资料会占用模型上下文（48KB + 96KB 上限），扫描版 PDF 提取不到文本，依赖外部资料时仍应让模型标注待核对事实；导入的 PDF/文档不会自动更新，删除或替换需要在用户目录中手动处理。

### 编辑器

- egui `TextEdit` 要求 `TextBuffer::as_str()` 返回连续切片，所以不要直接接 ropey。`EditorBuffer` 用 `String` + `Rc<Cell<u64>>` revision，所有插入/删除都会递增 revision。
- 高亮缓存键是 `(revision, wrap_width, font_size)`；修改文本后 `layout_cached` 会自动重建。换字号、换自动换行宽度也由键覆盖。Makefile / CMake 不走 syntect 的逐 token 语法，而用行级轻量高亮（变量名、注释、目标），避免 `C_INCLUDES` / `CFLAGS` 长行生成大量 section 导致输入卡顿。
- 文档打开时会规范化到内部 LF，保存时恢复原 CRLF/LF 和 BOM。
- `util::read_text_file` 先按 UTF-8 读取，失败后回退 GBK；`LoadedText.encoding` / `Document.encoding` 记录编码，保存时用 `write_text_file` 按编码写回。工具栏可切换 UTF-8 / GBK。
- 文件上限 16 MB；二进制文件不按文本打开，而是用只读十六进制视图显示（上限 32 MB）。
- `Document.binary` 保存 `HexDump`：`.bin` / `.elf` / `.axf` / `.o` / `.a` / `.lib` / `.exe` / `.pack` / 图片等扩展名直接进入十六进制模式；文本读取遇到 NUL 二进制错误也会回退到十六进制。编辑器中央显示偏移、HEX 和 ASCII，只读，状态栏隐藏编辑/编码操作；右键“十六进制查看”的独立窗口仍保留。
- 打开文件/切换 Tab 时，`focus_editor` 会请求焦点并清掉跨 Tab 的旧选区。`Ctrl+A` 由编辑器自行设置全选范围并消费按键，避免 egui 在选区变化时把视图滚到文件末尾。
- 左侧行号与当前行高亮；`Settings.relative_line_numbers` 切换相对行号，`Settings.ruler_column` 可选 80/120 列标尺，`Settings.show_whitespace` 显示空格/Tab/换行标记。
- `minimap.rs` 在编辑区右侧绘制约 76px 的代码预览：按 revision 缓存抽样行，用缩进/长度和近似语法色渲染；缩略图可点击或拖动跳转，并显示当前视口位置。`Settings.show_minimap` 控制开关，窗口过窄时自动隐藏。
- 缩进参考线按 `Settings.indent_width` 在每级缩进位置绘制，当前行所在缩进级别的参考线会高亮；`Settings.show_indent_guides` 控制开关。
- 书签按 `Document.bookmarks` 保存在内存中；`Ctrl+F2` 切换，`F2` / `Shift+F2` 前后跳转，书签行在行号栏左侧显示强调色竖条。
- `brackets.rs` 提供 C 风格注释/字符串感知的括号匹配；光标靠近括号时高亮配对，`Ctrl+]` 跳到配对括号。
- 查找/替换由 `FindState` 管理，支持大小写、全字匹配（`ab`）、正则和替换时保留大小写（`AB`）；匹配范围通过 `LayoutSpec.search_ranges` 进入高亮缓存，替换使用 `EditorBuffer::set_text`。当前文件内查找由右上角浮层完成。
- `Ctrl+G` 打开 `show_goto_overlay`：输入行号后回车，`goto_line_apply` 校验范围并设置光标/滚动到目标行；Esc 关闭。
- `search.rs` 负责全工程搜索：按 `IgnoreRules` 剪枝目录，跳过 2 MB 以上文件和二进制文件，最多 500 条结果；`Ctrl+Shift+F` 或查找浮层 `≡` 打开结果窗口，点击跳转。
- `ignore.rs` 实现 gitignore 风格匹配（`*`、`**`、`?`、目录、否定规则）。资源管理器读取工程根 `.gitignore`，也可在设置中追加分号分隔的忽略规则；`Settings.respect_gitignore` 控制是否启用。
- `indent.rs` 负责缩进：`Tab` / `Shift+Tab` 插删缩进单元（选中多行时整块处理），回车调用 `auto_indent` 保持缩进并在左括号后增加一级。`Settings.use_spaces` / `indent_width` 控制缩进风格。
- 行注释切换由 `src/comment.rs` 和 `App::toggle_line_comments` 实现：选中一行或多行后按 `Ctrl+Enter`（或标准 `Ctrl+/`），在每行缩进后插入行注释；若所选非空行都已注释则统一取消注释。空行跳过，已注释行在“添加”模式下不会重复加标记；按文件类型选择 `//`、`#`、`--`、`;`、`!` 或 `REM`，HTML/XML/CSS/JSON/Markdown 等没有行注释的文件不处理。普通编辑器和折叠编辑器都可用，选区会按插入/删除的字符数调整；执行后清除多光标/矩形选择并刷新查找匹配。
- 自动保存由 `App::tick_autosave` 的脏文档签名驱动：编辑停止 `Settings.autosave_delay_ms` 后调用 `save_all_dirty`，并在无输入时请求重绘；默认关闭。
- 每个 `Document` 有内存态 `read_only`；只读时编辑器不可交互，替换、编码切换、格式化与保存都会被拒绝。工具栏按钮切换，不持久化。
- `formatting.rs` 调用 clang-format：优先 `Settings.clang_format_path`，否则查找 PATH 和常见 LLVM 目录；`format_on_save` 失败只提示，仍会保存原文。没有内置格式化器，不要伪造成功。
- `git.rs` 通过本机 `git` 命令读取状态：`status --porcelain -z -uall --branch`，并用 `rev-parse --show-prefix` 把仓库根路径映射到工程子目录；`git diff HEAD -U0` 提供当前文件修改行。资源管理器显示 `M/U/A/D/R` 徽标，行号栏左侧黄色条表示修改行；`Ctrl+Shift+G` 打开只读状态窗口，不执行提交或网络操作。
- `todo.rs` 扫描工程中的 `TODO` / `FIXME` / `HACK` / `NOTE`（大写单词边界），跳过忽略目录和 2 MB 以上文件，最多 500 条；`Ctrl+Shift+T` 或资源管理器 TODO 按钮打开窗口，支持类型过滤与点击跳转。
- `symbols.rs` 用轻量正则索引 C/C++ 函数、struct/union/enum/class、宏和汇编标签，最多 2000 条；`Ctrl+Shift+O` 或工具栏“符号”打开过滤窗口。它不是完整的 C++ 解析器或 LSP，复杂声明可能漏检，不要宣称语义级准确。
- `hexview.rs` 读取最多 32 MB 文件，按 16 字节一行显示偏移/十六进制/ASCII；文件树右键或 `Ctrl+Shift+H` 打开当前文件，可从窗口把字节转给数组生成器。
- `arraygen.rs` 把选中文本或当前文件（最多 8 KB）生成 `const uint8_t` C 数组；工具栏“数组”或 `Ctrl+Shift+A` 打开，变量名会被清洗成合法标识符，支持复制。
- `registers.rs` 优先用正则解析工程内 `.svd`、`.pack` 内 SVD，或已安装 Keil DFP 的 `SVD/*.svd`，构建“外设 → 寄存器 → 位域”结构：peripheral/baseAddress/description、register/addressOffset/size/access、field 的 `bitOffset`+`bitWidth`、`lsb`+`msb` 或 `bitRange`；支持 peripheral 和 register 级的 `derivedFrom` 继承。没有可用 SVD 时回退扫描 `.h/.c` 中 `*_BASE` / `*_ADDR` / `*_ADDRESS` 宏。`Ctrl+Shift+R` 或工具栏“寄存器”打开浏览窗口，最多 2000 项并去重。
- `debugger.rs` 启动本机 gdb CLI（非 MI），通过 stdin/stdout 转发命令并收集控制台输出；`DebugState` 保存会话、断点、日志、当前暂停位置和 `running` 状态。右侧“调试”或 `F5` 启动/继续；目标运行中浮层主按钮显示暂停图案并发送中断请求，状态显示“运行中”，暂停后显示“已暂停”。ARM 目标暂停优先通过 OpenOCD 的 telnet 端口发送 `halt`，避免 Windows 管道下 Ctrl+C 无法中断；pyOCD / 主机回退到 gdb 异步 `interrupt`。`parse_stop_location` 同时识别 `#0 ... at file:line` 和 gdb 换行后的续行 `at file:line`。浮层最左侧的准心按钮（`DebugIcon::Locate`）可随时回到当前执行行，即使已经切换到其他文件。`F9` 或点击行号左侧空白切换断点，`F10` 下一步，`F11` 单步，`Shift+F11` 跳出，`Shift+F5` 停止。调试控制是编辑器右上角的小浮层 `show_debug_overlay`，使用 `Painter` 绘制继续/暂停/定位/停止/单步图案；调试日志显示在底部输出面板的“调试输出”页，可在页底输入 gdb 命令。
- 行号左侧的 gutter 点击范围覆盖整列（含断点红点），不要求先移动光标；断点按 `(路径, 行号)` 保存。gutter 行号使用与编辑器相同的 `galley.cursor_from_pos` 命中测试，避免滚动后行号偏移。`poll_debug_location` 只扫描新到达的 gdb 日志，用 `debugger::parse_stop_location` 解析 `... at file:line`，命中后自动打开对应文件、滚动到当前行并在 gutter 画黄色执行箭头和整行浅黄底；每次新停止都会重新跳转，即使上次已停在同一行。`next` / `step` / `finish` 后会额外发送 `frame`，因为 gdb 单步通常只回显源码行（`95  while (1)`）而不带 `at file:line`。gdb 报告 `No compiled code for line N in file ...` 时会移除对应断点并提示。日志扫描游标 `DebugState.log_scan` 在启动/停止时重置。
- 调试侧栏有 变量 / 寄存器 / 外设 三个页签。“外设”页使用 SVD 结构树：外设（`ADC1`）→ 寄存器（`ISR` / `CR` / `DR`）→ 位域（名称、`[msb:lsb]`、提取后的值）；外设和寄存器各自可展开/折叠，鼠标经过箭头显示手形，过滤词自动展开匹配外设。没有 SVD 时回退到头文件宏分组，并提供 **导入 SVD** 按钮把 `.svd` 复制到工程根目录后立即刷新。每次最多读取 160 个寄存器，用 `x/1xw` 读取实时值并以 `__SE_PREG_*` 标记路由到独立输出；目标停止且自动刷新开启时每 1.5 秒更新。每个寄存器和位域右侧的“改”在目标暂停时通过 gdb `set {unsigned ...}地址 = 值` 写入；寄存器按 SVD `size` 截断，位域使用读-改-写。OpenOCD 对未映射外设的 `Failed to read memory` 行会被过滤，读取失败的寄存器显示“无法读取”。
- 调试按目标类型分流：PE `.exe` 使用普通 gdb；ELF/AXF 进入 ARM 远程流程。ARM 流程复用 `BuildConfig.flash_tool` 的 OpenOCD / pyOCD 参数，用 `flash::openocd_server_spec` / `flash::pyocd_server_spec` 在空闲本地端口启动 GDB server，再由 `arm-none-eabi-gdb` `target extended-remote` 连接，依次执行 `monitor reset halt` / `load` / 再次复位，最后应用断点。首次启动继续时会下发 `tbreak main`，让目标先停在 `main` 并自动打开入口文件，而不是直接运行到第一个用户断点。启动在后台线程完成，通过 `debug_start` 通道回到 UI；停止/切换工程时同时结束 gdb 和 server。`debug_target` 优先使用工程的 `debug.program`；路径失效或为空时按修改时间在 `build/`、`Debug/`、`Release/`、`out/` 和工程根目录查找最新 ELF/AXF/EXE/OUT。打开工程时若 `debug.program` 为空会自动回填检测结果并写入工程配置。
- ARM gdb 由 `detect_arm_gdb` 自动检测：优先使用设置里的路径，其次搜索编译器目录、`extra_paths`、PATH 和常见 GNU Arm Toolchain 目录。配置 → 调试提供 gdb 路径浏览和自动检测按钮；`gdb_path` 既可填 exe，也可填 `bin` 目录。不要用 MinGW gdb 调 ARM 目标，否则会报 error 193。
- `navigation.rs` 提供标识符/前缀提取与字符区间替换；`F12` 在 `symbols.rs` 索引中找定义，`Shift+F12` 用全工程正则搜索列引用。
- 补全在输入至少 2 个标识符字符后自动弹出，锚定在光标下方；输入 `.` / `->` 时切换为成员补全。`↑` / `↓` 选择，`Enter` / `Tab` 插入，`Esc` 关闭；`Ctrl+Space` 仍可手动打开。全局候选来自 `symbols.rs` 的宏/函数/类型索引；成员候选由 `completion.rs` 用轻量正则解析 struct/union 和变量声明得到，成员索引在保存、刷新文件树或刷新符号时失效重建。它不是 C/C++ 语义解析：复杂宏展开、模板、继承、跨文件作用域和函数返回类型可能漏检。
- 编辑器内悬停标识符会弹出 `hover.rs` 构建的提示：先查当前文件声明，再查 `symbols.rs` 索引；显示声明签名、最近的 `/** */` 或 `//` 注释，以及“返回值 / 类型”。提示跟随鼠标显示（`on_hover_ui_at_pointer`）；`Settings.show_hover_info` 控制开关，配置 → 其他 的“变量 / 函数悬停提示”默认开启。类型提取按完整标识符边界匹配，避免把 `uint8_t i` 里的 `i` 误当成类型内部字符。符号索引在第一次渲染编辑器时确保扫描一次（`SymbolState.scanned`），项目切换时重置。复杂宏、跨文件局部变量和动态类型仍可能无法解析。
- `relations.rs` 在打开工程时用后台线程扫描已保存的 C/C++/头文件，去掉注释和字符串后建立函数定义、函数调用边和标识符引用索引；悬停卡片下方追加 **调用关系**（调用图 + 调用者/被调用树）和 **引用**（按文件分组）。调用图用 `Painter` 画圆角节点和箭头，左侧是调用者、右侧是被调用者；调用树只保留项目内函数到函数的关系。引用覆盖函数、变量、宏、结构体和枚举等标识符，定义为函数/宏/结构体声明时标“定义”。保存文件后 1.2 秒后台重建，项目切换时清空并重新扫描；索引建立中悬停会显示“更新中”。
- `macro_check.rs` 提供宏诊断：同一文件内同名 `#define` 重复始终标红并绘制红色波浪线；配置 → 其他 的“宏重复值提示（黄色波浪线）”（`Settings.check_duplicate_macro_values`，默认关闭）开启后，`PWM1PIN` / `PWM2PIN` / `PWM3PIN` 这类带数字的宏族使用相同替换值时绘制黄色波浪线。该启发式只针对项目代码，跳过 `Drivers` / `CMSIS` / `Middlewares` / `vendor` 等供应商头文件；C 语言本身允许不同宏名映射到相同值，所以它是可选提示而不是编译错误。宏结果缓存在 `Document.macro_issues`，按 revision 失效，设置保存和文件另存/重命名时强制重扫。
- 粘贴由编辑器在 TextEdit 前拦截 `Event::Paste`：`indent::adjust_pasted` 统一换行并按当前行缩进对齐后续行，同时保留粘贴内容相对缩进；只读或编辑器未聚焦时不拦截。
- 分屏只从标签页右键菜单进入：`向右拆分` / `拆分和移动 → 向上拆分` / `取消拆分`。`split_primary` + `split_path` + `split_direction` 决定两个窗格，两个窗格都调用 `show_code_editor`，都可编辑；`split_focus` 记录焦点窗格，点击窗格会切换 active。当前不支持同一个文件在两个窗格重复打开。
- 标签页右键菜单还提供关闭其他、关闭右侧标签页、关闭已保存、全部关闭；保存、另存为；复制完整路径、相对路径和面包屑路径；在文件资源管理器中显示、在资源管理器视图中显示；以及拆分操作。批量关闭遇到未保存文件时走 `PendingAction::CloseTabs` 确认。另存为会写入新路径并把标签页、分屏引用和语法识别一并切换到新文件。快捷键：Ctrl+F4 关闭当前，Ctrl+\ 向右拆分，Ctrl+K U 关闭已保存，Ctrl+K W 全部关闭，Ctrl+K S 保存全部，Ctrl+K Ctrl+\ 向上拆分，Ctrl+Shift+S 另存为，Shift+Alt+C 复制路径，Ctrl+K Ctrl+Shift+C 复制相对路径，Shift+Alt+R 在资源管理器中显示。
- 标签页右键菜单和所有弹出菜单跟随当前应用主题；`theme::install` 按主题的明暗设置 `ThemePreference::Dark` / `Light`，并从当前色板重装 egui 的菜单、弹窗、滚动条和输入框样式。窗口/弹窗的关闭 X 统一使用 `window_close_button`，尺寸和绘制方式与查找浮层的关闭按钮一致。`theme::install` 同时关闭 egui 的 `show_unaligned` 和 `warn_if_rect_changes_id` 调试覆盖层，避免 debug 构建在动画时出现红色控件边框。
- 查找是编辑器右上角的约 420px 紧凑浮层 `show_find_overlay`（内部复用 `show_find_bar`），只在 `find.open` 时由 `Ctrl+F` / `Ctrl+H` 打开，不占用独立行；左侧下拉箭头展开替换行，宽度不变，`AB` 保留大小写在替换输入框内，替换/全部替换为图标按钮。`Ctrl+G` 打开同宽的跳行浮层，输入框复用查找输入框样式；提示文字为普通次要色，不使用色块。
- 面包屑栏只显示从工程根目录开始的路径（`doc.rel_path` 的 `/` 替换为 ` > `）和未保存标记；编码、行尾、语言、只读、寄存器、数组、符号、格式化、自动换行、保存移到最底部状态栏。补全和列编辑不出现在可见工具栏。
- 左侧项目栏、编辑器区、右侧操作栏、底部输出区是四块带圆角的不同底色容器（外圈 3–4px `theme::gutter()` 深色间隙），不再用亮色分隔线；所有 `Panel` 设置 `show_separator_line(false)`。右侧操作栏宽约 58px（原 110px 的一半），按钮内边距同步收窄。编辑器卡底部两角由 `paint_bottom_corners` 补绘 `gutter` 圆角，输出面板标题栏紧贴面板顶部，拖动区域合并到标题栏顶部 4px，日志区使用 `editor_bg` 并撑满剩余空间。输出面板标题提供“构建输出 / 调试输出”切换，`OutputView` 决定显示哪一份日志；构建开始时切回构建，启动调试时切到调试。
- 右侧操作栏最底部是 **设置** 按钮，大小与构建 / 烧录等按钮一致；点击打开标题为“设置”的独立窗口。窗口左侧第一个菜单是 **编辑器主题**，右侧列出 40 套暗色 + 10 套亮色共 50 套主题，支持按名称或色值搜索。主题卡片展示名称、明暗标签、语法示例和 9 色色板；点击卡片即时预览整个应用，保存后写入 `Settings.editor_theme`，取消 / 关闭 / Esc 恢复原主题。`codetheme.rs` 只保存 9 个代码色字段，`theme.rs` 从中派生 bg / gutter / panel / surface / text / accent / border / success 等整套 UI 色，因此侧栏、菜单、状态栏和 Windows 标题栏会一起切换；不要把它退回成只给代码区换色。
- “配置”弹窗第一行是 构建 / 编辑器 / 调试 / 其他 四个页签，下方只显示当前页；构建页包含工具链下拉、自动检测、Build/Clean/Rebuild/Flash 命令、“并行任务数”（0 = 不启用，可一键填入 CPU 核心数）和“工具链路径”。**自动检测** 按钮紧挨工具链下拉，完整说明作为鼠标下方的悬停提示；“并行任务数”直接跟在 Rebuild（或 Flash）后面，不再有“并行构建”小标题和解释段落。`form_line` 的单行输入框高 26px，构建页行距收紧到 3px，命令行之间只留 1px 额外间距；配置页内容区固定为 400px（以构建页高度为准），切换页签时弹窗高度不变。“工具链路径”与 Build 等标签同列，右侧多行输入框高度为 Build 输入框的三倍（约 78px），内容超出时在框内滚动；它对应 `BuildConfig.extra_paths`，并在打开配置时合并自动检测到的工具目录。构建页不再显示工程路径，底部的“本地或 SSH 构建共用同一组命令”说明也已删除，只保留右下角的保存 / 取消按钮。“当前烧录工具为…”和并行构建说明段落已删除。自动检测先看工程根目录的构建文件，再查 PATH/常见安装目录。原来的“构建工具链”页已删除；软件自带 `tools/` 存在时自动优先使用，否则回退到旧配置路径、额外 PATH 和系统 PATH。调试页的调试程序和 gdb 路径在工程打开时读写工程的 `debug` 节点（无工程时才用全局设置），提供浏览和自动检测；调试程序为空时会自动回填 build 产物，gdb 路径既可填 exe，也可填 `bin` 目录。工具链选择 **MicroPython** 时，构建页在工具链行下方改为串口连接面板：端口下拉（弹层内可手动输入）、波特率下拉、端口旁的 **自动检测**，并隐藏构建位置、SSH、命令、并行任务和工具链路径；端口/波特率来自 `BuildConfig.serial`。
- 最底部状态栏是贯穿整个窗口宽度的矩形条（无圆角、无外圈间隙），内容靠右排列：右侧操作按钮 + 行列/缩进/编码/行尾/语言信息。
- 资源管理器、工程信息、构建配置、芯片支持包、烧录配置、C/C++ 列表等可折叠区域使用缓动动画，不直接瞬间显示/隐藏：侧栏高度动画 0.22s、`cubic_in_out`，其余折叠组使用 egui `CollapsingState` 和全局 0.20s 动画；箭头随 openness 旋转。动画期间主动 `request_repaint()`，避免掉帧式跳变。
- Toast 使用 12.5px 正文、10.5px 状态文字、14×10 内边距，最大宽度 460px；不要再把提示字号设为 9–11px。
- `column.rs` 仍保留旧列编辑算法；`column_select.rs` + `ColumnSelectionState` 提供 Ctrl+Alt+鼠标左键拖动的矩形列选择，蓝色高亮，Ctrl+C 复制、Delete/Backspace 删除、直接输入替换。列编辑对话框已不在任何可见入口暴露。
- `multi_cursor.rs` + `MultiCursorState` 提供多光标：Alt+Click 添加光标，Ctrl+Alt+Up/Down 在上下行添加光标，Ctrl+D 选择当前词并添加下一个匹配，Esc 清除；输入、Backspace/Delete、Enter、Tab、粘贴对所有主/次选择生效，Ctrl+C 复制全部选择。次级选择用半透明高亮和独立竖线绘制；普通点击或方向键移动主光标时清除多光标。多光标和矩形列选择互斥。
- `fold.rs` 提供 C 风格大括号块折叠（跳过注释和字符串）。鼠标悬停在可折叠起始行时，行号右侧显示下箭头 chevron，点击即可折叠；折叠后隐藏起始行和结束行之间的块内行，结束行（右花括号）保留可见，行号右侧显示右箭头，点击右箭头恢复（鼠标经过时变为手形）；折叠和展开时通过 `pending_scroll` 保持当前滚动偏移，不会跳回文件顶部。快捷键 `Ctrl+Shift+[` 折叠当前光标所在的内层块，`Ctrl+Shift+]` 展开当前文件全部块。行号栏继续显示源文件行号；`FoldBuffer` 把可见行的编辑映射回完整源文本，所以折叠状态仍可直接输入、删除和粘贴，保存时写完整内容。
- 撤销 / 重做是 `App.undo` 的跨文件快照栈：每帧比较各 Tab 的 `EditorBuffer` revision，记录 before/after 文本和光标；`Ctrl+Z` 撤销、`Ctrl+Shift+Z` / `Ctrl+Y` 重做，若目标文件已关闭会重新打开；普通新编辑会清空 redo。深度来自配置 → 其他 → **撤销栈深度**（默认 64，最大 64）。
- `log.rs` 的 `parse_location` 解析 `file:line:col`；日志行带位置时可点击跳转。`LogStore` 维护最多 1000 条问题诊断，`App::refresh_diagnostics` 解析为本地路径并在行号栏右侧画红/黄竖条；输出面板的构建视图可过滤全部/错误/警告，调试视图显示 gdb 日志和命令输入。
- 内置可选的 clangd LSP；未安装或未启用 clangd 时，补全、跳转和引用搜索回退到轻量符号索引。调试仍是基础 gdb 控制台，不是完整 IDE 调试前端。

### clangd 语言服务

- `lsp.rs` 用 `std::process` 启动 clangd，通过 stdio 交换 LSP JSON-RPC；读线程把消息送回 `mpsc`，UI 每帧 `poll`，不阻塞。`Session` 在 Drop 时发送 shutdown/exit 并回收进程。clangd 以 `--background-index=0` 启动，避免大工程后台索引长时间占用 CPU 导致编辑器输入卡顿；跨文件定义/引用以已打开文件和头文件为主。
- 启动时生成 `%APPDATA%\SimpleEmbedded\clangd\<hash>\compile_commands.json`：包含目录来自 `effective_includes()`，宏来自 `effective_defines()`，CPU/FPU 从根 Makefile 的 `CPU` / `FPU` / `FLOAT-ABI` / `MCU` 解析；ARM 工程补 `--target=arm-none-eabi`，并通过 `arm-none-eabi-gcc -E -Wp,-v` 收集 newlib 系统头目录。
- `detect_clangd` 依次查找设置里的路径、PATH、LLVM 默认目录、VS Code / Cursor clangd 扩展自带的 clangd；`Settings.clangd_enabled` / `clangd_path` 在 **配置 → 其他 → 语言服务** 修改。
- 打开/修改的 `.c/.h/.cpp` 通过 didOpen/didChange（280ms 防抖）同步；关闭标签或切换工程时 didClose / 重启 clangd。
- `F12` 定义、`Shift+F12` 引用优先走 clangd，失败或未就绪时回退到 `symbols.rs` 和工程搜索。引用结果在“LSP 引用”窗口列出并点击跳转。
- 悬停时发送 `textDocument/hover`，clangd 的 markdown 清洗后显示在原悬停卡片下方；补全在本地索引项之后异步替换为 clangd 结果（revision 和光标一致时才接受）。
- `Ctrl+Shift+F2` 或底栏 **重命名** 按钮发送 `textDocument/rename`，返回的 WorkspaceEdit 按 UTF-16 位置转换后应用：打开的文档改缓冲区并保持未保存，关闭的文件按原编码/行尾写回。
- 限制：没有 semantic tokens、clangd 诊断面板、代码操作、格式化或 inlay hints；clangd 缺失时功能自动回退；大工程首次后台索引时补全可能延迟。

### 工程信息

- `project_meta.rs` 按 `BuildConfig.effective_build_system()` 读取 Makefile 或 Keil `.uvprojx`，提取目标名、芯片、CPU、浮点、链接脚本、内存布局、包含目录、宏定义等。CubeMX 同时生成 `MDK-ARM/*.uvprojx` 和根 Makefile 时，Make 构建优先 Makefile 元数据，Keil 工具链优先 `.uvprojx`，不会再无条件显示 Keil。Makefile 的 `CC` / `PREFIX` 识别 GCC、Clang、armclang（AC6）和 armcc（AC5）；芯片型号优先从具体链接脚本名（如 `STM32F407ZGTx_FLASH.ld`）提取，`XX` 通用名回退到 `C_DEFS`。
- 工程信息 → 构建配置里的 **工具链** 只读显示当前有效工具链；鼠标悬停在工具链值上时，才在鼠标下方显示“在 配置 → 构建 中修改”，不再占用单独一行。不能在这里直接改。修改入口是配置 → 构建 → **工具链** 下拉（自动检测、Make · GCC/Clang/AC5/AC6、Keil MDK · AC5/AC6、Cargo、CMake、MicroPython、自定义）。保存设置后重新计算工程元数据、内存布局、芯片支持包和调试产物；构建命令与所选工具链不一致时显示黄色警告。`BuildConfig.toolchain` 保存在工程配置中。配置里的 **自动检测** 顺序是：先看工程根目录的 `Cargo.toml` / Makefile / `CMakeLists.txt` / `*.uvprojx`，确定期望的构建系统；再在 PATH 和常见安装目录中查找对应工具（Keil 会查 `UV4/UV5` 和常见 `Keil_v5\UV4` 目录）。根目录只有 `main.py` / `boot.py` 时识别为 MicroPython；没有构建文件时才回退到 PATH 中的 cargo / cmake / make。
- MicroPython / ESP32：`BuildConfig.serial` 保存 `port` 和 `baud`（默认 115200），随工程 `.simple-embedded/settings.json` 一起持久化。构建页选中 MicroPython 后隐藏构建位置、SSH、Build/Clean/Rebuild/Flash、并行任务和工具链路径，改为串口连接面板：端口下拉（弹层内可手动输入 COM3 或 `/dev/ttyUSB0`）、波特率下拉和 **自动检测**。`serial.rs::scan()` 使用 `serialport::available_ports()`，按 VID/产品名给端口打分并排序，优先 Espressif `0x303A`、CP210x、CH340/CH910x、FTDI、PL2303 等；打开配置或切换工具链时扫描，自动检测按钮总是选择当前最佳端口。面板提示未检测到设备时仍允许手动端口，保存时要求端口非空、波特率在 1200–4000000。右侧操作栏在 MicroPython 模式变为 **运行 / 上传 / 固件 / 复位**：运行把当前 `.py` 内容通过 `micropython.rs` 的 raw REPL 上传为 `main.py` 并发送 Ctrl-D 软复位；上传保留当前文件名写入开发板；固件调用 esptool 烧录 `.bin`；复位只发送 Ctrl-D。`micropython.rs` 负责 Ctrl-A raw REPL、base64 分块上传脚本、`\x04>` 响应解析和 1.6s 启动日志采集；上传上限 128 KB。MicroPython 模式隐藏 Clean/Rebuild/检测，运行/上传/固件/复位在准备就绪时可用；构建命令不会被调用。
- MicroPython 模式的左栏把“工程信息”替换为 **芯片文件**：展开后自动通过 raw REPL 执行 `os.ilistdir` 遍历根目录和最多两层子目录，列出相对路径、目录标记和文件大小；**刷新** 按钮固定在分区底部，读取会暂停板上正在运行的程序。文件列表使用与资源管理器一致的树形行：缩进参考线、可展开/收起的文件夹、文件夹/文件图标、悬停高亮和右对齐文件大小；目录层级由 `micropython::board_tree` 从扁平列表构建。点击文件会读取芯片内容（上限 128 KB），保存到工程对应相对路径；同名文件自动加“副本”后缀，然后打开到编辑器。下载时记录本地路径到芯片原路径的映射，之后点击 **上传** 会优先写回原芯片路径，即使本地是“副本”文件；`main.py` 修改后用 **运行** 会写回 `main.py`。芯片文件列表状态保存在内存中，切换工程或保存串口配置后失效并自动重读；列表读取、单文件读取、运行/上传/复位和线程异常都会同时写入芯片文件面板提示与 MicroPython 输出。
- MicroPython 固件烧录：`BuildConfig.firmware` 保存 `esptool`、`file`、`chip`、`address`、`erase`、`verify`，随工程配置持久化。串口面板提供固件 `.bin`/esptool 路径（可浏览和自动检测）、芯片下拉、烧录地址、整片擦除和写入校验；右侧 **固件** 按钮打开确认窗口，确认后后台调用 esptool。ESP32/ESP32-S2 默认地址 `0x1000`，S3/C3/C6/H2 默认 `0x0`；`erase_flash` 作为独立步骤执行，随后 `write_flash -z`。`micropython.rs` 先运行 `esptool version` 探测主版本：4.x 使用 `erase_flash` / `write_flash`，关闭写入校验时传 `--no-verify`；5.x 使用 `erase-flash` / `write-flash`，校验由 esptool 自动完成，不再传 `--verify`/`--no-verify`，取消勾选时会提示该开关被忽略。烧录成功会清空芯片文件列表和本地→芯片路径映射；烧录失败、esptool 缺失、串口占用等会写入 MicroPython 输出和芯片文件面板。需要用户自行安装 esptool 并准备合并固件。
- 左栏先显示资源管理器，再显示“工程信息”；分割线紧贴“工程信息”标题上下，包含“构建配置”“芯片支持包”“烧录配置”“C/C++ 属性”四个分组。分组标题以及三个 C/C++ 列表标题左侧各有折叠箭头，整行可点击并切换展开；点击行会记录选中状态并显示淡蓝色背景，加号只在鼠标悬停对应行时显示，点击只添加、不折叠。信息行固定 18px 高、行距 2px，`interact_size.y` 在工程信息内收紧到 20px，整体密度接近资源管理器。构建配置的“编辑”、C/C++ 属性的“自动检测包含目录”/“批量编辑”、烧录配置的“编辑”和“内存布局...”都使用内容宽度的按钮并左对齐。展开和选中状态存在 egui 内存中。数据按工程缓存，刷新文件树时重新读取。
- “芯片支持包”分组右侧的绿色 `+` 打开文件选择器，只接受 Keil/CMSIS `.pack` / `.pdsc`，并把文件复制到工程根目录（同名文件不覆盖）。`support_pack.rs` 解析包名、厂商、版本、设备列表和 SVD 数量；`.pack` 通过 zip 读取包内 `.pdsc`，不整体解压。寄存器浏览器在工程内没有独立 `.svd` 时，会直接读取第一个 `.pack` 内的 SVD。支持包不会生成自定义工程文件，也不代表完整的 Keil 设备树/启动文件/烧录算法导入。
- 包含目录、库搜索目录和预处理器定义支持在左栏编辑：每个列表右侧有绿色 `+`，批量编辑仍可用。包含目录下方的 **自动检测包含目录** 只递归扫描工程内 `.h` / `.hpp` / `.hh` / `.hxx` / `.inl` 头文件，跳过构建目录、隐藏目录和忽略目录，按绝对路径去重后把父目录追加到列表；再次点击时会移除只含 `.c` 而没有头文件的目录（旧版本误加的 `Core/Src`、`Drivers/.../Src`）。旁边的 **同步源文件到 Makefile** 扫描 `.c` / `.cc` / `.cpp` / `.cxx`，把未加入的源文件追加到 `C_SOURCES`；跳过 Drivers / Middlewares / CMSIS / third_party / vendor 等供应商目录，修改前备份，已打开且未保存的 Makefile 会拒绝写入。覆盖值存 `BuildConfig.meta_includes` / `meta_library_dirs` / `meta_defines`（`None` 表示自动检测）。仅当有效构建系统是 Make 时保存会同步到 `C_INCLUDES` / `LIBDIR` / `C_DEFS`，修改前备份到 `%APPDATA%\SimpleEmbedded\backups`；选择 Keil AC5/AC6 时只写工程配置，`.uvprojx` 不会被修改。
- “构建选项”编辑器存 `BuildConfig.build_options`，分为全局选项、C/C++ 编译器、汇编器、链接器四个页面；显示在构建配置中。打开 Make 工程时会从 `OPT` / `CFLAGS` / `AS_DEFS` 等解析当前值，保存时只回写发生变化的字段；链接器输出格式选 `AXF` 时会在 Makefile 末尾添加受管理的 `.axf` 复制规则。全局选项页按工具链切换：GCC/Clang 显示 Use MicroLIB 作为总开关，勾选后才启用 newlib-nano `printf_float` / `scanf_float`（`-u _printf_float` / `-u _scanf_float`），并显示函数分段/LTO；Keil AC5/AC6 显示 Use MicroLIB、Split Load/Store 和 Keil 链接器字段，隐藏 newlib 开关。弹窗内容区固定为 320px（以 C/C++ 编译器页高度为准），切换页签时高度不变；工具链信息由左侧构建卡片显示，不再在弹窗内重复。只有有效构建系统为 Make 且工具链为 GCC/Clang 时才同步构建选项到 Makefile；Keil 和 Make AC5/AC6 只写入工程的 `.simple-embedded/settings.json`，不修改 `.uvprojx`。
- 没有可解析的工程文件时只依赖 `BuildConfig`，不要为空字段编造值。

### 文件树

- 工程根目录作为文件树第一行显示（默认展开），整棵树相对“资源管理器”标题再缩进一级，使根目录明确作为该分组的子级；子项相对根继续缩进。根行同样可点击展开/收起，右键提供新建文件/文件夹、粘贴、在文件资源管理器中显示、复制路径/相对路径、用 VS Code 打开。
- 目录展开/收起使用每个目录独立的 egui `CollapsingState`（0.20s 动画）和裁剪区域，行内容随 openness 高度渐显/收起；箭头是和工程信息一致的蓝色双线 chevron，按 openness 旋转，并为每级深度绘制 `BORDER` 垂直缩进参考线，参考线位于父级 chevron 正下方，文件行也延续父级竖线。`expanded` 集合同步保存逻辑展开状态，`reveal` 展开父目录时也走同一动画。
- 文件右键菜单：在侧边打开、剪切、复制、复制路径、复制相对路径、重命名...、删除，以及查看/十六进制/在资源管理器中显示；文件夹右键额外提供新建文件/文件夹和粘贴，也支持剪切、复制、重命名和删除。
- 文件操作使用应用内 `FileClipboard`（复制/剪切），不读写系统剪贴板文件；粘贴到文件时使用其父目录。复制重名时生成 `名称 - 副本.ext`、`名称 - 副本 (2).ext`，剪切遇到同名目标会提示，跨卷移动失败时回退为递归复制再删除。重命名和删除使用独立确认弹窗；删除前若存在未保存 Tab 会拒绝执行。重命名/剪切会同步更新打开 Tab 的路径、分屏引用、断点、调试暂停位置和图像预览；删除会关闭相关 Tab 并清除这些引用。删除 `.c/.cpp` 文件或包含源文件的目录后，`makefile::remove_sources` 会从 Makefile 的 `C_SOURCES` 中移除对应条目（Makefile 有未保存修改时跳过并提示）。
- 懒加载目录子项，目录优先、名称不区分大小写排序。
- 默认隐藏 `.` 开头的文件/目录，可通过设置或左下按钮切换。
- `FileTree::reveal` 会展开当前打开文件的父目录。
- 目录缓存存在内存中；刷新按钮清空缓存。当前没有文件系统 watcher。

### 构建与日志

- `BuildConfig::make_spec` 负责把用户命令变成 `RunSpec`：
  - 本地：Windows `cmd /S /C`，Unix `sh -c`。
  - 远程：`ssh -o BatchMode=yes -o ConnectTimeout=10 [ -p port ] host "cd '<remote_dir>' && <command>"`。
  - Flash 永远本地执行。
- `BuildConfig.flash_tool` 为 `custom` / `openocd` / `pyocd`。`custom` 使用 `flash_command` 经 shell 执行；`openocd` 使用 `flash.rs` 直接构造参数运行 `openocd -f interface/... -f target/... -c "program ... verify reset exit"`；`pyocd` 直接运行 `pyocd flash -t <target> [--pack <pack>] <program>`，两者都不经过 shell，路径空格由 `Command` 处理。`OpenOcdConfig` 保存工具路径、程序文件、芯片配置和接口配置；`PyOcdConfig` 保存工具路径、目标芯片、可选 Pack 和程序文件。程序文件支持 `${ExecutableName}`、`${ProjectName}`、`${workspaceFolder}`、`${workspaceRoot}`。工具路径下拉会检测 PATH、OpenOCD 安装目录、Python Scripts 等常见位置。
- OpenOCD / pyOCD 的程序文件默认按 `ProjectMeta` 的 output_dir 和常见 `build/Debug/Release` 目录选择最新产物；Flash 前找不到程序文件会直接报错，不自动构建。切到 pyOCD 时会用工程芯片名转小写推断目标名，并优先把工程根目录的 `.pack` 填入 Pack。Flash 不依赖编译器/make，软件自带工具链缺失时不应阻止烧录。
- `BuildConfig.build_jobs`（0 = 不启用）用于并行构建：Make 注入 `MAKEFLAGS=-jN`，Cargo 注入 `CARGO_BUILD_JOBS`，CMake 注入 `CMAKE_BUILD_PARALLEL_LEVEL`。命令里已有 `-j` / `--jobs` / `--parallel` 时不再注入；只作用于 Build/Clean/Rebuild，不影响 Flash。本地通过 `RunSpec.env` 传给子进程，远程 SSH 在远端命令前拼接 `export NAME=...;`。
- 构建前自动保存所有脏 Tab。
- `BuildController` 用 `std::process`、线程和 `mpsc`，不使用 tokio。UI 每帧 `poll`，不阻塞。
- stdout/stderr 各一个读线程；Windows 上停止用 `taskkill /PID <pid> /T /F`，随后 `Child::kill`。
- 日志经过 ANSI SGR 解析，保留颜色；错误/警告行有更高优先级颜色。日志上限约 20,000 行 / 8 MB。
- 构建结束若为链接失败且日志含 `undefined reference to`，`App::report_link_problem` 会提取缺失符号并在输出面板追加提示：检查对应 `.c` 是否加入 `C_SOURCES`、函数是否实现、是否误加 `static`。
- 日志是固定行高、虚拟化绘制；超长行横向裁剪，用户可悬停查看或复制全部日志。
- Make 命令在启动时探测 PATH：`make` → `gmake` → `mingw32-make`。
- 本地 Make 构建自动探测 Git for Windows / MSYS / Cygwin 的 `usr\bin` 并加入 PATH，保证 `rm` 等 POSIX 命令可执行。
- `BuildConfig.extra_paths` 允许用户追加工具链目录，构建时统一前置到 PATH。
- `Settings.compiler_path` / `make_path` / `use_bundled_toolchain` / `bundled_tools_dir` 是为旧版本保留的全局构建工具链字段，设置界面已不再暴露。`build_env_dirs` 优先使用软件自带 `tools/`（`bundled_tools_dir` 留空时 `<exe>/tools`，否则 `%APPDATA%\SimpleEmbedded\tools`）；自带 make/compiler 不完整时回退到旧 `compiler_path` / `make_path`、构建页的额外 PATH 和系统 PATH，不再因为缺少自带工具而阻止本地构建。`toolchain.rs` 仍保留检测和复制安装函数供兼容/测试使用。只影响本地构建和本地烧录，远程 SSH 不注入本地 PATH。
- 软件自带工具的标准发布布局是 `simple-embedded.exe` + `tools/toolchain.json` + `tools/make/make.exe` + `tools/compiler/bin/arm-none-eabi-gcc.exe`。`scripts/package-tools.ps1` 会把 release exe、当前 ARM GCC 和 make 打包到 `dist/simple-embedded/`。仓库只追踪 `dist/` 下的所有 `*.exe`（应用 exe 和 `tools` 里的工具 exe）；编译器 DLL、库、头文件、`tools.zip`、`.compiler.tmp`、`.make.tmp` 仍被 `.gitignore` 忽略。仅 exe 不能构成完整可运行工具链，完整发布包仍需本地 `tools/` 目录。
- 远程停止只保证结束本地 ssh 进程；远端进程是否退出取决于远端 shell。

### 配置

全局设置路径：

```text
%APPDATA%\SimpleEmbedded\settings.json
```

工程设置路径：

```text
<工程目录>\.simple-embedded\settings.json
```

- 打开工程时优先读取工程的 `.simple-embedded/settings.json`；不存在时回退到用户目录里的每工程配置，并自动迁移写入工程文件。迁移成功后删除用户目录里的旧条目，避免删除工程配置后被旧缓存恢复；只有工程文件写入失败时才保留用户目录回退。
- 工程文件是普通 JSON（`ProjectSettings { version, build, debug, cli }`），保存 Build/Clean/Rebuild/Flash/SSH、OpenOCD/pyOCD、MicroPython 串口 `serial.port` / `serial.baud`、MicroPython 固件 `firmware.file` / `firmware.esptool` / `firmware.chip` / `firmware.address` / `firmware.erase` / `firmware.verify`、并行任务数、C/C++ 属性覆盖、构建选项、内存布局 `build.memory`、调试程序 `debug.program` 和 gdb 路径 `debug.gdb_path`，以及串口 CLI `cli.port` / `cli.baud` / `cli.line_ending` / `cli.timeout_ms`。可以手改，也可以和源码一起提交。
- 全局设置继续保存最近工程、编辑器偏好、主题（`Settings.editor_theme`）、输出面板状态、clangd 开关和路径（`Settings.clangd_enabled` / `clangd_path`）等；启动时先读取该字段并用 `theme::apply` 派生整套 UI 配色，再打开上次工程。`Settings::load` 容忍 UTF-8 BOM，解析失败会回退默认值并输出到 stderr。
- clangd 的编译数据库写到用户目录 `%APPDATA%\SimpleEmbedded\clangd\<工程路径 hash>\compile_commands.json`，不写入工程；工程配置变化后保存设置会重新生成。
- Makefile 备份位于 `%APPDATA%\SimpleEmbedded\backups\<工程名>-<hash>\`，默认保留最近 12 份。
- 自动检测结果只是初值；用户可以随时在设置里改。修改设置后必须点击保存。
- 工作区文件：`<工程目录>\<工程名>.se-workspace`，内容是 VS Code 风格 JSON（`version`、`name`、`folders`、`settings`）。`config::write_workspace_file` 生成，`config::resolve_workspace_file` 解析；只使用第一个 folder。`register_workspace_association` 用 `reg.exe` 写入 HKCU 的 `.se-workspace` 关联，关联当前 exe。
- 不要创建 `.eide`、`.mini-ide` 等专有工程格式，也不要修改 Keil `.uvprojx`。允许写入工程的场景：自动创建/保存 `.simple-embedded/settings.json`、用户主动生成 `.se-workspace`、保存 Makefile 工程的构建选项/C/C++ 属性时的差异同步、用户主动添加芯片支持包时复制 `.pack`/`.pdsc`。

### 系统安装与 Explorer 集成

- `install.rs` 提供用户级安装，不请求管理员权限。默认安装到 `%LOCALAPPDATA%\Programs\SimpleEmbedded`；可用环境变量 `SIMPLE_EMBEDDED_INSTALL_DIR` 覆盖（测试用）。安装时复制/硬链接当前 exe 和旁边的 `tools/`（跨盘时自动退回复制），跳过 `.compiler.tmp` / `.make.tmp` 打包临时目录。
- 安装后写入 `HKCU\Software\Classes\Directory\shell\SimpleEmbedded` 和 `Directory\Background\shell\SimpleEmbedded` 的 `用 Simple embedded 打开` 菜单，命令指向安装后的 exe；开始菜单快捷方式写入 `%APPDATA%\Microsoft\Windows\Start Menu\Programs\Simple embedded.lnk`。
- Windows 应用列表卸载项位于 `HKCU\Software\Microsoft\Windows\CurrentVersion\Uninstall\SimpleEmbedded`，`UninstallString` 为 `"<安装目录>\simple-embedded.exe" --uninstall-system`。卸载会删除右键菜单、快捷方式、`.se-workspace` 关联和安装目录；运行中的 exe 通过 `%TEMP%` 下的延迟 PowerShell 脚本在退出后删除，`%APPDATA%\SimpleEmbedded` 用户设置保留。
- 配置 → 其他 → **系统集成** 提供“安装到系统 / 重新安装更新”和“移除右键菜单与关联”；安装任务在后台线程执行，状态通过 Toast 和设置页提示反馈。CLI `--install-system` / `--uninstall-system` 供脚本和 Windows 应用列表调用。
- 右键菜单是 HKCU 用户级注册表；Windows 11 默认精简菜单中可能位于“显示更多选项”。不做 MSI、系统级 Program Files 安装或提权卸载。

### 工程资源报告

- 工程信息底部的 **资源报告** 分组点击 **生成资源报告** 后，`project_report.rs` 在后台线程扫描工程，并写入根目录 `<工程名>.resources.json`（如 `blink.resources.json`）。
- 报告 JSON 包含：`application`（版本 / 作者 / 可执行文件 / debug/release）、`project`（路径、工程配置、构建系统、工具链、构建 / 调试配置、有效包含目录 / 库目录 / 宏）、`hardware`（芯片、CPU、FPU、Flash/RAM、使用率、固件 ELF 数据 / bss / 栈堆 / 最大全局变量、SVD 外设 / 寄存器、`.ioc` 引脚和组件）、`software`（语言、源码 / 头文件 / Python / 链接脚本 / 启动文件 / 库清单、源码行数、组件识别、符号统计、调用图统计、支持包）、`system`（系统、架构、报告文件、扫描统计）和 `warnings` / `notes`。
- 文件清单跳过 `build` / `Debug` / `Release` / `out` / `target` / `Objects` / `Listings` / `tools` / `dist` / `.git` / `.simple-embedded` / `node_modules` 等目录；最多扫描 2 万文件、单文件 2 MB，源码/头文件列表最多各 500 个，超限在 `counts.truncated` 标记。
- `.ioc` 解析 `Mcu.IP*`、`ProjectManager.DeviceId`、`*.Signal` 和 `*.GPIO_Label`；SVD/寄存器索引和符号索引优先复用当前内存中的结果，缺失时由报告线程重新扫描。报告只读工程，不改写 `.uvprojx`、Makefile 或链接脚本。
- 报告是静态分析，不是电路网表或 BOM；引脚和组件来自 `.ioc` / 文件路径启发式，函数调用图只统计项目内函数，未就绪时 `warnings` 会说明。报告使用已保存文件和当前内存索引，未保存的编辑不会进入清单或符号统计。

## UI/产品约定

- 用户界面使用中文；默认暗色主题，同时支持 50 套暗色 / 亮色主题，保持 `docs/` 截图的深色工程风格。
- 界面不显示 emoji/生僻符号；使用 `Painter` 画图形或已验证的字符。
- 右侧操作栏：纵向排列 Build、Clean/重构、Flash、调试、检测，最底部是 **设置**；构建配置仍从构建卡片“配置”或 `Ctrl+,` 打开。MicroPython 模式下隐藏 Clean/重构/检测，Build/Flash/调试分别显示为 **运行 / 上传 / 复位**，并额外显示 **固件** 按钮；运行上传为 `main.py` 并软复位，上传写回原芯片路径，固件打开 esptool 烧录确认窗口，复位只发送 Ctrl-D。检测按钮在工程打开后即可用：无调试会话时运行静态扫描并打开“非法检测 → 静态检测”；有调试会话时打开“动态监测”，可添加硬件 watchpoint（写入 `watch`、读取 `rwatch`、读写 `awatch`，GDB 优先使用硬件比较器，Cortex-M 通常 2-4 个），命中后目标自动暂停并记录本机时间、旧值 → 新值、GDB 停止位置和调用栈（每个监测点最多 200 条记录）。动态监测页可勾选“调试时自动启动动态监测”，启动调试时自动打开面板，并在临时 `main` 断点暂停后再重新启用已保存的 watchpoint（避免启动清 `.bss` 时误命中）；停止调试后表达式和历史保留。OpenOCD 把命中报告为 `SIGTRAP` 而不是 `Old value` / `New value` 时，应用通过 GDB convenience variable 查询命中前的值和当前值补记录。静态检测扫描所有 `.c/.h`：数组常量下标越界、`strcpy`/`strcat`/`strncpy`/`memcpy`/`memset` 常量长度、`gets`/`sprintf`、重复 `free`、可疑 `malloc` 字节数、返回数组名；结果在窗口内按文件:行显示，可点击跳转。静态扫描是启发式检查，不替代 ASAN/Valgrind；动态 watchpoint 依赖 gdb / OpenOCD / pyOCD 会话。
- 构建结果和内存占用显示在输出面板标题和日志中；构建视图标题显示 `Flash 已用 / 总量 (百分比) · RAM 已用 / 总量 (百分比)`，占用来自 `text/data/bss` 或 ELF 段，容量来自链接脚本 / PDSC / 工程信息。输出面板可切换“构建输出 / 调试输出”；MicroPython 模式第一个页签显示为“MicroPython 输出”，运行/上传/固件/复位的串口日志写入这里。底部不再保留重复的状态栏。
- 构建输出面板顶部有 10px 拖动手柄，收起时也能向上拉出；高度存 `Settings.output_height`，最大到主区域顶部。
- 构建输出视图右侧固定显示 **内存可视化** 侧栏（默认 380px，可拖动 6px 分隔条调整，高度与调试侧栏一致）：`memory.rs::analyze_elf` 解析 ELF32/64 符号表，列出写入型 `.data` / `.bss` 全局变量（最多 200 个，展示前 14 个）、`_estack` / `_Min_Stack_Size` 栈区、`ucHeap` / `.heap` FreeRTOS 堆区；调试暂停且寄存器可用时用 SP 估算当前栈使用，否则显示静态预留。缓存按 ELF 路径/修改时间刷新，面板内 **刷新** 强制重读。
- 调试控制是编辑器右上角的小浮层，位置参考查找浮层；停止、单步等使用 `Painter` 绘制的图案按钮。调试信息放在输出面板“调试输出”页，不在独立窗口里。
- “工程信息 → 烧录配置”提供烧录工具下拉（自定义命令 / OpenOCD / pyOCD）。OpenOCD 模式显示工具路径、程序文件、芯片配置和接口配置；接口配置是预设下拉（ST-Link、CMSIS-DAP / DAPLink、J-Link、XDS110、ULINK 等）+ 自定义文本框 + **自动检测** 按钮。`OpenOcdConfig::default()` 的接口为空，不再自动填 `stlink.cfg`；自动检测通过 Windows PnP（或 `lsusb`）VID 识别已连接的 ST-Link / CMSIS-DAP / DAPLink / J-Link，失败时提示手动选择。pyOCD 模式显示工具路径、程序文件、目标芯片和可选 Pack；“编辑”打开独立烧录配置弹窗，含命令预览。分组还显示 Flash/RAM 容量和 **内存布局...** 按钮：弹窗采用 Cortex-Debug 风格两列网格，Read/Write 侧为 IRAM1/2 与 RAM1-3（Default、起始地址、大小、NoInit），Read/Only 侧为 IROM1/2 与 ROM1-3（Default、起始地址、大小、启动），底部显示 RAM Total / ROM Total；自动值优先来自链接脚本，其次来自匹配 PDSC 的 `<memory>`；保存到工程配置但不改写链接脚本。工具/路径下拉修改后立即写入工程 `.simple-embedded/settings.json`，弹窗中的字段点击保存后写入。**内存布局...** 按钮放在 Flash/RAM 行的上方，**编辑** 是居中显示在分组底部的按钮。
- 关闭窗口、关闭 Tab、切换工程时都要处理未保存确认。
- 不做弹窗堆叠；确认弹窗和配置弹窗互斥。
- 无工程时左上角仍显示同一张构建卡片，使用“导入 / 配置 / 工作区”按钮；中间显示单行提示。第一次启动且未打开工程时不写入任何工程文件；打开工程会创建/更新 `.simple-embedded/settings.json`。测试后如果产生 `%APPDATA%\SimpleEmbedded\settings.json` 或临时工程配置，提交前应清理，避免把测试工程路径带入首启体验。

## 已知边界（当前版本）

- 没有 `.eide`/`.mini-ide` 等专有工程格式、Keil 自动导入或链接脚本生成器；`.simple-embedded/settings.json` 和 `.se-workspace` 都是普通 JSON。芯片支持包只读取元数据、SVD 和器件内存区域，不导入 Keil 设备树、启动文件、链接脚本或烧录算法。Keil 工程可以通过手动命令或外部包装脚本接入。
- 工具链下拉只改变元数据、内存布局和构建选项展示，不会自动改写 Build/Clean/Rebuild 命令；选择 Keil AC5/AC6 后仍需把构建命令配置为 UV4/UV5 或自定义包装脚本，界面会对“构建命令与工具链不一致”给出黄色提示。Keil 构建选项只写入工程配置，不修改 `.uvprojx`。
- MicroPython 已接入工具链选择、串口配置、当前文件上传、运行（上传为 `main.py` + Ctrl-D 软复位）、复位、左侧芯片文件列表/读取到工程，以及通过 esptool 烧录 MicroPython `.bin` 固件；尚未实现交互式 REPL、目录上传、`boot.py` 特殊流程或自动下载固件。固件烧录需要系统安装 esptool，并正确选择芯片和烧录地址（ESP32/ESP32-S2 默认 `0x1000`，S3/C3/C6/H2 默认 `0x0`）；整片擦除会清除板上文件系统。上传和读取上限均为 128 KB，读取芯片文件会暂停板上程序；上传期间串口被占用、开发板未运行 MicroPython 或超时都会在 MicroPython 输出中显示错误。端口自动检测依赖 `serialport` crate 的枚举结果，按 USB VID/产品名启发式排序，不能保证每个 ESP32 开发板都被识别为“ESP32”。
- OpenOCD 自动检测是 USB VID/PID 启发式：Windows 使用 PnP，Linux 使用 `lsusb`；同时连接多个探针时按 ST-Link、CMSIS-DAP/DAPLink、J-Link 的优先级取第一个，用户可在接口下拉中改选。它不是对 OpenOCD 支持的全部适配器做穷举。
- SVD 外设树不展开 `dim` 数组、`cluster` 和 `enumeratedValues`；寄存器读取固定为 32 位 `x/1xw`，64 位寄存器只显示低 32 位。写入依赖 gdb `set {unsigned ...}`，目标必须暂停，只读位/外设是否真正接受写入由硬件决定。
- 没有安装或启用 clangd 时，符号搜索、TODO、寄存器浏览器和补全/定义/引用仍是轻量索引；成员补全只解析常见的 struct/union 和变量声明，不处理宏展开、模板、继承或函数返回类型。clangd 启用后语义级定义/引用/悬停/补全/重命名以 clangd 为准；没有 clangd 诊断面板、语义高亮、代码操作或 inlay hints。
- 悬停调用/引用索引是文本级预览，不是语义分析：同名 `static` 函数会合并，函数指针调用、宏展开、重载、继承和条件编译块无法准确解析；引用最多保留每个符号 24 处、全局 6 万条，悬停只展示前 3 个文件各 2 处。只索引已保存文件和常见 C/C++ 源码/头文件，跳过 `build` / `Debug` / `Release` / `out` / `target` / `Objects` / `Listings` 等生成目录；注释和字符串不参与。
- 系统安装是 HKCU 用户级集成，不写 Program Files，不需要管理员权限；Windows 11 精简右键菜单可能需要点“显示更多选项”。卸载保留 `%APPDATA%\SimpleEmbedded` 用户设置、备份和最近工程；安装目录中的自带工具链较大，首次安装/更新可能需要等待后台复制或硬链接完成。
- 工程资源报告是静态启发式分析：SVD 外设是芯片能力清单，不代表全部已使用；引脚和 CubeMX 组件来自 `.ioc`，没有 `.ioc` 时为空；组件识别依赖路径关键字，外部器件、网络和实际连线无法自动发现；源码清单跳过 `tools` / `dist` 等目录，同名报告会被覆盖，不替代原理图、BOM 或链接映射文件。
- AI 模式使用 OpenAI 兼容的 SSE 流式接口：没有 function calling、自动应用、一键撤销或请求取消；文件修改使用 `se-file` / `se-replace` / `se-delete` 文本协议，模型不遵循时只能手动复制，应用/删除前必须确认并自动备份；API Key 以明文保存在用户全局设置中，不写入工程。对话记录是 Markdown 文本，重新加载只恢复正文和思考过程，不恢复待应用的文件修改/删除/确认卡片；清空会清除该工程的历史记录，超过约 2MB 时只保留最近消息。`se-ask` 只在当前会话可提交，历史中作为文本保留。`se-build` 需要工程已配置 Build 命令且不是 MicroPython 模式；自动构建结果最多回传 5 轮，用户仍需确认应用 AI 修复；停止、退出、清空或发送新消息会中止自动验证；构建日志受 1000 行上限影响，极长日志的早期内容可能已被丢弃。系统提示词会附带 skill、工程关键文件和资源报告摘要，大工程或小上下文模型可能超出窗口，错误直接显示在聊天里。skill 目录不存在时使用编译进 exe 的内置副本；构建时 `.opencode/skills/stm32-embedded-dev/` 必须存在。进入 AI 模式的转场固定为 1 秒淡入，不再依赖 Win32 分层窗口或桌面透视；输入/输出分隔条的比例保存在内存中，重启后恢复默认 0.64。
- AI 串口 Debug 依赖固件已实现 CLI 并烧录，IDE 不负责自动烧录；`se-cli` 按行发送，每条命令等 `cli.timeout_ms` 后截取输出，最多 32 条、自动回传 5 轮，长输出会截断。串口被其他程序占用、端口错误、固件未运行 `cli_poll()` 时只能回传失败/空输出，不会伪造 `OK`。CLI 同一时间只保留一个会话；窗口关闭、切换工程、退出 AI 模式或应用退出会断开。`.ai-issues.md` 按标题（不区分大小写）合并，同名但语义不同的问题可能被合并，需要手动改标题；该文件由 IDE 维护，AI 不能用 `se-file`/`se-delete` 修改。
- 上下文索引是本地词法检索（BM25 风格 + 路径加权），不理解同义词、类型关系或宏展开；最多索引 1200 个文件/6000 个片段，大工程可能只覆盖前一部分。首次打开工程或点“重建索引”后需要等后台构建完成；构建中或索引 stale 时本次发送回退到完整源文件，不会用旧索引。索引不写入工程，只保存在内存中；切换工程会重建。
- 内存布局的起始地址/大小只写入工程 `.simple-embedded/settings.json`，不会自动改写链接脚本；Flash/RAM 总量优先来自链接脚本，其次来自芯片支持包。`.bin` 使用内存布局里的下载起始地址。
- 多光标支持 Alt+Click、Ctrl+Alt+Up/Down、Ctrl+D 和统一编辑；方向键只移动主光标并清除次级光标，不实现 VS Code 的多光标同时移动、Ctrl+U 回退光标和跨文件多光标。Ctrl+Alt+左键仍提供单一矩形列选择；分屏的两个窗格都可编辑，但同一个文件不能在两个窗格重复打开。
- 代码折叠只识别 C 风格大括号块，不识别 `#if` / 函数式语言缩进块；折叠时隐藏起始行与结束行之间的块内行，保留结束行（右花括号），起始行仍可编辑，折叠状态不写入工程配置。
- 行注释切换只处理单行注释标记，不提供块注释；HTML / XML / CSS / JSON / Markdown 等没有单行注释的文件不处理。多光标或矩形列选择激活时按主光标选区切换并清除次级状态。
- 调试支持主机 gdb 和 ARM GDB server：ARM ELF/AXF 会复用 OpenOCD / pyOCD 配置启动 server，连接 arm-none-eabi-gdb 并 `load` 程序；调试侧栏提供变量 Live Watch 和只读寄存器表，通过 gdb `echo` 标记解析输出；没有调用栈和独立内存编辑面板，但外设页的寄存器和位域可直接写入。“动态监测”支持 `watch` / `rwatch` / `awatch` 硬件 watchpoint，命中时暂停并记录本机时间、新旧值、源码位置和 `bt 6` 调用栈；OpenOCD 的 Cortex-M 命中常报告为 `SIGTRAP`，应用会查询创建时保存的旧值和当前表达式值补记录。数量受目标硬件限制，GDB 可能退化为软件 watchpoint（界面标记“软件”），OpenOCD/pyOCD 不支持时以 gdb 错误为准。watchpoint 表达式和历史在停止调试后保留，下次会话在 `main` 暂停后自动或手动重新启用；调试控制浮层可关闭但不会自动停止会话。
- 工作区只使用第一个 folder，不实现 VS Code 的多根工作区、任务和扩展配置；双击关联写入 HKCU，移动程序后需要重新关联。
- 没有内嵌 ssh-mcp 或 jlink-mcp 客户端；远程 SSH 走系统 OpenSSH，Flash 走本地自定义命令、内置 OpenOCD 或内置 pyOCD。内置烧录支持 `.hex` / `.elf` / `.axf`，以及按内存布局下载地址烧录的 `.bin`。
- 源码支持 UTF-8（含 BOM）和 GBK；构建输出在 Windows 上会尝试按 GBK 解码。
- 不支持文件系统外部修改的自动合并。
- 长构建日志会横向裁剪，但完整内容可复制。

## 验证清单

提交前至少执行：

```powershell
cargo fmt --all -- --check
cargo test
cargo clippy --all-targets -- -D warnings
cargo build --release
```

当前测试覆盖：配置检测、Make 命令识别与 POSIX 工具路径、SSH 命令构造、CRLF/GBK 往返、路径显示、文件树展开、高亮语言识别、ANSI 解析、日志位置与裁剪、后台命令执行、缩进/粘贴/括号、查找、gitignore/工程搜索、Git 状态与 diff、TODO、符号索引、补全替换、SVD 外设/寄存器/位域（含 peripheral 与 register 的 derivedFrom、bitRange/lsb-msb）、支持包 `.pdsc` 解析与 `.pack` 导入、OpenOCD / pyOCD 配置与命令构造、OpenOCD 接口预设与 USB VID 探针检测、OpenOCD / pyOCD GDB server 参数、并行构建环境注入、补全上下文与成员索引、十六进制、数组生成、列编辑、gdb 路径检测、ARM ELF 目标识别、ARM gdb 检测、空闲调试端口、gdb 停止位置与无代码断点解析、gdb watchpoint 行解析（含 `(gdb)` 前缀/软硬件与删除行）、OpenOCD SIGTRAP 的 print 值/探针标记解析、头文件父目录扫描与包含目录去重、ELF 全局变量/栈区/FreeRTOS 堆符号解析、二进制文件只读十六进制文档、工程配置往返、损坏工程配置检测、工作区文件解析、调试产物检测、文件复制命名和路径重映射、ELF/日志内存占用、链接脚本/PDSC 内存区域、.bin 烧录地址、C 数组越界/危险字符串函数/重复释放静态扫描、同名宏重复/宏族相同值、标识符悬停/文档注释/返回类型、设置默认值、Makefile newlib-nano 浮点开关同步、混合 CubeMX 工程按构建系统选择 Make/Keil 元数据、Makefile armclang/AC6、链接脚本芯片型号、ToolchainKind 自动解析、自动检测根目录优先并识别 Keil / MicroPython 工程、MicroPython 串口配置与端口排序/ESP32 桥接芯片识别、MicroPython base64 分块上传脚本/raw REPL 成功与错误响应解析、MicroPython 芯片文件列表解析/base64 解码、芯片文件树构建、芯片文件读取脚本缩进、芯片文件下载路径/副本命名、多光标插入/删除/选择替换/缩进/位置重映射、大括号折叠范围/可编辑折叠缓冲/源行映射、Makefile 行级高亮、C_SOURCES 源文件同步/删除同步、clangd compile_commands/URI/UTF-16 位置/悬停清洗/C/C++ 文件过滤、undefined reference 符号提取、撤销深度默认值与上限、MicroPython 固件芯片默认地址/配置校验、esptool 4/5 版本解析与命令参数、命令可用性检查、50 套主题名称/色板与暗亮分组、亮色/暗色应用调色板派生、悬停调用/引用索引（函数调用、跨文件引用、注释/字符串过滤、多行签名、循环调用、生成目录跳过）、系统安装目录复制/临时目录跳过/PowerShell 引号与快捷方式/安装路径判断、工程资源报告（源码/头文件/链接脚本/组件识别、.ioc 引脚与组件、外设/寄存器 JSON、报告文件名）、Keil SVD 设备/系列 token 优先匹配、图标 ICO 条目与 exe 嵌入资源一致性、AI skill 目录查找/加载/内置副本/系统提示词、AI 设置默认值与字段清理、AI 模式转场缓动/窗口透明度/雾噪声/贝塞尔曲线/LCG 确定性/字符截断、AI 文件修改协议 se-file/se-replace 解析、同一文件多个 se-replace 合并、路径逃逸/生成目录/二进制拦截、SEARCH 唯一匹配、base hash 与 diff、SSE 流式 reasoning/content 与 message 解析、本地 mock SSE 流、工程架构分层/裸机主循环/RTOS 任务/状态机/设计模式/中断回调关联/事件驱动框架/执行范式/调度模式/甘特图、AI 额外 Skill/参考资料加载与导入、PDF 旁车命名、资源重名处理、framework.md 章节与 Mermaid gantt 解析、4 反引号 se-file 嵌套代码围栏、旧三反引号 Markdown 内部代码围栏、SSE `finish_reason=length` 自动续写与截断保护、`se-evolve` 规则解析/用户 evolution 注入/规则去重/技能导出、行注释多行切换/取消注释/空行跳过/选区调整/语言标记识别、`se-delete` 解析/删除目标校验/生成后变化拦截、`se-ask` 问题/选项解析与回答拼装、`.ai-chat.md` 对话记录往返/路径生成/隐藏标记解析/空记录加载、`se-build` 请求解析与构建报告成功/失败/取消、`se-issue` 字段解析/按标题更新/问题文件创建、`se-cli` 命令解析、串口 CLI 行尾/参数校验与工程配置往返、AI 编辑失败报告（当前文件内容/哈希提示/缺失文件/长文件窗口截取）、framework.md 引用路径解析（大小写不敏感/裸文件名/版本号排除）、上下文索引分块/BM25 检索/路径加权/生成目录跳过/输出压缩、索引检索片段进入 AI prompt、项目级 evolution-user.md 注入、代码风格样例收集/风格请求识别与 code_style.md 注入、AI 文件检索协议 se-list/se-read 解析、目录遍历/行范围/路径拒绝、检索结果注入、活动文件在超大 framework.md 与 Skill 下的提示词保留、T8 系统整体感知任务/模板 H/system-perception 引用、正文确认问句回退为问答卡片、SEARCH 行内锚点末尾换行回退、仅思考无正文时自动重试、检索正文写入用户消息的 model_content、只写读取计划没有 se-read 块时自动重试、只承诺落盘没有 se-replace/se-file 块时自动重试、四层 skill 优先级加载/超大文件整份跳过/`se-read` 只读 skill 参考文件、Mermaid flowchart/sequenceDiagram 解析与布局、Markdown 表格/编号列表/引用/链接解析。当前共 347 个测试。

UI 改动后建议：

1. 启动一次 release 或 debug 版本
2. 用 `examples/blink` 打开 `src/main.c`
3. 实际点击右侧操作栏 Build，确认输出面板、操作栏、文件树正常
4. 如果界面视觉有变化，更新 `docs/editor.png` / `docs/welcome.png`

## Context-mode 使用原则

本项目安装了 context-mode 1.0.169，作为可选的 MCP + skill 工具，用于抑制大输出占用上下文。

- 安装位置：MCP 配置 `.opencode/opencode.json`；本地 npm 包 `tools/context-mode`（`tools/` 已 gitignore）；skill 在 `.opencode/skills/context-mode` 及 `ctx-doctor` / `ctx-index` / `ctx-search` / `ctx-stats` / `ctx-upgrade` / `ctx-purge` / `ctx-insight`。
- 当前 OpenCode 2.x 使用 V2 插件 API，而 context-mode 1.0.169 的 OpenCode 适配器是 V1 hook 插件，因此采用 MCP-only 安装：工具名为 `context-mode_ctx_*`；没有 PreToolUse 硬拦截和自动 SessionStart / 压缩快照。`ctx_doctor` 的 `Plugin registration: FAIL` 是 MCP-only 模式下的预期项。
- 默认使用原生工具完成普通编辑、构建、git 和小范围查看；不要把所有命令都改走 context-mode。
- 输出可能较大（长构建/测试日志、全仓扫描、批量 JSON/日志、网页/API 文档、跨会话资料）时，优先用 `context-mode_ctx_execute` / `context-mode_ctx_batch_execute` / `context-mode_ctx_fetch_and_index` / `context-mode_ctx_index` / `context-mode_ctx_search`，只把摘要或检索结果放回上下文。
- 读取文件是为了编辑时直接用编辑器/Read；为了分析/汇总大文件时用 `context-mode_ctx_execute_file`。
- 项目 `AGENTS.md` 和 `safe-shell` 的安全规则优先于 context-mode 的 routing skill；不得用 context-mode 绕过删除、覆盖、递归清理等确认流程。

## Git / 工作流

- 远端：`origin` = https://github.com/Chenalanson/simpleide.git，`gitee` = https://gitee.com/chen_alanson/embedd.git
- 推送要求：每次推送必须同时推送到 `origin` 和 `gitee` 的当前分支（例如 `git push origin xiug` + `git push gitee xiug`）；只推一个远端视为未完成。
- 主分支：`master`（跟踪 `origin/master`）；当前开发分支 `xiug`（跟踪 `origin/xiug`）。
- 本仓库开发时不要提交 `target/`、`examples/blink/build/`、`*.log`、`.simple-embedded/` 或 `*.se-workspace`；这些已写入 `.gitignore`。`.opencode/skills/` 随工程跟踪，包含当前可用的技能说明和参考资料。普通用户工程是否提交 `.simple-embedded/settings.json` / `.se-workspace` 由用户决定。
- `dist/` 只追踪 `*.exe`：`.gitignore` 使用 `/dist/**/*` + `!/dist/**/` + `!/dist/**/*.exe`，并显式忽略 `.compiler.tmp` / `.make.tmp`。不要用 `git add -f` 把 DLL、工具链数据或 `tools.zip` 加进仓库。
- 除非用户明确要求，不要自动 commit 或 push。
- 本机 Git 全局配置路径 `D:\Cadence\SPB_Data\.gitconfig` 不存在时，Git 会报 global config 错误；可用仓库本地配置，或在命令中临时指定有效的 `GIT_CONFIG_GLOBAL`。
- 本仓库当前本地作者配置：`chen_alanson <chen_alanson@user.noreply.gitee.com>`。

## 文档

- `README.md`：项目定位、功能、依赖和设计取舍
- `docs/USAGE.md`：按原始模式 / AI 模式分栏的图文速查教程，引用 `docs/screens/` 下的界面截图
- `docs/TUTORIAL.md`：面向用户的中文操作教程
- `docs/DYNAMIC.md`：动态监测（watchpoint）教程，含时间戳、调用栈和自动启动
- `docs/FLASH.md`：烧录配置教程（ST-Link、CubeProgrammer、OpenOCD、pyOCD、J-Link、Keil）
- `.opencode/skills/simple-embedded/`：本软件的使用 skill（SKILL.md + references），并同步安装到 `~/.config/opencode/skills/simple-embedded/`
- `docs/editor.png` / `docs/welcome.png`：界面截图
- 本文件：面向后续开发/代理的工程上下文

修改行为后，同步更新 README、教程和本文件，避免文档与实现不一致。