# Simple embedded

一个 Windows 桌面端的极简嵌入式编辑器：**文件夹就是工程**，导入目录 → 编辑代码 → 一键构建 → 查看彩色日志。没有强制工程向导，也不修改 Keil `.uvprojx`；工程构建与烧录配置保存在工程内 `.simple-embedded/settings.json`。

使用 Rust + `eframe/egui` 编写。界面左侧构建卡片显示工程名、路径和构建命令，下面是文件树；中间是多标签代码编辑，底部是异步构建输出，操作集中在右侧栏。

![Simple embedded](docs/editor.png)

未打开工程时，左上角仍显示同一个构建卡片，只是名称/路径显示为未导入状态，操作按钮为“导入 / 配置 / 工作区”。

![导入工程](docs/welcome.png)

从打开目录到构建、SSH、烧录和常见问题的完整中文教程：[docs/TUTORIAL.md](docs/TUTORIAL.md)。烧录工具（ST-Link、CubeProgrammer、OpenOCD、pyOCD、J-Link、Keil）的详细配置：[docs/FLASH.md](docs/FLASH.md)。

## 功能

- 打开任意文件夹作为工程；构建系统仍以 `Makefile` / `Cargo.toml` / `CMakeLists.txt` 为准，工程配置只存在于 `.simple-embedded/settings.json`
- 深色标题栏与界面配色一致，构建 / 清理 / 重构建 / 烧录集中在右侧操作栏；设置从构建卡片的“配置”或 `Ctrl+,` 打开
- 左侧栏全高，资源管理器与工程信息默认各占一半高度；文件树较短时工程信息自动上移紧接文件列表；文件树较长时可拖动中间分割线调整比例；拖动左栏右边缘可调整宽度；分割比例和折叠状态会保存
- 左侧构建卡片显示工程名（加粗）、完整路径、构建模式和命令；未导入时按钮为“导入 / 配置 / 工作区”，导入后为“更换 / 配置 / 工作区”
- 资源管理器、工程信息、构建配置等可折叠区域带缓动动画和旋转箭头；状态提示 Toast 使用更大字号，便于阅读
- 工程信息树：分组和列表标题左侧都有折叠箭头，整行可点击并显示淡蓝色选中背景；**工具链** 在这里只读显示，修改入口是 **设置 → 构建 → 工具链**（自动检测 / Make GCC / Clang / AC5 / AC6 / Keil MDK AC5 / AC6 / Cargo / CMake）。自动检测先看工程根目录的 Cargo.toml / Makefile / CMakeLists.txt / `.uvprojx`，再查 PATH 和常见安装目录。CubeMX 同时生成 MDK-ARM 和 Makefile 时，Make 构建读取 Makefile 元数据，Keil 工具链读取 `.uvprojx`，不再无条件显示 Keil；支持识别 armclang（AC6）和 armcc（AC5）。列表右侧 `+` 可添加；仅有效构建系统为 Make 时同步到 Makefile（自动备份），Keil `.uvprojx` 只读
- 芯片支持包：在工程信息中点击 `+` 添加 Keil/CMSIS `.pack` / `.pdsc`，文件放在工程根目录；显示包名、版本、厂商、设备和 SVD 数量，寄存器浏览器可直接使用包内 SVD
- 构建选项：包含全局选项、C/C++ 编译器、汇编器和链接器四个页面，并按当前工具链切换内容。GCC/Clang 显示 newlib-nano `printf` / `scanf` 浮点支持（`-u _printf_float` / `-u _scanf_float`）、LTO 和函数分段；Keil AC5/AC6 显示 Use MicroLIB、Split Load/Store 和 Keil 链接器字段。Make + GCC/Clang 工程打开时读取现有 `OPT` / `CFLAGS`，保存时按差异写回；选择 `AXF` 会生成 `.axf` 副本规则；Keil 和 Make AC5/AC6 只写入工程配置，不修改 `.uvprojx`
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
- 外设寄存器浏览器：工程内 SVD、芯片支持包内 SVD / CMSIS 头文件宏回退
- Tab / Shift+Tab 缩进、选中行整块缩进、回车自动缩进，可配置空格或 Tab 与缩进宽度
- 右侧代码预览小地图，可点击或拖动跳转；缩进参考线会高亮当前缩进层级，二者均可在设置中关闭
- 调试：ARM 目标自动复用烧录配置的 OpenOCD / pyOCD 参数启动 GDB server，再用 arm-none-eabi-gdb 连接并加载 ELF/AXF；主机 EXE 使用普通 gdb。启动后先停在 `main` 并自动定位入口文件；按钮显示运行状态，运行中主按钮变为暂停图案；点击行号左侧或 F9 设置断点，F5 继续/暂停、F10/F11 单步、Shift+F5 停止；暂停时自动打开并高亮当前执行行，浮层准心按钮可随时回到当前执行行；右侧提供变量 Live Watch、CPU 寄存器和按外设分组的外设寄存器实时值（需 SVD），可输入任意 gdb 命令
- 内存检测：工程打开后即可点击右侧 **检测**。未启动调试时打开 **静态检测**，扫描工程内数组常量下标越界、`strcpy`/`memcpy` 等常量长度、`gets`/`sprintf`、重复 `free`、可疑 `malloc` 和返回数组名；调试会话存在时打开 **动态监测**，可添加 `watch` / `rwatch` / `awatch` 硬件 watchpoint，命中后目标暂停并记录新旧值（数量受目标硬件限制，GDB 可能退化为软件 watchpoint）。结果显示在窗口内并支持点击跳转。静态扫描不替代 ASAN/Valgrind
- 轻量导航/补全：F12 跳定义、Shift+F12 查引用；输入 2 个字符后自动在光标旁弹出补全，输入 `.` / `->` 时补全结构体成员；↑↓ 选择，Enter/Tab 插入，Esc 关闭，Ctrl+Space 手动打开
- 编辑器内悬停函数名、变量名、结构体或宏时，显示声明签名、最近的注释以及返回值 / 类型；函数跨文件时会查轻量符号索引。提示跟随鼠标显示，可在设置 → 其他 关闭
- 宏诊断：同一文件内同名 `#define` 重复显示红色波浪线；设置 → 其他 的“宏重复值提示（黄色波浪线）”开启后，`PWM1PIN` / `PWM2PIN` / `PWM3PIN` 这类宏族使用相同替换值时显示黄色波浪线（C 语言本身允许这种写法，默认关闭；供应商头文件不参与）
- 多行粘贴自动对齐当前缩进；`Ctrl+Alt+左键拖动` 矩形列选择，可复制、删除或直接输入替换
- 标签页右键菜单：关闭其他/右侧/已保存/全部；保存、另存为；复制完整路径、相对路径或面包屑路径；在文件资源管理器 / 资源管理器视图中显示；向右拆分、拆分和移动 → 向上拆分、取消拆分。两个拆分窗格都可编辑
- 标签页下显示工程相对路径面包屑；最底部状态栏显示行列、缩进、编码、行尾、语言和保存/换行/格式化等操作
- `Ctrl+F` 在编辑器右上角弹出约 420px 的紧凑查找浮层（Aa / ab / 正则 / 箭头上下一个 / 替换 / AB 保留大小写），`Ctrl+G` 打开同宽跳行浮层
- Build / Clean / Rebuild / Flash 在后台线程执行，UI 不冻结；调试服务与 gdb 在后台启动，日志实时进入调试输出
- 实时抓取 stdout/stderr，解析 ANSI 颜色，按 `error` / `warning` 自动标红/标黄；错误行可点击跳转，行号栏显示红/黄诊断条，日志可过滤错误/警告
- 构建前自动保存所有脏文件；可停止构建进程树
- 构建输出面板顶部可拖动调高度，收起后也能向上拉出；高度会保存
- 本地构建或远程 SSH 构建（复用系统 OpenSSH）
- 并行构建：设置 → 构建 → 并行任务数；Make 注入 `MAKEFLAGS=-jN`，Cargo 注入 `CARGO_BUILD_JOBS`，CMake 注入 `CMAKE_BUILD_PARALLEL_LEVEL`，命令里已有 `-j` 时不重复添加
- 烧录工具可选 **自定义命令** / **OpenOCD** / **pyOCD**：支持工具路径下拉/浏览、`${ExecutableName}.hex` 等程序文件变量、OpenOCD 芯片配置和接口配置。接口配置是预设下拉（ST-Link、CMSIS-DAP / DAPLink、J-Link、XDS110、ULINK 等）+ 自定义输入 + **自动检测**；默认不再自动填 `stlink.cfg`，自动检测通过 USB VID 识别已连接探针。pyOCD 目标芯片与可选 Pack、命令预览；Flash 始终在本地工程目录执行。工程信息 → 烧录配置 → **内存布局** 可设置 Flash/RAM 起始地址、大小和 .bin 下载起始地址
- 构建输出标题栏显示 Flash / RAM 占用和芯片容量百分比；容量优先读取链接脚本，其次读取匹配的芯片支持包 `.pdsc`，没有 `size` 输出时直接解析 ELF 段大小
- 每工程独立的构建、烧录、C/C++ 和调试配置保存在工程内 `.simple-embedded/settings.json`（包括 `debug.gdb_path`）；用户目录作为旧配置迁移和全局偏好存储
- 工作区文件：构建卡片“工作区”可生成 `<工程名>.se-workspace` 并关联双击打开；也可从“工作区 → 打开工作区文件…”选择 `.se-workspace` / `.code-workspace`
- 设置分为 构建 / 构建工具链 / 编辑器 / 调试 / 其他：构建页可设置命令和并行任务数；构建工具链可浏览编译器和 make，或一键把当前工具复制到软件自带目录，之后不依赖系统 PATH
- 可追加工具链 PATH；本地 Make 构建会自动接入 Git for Windows / MSYS 的 `rm` 等 Unix 工具

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
| `Ctrl+,` | 设置 |
| `Ctrl+=` / `Ctrl+-` | 放大 / 缩小代码字号 |

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

默认优先使用 `tools/`；没有 `tools/` 时回退到用户目录工具链。设置 → **构建工具链** 可以在“系统已安装 / 软件自带”之间切换，也可以浏览编译器和 make、从 PATH 自动检测，或把当前工具一键复制到软件目录。

本地打包当前工具链和 release exe：

```powershell
cargo build --release
powershell -ExecutionPolicy Bypass -File scripts\package-tools.ps1
```

生成 `dist\simple-embedded\`（含 exe 和约 1.1 GB 的 ARM GNU 工具链 + make）。仓库只追踪 `dist/` 下的所有 `.exe`（应用 exe 和 `tools` 里的工具 exe）；编译器 DLL、库、头文件、`tools.zip` 以及 `.compiler.tmp` / `.make.tmp` 仍被忽略，仅 exe 不足以构成完整可运行工具链。

构建前会自动保存所有未保存文件，然后通过本机 shell（Windows 为 `cmd /S /C`）在工程根目录启动进程。日志从 stdout/stderr 双流读取；Windows 下停止会调用 `taskkill /T` 终止子进程树。

POSIX 风格的 Makefile 常用 `rm`、`cp` 等命令；在 Windows 上运行本地 Make 构建时，程序会自动寻找 Git for Windows / MSYS / Cygwin 的 `usr\bin` 并前置到 PATH。也可以在设置中手动填写额外工具链路径。

### 远程 SSH 构建

设置 → 构建位置选择 **远程 SSH**，填写：

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
  main.rs        窗口、字体、图标
  app.rs         应用状态、快捷键、右侧操作栏、构建输出
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
  theme.rs       配色
  util.rs        路径、换行、文件管理器集成
examples/blink/  可直接打开的演示工程
```

## 设计取舍

- **没有使用 ropey**：egui 的 `TextEdit` 需要一个可返回连续 `&str` 的 `TextBuffer`，ropey 无法直接满足；这里使用带 revision 的 `String` 缓冲，并在编辑时即时更新高亮缓存。
- **没有使用 tokio**：构建是单进程 + 双流读取，`std::process` + 线程 + `mpsc` 足够，依赖更轻，停止/退出行为也更直接。
- **eframe 0.34.3**：开发时采用当前稳定 API；相比需求里的 0.29，Panel、字体等 API 有升级。
- **不做完整 LSP / 调试前端**：保持“文件夹 + 编译 + 日志”的核心。提供轻量符号/寄存器索引和基础调试控制（主机 gdb 或 ARM GDB server）；深度代码跳转、自动补全和可视化调试仍建议使用 VS Code / J-Link 工具链。

## 已实现边界

- 文本编辑器支持 UTF-8 / GBK，上限 16 MB；二进制文件用十六进制查看器打开
- 不监听外部文件修改，保存前不会自动合并磁盘差异
- 构建日志按固定行高虚拟化显示，特别长的行会横向裁剪；可悬停查看或复制全部日志
- 调试提供 gdb 控制台 + 基本执行控制：主机 EXE 使用普通 gdb；ARM ELF/AXF 会复用烧录配置中的 OpenOCD / pyOCD 参数启动 GDB server，并用 arm-none-eabi-gdb 连接、加载程序。调试控制是编辑器右上角的小浮层，调试输出显示在底部输出面板；右侧提供变量 Live Watch 和只读寄存器表，但没有调用栈、内存或外设寄存器编辑面板。动态监测支持硬件 watchpoint，但数量受目标限制，GDB 可能退化为软件 watchpoint。芯片支持包只解析元数据和 SVD，不会自动导入 Keil 设备树、启动文件、链接脚本或烧录算法
- 工程设置文件和工作区文件都是普通 JSON；工作区只使用第一个文件夹，不实现 VS Code 的多根工作区、任务和扩展配置。双击关联写入 `HKCU\Software\Classes`，移动程序后需重新关联
- 跳转/引用/补全基于轻量符号索引，不是 LSP；复杂 C++ 声明可能漏检。成员补全只解析常见的 struct/union 和变量声明，不处理宏展开、模板、继承或函数返回类型
- 不支持多光标；Ctrl+Alt+左键只提供单一矩形列选择。分屏两个窗格都可编辑，但同一个文件不能在两个窗格重复打开
- 远程构建不支持交互式密码；请使用 SSH key
- 当前没有接入 `ssh-mcp` / `jlink-mcp`，远程构建走 OpenSSH；烧录走本地自定义命令、内置 OpenOCD 或内置 pyOCD
- OpenOCD / pyOCD 直接以参数启动，不经过 shell；烧录使用 `program` / `flash`，调试时启动 `gdbserver` 并由 arm-none-eabi-gdb 连接。内置烧录支持 `.hex` / `.elf` / `.axf`，也支持带起始地址的 `.bin`（地址来自内存布局）
- 并行构建只对 Make / Cargo / CMake 自动注入环境变量；自定义脚本和直接调用 Ninja 需要自己添加并行参数
- 工具链下拉只影响工程元数据、内存布局和构建选项的展示，不会自动改写 Build/Clean/Rebuild 命令；选择 Keil AC5/AC6 后请把构建命令配置为 UV4/UV5 或自定义包装脚本
- OpenOCD 接口自动检测基于 USB VID/PID 启发式；同时连接多个探针时取第一个，建议用接口下拉确认实际使用的烧录器

## 开发

```powershell
cargo test
cargo clippy --all-targets -- -D warnings
cargo run
```

测试覆盖自动检测、混合 CubeMX 工程的 Make/Keil 元数据选择、AC6/armclang 识别、SSH 命令构造、CRLF 保存、文件树展开、ANSI 解析、后台命令执行、芯片支持包解析、OpenOCD 接口/USB 探针检测、OpenOCD / pyOCD 命令构造、并行构建环境注入、工程配置往返、工作区文件解析、宏重复/可选同值提示、gdb watchpoint 行解析和悬停类型提取等。

面向后续开发者和 AI 代理的工程约定、架构与验证清单见 [AGENTS.md](AGENTS.md)。
