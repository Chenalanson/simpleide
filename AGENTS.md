# AGENTS.md

本文件记录 Simple embedded 的项目约定、架构和验证方式，供后续编码代理快速进入状态。

## 项目概览

Simple embedded 是一个 Windows 桌面端、文件夹优先的极简嵌入式 IDE：

- 左侧文件树
- 中间多标签代码编辑 + syntect 语法高亮
- 底部异步构建日志
- 本地 / 远程 SSH 构建
- 可配置的本地 Flash 命令

核心产品约束：**工程文件夹就是工程**。不要引入自定义工程文件（如 `.eide`、`.mini-ide`）。用户设置默认只写到用户目录。例外：Makefile 工程在用户保存构建选项或 C/C++ 属性时，会按差异同步到现有 Makefile（修改前自动备份到用户目录），使修改真正参与构建；用户主动添加芯片支持包时，会把选中的 `.pack` / `.pdsc` 复制到工程根目录。不得修改 Keil `.uvprojx`，也不得借机创建自定义工程文件。

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
src/config.rs          自动检测、每工程配置、SSH 命令构造、设置读写
src/project_meta.rs    Makefile / Keil .uvprojx 工程元数据提取
src/editor.rs          EditorBuffer（String + revision）和 Document
src/file_tree.rs       惰性目录树
src/flash.rs           烧录工具配置、OpenOCD 检测与命令构造
src/completion.rs      轻量成员索引、补全上下文和候选类型
src/highlight.rs       syntect 高亮和 galley 缓存
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
src/debugger.rs        gdb 会话
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
- 左侧构建卡片顶部显示工程名（粗体）和路径，路径使用 `util::display_path` 去掉 Windows `\\?\` 前缀；下方显示构建模式、命令和两个按钮。未导入时按钮为“导入 / 配置”，导入后为“更换 / 配置”。
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
- `registers.rs` 优先用正则解析工程内 `.svd` 的 peripheral/baseAddress/register/addressOffset/description；没有可用 SVD 时回退扫描 `.h/.c` 中 `*_BASE` / `*_ADDR` / `*_ADDRESS` 宏。`Ctrl+Shift+R` 或工具栏“寄存器”打开，最多 2000 项并去重。它是只读浏览器，不读写芯片寄存器。
- `debugger.rs` 启动本机 gdb CLI（非 MI），通过 stdin/stdout 转发命令并收集控制台输出；`DebugState` 保存会话、断点和日志。右侧“调试”或 `F5` 启动/继续，`F9` 切换断点，`F10` 下一步，`F11` 单步，`Shift+F11` 跳出，`Shift+F5` 停止。调试程序默认取设置路径，否则从 `build/`、`Debug/`、`Release/` 等目录挑最新的 ELF/AXF/EXE；gdb 架构必须与目标匹配（ARM 需 arm-none-eabi-gdb）。
- `navigation.rs` 提供标识符/前缀提取与字符区间替换；`F12` 在 `symbols.rs` 索引中找定义，`Shift+F12` 用全工程正则搜索列引用。
- 补全在输入至少 2 个标识符字符后自动弹出，锚定在光标下方；输入 `.` / `->` 时切换为成员补全。`↑` / `↓` 选择，`Enter` / `Tab` 插入，`Esc` 关闭；`Ctrl+Space` 仍可手动打开。全局候选来自 `symbols.rs` 的宏/函数/类型索引；成员候选由 `completion.rs` 用轻量正则解析 struct/union 和变量声明得到，成员索引在保存、刷新文件树或刷新符号时失效重建。它不是 C/C++ 语义解析：复杂宏展开、模板、继承、跨文件作用域和函数返回类型可能漏检。
- 粘贴由编辑器在 TextEdit 前拦截 `Event::Paste`：`indent::adjust_pasted` 统一换行并按当前行缩进对齐后续行，同时保留粘贴内容相对缩进；只读或编辑器未聚焦时不拦截。
- 分屏只从标签页右键菜单进入：`向右拆分` / `拆分和移动 → 向上拆分` / `取消拆分`。`split_primary` + `split_path` + `split_direction` 决定两个窗格，两个窗格都调用 `show_code_editor`，都可编辑；`split_focus` 记录焦点窗格，点击窗格会切换 active。当前不支持同一个文件在两个窗格重复打开。
- 标签页右键菜单还提供关闭其他、关闭右侧标签页、关闭已保存、全部关闭；保存、另存为；复制完整路径、相对路径和面包屑路径；在文件资源管理器中显示、在资源管理器视图中显示；以及拆分操作。批量关闭遇到未保存文件时走 `PendingAction::CloseTabs` 确认。另存为会写入新路径并把标签页、分屏引用和语法识别一并切换到新文件。快捷键：Ctrl+F4 关闭当前，Ctrl+\ 向右拆分，Ctrl+K U 关闭已保存，Ctrl+K W 全部关闭，Ctrl+K S 保存全部，Ctrl+K Ctrl+\ 向上拆分，Ctrl+Shift+S 另存为，Shift+Alt+C 复制路径，Ctrl+K Ctrl+Shift+C 复制相对路径，Shift+Alt+R 在资源管理器中显示。
- 标签页右键菜单和所有弹出菜单使用深色主题；`theme::install` 强制 `ThemePreference::Dark`，避免系统浅色主题把菜单画成白色。窗口/弹窗的关闭 X 统一使用 `window_close_button`，尺寸和绘制方式与查找浮层的关闭按钮一致。
- 查找是编辑器右上角的约 420px 紧凑浮层 `show_find_overlay`（内部复用 `show_find_bar`），只在 `find.open` 时由 `Ctrl+F` / `Ctrl+H` 打开，不占用独立行；左侧下拉箭头展开替换行，宽度不变，`AB` 保留大小写在替换输入框内，替换/全部替换为图标按钮。`Ctrl+G` 打开同宽的跳行浮层，输入框复用查找输入框样式；提示文字为普通次要色，不使用色块。
- 面包屑栏只显示从工程根目录开始的路径（`doc.rel_path` 的 `/` 替换为 ` > `）和未保存标记；编码、行尾、语言、只读、寄存器、数组、符号、格式化、自动换行、保存移到最底部状态栏。补全和列编辑不出现在可见工具栏。
- 左侧项目栏、编辑器区、右侧操作栏、底部输出区是四块带圆角的不同底色容器（外圈 3–4px `theme::GUTTER` 深色间隙），不再用亮色分隔线；所有 `Panel` 设置 `show_separator_line(false)`。右侧操作栏宽约 58px（原 110px 的一半），按钮内边距同步收窄。编辑器卡底部两角由 `paint_bottom_corners` 补绘 `GUTTER` 圆角，输出面板标题栏紧贴面板顶部，拖动区域合并到标题栏顶部 4px，日志区使用 `EDITOR_BG` 并撑满剩余空间。
- 设置弹窗第一行是 构建 / 构建工具链 / 编辑器 / 调试 / 其他 五个页签，下方只显示当前页；构建工具链页提供编译器、make 的输入框和浏览按钮，可从 PATH 自动检测，也提供“软件自带”目录、工具状态和一键安装。
- 最底部状态栏是贯穿整个窗口宽度的矩形条（无圆角、无外圈间隙），内容靠右排列：右侧操作按钮 + 行列/缩进/编码/行尾/语言信息。
- `column.rs` 仍保留旧列编辑算法；`column_select.rs` + `ColumnSelectionState` 提供 Ctrl+Alt+鼠标左键拖动的矩形列选择，蓝色高亮，Ctrl+C 复制、Delete/Backspace 删除、直接输入替换。列编辑对话框已不在任何可见入口暴露。
- 目前没有多光标；列选择是单一矩形区域，不支持多个独立光标。
- `log.rs` 的 `parse_location` 解析 `file:line:col`；日志行带位置时可点击跳转。`LogStore` 维护最多 1000 条问题诊断，`App::refresh_diagnostics` 解析为本地路径并在行号栏右侧画红/黄竖条；输出面板可过滤全部/错误/警告。
- 当前版本未内置 LSP；补全、跳转和引用搜索是轻量符号索引，调试是基础 gdb 控制台，不是完整 IDE 调试前端。后续增强在 `xiug` 分支分批接入，不要伪造未接通的能力。

### 工程信息

- `project_meta.rs` 在打开工程时读取 Makefile 或 Keil `.uvprojx`，提取目标名、芯片、CPU、浮点、链接脚本、内存布局、包含目录、宏定义等。
- 左栏先显示资源管理器，再显示“工程信息”；分割线紧贴“工程信息”标题上下，包含“构建配置”“芯片支持包”“烧录配置”“C/C++ 属性”四个分组。分组标题以及三个 C/C++ 列表标题左侧各有折叠箭头，整行可点击并切换展开；点击行会记录选中状态并显示淡蓝色背景，加号点击只添加、不折叠。展开和选中状态存在 egui 内存中。数据按工程缓存，刷新文件树时重新读取。
- “芯片支持包”分组右侧的绿色 `+` 打开文件选择器，只接受 Keil/CMSIS `.pack` / `.pdsc`，并把文件复制到工程根目录（同名文件不覆盖）。`support_pack.rs` 解析包名、厂商、版本、设备列表和 SVD 数量；`.pack` 通过 zip 读取包内 `.pdsc`，不整体解压。寄存器浏览器在工程内没有独立 `.svd` 时，会直接读取第一个 `.pack` 内的 SVD。支持包不会生成自定义工程文件，也不代表完整的 Keil 设备树/启动文件/烧录算法导入。
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
- `BuildConfig.flash_tool` 为 `custom` / `openocd`。`custom` 使用 `flash_command` 经 shell 执行；`openocd` 使用 `flash.rs` 直接构造参数运行 `openocd -f interface/... -f target/... -c "program ... verify reset exit"`，不经过 shell，路径空格由 `Command` 处理。`OpenOcdConfig` 保存工具路径、程序文件、芯片配置和接口配置；只写 `stm32f1x.cfg` / `stlink.cfg` 时会自动补 `target/` / `interface/`，程序文件支持 `${ExecutableName}`、`${ProjectName}`、`${workspaceFolder}`、`${workspaceRoot}`。工具路径下拉会检测 PATH、Program Files、PlatformIO 等常见 OpenOCD 安装位置。
- OpenOCD 的程序文件默认按 `ProjectMeta` 的 output_dir 和常见 `build/Debug/Release` 目录选择最新产物；Flash 前找不到程序文件会直接报错，不自动构建。Flash 不依赖编译器/make，软件自带工具链缺失时不应阻止烧录。
- pyOCD 目前没有内置；可继续用 `custom` 命令接 `pyocd flash`。
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

设置路径：

```text
%APPDATA%\SimpleEmbedded\settings.json
```

- 每个工程按规范化路径独立保存构建配置。
- Makefile 备份位于 `%APPDATA%\SimpleEmbedded\backups\<工程名>-<hash>\`，默认保留最近 12 份。
- 自动检测结果只是初值；用户可以随时在设置里改。
- 修改设置后必须点击保存。
- `Settings::load` 容忍 UTF-8 BOM；解析失败会回退默认值并输出到 stderr。
- 不要在工程目录创建任何配置文件。允许写入工程文件的场景只有两个：保存 Makefile 工程的构建选项 / C/C++ 属性，以及用户主动添加芯片支持包时把 `.pack` / `.pdsc` 复制到工程根目录；Keil `.uvprojx` 始终只读。

## UI/产品约定

- 用户界面使用中文；保持现有深色配色和 `docs/` 截图风格。
- 界面不显示 emoji/生僻符号；使用 `Painter` 画图形或已验证的字符。
- 右侧操作栏：纵向排列 Build、Clean/Rebuild/Flash、调试；不再放设置按钮，设置从构建卡片“配置”或 `Ctrl+,` 打开。
- 构建结果显示在输出面板标题和日志中；底部不再保留重复的状态栏。
- 构建输出面板顶部有 10px 拖动手柄，收起时也能向上拉出；高度存 `Settings.output_height`，最大到主区域顶部。
- “工程信息 → 烧录配置”提供烧录工具下拉（自定义命令 / OpenOCD）。OpenOCD 模式显示工具路径下拉和浏览按钮，以及程序文件、芯片配置、接口配置；“编辑”打开独立烧录配置弹窗，含命令预览。工具/路径下拉修改后立即写入本机设置，弹窗中的字段点击保存后写入。
- 关闭窗口、关闭 Tab、切换工程时都要处理未保存确认。
- 不做弹窗堆叠；确认弹窗和设置弹窗互斥。
- 无工程时左上角仍显示同一张构建卡片，使用“导入 / 配置”按钮；中间显示单行提示。第一次启动不写入任何工程文件。测试后如果产生 `%APPDATA%\SimpleEmbedded\settings.json`，提交前应清理，避免把测试工程路径带入首启体验。

## 已知边界（当前版本）

- 没有自定义工程格式、Keil 自动导入或内存布局编辑器；芯片支持包只读取元数据和 SVD，不导入 Keil 设备树、启动文件、链接脚本或烧录算法。Keil 工程可以通过手动命令或外部包装脚本接入。
- 没有 LSP/语义级跳转；符号搜索、TODO、寄存器浏览器都是轻量索引。补全/定义/引用是启发式功能；成员补全只解析常见的 struct/union 和变量声明，不处理宏展开、模板、继承或函数返回类型。
- 不支持多光标；Ctrl+Alt+左键只提供单一矩形列选择。分屏的两个窗格都可编辑，但同一个文件不能在两个窗格重复打开。
- 调试只提供 gdb 控制台和基本执行控制，没有变量/调用栈/寄存器面板，也不会自动下载固件；需要用户提供与目标架构匹配的 gdb。
- 没有内嵌 ssh-mcp 或 jlink-mcp 客户端；远程 SSH 走系统 OpenSSH，Flash 走本地自定义命令或内置 OpenOCD 配置。pyOCD 尚未内置，需要先用自定义命令接入。
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

当前测试覆盖：配置检测、Make 命令识别与 POSIX 工具路径、SSH 命令构造、CRLF/GBK 往返、路径显示、文件树展开、高亮语言识别、ANSI 解析、日志位置与裁剪、后台命令执行、缩进/粘贴/括号、查找、gitignore/工程搜索、Git 状态与 diff、TODO、符号索引、补全替换、SVD/寄存器、支持包 `.pdsc` 解析与 `.pack` 导入、OpenOCD 配置与命令构造、补全上下文与成员索引、十六进制、数组生成、列编辑、gdb 路径检测。当前共 107 个测试。

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