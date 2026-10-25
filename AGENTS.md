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

技术栈：Rust 2021、eframe/egui 0.34.3（glow）、syntect 5.3、serde、zip 9.1（读取 Keil/CMSIS 支持包）、OpenSSH。

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
src/config.rs          全局/工程设置、自动检测、SSH 命令构造、工作区文件与关联
src/project_meta.rs    Makefile / Keil .uvprojx 工程元数据提取
src/editor.rs          EditorBuffer（String + revision）和 Document
src/file_tree.rs       惰性目录树
src/flash.rs           烧录工具配置、OpenOCD / pyOCD 检测与命令构造
src/memory.rs          ELF / 链接脚本 / PDSC 内存区域与固件占用统计
src/memory_check.rs    数组越界、危险字符串函数、重复释放等 C 内存安全静态扫描
src/macro_check.rs     同名宏重复、宏族相同替换值（可选黄色波浪线）诊断
src/completion.rs      轻量成员索引、补全上下文和候选类型
src/highlight.rs       syntect 高亮和 galley 缓存
src/hover.rs           标识符悬停：声明签名、注释、返回值/类型
src/minimap.rs         编辑器右侧代码预览、缩略行缓存和滚动跳转
src/log.rs             ANSI SGR、错误分级、日志上限
src/theme.rs           配色和按钮样式
src/toolchain.rs       系统 / 软件自带工具链检测、PATH 目录和复制安装
src/util.rs            路径、换行、系统集成辅助
src/brackets.rs        注释/字符串感知的括号匹配
src/indent.rs          缩进单元、行缩进/反缩进、回车和粘贴缩进
src/ignore.rs          gitignore 风格匹配
src/search.rs          全工程文本搜索
src/support_pack.rs    Keil/CMSIS .pack/.pdsc 扫描、导入和元数据解析
src/symbols.rs         C/C++/汇编符号索引
src/navigation.rs      标识符提取与补全替换
src/todo.rs            TODO/FIXME 扫描
src/git.rs             git 状态/修改行
src/registers.rs       SVD / 头文件寄存器索引
src/hexview.rs         十六进制模型
src/arraygen.rs        C 数组生成
src/formatting.rs      clang-format
src/debugger.rs        gdb 会话、ARM GDB server 与目标识别
src/column.rs          旧批量列编辑算法
src/column_select.rs   矩形列选择文本操作
examples/blink/        可编译的演示固件
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
- 左侧构建卡片顶部显示工程名（粗体）和路径，路径使用 `util::display_path` 去掉 Windows `\\?\` 前缀；下方显示构建模式、命令和三个按钮。未导入时按钮为“导入 / 配置 / 工作区”，导入后为“更换 / 配置 / 工作区”。工作区按钮弹出菜单：生成并关联工作区文件、打开工作区文件、打开工程配置 `settings.json`。
- 左侧栏全高；资源管理器（文件树）与工程信息的分界受 `left_split` 控制：文件树内容较短时区域自动收缩，工程信息标题紧跟最后一个可见文件；折叠资源管理器时工程信息标题紧贴资源管理器标题。文件树内容超出时按比例截断；展开时工程信息标题上下各留 6px，折叠且树超出时上下各留 8px，标题自身保留 6px 内边距和最小高度，避免贴住底部或上下间距不一致。中间分割线可拖动。折叠/展开工程信息只影响其内容，不改变标题位置。`left_split`、`resource_open`、`info_open` 都持久化在 `Settings`，下次启动保持位置。左栏宽度由可调整的 `Panel::left` 提供，并加大了拖拽命中半径。无工程时中间只显示一行提示“请在左上角选择工程并开始”。
- Windows 标题栏通过 DWM 深色属性和 `ViewportCommand::SetTheme(Dark)` 与面板配色保持一致。

### 编辑器

- egui `TextEdit` 要求 `TextBuffer::as_str()` 返回连续切片，所以不要直接接 ropey。`EditorBuffer` 用 `String` + `Rc<Cell<u64>>` revision，所有插入/删除都会递增 revision。
- 高亮缓存键是 `(revision, wrap_width, font_size)`；修改文本后 `layout_cached` 会自动重建。换字号、换自动换行宽度也由键覆盖。
- 文档打开时会规范化到内部 LF，保存时恢复原 CRLF/LF 和 BOM。
- `util::read_text_file` 先按 UTF-8 读取，失败后回退 GBK；`LoadedText.encoding` / `Document.encoding` 记录编码，保存时用 `write_text_file` 按编码写回。工具栏可切换 UTF-8 / GBK。
- 文件上限 16 MB，二进制文件拒绝打开。
- 打开文件/切换 Tab 时，`focus_editor` 会请求焦点并清掉跨 Tab 的旧选区。
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
- 自动保存由 `App::tick_autosave` 的脏文档签名驱动：编辑停止 `Settings.autosave_delay_ms` 后调用 `save_all_dirty`，并在无输入时请求重绘；默认关闭。
- 每个 `Document` 有内存态 `read_only`；只读时编辑器不可交互，替换、编码切换、格式化与保存都会被拒绝。工具栏按钮切换，不持久化。
- `formatting.rs` 调用 clang-format：优先 `Settings.clang_format_path`，否则查找 PATH 和常见 LLVM 目录；`format_on_save` 失败只提示，仍会保存原文。没有内置格式化器，不要伪造成功。
- `git.rs` 通过本机 `git` 命令读取状态：`status --porcelain -z -uall --branch`，并用 `rev-parse --show-prefix` 把仓库根路径映射到工程子目录；`git diff HEAD -U0` 提供当前文件修改行。资源管理器显示 `M/U/A/D/R` 徽标，行号栏左侧黄色条表示修改行；`Ctrl+Shift+G` 打开只读状态窗口，不执行提交或网络操作。
- `todo.rs` 扫描工程中的 `TODO` / `FIXME` / `HACK` / `NOTE`（大写单词边界），跳过忽略目录和 2 MB 以上文件，最多 500 条；`Ctrl+Shift+T` 或资源管理器 TODO 按钮打开窗口，支持类型过滤与点击跳转。
- `symbols.rs` 用轻量正则索引 C/C++ 函数、struct/union/enum/class、宏和汇编标签，最多 2000 条；`Ctrl+Shift+O` 或工具栏“符号”打开过滤窗口。它不是完整的 C++ 解析器或 LSP，复杂声明可能漏检，不要宣称语义级准确。
- `hexview.rs` 读取最多 32 MB 文件，按 16 字节一行显示偏移/十六进制/ASCII；文件树右键或 `Ctrl+Shift+H` 打开当前文件，可从窗口把字节转给数组生成器。
- `arraygen.rs` 把选中文本或当前文件（最多 8 KB）生成 `const uint8_t` C 数组；工具栏“数组”或 `Ctrl+Shift+A` 打开，变量名会被清洗成合法标识符，支持复制。
- `registers.rs` 优先用正则解析工程内 `.svd`、`.pack` 内 SVD，或已安装 Keil DFP 的 `SVD/*.svd` 的 peripheral/baseAddress/register/addressOffset/description，支持 `derivedFrom` 继承；没有可用 SVD 时回退扫描 `.h/.c` 中 `*_BASE` / `*_ADDR` / `*_ADDRESS` 宏。`Ctrl+Shift+R` 或工具栏“寄存器”打开浏览窗口，最多 2000 项并去重；调试侧栏“外设”页复用同一索引显示实时值。
- `debugger.rs` 启动本机 gdb CLI（非 MI），通过 stdin/stdout 转发命令并收集控制台输出；`DebugState` 保存会话、断点、日志、当前暂停位置和 `running` 状态。右侧“调试”或 `F5` 启动/继续；目标运行中浮层主按钮显示暂停图案并发送中断请求，状态显示“运行中”，暂停后显示“已暂停”。浮层最左侧的准心按钮（`DebugIcon::Locate`）可随时回到当前执行行，即使已经切换到其他文件。`F9` 或点击行号左侧空白切换断点，`F10` 下一步，`F11` 单步，`Shift+F11` 跳出，`Shift+F5` 停止。调试控制是编辑器右上角的小浮层 `show_debug_overlay`，使用 `Painter` 绘制继续/暂停/定位/停止/单步图案；调试日志显示在底部输出面板的“调试输出”页，可在页底输入 gdb 命令。
- 行号左侧的 gutter 点击范围覆盖整列（含断点红点），不要求先移动光标；断点按 `(路径, 行号)` 保存。gutter 行号使用与编辑器相同的 `galley.cursor_from_pos` 命中测试，避免滚动后行号偏移。`poll_debug_location` 只扫描新到达的 gdb 日志，用 `debugger::parse_stop_location` 解析 `... at file:line`，命中后自动打开对应文件、滚动到当前行并在 gutter 画黄色执行箭头和整行浅黄底；每次新停止都会重新跳转，即使上次已停在同一行。`next` / `step` / `finish` 后会额外发送 `frame`，因为 gdb 单步通常只回显源码行（`95  while (1)`）而不带 `at file:line`。gdb 报告 `No compiled code for line N in file ...` 时会移除对应断点并提示。日志扫描游标 `DebugState.log_scan` 在启动/停止时重置。
- 调试侧栏有 变量 / 寄存器 / 外设 三个页签。“外设”页从工程内 `.svd`、`.pack` 内 SVD 或已安装 Keil DFP 的 `SVD/*.svd` 读取外设寄存器（支持 `derivedFrom` 继承），按外设名分组并可展开/折叠；按名称/地址/说明过滤时自动展开匹配分组。每次最多读取 160 个寄存器，用 `x/1xw` 读取实时值并以 `__SE_PREG_*` 标记路由到独立输出；目标停止且自动刷新开启时每 1.5 秒更新。OpenOCD 对未映射外设的 `Failed to read memory` 行会被过滤，读取失败的寄存器显示“无法读取”。
- 调试按目标类型分流：PE `.exe` 使用普通 gdb；ELF/AXF 进入 ARM 远程流程。ARM 流程复用 `BuildConfig.flash_tool` 的 OpenOCD / pyOCD 参数，用 `flash::openocd_server_spec` / `flash::pyocd_server_spec` 在空闲本地端口启动 GDB server，再由 `arm-none-eabi-gdb` `target extended-remote` 连接，依次执行 `monitor reset halt` / `load` / 再次复位，最后应用断点。首次启动继续时会下发 `tbreak main`，让目标先停在 `main` 并自动打开入口文件，而不是直接运行到第一个用户断点。启动在后台线程完成，通过 `debug_start` 通道回到 UI；停止/切换工程时同时结束 gdb 和 server。`debug_target` 优先使用工程的 `debug.program`；路径失效或为空时按修改时间在 `build/`、`Debug/`、`Release/`、`out/` 和工程根目录查找最新 ELF/AXF/EXE/OUT。打开工程时若 `debug.program` 为空会自动回填检测结果并写入工程配置。
- ARM gdb 由 `detect_arm_gdb` 自动检测：优先使用设置里的路径，其次搜索编译器目录、`extra_paths`、PATH 和常见 GNU Arm Toolchain 目录。设置 → 调试提供 gdb 路径浏览和自动检测按钮；`gdb_path` 既可填 exe，也可填 `bin` 目录。不要用 MinGW gdb 调 ARM 目标，否则会报 error 193。
- `navigation.rs` 提供标识符/前缀提取与字符区间替换；`F12` 在 `symbols.rs` 索引中找定义，`Shift+F12` 用全工程正则搜索列引用。
- 补全在输入至少 2 个标识符字符后自动弹出，锚定在光标下方；输入 `.` / `->` 时切换为成员补全。`↑` / `↓` 选择，`Enter` / `Tab` 插入，`Esc` 关闭；`Ctrl+Space` 仍可手动打开。全局候选来自 `symbols.rs` 的宏/函数/类型索引；成员候选由 `completion.rs` 用轻量正则解析 struct/union 和变量声明得到，成员索引在保存、刷新文件树或刷新符号时失效重建。它不是 C/C++ 语义解析：复杂宏展开、模板、继承、跨文件作用域和函数返回类型可能漏检。
- 编辑器内悬停标识符会弹出 `hover.rs` 构建的提示：先查当前文件声明，再查 `symbols.rs` 索引；显示声明签名、最近的 `/** */` 或 `//` 注释，以及“返回值 / 类型”。提示跟随鼠标显示（`on_hover_ui_at_pointer`）；`Settings.show_hover_info` 控制开关，设置 → 其他 的“变量 / 函数悬停提示”默认开启。类型提取按完整标识符边界匹配，避免把 `uint8_t i` 里的 `i` 误当成类型内部字符。符号索引在第一次渲染编辑器时确保扫描一次（`SymbolState.scanned`），项目切换时重置。复杂宏、跨文件局部变量和动态类型仍可能无法解析。
- `macro_check.rs` 提供宏诊断：同一文件内同名 `#define` 重复始终标红并绘制红色波浪线；设置 → 其他 的“宏重复值提示（黄色波浪线）”（`Settings.check_duplicate_macro_values`，默认关闭）开启后，`PWM1PIN` / `PWM2PIN` / `PWM3PIN` 这类带数字的宏族使用相同替换值时绘制黄色波浪线。该启发式只针对项目代码，跳过 `Drivers` / `CMSIS` / `Middlewares` / `vendor` 等供应商头文件；C 语言本身允许不同宏名映射到相同值，所以它是可选提示而不是编译错误。宏结果缓存在 `Document.macro_issues`，按 revision 失效，设置保存和文件另存/重命名时强制重扫。
- 粘贴由编辑器在 TextEdit 前拦截 `Event::Paste`：`indent::adjust_pasted` 统一换行并按当前行缩进对齐后续行，同时保留粘贴内容相对缩进；只读或编辑器未聚焦时不拦截。
- 分屏只从标签页右键菜单进入：`向右拆分` / `拆分和移动 → 向上拆分` / `取消拆分`。`split_primary` + `split_path` + `split_direction` 决定两个窗格，两个窗格都调用 `show_code_editor`，都可编辑；`split_focus` 记录焦点窗格，点击窗格会切换 active。当前不支持同一个文件在两个窗格重复打开。
- 标签页右键菜单还提供关闭其他、关闭右侧标签页、关闭已保存、全部关闭；保存、另存为；复制完整路径、相对路径和面包屑路径；在文件资源管理器中显示、在资源管理器视图中显示；以及拆分操作。批量关闭遇到未保存文件时走 `PendingAction::CloseTabs` 确认。另存为会写入新路径并把标签页、分屏引用和语法识别一并切换到新文件。快捷键：Ctrl+F4 关闭当前，Ctrl+\ 向右拆分，Ctrl+K U 关闭已保存，Ctrl+K W 全部关闭，Ctrl+K S 保存全部，Ctrl+K Ctrl+\ 向上拆分，Ctrl+Shift+S 另存为，Shift+Alt+C 复制路径，Ctrl+K Ctrl+Shift+C 复制相对路径，Shift+Alt+R 在资源管理器中显示。
- 标签页右键菜单和所有弹出菜单使用深色主题；`theme::install` 强制 `ThemePreference::Dark`，避免系统浅色主题把菜单画成白色。窗口/弹窗的关闭 X 统一使用 `window_close_button`，尺寸和绘制方式与查找浮层的关闭按钮一致。`theme::install` 同时关闭 egui 的 `show_unaligned` 和 `warn_if_rect_changes_id` 调试覆盖层，避免 debug 构建在动画时出现红色控件边框。
- 查找是编辑器右上角的约 420px 紧凑浮层 `show_find_overlay`（内部复用 `show_find_bar`），只在 `find.open` 时由 `Ctrl+F` / `Ctrl+H` 打开，不占用独立行；左侧下拉箭头展开替换行，宽度不变，`AB` 保留大小写在替换输入框内，替换/全部替换为图标按钮。`Ctrl+G` 打开同宽的跳行浮层，输入框复用查找输入框样式；提示文字为普通次要色，不使用色块。
- 面包屑栏只显示从工程根目录开始的路径（`doc.rel_path` 的 `/` 替换为 ` > `）和未保存标记；编码、行尾、语言、只读、寄存器、数组、符号、格式化、自动换行、保存移到最底部状态栏。补全和列编辑不出现在可见工具栏。
- 左侧项目栏、编辑器区、右侧操作栏、底部输出区是四块带圆角的不同底色容器（外圈 3–4px `theme::GUTTER` 深色间隙），不再用亮色分隔线；所有 `Panel` 设置 `show_separator_line(false)`。右侧操作栏宽约 58px（原 110px 的一半），按钮内边距同步收窄。编辑器卡底部两角由 `paint_bottom_corners` 补绘 `GUTTER` 圆角，输出面板标题栏紧贴面板顶部，拖动区域合并到标题栏顶部 4px，日志区使用 `EDITOR_BG` 并撑满剩余空间。输出面板标题提供“构建输出 / 调试输出”切换，`OutputView` 决定显示哪一份日志；构建开始时切回构建，启动调试时切到调试。
- 设置弹窗第一行是 构建 / 构建工具链 / 编辑器 / 调试 / 其他 五个页签，下方只显示当前页；构建页包含 Build/Clean/Rebuild/Flash 命令和“并行任务数”（0 = 不启用，可一键填入 CPU 核心数）；构建工具链页提供编译器、make 的输入框和浏览按钮，可从 PATH 自动检测，也提供“软件自带”目录、工具状态和一键安装。调试页的调试程序和 gdb 路径在工程打开时读写工程的 `debug` 节点（无工程时才用全局设置），提供浏览和自动检测；调试程序为空时会自动回填 build 产物，gdb 路径既可填 exe，也可填 `bin` 目录。
- 最底部状态栏是贯穿整个窗口宽度的矩形条（无圆角、无外圈间隙），内容靠右排列：右侧操作按钮 + 行列/缩进/编码/行尾/语言信息。
- 资源管理器、工程信息、构建配置、芯片支持包、烧录配置、C/C++ 列表等可折叠区域使用缓动动画，不直接瞬间显示/隐藏：侧栏高度动画 0.22s、`cubic_in_out`，其余折叠组使用 egui `CollapsingState` 和全局 0.20s 动画；箭头随 openness 旋转。动画期间主动 `request_repaint()`，避免掉帧式跳变。
- Toast 使用 12.5px 正文、10.5px 状态文字、14×10 内边距，最大宽度 460px；不要再把提示字号设为 9–11px。
- `column.rs` 仍保留旧列编辑算法；`column_select.rs` + `ColumnSelectionState` 提供 Ctrl+Alt+鼠标左键拖动的矩形列选择，蓝色高亮，Ctrl+C 复制、Delete/Backspace 删除、直接输入替换。列编辑对话框已不在任何可见入口暴露。
- 目前没有多光标；列选择是单一矩形区域，不支持多个独立光标。
- `log.rs` 的 `parse_location` 解析 `file:line:col`；日志行带位置时可点击跳转。`LogStore` 维护最多 1000 条问题诊断，`App::refresh_diagnostics` 解析为本地路径并在行号栏右侧画红/黄竖条；输出面板的构建视图可过滤全部/错误/警告，调试视图显示 gdb 日志和命令输入。
- 当前版本未内置 LSP；补全、跳转和引用搜索是轻量符号索引，调试是基础 gdb 控制台，不是完整 IDE 调试前端。后续增强在 `xiug` 分支分批接入，不要伪造未接通的能力。

### 工程信息

- `project_meta.rs` 在打开工程时读取 Makefile 或 Keil `.uvprojx`，提取目标名、芯片、CPU、浮点、链接脚本、内存布局、包含目录、宏定义等。
- 左栏先显示资源管理器，再显示“工程信息”；分割线紧贴“工程信息”标题上下，包含“构建配置”“芯片支持包”“烧录配置”“C/C++ 属性”四个分组。分组标题以及三个 C/C++ 列表标题左侧各有折叠箭头，整行可点击并切换展开；点击行会记录选中状态并显示淡蓝色背景，加号点击只添加、不折叠。展开和选中状态存在 egui 内存中。数据按工程缓存，刷新文件树时重新读取。
- “芯片支持包”分组右侧的绿色 `+` 打开文件选择器，只接受 Keil/CMSIS `.pack` / `.pdsc`，并把文件复制到工程根目录（同名文件不覆盖）。`support_pack.rs` 解析包名、厂商、版本、设备列表和 SVD 数量；`.pack` 通过 zip 读取包内 `.pdsc`，不整体解压。寄存器浏览器在工程内没有独立 `.svd` 时，会直接读取第一个 `.pack` 内的 SVD。支持包不会生成自定义工程文件，也不代表完整的 Keil 设备树/启动文件/烧录算法导入。
- 包含目录、库搜索目录和预处理器定义支持在左栏编辑：每个列表右侧有绿色 `+`，批量编辑仍可用。覆盖值存 `BuildConfig.meta_includes` / `meta_library_dirs` / `meta_defines`（`None` 表示自动检测）。Makefile 工程保存时会同步到 `C_INCLUDES` / `LIBDIR` / `C_DEFS`，修改前备份到 `%APPDATA%\SimpleEmbedded\backups`；Keil `.uvprojx` 不会被修改。
- “构建选项”编辑器存 `BuildConfig.build_options`，分为全局选项、C/C++ 编译器、汇编器、链接器四个页面；显示在构建配置中。打开 Make 工程时会从 `OPT` / `CFLAGS` / `AS_DEFS` 等解析当前值，保存时只回写发生变化的字段；链接器输出格式选 `AXF` 时会在 Makefile 末尾添加受管理的 `.axf` 复制规则。全局选项页提供 newlib-nano 浮点开关 `printf_float` / `scanf_float`（`-u _printf_float` / `-u _scanf_float`），保存时同步为 Makefile 的 `LDFLAGS += ...`；默认关闭，解决 `%f` 空白/为 0 的问题，但会增加几 KB 体积。Keil 工程只写入工程的 `.simple-embedded/settings.json`，不修改 `.uvprojx`。
- 没有可解析的工程文件时只依赖 `BuildConfig`，不要为空字段编造值。

### 文件树

- 工程根目录作为文件树第一行显示（默认展开），子项相对根缩进一级；根行同样可点击展开/收起，右键提供新建文件/文件夹、粘贴、在文件资源管理器中显示、复制路径/相对路径、用 VS Code 打开。
- 文件右键菜单：在侧边打开、剪切、复制、复制路径、复制相对路径、重命名...、删除，以及查看/十六进制/在资源管理器中显示；文件夹右键额外提供新建文件/文件夹和粘贴，也支持剪切、复制、重命名和删除。
- 文件操作使用应用内 `FileClipboard`（复制/剪切），不读写系统剪贴板文件；粘贴到文件时使用其父目录。复制重名时生成 `名称 - 副本.ext`、`名称 - 副本 (2).ext`，剪切遇到同名目标会提示，跨卷移动失败时回退为递归复制再删除。重命名和删除使用独立确认弹窗；删除前若存在未保存 Tab 会拒绝执行。重命名/剪切会同步更新打开 Tab 的路径、分屏引用、断点、调试暂停位置和图像预览；删除会关闭相关 Tab 并清除这些引用。
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
- 日志是固定行高、虚拟化绘制；超长行横向裁剪，用户可悬停查看或复制全部日志。
- Make 命令在启动时探测 PATH：`make` → `gmake` → `mingw32-make`。
- 本地 Make 构建自动探测 Git for Windows / MSYS / Cygwin 的 `usr\bin` 并加入 PATH，保证 `rm` 等 POSIX 命令可执行。
- `BuildConfig.extra_paths` 允许用户追加工具链目录，构建时统一前置到 PATH。
- `Settings.compiler_path` / `make_path` / `use_bundled_toolchain` / `bundled_tools_dir` 是全局构建工具链配置，默认 `use_bundled_toolchain = true`。系统模式把所选编译器/make 的父目录前置到 PATH；软件自带模式使用 `bundled_tools_dir`（留空时优先 `<exe>/tools`，否则 `%APPDATA%\SimpleEmbedded\tools`；相对路径相对于 exe）中的工具。`toolchain.rs` 负责检测、PATH 目录和复制安装：安装时复制编译器根目录、make 及其 DLL，并写入 `toolchain.json`；安装完成后自动切换到软件自带模式。只影响本地构建和本地烧录，远程 SSH 不注入本地 PATH。
- 软件自带工具的标准发布布局是 `simple-embedded.exe` + `tools/toolchain.json` + `tools/make/make.exe` + `tools/compiler/bin/arm-none-eabi-gcc.exe`。`scripts/package-tools.ps1` 会把 release exe、当前 ARM GCC 和 make 打包到 `dist/simple-embedded/`；`dist/` 和 `tools/` 必须在 `.gitignore` 中，不要把 1 GB 级工具链提交进仓库。
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
- 工程文件是普通 JSON（`ProjectSettings { version, build, debug }`），保存 Build/Clean/Rebuild/Flash/SSH、OpenOCD/pyOCD、并行任务数、C/C++ 属性覆盖、构建选项、内存布局 `build.memory`，以及调试程序 `debug.program` 和 gdb 路径 `debug.gdb_path`。可以手改，也可以和源码一起提交。
- 全局设置继续保存最近工程、编辑器偏好、输出面板状态等；`Settings::load` 容忍 UTF-8 BOM，解析失败会回退默认值并输出到 stderr。
- Makefile 备份位于 `%APPDATA%\SimpleEmbedded\backups\<工程名>-<hash>\`，默认保留最近 12 份。
- 自动检测结果只是初值；用户可以随时在设置里改。修改设置后必须点击保存。
- 工作区文件：`<工程目录>\<工程名>.se-workspace`，内容是 VS Code 风格 JSON（`version`、`name`、`folders`、`settings`）。`config::write_workspace_file` 生成，`config::resolve_workspace_file` 解析；只使用第一个 folder。`register_workspace_association` 用 `reg.exe` 写入 HKCU 的 `.se-workspace` 关联，关联当前 exe。
- 不要创建 `.eide`、`.mini-ide` 等专有工程格式，也不要修改 Keil `.uvprojx`。允许写入工程的场景：自动创建/保存 `.simple-embedded/settings.json`、用户主动生成 `.se-workspace`、保存 Makefile 工程的构建选项/C/C++ 属性时的差异同步、用户主动添加芯片支持包时复制 `.pack`/`.pdsc`。

## UI/产品约定

- 用户界面使用中文；保持现有深色配色和 `docs/` 截图风格。
- 界面不显示 emoji/生僻符号；使用 `Painter` 画图形或已验证的字符。
- 右侧操作栏：纵向排列 Build、Clean/重构、Flash、调试、检测；不再放设置按钮，设置从构建卡片“配置”或 `Ctrl+,` 打开。检测按钮在工程打开后即可用：无调试会话时运行静态扫描并打开“内存检测 → 静态检测”；有调试会话时打开“动态监测”，可添加硬件 watchpoint（写入 `watch`、读取 `rwatch`、读写 `awatch`，GDB 优先使用硬件比较器，Cortex-M 通常 2-4 个），命中后目标自动暂停并记录新旧值。静态检测扫描所有 `.c/.h`：数组常量下标越界、`strcpy`/`strcat`/`strncpy`/`memcpy`/`memset` 常量长度、`gets`/`sprintf`、重复 `free`、可疑 `malloc` 字节数、返回数组名；结果在窗口内按文件:行显示，可点击跳转。静态扫描是启发式检查，不替代 ASAN/Valgrind；动态 watchpoint 依赖 gdb / OpenOCD / pyOCD 会话。
- 构建结果和内存占用显示在输出面板标题和日志中；构建视图标题显示 `Flash 已用 / 总量 (百分比) · RAM 已用 / 总量 (百分比)`，占用来自 `text/data/bss` 或 ELF 段，容量来自链接脚本 / PDSC / 工程信息。输出面板可切换“构建输出 / 调试输出”，底部不再保留重复的状态栏。
- 构建输出面板顶部有 10px 拖动手柄，收起时也能向上拉出；高度存 `Settings.output_height`，最大到主区域顶部。
- 调试控制是编辑器右上角的小浮层，位置参考查找浮层；停止、单步等使用 `Painter` 绘制的图案按钮。调试信息放在输出面板“调试输出”页，不在独立窗口里。
- “工程信息 → 烧录配置”提供烧录工具下拉（自定义命令 / OpenOCD / pyOCD）。OpenOCD 模式显示工具路径、程序文件、芯片配置和接口配置；pyOCD 模式显示工具路径、程序文件、目标芯片和可选 Pack；“编辑”打开独立烧录配置弹窗，含命令预览。分组还显示 Flash/RAM 容量和 **内存布局...** 按钮：弹窗采用 Cortex-Debug 风格两列网格，Read/Write 侧为 IRAM1/2 与 RAM1-3（Default、起始地址、大小、NoInit），Read/Only 侧为 IROM1/2 与 ROM1-3（Default、起始地址、大小、启动），底部显示 RAM Total / ROM Total；自动值优先来自链接脚本，其次来自匹配 PDSC 的 `<memory>`；保存到工程配置但不改写链接脚本。工具/路径下拉修改后立即写入工程 `.simple-embedded/settings.json`，弹窗中的字段点击保存后写入。
- 关闭窗口、关闭 Tab、切换工程时都要处理未保存确认。
- 不做弹窗堆叠；确认弹窗和设置弹窗互斥。
- 无工程时左上角仍显示同一张构建卡片，使用“导入 / 配置 / 工作区”按钮；中间显示单行提示。第一次启动且未打开工程时不写入任何工程文件；打开工程会创建/更新 `.simple-embedded/settings.json`。测试后如果产生 `%APPDATA%\SimpleEmbedded\settings.json` 或临时工程配置，提交前应清理，避免把测试工程路径带入首启体验。

## 已知边界（当前版本）

- 没有 `.eide`/`.mini-ide` 等专有工程格式、Keil 自动导入或链接脚本生成器；`.simple-embedded/settings.json` 和 `.se-workspace` 都是普通 JSON。芯片支持包只读取元数据、SVD 和器件内存区域，不导入 Keil 设备树、启动文件、链接脚本或烧录算法。Keil 工程可以通过手动命令或外部包装脚本接入。
- 没有 LSP/语义级跳转；符号搜索、TODO、寄存器浏览器都是轻量索引。补全/定义/引用是启发式功能；成员补全只解析常见的 struct/union 和变量声明，不处理宏展开、模板、继承或函数返回类型。
- 内存布局的起始地址/大小只写入工程 `.simple-embedded/settings.json`，不会自动改写链接脚本；Flash/RAM 总量优先来自链接脚本，其次来自芯片支持包。`.bin` 使用内存布局里的下载起始地址。
- 不支持多光标；Ctrl+Alt+左键只提供单一矩形列选择。分屏的两个窗格都可编辑，但同一个文件不能在两个窗格重复打开。
- 调试支持主机 gdb 和 ARM GDB server：ARM ELF/AXF 会复用 OpenOCD / pyOCD 配置启动 server，连接 arm-none-eabi-gdb 并 `load` 程序；调试侧栏提供变量 Live Watch 和只读寄存器表，通过 gdb `echo` 标记解析输出，但没有调用栈、内存或外设寄存器编辑面板。“动态监测”支持 `watch` / `rwatch` / `awatch` 硬件 watchpoint，命中时暂停并记录新旧值；数量受目标硬件限制，GDB 可能退化为软件 watchpoint（界面标记“软件”），OpenOCD/pyOCD 不支持时以 gdb 错误为准。调试控制浮层可关闭但不会自动停止会话。
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

当前测试覆盖：配置检测、Make 命令识别与 POSIX 工具路径、SSH 命令构造、CRLF/GBK 往返、路径显示、文件树展开、高亮语言识别、ANSI 解析、日志位置与裁剪、后台命令执行、缩进/粘贴/括号、查找、gitignore/工程搜索、Git 状态与 diff、TODO、符号索引、补全替换、SVD/寄存器（含 derivedFrom）、支持包 `.pdsc` 解析与 `.pack` 导入、OpenOCD / pyOCD 配置与命令构造、OpenOCD / pyOCD GDB server 参数、并行构建环境注入、补全上下文与成员索引、十六进制、数组生成、列编辑、gdb 路径检测、ARM ELF 目标识别、ARM gdb 检测、空闲调试端口、gdb 停止位置与无代码断点解析、gdb watchpoint 行解析（含 `(gdb)` 前缀/软硬件）、工程配置往返、损坏工程配置检测、工作区文件解析、调试产物检测、文件复制命名和路径重映射、ELF/日志内存占用、链接脚本/PDSC 内存区域、.bin 烧录地址、C 数组越界/危险字符串函数/重复释放静态扫描、同名宏重复/宏族相同值、标识符悬停/文档注释/返回类型、设置默认值、Makefile newlib-nano 浮点开关同步。当前共 161 个测试。

UI 改动后建议：

1. 启动一次 release 或 debug 版本
2. 用 `examples/blink` 打开 `src/main.c`
3. 实际点击右侧操作栏 Build，确认输出面板、操作栏、文件树正常
4. 如果界面视觉有变化，更新 `docs/editor.png` / `docs/welcome.png`

## Git / 工作流

- 远端：`origin` = https://gitee.com/chen_alanson/embedd.git
- 主分支：`master`（跟踪 `origin/master`）
- 本仓库开发时不要提交 `target/`、`examples/blink/build/`、`*.log`、`.opencode/`、`.simple-embedded/` 或 `*.se-workspace`；这些已写入 `.gitignore`。普通用户工程是否提交 `.simple-embedded/settings.json` / `.se-workspace` 由用户决定。
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