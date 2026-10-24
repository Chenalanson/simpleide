# AGENTS.md

本文件记录 Simple embedded 的项目约定、架构和验证方式，供后续编码代理快速进入状态。

## 项目概览

Simple embedded 是一个 Windows 桌面端、文件夹优先的极简嵌入式 IDE：

- 左侧文件树
- 中间多标签代码编辑 + syntect 语法高亮
- 底部异步构建日志
- 本地 / 远程 SSH 构建
- 可配置的本地 Flash 命令

核心产品约束：**工程文件夹就是工程**。不要引入自定义工程文件（如 `.eide`、`.mini-ide`）。用户设置默认只写到用户目录。例外：Makefile 工程在用户保存构建选项或 C/C++ 属性时，会按差异同步到现有 Makefile（修改前自动备份到用户目录），使修改真正参与构建；不得修改 Keil `.uvprojx`，也不得借机创建自定义工程文件。

技术栈：Rust 2021、eframe/egui 0.34.3（glow）、syntect 5.3、serde、OpenSSH。

## 快速命令

```powershell
cargo run --release
cargo run --release -- examples\blink examples\blink\src\main.c
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
src/main.rs            窗口、图标、CJK 字体、eframe 启动
src/app.rs             App 状态、快捷键、打开/保存、右侧操作栏、构建输出
src/app/ui.rs          app 的子模块：文件树、Tab、编辑器、空工程/空标签占位、设置/确认弹窗
src/build.rs           子进程、stdout/stderr、停止、构建事件与结果
src/config.rs          自动检测、每工程配置、SSH 命令构造、设置读写
src/project_meta.rs    Makefile / Keil .uvprojx 工程元数据提取
src/editor.rs          EditorBuffer（String + revision）和 Document
src/file_tree.rs       惰性目录树
src/highlight.rs       syntect 高亮和 galley 缓存
src/log.rs             ANSI SGR、错误分级、日志上限
src/theme.rs           配色和按钮样式
src/util.rs            路径、换行、系统集成辅助
examples/blink/        可编译的演示固件
docs/                  README 截图和使用教程
```

## 架构要点

### UI

- 使用 eframe 0.34 的 `eframe::App::ui(&mut self, ui, ...)`，界面挂在 root `Ui` 下。
- 面板使用 `egui::Panel::top/bottom/left(...).show_inside(ui, ...)` 和 `CentralPanel::show_inside`。不要回退到旧的 `update(ctx)` + `.show(ctx)` 写法。
- 事件采用 immediate mode：`App::ui` 收集 `Action`，再在帧末 `apply_actions`。部分明显的即时状态（滚动、输出开关）直接修改。
- `src/app/ui.rs` 是 `app` 的子模块，可以访问父模块私有字段；跨模块公开方法用 `pub(super)`。
- 自定义画线/图标优先用 `Painter`，不要依赖可能没有字形的 Unicode 图标。CJK 字体在 `main.rs` 运行时从系统字体加载。
- 左侧构建卡片顶部显示工程名（粗体）和路径，路径使用 `util::display_path` 去掉 Windows `\\?\` 前缀；下方显示构建模式、命令和两个按钮。未导入时按钮为“导入 / 配置”，导入后为“更换 / 配置”。
- 左侧栏全高；资源管理器（文件树）与工程信息的分界受 `left_split` 控制：文件树内容较短时区域自动收缩，工程信息标题紧跟最后一个可见文件；内容超出时按比例截断，中间分割线可拖动。折叠/展开工程信息只影响其内容，不改变标题位置。`left_split`、`resource_open`、`info_open` 都持久化在 `Settings`，下次启动保持位置。左栏宽度由可调整的 `Panel::left` 提供，并加大了拖拽命中半径。无工程时中间只显示一行提示“请在左上角选择工程并开始”。
- Windows 标题栏通过 DWM 深色属性和 `ViewportCommand::SetTheme(Dark)` 与面板配色保持一致。

### 编辑器

- egui `TextEdit` 要求 `TextBuffer::as_str()` 返回连续切片，所以不要直接接 ropey。`EditorBuffer` 用 `String` + `Rc<Cell<u64>>` revision，所有插入/删除都会递增 revision。
- 高亮缓存键是 `(revision, wrap_width, font_size)`；修改文本后 `layout_cached` 会自动重建。换字号、换自动换行宽度也由键覆盖。
- 文档打开时会规范化到内部 LF，保存时恢复原 CRLF/LF 和 UTF-8 BOM。
- 支持 UTF-8，文件上限 16 MB，二进制文件拒绝打开。
- 打开文件/切换 Tab 时，`focus_editor` 会请求焦点并清掉跨 Tab 的旧选区。
- 左侧行号与当前行高亮；`Settings.relative_line_numbers` 切换相对行号，`Settings.ruler_column` 可选 80/120 列标尺。
- 当前版本仍未内置 LSP、调试器或自动补全；用户已规划这些增强，后续在 `xiug` 分支分批接入，不要伪造未接通的能力。

### 工程信息

- `project_meta.rs` 在打开工程时读取 Makefile 或 Keil `.uvprojx`，提取目标名、芯片、CPU、浮点、链接脚本、内存布局、包含目录、宏定义等。
- 左栏先显示资源管理器，再显示“工程信息”；分割线紧贴“工程信息”标题上下，只保留“构建配置”和“C/C++ 属性”两个分组。两个分组标题以及三个 C/C++ 列表标题左侧各有折叠箭头，整行可点击并切换展开；点击行会记录选中状态并显示淡蓝色背景，加号点击只添加、不折叠。展开和选中状态存在 egui 内存中。数据按工程缓存，刷新文件树时重新读取。
- 包含目录、库搜索目录和预处理器定义支持在左栏编辑：每个列表右侧有绿色 `+`，批量编辑仍可用。覆盖值存 `BuildConfig.meta_includes` / `meta_library_dirs` / `meta_defines`（`None` 表示自动检测）。Makefile 工程保存时会同步到 `C_INCLUDES` / `LIBDIR` / `C_DEFS`，修改前备份到 `%APPDATA%\SimpleEmbedded\backups`；Keil `.uvprojx` 不会被修改。
- “构建选项”编辑器存 `BuildConfig.build_options`，分为全局选项、C/C++ 编译器、汇编器、链接器四个页面；显示在构建配置中。打开 Make 工程时会从 `OPT` / `CFLAGS` / `AS_DEFS` 等解析当前值，保存时只回写发生变化的字段；链接器输出格式选 `AXF` 时会在 Makefile 末尾添加受管理的 `.axf` 复制规则。Keil 工程仍只保存到本机设置。
- 没有可解析的工程文件时只依赖 `BuildConfig`，不要为空字段编造值。

### 文件树

- 工程根目录作为文件树第一行显示（默认展开），子项相对根缩进一级；根行同样可点击展开/收起，右键可在资源管理器或 VS Code 中打开。
- 懒加载目录子项，目录优先、名称不区分大小写排序。
- 默认隐藏 `.` 开头的文件/目录，可通过设置或左下按钮切换。
- `FileTree::reveal` 会展开当前打开文件的父目录。
- 目录缓存存在内存中；刷新按钮清空缓存。当前没有文件系统 watcher。

### 构建与日志

- `BuildConfig::make_spec` 负责把用户命令变成 `RunSpec`：
  - 本地：Windows `cmd /S /C`，Unix `sh -c`。
  - 远程：`ssh -o BatchMode=yes -o ConnectTimeout=10 [ -p port ] host "cd '<remote_dir>' && <command>"`。
  - Flash 永远本地执行。
- 构建前自动保存所有脏 Tab。
- `BuildController` 用 `std::process`、线程和 `mpsc`，不使用 tokio。UI 每帧 `poll`，不阻塞。
- stdout/stderr 各一个读线程；Windows 上停止用 `taskkill /PID <pid> /T /F`，随后 `Child::kill`。
- 日志经过 ANSI SGR 解析，保留颜色；错误/警告行有更高优先级颜色。日志上限约 20,000 行 / 8 MB。
- 日志是固定行高、虚拟化绘制；超长行横向裁剪，用户可悬停查看或复制全部日志。
- Make 命令在启动时探测 PATH：`make` → `gmake` → `mingw32-make`。
- 本地 Make 构建自动探测 Git for Windows / MSYS / Cygwin 的 `usr\bin` 并加入 PATH，保证 `rm` 等 POSIX 命令可执行。
- `BuildConfig.extra_paths` 允许用户追加工具链目录，构建时统一前置到 PATH。
- 远程停止只保证结束本地 ssh 进程；远端进程是否退出取决于远端 shell。

### 配置

设置路径：

```text
%APPDATA%\SimpleEmbedded\settings.json
```

- 每个工程按规范化路径独立保存构建配置。
- Makefile 备份位于 `%APPDATA%\SimpleEmbedded\backups\<工程名>-<hash>\`，默认保留最近 12 份。
- 自动检测结果只是初值；用户可以随时在设置里改。
- 修改设置后必须点击保存。
- `Settings::load` 容忍 UTF-8 BOM；解析失败会回退默认值并输出到 stderr。
- 不要在工程目录创建任何配置文件。唯一允许写入工程文件的场景是保存 Makefile 工程的构建选项 / C/C++ 属性；Keil `.uvprojx` 始终只读。

## UI/产品约定

- 用户界面使用中文；保持现有深色配色和 `docs/` 截图风格。
- 界面不显示 emoji/生僻符号；使用 `Painter` 画图形或已验证的字符。
- 右侧操作栏：纵向排列 Build、Clean/Rebuild/Flash；不再放设置按钮，设置从构建卡片“配置”或 `Ctrl+,` 打开。
- 构建结果显示在输出面板标题和日志中；底部不再保留重复的状态栏。
- 构建输出面板顶部有 10px 拖动手柄，收起时也能向上拉出；高度存 `Settings.output_height`，最大到主区域顶部。
- 关闭窗口、关闭 Tab、切换工程时都要处理未保存确认。
- 不做弹窗堆叠；确认弹窗和设置弹窗互斥。
- 无工程时左上角仍显示同一张构建卡片，使用“导入 / 配置”按钮；中间显示单行提示。第一次启动不写入任何工程文件。测试后如果产生 `%APPDATA%\SimpleEmbedded\settings.json`，提交前应清理，避免把测试工程路径带入首启体验。

## 已知边界（当前版本）

- 没有自定义工程格式、Keil 自动导入、芯片包、内存布局、调试器。Keil 工程可以通过手动命令或外部包装脚本接入。
- 没有 LSP/补全/跳转定义。
- 没有内嵌 ssh-mcp 或 jlink-mcp 客户端；远程 SSH 走系统 OpenSSH，Flash 走用户配置的本地命令。
- 仅读取 UTF-8（支持 BOM），不支持 GBK 等源码编码；构建输出在 Windows 上会尝试按 GBK 解码。
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

当前测试覆盖：配置检测、Make 命令识别与 POSIX 工具路径、SSH 命令构造、CRLF 往返、路径显示、文件树展开、高亮语言识别、ANSI 解析、日志裁剪、后台命令执行。

UI 改动后建议：

1. 启动一次 release 或 debug 版本
2. 用 `examples/blink` 打开 `src/main.c`
3. 实际点击右侧操作栏 Build，确认输出面板、操作栏、文件树正常
4. 如果界面视觉有变化，更新 `docs/editor.png` / `docs/welcome.png`

## Git / 工作流

- 远端：`origin` = https://gitee.com/chen_alanson/embedd.git
- 主分支：`master`（跟踪 `origin/master`）
- 不要提交 `target/`、`examples/blink/build/`、`*.log` 或 `.opencode/`；这些已写入 `.gitignore`。
- 除非用户明确要求，不要自动 commit 或 push。
- 本机 Git 全局配置路径 `D:\Cadence\SPB_Data\.gitconfig` 不存在时，Git 会报 global config 错误；可用仓库本地配置，或在命令中临时指定有效的 `GIT_CONFIG_GLOBAL`。
- 本仓库当前本地作者配置：`chen_alanson <chen_alanson@user.noreply.gitee.com>`。

## 文档

- `README.md`：项目定位、功能、依赖和设计取舍
- `docs/TUTORIAL.md`：面向用户的中文操作教程
- `docs/FLASH.md`：烧录配置教程（ST-Link、CubeProgrammer、OpenOCD、pyOCD、J-Link、Keil）
- `docs/editor.png` / `docs/welcome.png`：界面截图
- 本文件：面向后续开发/代理的工程上下文

修改行为后，同步更新 README、教程和本文件，避免文档与实现不一致。