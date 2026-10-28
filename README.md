# Simple embedded

一个 Windows 桌面端的极简嵌入式编辑器：**文件夹就是工程**，导入目录 → 编辑代码 → 一键构建 → 查看彩色日志。没有强制工程向导，也不修改 Keil `.uvprojx`；工程构建与烧录配置保存在工程内 `.simple-embedded/settings.json`。

使用 Rust + `eframe/egui` 编写。界面左侧构建卡片显示工程名、路径、检测到的芯片型号和构建模式，下面是文件树；中间是多标签代码编辑，底部是异步构建输出，操作集中在右侧栏。

![Simple embedded](docs/editor.png)

未打开工程时，左上角仍显示同一个构建卡片，只是名称/路径显示为未导入状态，操作按钮为“导入 / 配置 / 工作区”。

![导入工程](docs/welcome.png)

主题设置窗口（亮色示例）：

![主题设置](docs/theme.png)

AI 模式（三栏工作台，进入时带粉碎 / 流星转场）：

![AI 模式](docs/ai.png)

从打开目录到构建、SSH、烧录和常见问题的完整中文教程：[docs/TUTORIAL.md](docs/TUTORIAL.md)。烧录工具（ST-Link、CubeProgrammer、OpenOCD、pyOCD、J-Link、Keil）的详细配置：[docs/FLASH.md](docs/FLASH.md)。动态监测（watchpoint）的时间戳、调用栈和自动启动：[docs/DYNAMIC.md](docs/DYNAMIC.md)。

## 功能

- 打开任意文件夹作为工程；构建系统以 `Makefile` / `Cargo.toml` / `CMakeLists.txt` / `.uvprojx` 为准，只有 `main.py` / `boot.py` 的目录识别为 MicroPython；工程配置只存在于 `.simple-embedded/settings.json`
- 标题栏与界面配色一致（亮色主题使用亮色标题栏），构建 / 清理 / 重构建 / 烧录集中在右侧操作栏；配置界面从构建卡片的“配置”或 `Ctrl+,` 打开
- 全应用主题：右侧操作栏底部 **设置** 打开设置窗口，第一个菜单是 **编辑器主题**，内置 40 套暗色 + 10 套亮色（共 50 套）。点击主题即时预览，整个软件（编辑器、侧栏、面板、菜单、按钮、状态栏和 Windows 标题栏）一起换肤；保存后写入全局设置，取消或关闭会恢复原主题
- AI 模式：右侧操作栏底部 **AI模式**（`Ctrl+Alt+A`）进入，界面变为 AI 输出 / 输入 / AI 工作台三栏。首次使用点 **设置** 填写 API Key、API 地址和模型名称（默认 DeepSeek 兼容接口），保存在用户目录、不写入工程。AI 会加载工程或其上级目录的 `.opencode/skills/stm32-embedded-dev` skill（找不到时使用内置副本），结合芯片、构建配置、文件树、`hardware.dts`、已保存的 `main.c`、`.ioc` 和资源报告生成带文件路径的代码；回复支持 Markdown、代码块复制和聊天记录。进入时编辑器粉碎消散、流星从右下角升起并展开三栏，转场为纯视觉动画
- 左侧栏全高，资源管理器与工程信息默认各占一半高度；文件树较短时工程信息自动上移紧接文件列表；文件树较长时可拖动中间分割线调整比例；拖动左栏右边缘可调整宽度；分割比例和折叠状态会保存
- 左侧构建卡片显示工程名（加粗）、完整路径、检测到的芯片型号和构建模式；未导入时按钮为“导入 / 配置 / 工作区”，导入后为“更换 / 配置 / 工作区”
- 资源管理器、工程信息、构建配置等可折叠区域带缓动动画和旋转箭头；状态提示 Toast 使用更大字号，便于阅读
- 工程信息树：分组和列表标题左侧都有折叠箭头，整行可点击并显示淡蓝色选中背景；**工具链** 在这里只读显示，修改入口是 **配置 → 构建 → 工具链**（自动检测 / Make GCC / Clang / AC5 / AC6 / Keil MDK AC5 / AC6 / Cargo / CMake / MicroPython）。自动检测先看工程根目录的 Cargo.toml / Makefile / CMakeLists.txt / `.uvprojx`，再查 PATH 和常见安装目录；根目录只有 `main.py` / `boot.py` 时识别为 MicroPython。CubeMX 同时生成 MDK-ARM 和 Makefile 时，Make 构建读取 Makefile 元数据，Keil 工具链读取 `.uvprojx`，不再无条件显示 Keil；支持识别 armclang（AC6）和 armcc（AC5）。列表右侧 `+` 可添加；包含目录支持 **自动检测包含目录**，递归扫描 `.h` / `.hpp` 等头文件并把所在父目录追加进去，并清理只含 `.c` 的目录；**同步源文件到 Makefile** 会把工程内未加入的 `.c` / `.cpp` 追加到 `C_SOURCES`（跳过 Drivers / Middlewares / CMSIS 等供应商目录）；仅有效构建系统为 Make 时同步到 Makefile（自动备份），Keil `.uvprojx` 只读
- MicroPython / ESP32：在 **配置 → 构建 → 工具链** 选择 **MicroPython** 后，下方内容切换为 **串口连接**，可选择端口和波特率（默认 115200），端口旁的 **自动检测** 会扫描串口并优先选择 ESP32 / CP210x / CH340 / FTDI 等 USB 串口；端口和波特率保存在工程 `.simple-embedded/settings.json` 的 `serial` 字段。右侧操作栏切换为 **运行 / 上传 / 固件 / 复位**：**运行** 把当前 `.py` 上传为 `main.py` 并软复位 ESP32，**上传** 写回开发板（从芯片下载的文件会优先写回原芯片路径），**固件** 调用 esptool 烧录 MicroPython `.bin`，**复位** 发送 Ctrl-D；串口和 esptool 输出显示在底部的 **MicroPython 输出** 页。左侧“工程信息”在 MicroPython 模式下替换为 **芯片文件**，展开后自动读取开发板文件（目录 + 大小），使用与资源管理器一致的树形行（文件夹可展开、文件大小右对齐），刷新按钮固定在分区底部；点击文件会读取到工程并打开到编辑器。固件烧录需要单独安装 esptool，并配置合并固件、芯片型号、烧录地址、整片擦除和写入校验。当前未内置交互式 REPL
- 芯片支持包：在工程信息中点击 `+` 添加 Keil/CMSIS `.pack` / `.pdsc`，文件放在工程根目录；显示包名、版本、厂商、设备和 SVD 数量，寄存器浏览器可直接使用包内 SVD
- 构建选项：包含全局选项、C/C++ 编译器、汇编器和链接器四个页面，并按当前工具链切换内容。GCC/Clang 先用 Use MicroLIB 作为浮点选项总开关，勾选后才能启用 newlib-nano `printf` / `scanf` 浮点支持（`-u _printf_float` / `-u _scanf_float`），另有 LTO 和函数分段；Keil AC5/AC6 显示 Use MicroLIB、Split Load/Store 和 Keil 链接器字段。Make + GCC/Clang 工程打开时读取现有 `OPT` / `CFLAGS`，保存时按差异写回；选择 `AXF` 会生成 `.axf` 副本规则；Keil 和 Make AC5/AC6 只写入工程配置，不修改 `.uvprojx`
- 左侧目录树：第一行显示工程根目录，可展开/折叠；支持隐藏文件、刷新、点击打开文件；右键可新建文件/文件夹、粘贴、剪切/复制、重命名、删除、复制路径、在资源管理器中显示或用 VS Code 打开
- 多文件 Tab、脏标记、关闭前提示保存
- 文本编辑：`Ctrl+S` 保存、Tab 缩进、撤销/重做、复制粘贴等由 egui 文本编辑控件提供
- syntect 语法高亮：C/C++/ASM/Rust/Makefile/CMake/TOML/JSON 等
- 自动换行开关、代码字号调节、相对行号、空白字符、换行标尺、CRLF/LF 保持
- UTF-8 / GBK 自动识别与保存编码切换（兼容老 Keil 工程）
- 查找 / 替换：大小写、正则、上下一个、全部替换，匹配高亮
- 全工程搜索：结果列表、路径/行号/预览、点击跳转
- 资源管理器遵循 `.gitignore`，支持追加全局忽略规则
- 自动保存（可配置延迟）、文件只读锁定、clang-format 手动/保存时格式化
- Git 状态：文件树 M/U/A/D/R 徽标、修改行标记、分支与变更列表
- TODO / FIXME / HACK / NOTE 工程扫描，类型过滤与点击跳转
- 工程符号搜索：C/C++ 函数、结构体、宏与汇编标签
- 十六进制查看器，以及从选中内容 / 文件 / 二进制数据生成 C 数组
- 单击 `.bin` / `.elf` / `.axf` 等二进制文件，在中央编辑器以只读十六进制 + ASCII 标签页打开，可滚动查看；右键十六进制查看器仍保留
- 外设寄存器树：解析 SVD 的外设 → 寄存器 → 位域，显示位域值，目标暂停时可直接修改寄存器和位域；没有 SVD 时回退到 CMSIS 头文件宏，并支持导入 `.svd`
- Tab / Shift+Tab 缩进、选中行整块缩进、回车自动缩进，可配置空格或 Tab 与缩进宽度
- 右侧代码预览小地图，可点击或拖动跳转；缩进参考线会高亮当前缩进层级，二者均可在设置中关闭
- 构建输出右侧的 **内存可视化**：解析 ELF 符号，显示 `.data` / `.bss` 全局变量分布、栈区预留和 SP 使用预估、FreeRTOS `ucHeap` 堆区；分隔条可调宽度，高度与调试侧栏一致
- 沉浸式编码：`Ctrl+Shift+Enter` 让窗口进入无边框全屏并隐藏标题栏、任务栏、左右栏、底部输出面板和状态栏，编辑器占满屏幕；右侧悬浮资源管理器可展开目录并打开文件，点标题栏三角可收起成一条标题栏、再次点击展开，同一快捷键或关闭悬浮窗退出并恢复窗口
- 调试：ARM 目标自动复用烧录配置的 OpenOCD / pyOCD 参数启动 GDB server，再用 arm-none-eabi-gdb 连接并加载 ELF/AXF；主机 EXE 使用普通 gdb。启动后先停在 `main` 并自动定位入口文件；按钮显示运行状态，运行中主按钮变为暂停图案；点击行号左侧或 F9 设置断点，F5 继续/暂停、F10/F11 单步、Shift+F5 停止；暂停时自动打开并高亮当前执行行，浮层准心按钮可随时回到当前执行行；右侧提供变量 Live Watch、CPU 寄存器和按外设/寄存器/位域展开的外设树（需 SVD），暂停时可修改寄存器，也可输入任意 gdb 命令
- 非法检测：工程打开后即可点击右侧 **检测**。未启动调试时打开 **静态检测**，扫描工程内数组常量下标越界、`strcpy`/`memcpy` 等常量长度、`gets`/`sprintf`、重复 `free`、可疑 `malloc` 和返回数组名；调试会话存在时打开 **动态监测**，可添加 `watch` / `rwatch` / `awatch` 硬件 watchpoint，命中后目标暂停并记录时间、旧值 → 新值、源码位置和调用栈。勾选“调试时自动启动动态监测”后，启动调试会自动打开面板并重新启用已保存的 watchpoint。数量受目标硬件限制，GDB 可能退化为软件 watchpoint。结果显示在窗口内并支持点击跳转。静态扫描不替代 ASAN/Valgrind
- 轻量导航/补全：F12 跳定义、Shift+F12 查引用；输入 2 个字符后自动在光标旁弹出补全，输入 `.` / `->` 时补全结构体成员；↑↓ 选择，Enter/Tab 插入，Esc 关闭，Ctrl+Space 手动打开
- 编辑器内悬停函数名、变量名、结构体或宏时，显示声明签名、最近的注释以及返回值 / 类型；函数跨文件时会查轻量符号索引。提示下方继续显示 **调用关系**（左侧调用者、右侧被调用者的框图，以及调用树）和 **引用** 树（按文件分组，覆盖函数、变量、宏和结构体）。提示跟随鼠标显示，可在配置 → 其他 关闭
- 宏诊断：同一文件内同名 `#define` 重复显示红色波浪线；配置 → 其他 的“宏重复值提示（黄色波浪线）”开启后，`PWM1PIN` / `PWM2PIN` / `PWM3PIN` 这类宏族使用相同替换值时显示黄色波浪线（C 语言本身允许这种写法，默认关闭；供应商头文件不参与）
- 多行粘贴自动对齐当前缩进；`Ctrl+Alt+左键拖动` 矩形列选择，可复制、删除或直接输入替换
- 多光标：`Alt+点击` 添加光标，`Ctrl+Alt+↑/↓` 在上下行添加光标，`Ctrl+D` 选择当前词并添加下一个匹配，`Esc` 清除；输入、删除、回车、Tab、粘贴会作用于所有光标/选择，`Ctrl+C` 复制全部选择
- 代码折叠：鼠标经过可折叠行时在行号右侧显示下箭头，点击折叠；折叠后隐藏起始行和结束行之间的块内行，结束行（右花括号）保留可见，点击行号右侧右箭头恢复。也支持 `Ctrl+Shift+[` 折叠当前块、`Ctrl+Shift+]` 展开全部；行号栏保留源文件行号，可见行仍可直接编辑
- 撤销/重做：跨文件快照，`Ctrl+Z` 撤销，`Ctrl+Shift+Z` / `Ctrl+Y` 重做；深度在 **配置 → 其他 → 撤销栈深度** 设置，默认和上限为 64 次
- clangd 语言服务（可选）：在 **配置 → 其他 → 语言服务** 启用；自动用工程的 `-I`、`-D` 和 Makefile 的 `-mcpu/-mfpu/-mfloat-abi` 生成 `compile_commands.json`，提供 `F12` 跳转定义、`Shift+F12` 查找引用、类型悬停、自动补全和 `Ctrl+Shift+F2` 重命名符号
- 系统安装：配置 → 其他 → **系统集成** 可安装到 `%LOCALAPPDATA%\Programs\SimpleEmbedded`，复制/链接自带工具链，创建开始菜单快捷方式、Windows 应用列表卸载项和文件夹右键菜单。安装后右键任意文件夹选择 **用 Simple embedded 打开** 即可打开工程；Windows 11 可能位于“显示更多选项”。也可运行 `simple-embedded.exe --install-system` / `--uninstall-system`
- 工程资源报告：工程信息 → **资源报告** → **生成资源报告**（工作区菜单也可），后台扫描并保存 `<工程名>.resources.json` 到工程根目录。报告包含芯片 / 内核 / Flash / RAM、SVD 外设和寄存器、`.ioc` 引脚和组件、固件 ELF 栈堆和全局变量、源码 / 头文件 / 链接脚本 / 库清单、源码行数、软件组件、符号统计、调用图统计、构建 / 调试配置和系统环境；只读工程，不改写 Makefile / Keil / 链接脚本
- 构建失败时识别链接器的 `undefined reference to`，在输出面板提示缺失的函数实现和 `C_SOURCES` 检查项
- 标签页右键菜单：关闭其他/右侧/已保存/全部；保存、另存为；复制完整路径、相对路径或面包屑路径；在文件资源管理器 / 资源管理器视图中显示；向右拆分、拆分和移动 → 向上拆分、取消拆分。两个拆分窗格都可编辑
- 标签页下显示工程相对路径面包屑；最底部状态栏显示行列、缩进、编码、行尾、语言和保存/换行/格式化等操作
- `Ctrl+F` 在编辑器右上角弹出约 420px 的紧凑查找浮层（Aa / ab / 正则 / 箭头上下一个 / 替换 / AB 保留大小写），`Ctrl+G` 打开同宽跳行浮层
- Build / Clean / Rebuild / Flash 在后台线程执行，UI 不冻结；调试服务与 gdb 在后台启动，日志实时进入调试输出
- 实时抓取 stdout/stderr，解析 ANSI 颜色，按 `error` / `warning` 自动标红/标黄；错误行可点击跳转，行号栏显示红/黄诊断条，日志可过滤错误/警告
- 构建前自动保存所有脏文件；可停止构建进程树
- 构建输出面板顶部可拖动调高度，收起后也能向上拉出；高度会保存
- 本地构建或远程 SSH 构建（复用系统 OpenSSH）
- 并行构建：配置 → 构建 → 并行任务数；Make 注入 `MAKEFLAGS=-jN`，Cargo 注入 `CARGO_BUILD_JOBS`，CMake 注入 `CMAKE_BUILD_PARALLEL_LEVEL`，命令里已有 `-j` 时不重复添加
- 烧录工具可选 **自定义命令** / **OpenOCD** / **pyOCD**：支持工具路径下拉/浏览、`${ExecutableName}.hex` 等程序文件变量、OpenOCD 芯片配置和接口配置。接口配置是预设下拉（ST-Link、CMSIS-DAP / DAPLink、J-Link、XDS110、ULINK 等）+ 自定义输入 + **自动检测**；默认不再自动填 `stlink.cfg`，自动检测通过 USB VID 识别已连接探针。pyOCD 目标芯片与可选 Pack、命令预览；Flash 始终在本地工程目录执行。工程信息 → 烧录配置 → **内存布局** 可设置 Flash/RAM 起始地址、大小和 .bin 下载起始地址
- 构建输出标题栏显示 Flash / RAM 占用和芯片容量百分比；容量优先读取链接脚本，其次读取匹配的芯片支持包 `.pdsc`，没有 `size` 输出时直接解析 ELF 段大小
- 每工程独立的构建、烧录、C/C++ 和调试配置保存在工程内 `.simple-embedded/settings.json`（包括 `debug.gdb_path`）；用户目录作为旧配置迁移和全局偏好存储
- 工作区文件：构建卡片“工作区”可生成 `<工程名>.se-workspace` 并关联双击打开；也可从“工作区 → 打开工作区文件…”选择 `.se-workspace` / `.code-workspace`
- 配置界面分为 构建 / 编辑器 / 调试 / 其他：构建页可设置工具链、命令、并行任务数和工具链路径；旧版本的“构建工具链”页已移除，软件自带 `tools/` 存在时仍会自动优先使用
- “工具链路径”多行输入对应工程配置中的 `extra_paths`，高度为 Build 输入框的三倍，内容超出时在框内滚动；本地构建时会前置到 PATH。本地 Make 构建会自动接入 Git for Windows / MSYS 的 `rm` 等 Unix 工具

## 运行

需要 Rust 1.92+（开发环境使用 Rust 1.99）。目标平台为 Windows，代码也保留了 Linux/macOS 的本地构建分支。

```powershell
cargo run --release
```

也可以直接带工程路径启动，第二个参数会额外打开一个文件：

```powershell
cargo run --release -- D:\firmware\my-board src\main.c
```

也可以直接传入 `.se-workspace` / `.code-workspace` 工作区文件：

```powershell
cargo run --release -- D:\firmware\my-board\my-board.se-workspace
```

也可以直接从 AI 模式启动（需要同时给出工程路径）：

```powershell
cargo run --release -- --ai D:\firmware\my-board
```

构建后的可执行文件：

```text
target\release\simple-embedded.exe
```

## 快捷键

| 快捷键 | 操作 |
| --- | --- |
| `Ctrl+O` | 打开工程文件夹 |
| `Ctrl+S` | 保存当前文件 |
| `Ctrl+Shift+S` | 另存为 |
| `Ctrl+K S` | 保存所有文件 |
| `Shift+Alt+C` | 复制当前文件完整路径 |
| `Shift+Alt+R` | 在文件资源管理器中显示当前文件 |
| `Ctrl+B` | Build |
| `Ctrl+Shift+B` | Rebuild |
| `Ctrl+W` | 关闭当前标签页 |
| `Ctrl+,` | 打开配置 |
| `Ctrl+=` / `Ctrl+-` | 放大 / 缩小代码字号 |
| `Ctrl+Shift+Enter` | 进入 / 退出沉浸式编码模式 |
| `Ctrl+Alt+A` | 进入 / 退出 AI 模式 |

## 构建命令如何工作

打开工程时自动识别：

| 检测到 | Build | Clean | Rebuild |
| --- | --- | --- | --- |
| `Cargo.toml` | `cargo build` | `cargo clean` | `cargo clean && cargo build` |
| `Makefile` | `make`，否则尝试 `gmake` / `mingw32-make`（Windows） | `<make> clean` | `<make> clean && <make>` |
| `CMakeLists.txt` | `cmake -S . -B build && cmake --build build` | 清理 target | clean-first |

检测结果只是初始值，点击左上角工程卡片的 **配置** 可随时修改。编辑器只负责调用命令，不接管 CMake/Make/Cargo 的构建逻辑。

### 软件自带工具链

发布版可以把 make 和编译器放在程序同级的 `tools/` 目录，导入 Makefile 工程后直接构建，不需要用户配置系统 `PATH`：

```text
simple-embedded.exe
tools/
  toolchain.json
  make/make.exe
  compiler/bin/arm-none-eabi-gcc.exe
```

默认优先使用 `tools/`；没有 `tools/` 时回退到用户目录工具链和系统 `PATH`。配置 → 构建 的 **工具链路径** 可以按行补充编译器、make、UV4 等目录，不需要再安装或切换工具来源。

本地打包当前工具链和 release exe：

```powershell
cargo build --release
powershell -ExecutionPolicy Bypass -File scripts\package-tools.ps1
```

生成 `dist\simple-embedded\`（含 exe 和约 1.1 GB 的 ARM GNU 工具链 + make）。仓库只追踪 `dist/` 下的所有 `.exe`（应用 exe 和 `tools` 里的工具 exe）；编译器 DLL、库、头文件、`tools.zip` 以及 `.compiler.tmp` / `.make.tmp` 仍被忽略，仅 exe 不足以构成完整可运行工具链。

构建前会自动保存所有未保存文件，然后通过本机 shell（Windows 为 `cmd /S /C`）在工程根目录启动进程。日志从 stdout/stderr 双流读取；Windows 下停止会调用 `taskkill /T` 终止子进程树。

POSIX 风格的 Makefile 常用 `rm`、`cp` 等命令；在 Windows 上运行本地 Make 构建时，程序会自动寻找 Git for Windows / MSYS / Cygwin 的 `usr\bin` 并前置到 PATH。也可以在配置中手动填写额外工具链路径。

### 远程 SSH 构建

配置 → 构建位置选择 **远程 SSH**，填写：

- SSH 主机：`user@host`，或 `~/.ssh/config` 中的别名
- SSH 端口（默认 22）
- 远程工程目录：如 `/home/user/firmware`

应用会执行：

```text
ssh -o BatchMode=yes -o ConnectTimeout=10 <host> "cd '<远程目录>' && <构建命令>"
```

因此需要提前配置 SSH key 免密登录。远程模式目前直接调用系统 OpenSSH，没有绑定 OpenCode 的 `ssh-mcp`；这样不依赖额外运行时，也便于在任意终端环境复用。

### 烧录

烧录命令始终在本地工程目录执行，例如：

```text
JLink.exe -CommanderScript flash.jlink
```

这样可以由 J-Link 直接访问本机 USB。留空时右侧操作栏的烧录按钮自动禁用。当前没有内置 MCP 客户端；如果后续要接 `jlink-mcp`，可以直接把 Flash 命令替换为对应的命令行封装。

## 配置与工程文件

全局设置位于：

```text
%APPDATA%\SimpleEmbedded\settings.json
```

保存最近工程、字号、自动换行、输出面板状态等全局偏好。

每个工程的构建与烧录配置位于：

```text
<工程目录>\.simple-embedded\settings.json
```

打开工程时优先读取该文件；旧版本保存在用户目录里的每工程配置会自动迁移过来。文件是普通 JSON，保存构建、烧录、C/C++ 属性以及调试配置；调试部分包含 `debug.program` 和 `debug.gdb_path`，gdb 路径可以填 exe 或它的 `bin` 目录。可以手改，也可以和源码一起提交到 Git。

构建卡片右侧的 **工作区** 菜单可生成 `<工程名>.se-workspace`（VS Code 风格的 `folders` 字段），并把 `.se-workspace` 关联到当前程序；之后双击工作区文件即可直接打开工程。也可以通过“打开工作区文件…”选择 `.se-workspace` 或 `.code-workspace`。如果移动了程序，重新点一次“生成并关联工作区文件”即可更新关联。

不会创建 `.eide`、`.mini-ide` 等自定义工程格式，也不会修改 Keil `.uvprojx`。

## 示例

`examples/blink` 是一个可编译的最小 C 工程（HAL 为 PC 上的空实现）：

```powershell
cargo run --release -- examples\blink examples\blink\src\main.c
```

在 Windows 上需要 `gcc` 和 `mingw32-make` 在 `PATH` 中。

## 目录结构

```text
src/
  main.rs        窗口、字体、图标生成
  icon_codec.rs  图标缩放与 ICO 编码（build.rs 共用）
  app.rs         应用状态、快捷键、右侧操作栏、构建输出
  app/ai_ui.rs   AI 模式三栏、聊天渲染、AI 设置弹窗与粉碎/流星转场
  ai.rs          OpenAI 兼容请求、工程上下文和 stm32-embedded-dev skill 加载
  app/ui.rs      工程卡片、文件树、Tab、编辑器、空状态、设置/确认弹窗
  build.rs       子进程、stdout/stderr、停止、日志事件
  config.rs      全局/工程设置、自动检测、SSH 命令、工作区与文件关联
  project_meta.rs Makefile / Keil 工程元数据提取
  support_pack.rs Keil/CMSIS .pack/.pdsc 扫描、导入与元数据解析
  flash.rs       烧录工具配置、OpenOCD / pyOCD 检测与命令构造
  completion.rs  补全上下文、struct/union 成员索引
  editor.rs      String 文档 + egui TextBuffer 适配 + 保存
  file_tree.rs   惰性目录树
  highlight.rs   syntect 高亮 + galley 缓存
  log.rs         ANSI SGR 解析、错误分级、日志裁剪
  serial.rs      MicroPython / ESP32 串口配置、端口扫描与 ESP32/CP210x/CH340 识别
  micropython.rs MicroPython raw REPL 上传、软复位与运行结果解析
  codetheme.rs   50 套编辑器/应用主题色板
  theme.rs       从主题派生面板、文字、强调色并安装 egui 样式
  util.rs        路径、换行、文件管理器集成
examples/blink/  可直接打开的演示工程
assets/         应用图标源图（Simple 字标 + S 标记）
build.rs        生成并嵌入 exe 图标资源
```

## 设计取舍

- **没有使用 ropey**：egui 的 `TextEdit` 需要一个可返回连续 `&str` 的 `TextBuffer`，ropey 无法直接满足；这里使用带 revision 的 `String` 缓冲，并在编辑时即时更新高亮缓存。
- **没有使用 tokio**：构建是单进程 + 双流读取，`std::process` + 线程 + `mpsc` 足够，依赖更轻，停止/退出行为也更直接。
- **eframe 0.34.3**：开发时采用当前稳定 API；相比需求里的 0.29，Panel、字体等 API 有升级。
- **不做完整 LSP / 调试前端**：保持“文件夹 + 编译 + 日志”的核心。提供轻量符号/寄存器索引和基础调试控制（主机 gdb 或 ARM GDB server）；深度代码跳转、自动补全和可视化调试仍建议使用 VS Code / J-Link 工具链。

## 已实现边界

- 文本编辑器支持 UTF-8 / GBK，文本上限 16 MB；二进制文件在中央编辑器以只读十六进制 + ASCII 标签页打开（上限 32 MB），也可右键打开独立十六进制窗口
- 不监听外部文件修改，保存前不会自动合并磁盘差异
- 构建日志按固定行高虚拟化显示，特别长的行会横向裁剪；可悬停查看或复制全部日志
- 调试提供 gdb 控制台 + 基本执行控制：主机 EXE 使用普通 gdb；ARM ELF/AXF 会复用烧录配置中的 OpenOCD / pyOCD 参数启动 GDB server，并用 arm-none-eabi-gdb 连接、加载程序。调试控制是编辑器右上角的小浮层，调试输出显示在底部输出面板；右侧提供变量 Live Watch、只读寄存器表和可写的外设寄存器/位域树；没有调用栈和独立内存编辑面板。动态监测支持硬件 watchpoint，但数量受目标限制，GDB 可能退化为软件 watchpoint。芯片支持包只解析元数据和 SVD，不会自动导入 Keil 设备树、启动文件、链接脚本或烧录算法
- 工程设置文件和工作区文件都是普通 JSON；工作区只使用第一个文件夹，不实现 VS Code 的多根工作区、任务和扩展配置。双击关联写入 `HKCU\Software\Classes`，移动程序后需重新关联
- 跳转/引用/补全基于轻量符号索引，不是 LSP；复杂 C++ 声明可能漏检。成员补全只解析常见的 struct/union 和变量声明，不处理宏展开、模板、继承或函数返回类型
- 悬停调用/引用树是文本级预览，不是语义分析：同名 `static` 函数会合并，函数指针调用、宏展开、重载、继承和条件编译无法准确解析；每个符号最多索引 24 处引用，悬停只展示前 3 个文件各 2 处。注释和字符串不参与，生成目录（`build` / `Debug` / `Release` / `out` / `target` 等）会被跳过
- 系统安装是用户级 HKCU 集成，不请求管理员权限，也不写入 Program Files；卸载保留 `%APPDATA%\SimpleEmbedded` 用户设置和最近工程。安装目录中的自带工具链较大，首次安装/更新可能需要等待后台复制或硬链接完成；Windows 11 的精简右键菜单可能需要点“显示更多选项”
- 工程资源报告是静态启发式分析：SVD 外设是芯片能力清单，不代表全部已使用；引脚和 CubeMX 组件来自 `.ioc`，没有 `.ioc` 时为空；组件识别依赖文件路径关键字，外部器件和实际连线无法自动发现；源码清单跳过 `tools` / `dist` 等目录，使用已保存文件和当前索引，同名报告会覆盖，不能替代原理图、BOM 或链接映射文件
- 多光标支持 Alt+点击、Ctrl+Alt+↑/↓、Ctrl+D 和统一编辑；方向键只移动主光标并清除次级光标，不实现 VS Code 的多光标同时移动、Ctrl+U 回退光标和跨文件多光标。Ctrl+Alt+左键仍提供单一矩形列选择。代码折叠只识别 C 风格大括号块，折叠时隐藏块内行和结束行，可见行仍可编辑，跨越换行的编辑会自动展开；折叠状态不持久化。分屏两个窗格都可编辑，但同一个文件不能在两个窗格重复打开
- 远程构建不支持交互式密码；请使用 SSH key
- 当前没有接入 `ssh-mcp` / `jlink-mcp`，远程构建走 OpenSSH；烧录走本地自定义命令、内置 OpenOCD 或内置 pyOCD
- OpenOCD / pyOCD 直接以参数启动，不经过 shell；烧录使用 `program` / `flash`，调试时启动 `gdbserver` 并由 arm-none-eabi-gdb 连接。内置烧录支持 `.hex` / `.elf` / `.axf`，也支持带起始地址的 `.bin`（地址来自内存布局）
- 并行构建只对 Make / Cargo / CMake 自动注入环境变量；自定义脚本和直接调用 Ninja 需要自己添加并行参数
- 工具链下拉只影响工程元数据、内存布局和构建选项的展示，不会自动改写 Build/Clean/Rebuild 命令；选择 Keil AC5/AC6 后请把构建命令配置为 UV4/UV5 或自定义包装脚本
- MicroPython 工具链已接入串口连接配置、当前文件上传和软复位（raw REPL + Ctrl-D）、左侧“芯片文件”列表/刷新/读取到工程，以及通过 esptool 烧录 MicroPython `.bin` 固件；运行/上传/固件/复位在右侧操作栏。固件烧录需要系统安装 esptool（支持 4.x / 5.x），并正确选择芯片和烧录地址（ESP32/ESP32-S2 默认 0x1000，S3/C3/C6/H2 默认 0x0）；勾选整片擦除会同时清除板上文件系统。尚未实现交互式 REPL、目录上传或自动下载 MicroPython 固件。读取芯片文件会暂停板上的程序；上传和读取上限均为 128 KB，超时、串口被占用或开发板未运行 MicroPython 时会在 MicroPython 输出中显示错误
- OpenOCD 接口自动检测基于 USB VID/PID 启发式；同时连接多个探针时取第一个，建议用接口下拉确认实际使用的烧录器

## 开发

```powershell
cargo test
cargo clippy --all-targets -- -D warnings
cargo run
```

测试覆盖自动检测、混合 CubeMX 工程的 Make/Keil 元数据选择、AC6/armclang 识别、SSH 命令构造、CRLF 保存、文件树展开、ANSI 解析、后台命令执行、芯片支持包解析、OpenOCD 接口/USB 探针检测、OpenOCD / pyOCD 命令构造、并行构建环境注入、工程配置往返、工作区文件解析、宏重复/可选同值提示、gdb watchpoint 行解析和悬停类型提取等。

面向后续开发者和 AI 代理的工程约定、架构与验证清单见 [AGENTS.md](AGENTS.md)。
