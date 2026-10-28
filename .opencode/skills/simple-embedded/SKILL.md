---
name: Simple embedded 使用指南
description: |
  Simple embedded 是一款 Windows 桌面嵌入式 IDE，核心模型是“工程文件夹就是工程”。
  指导用户打开/导入工程，配置构建工具链和 SSH，执行 Build / Clean / Rebuild / Flash / Debug，
  使用 Makefile、Keil MDK、Cargo、CMake、MicroPython，浏览 SVD 外设和寄存器，配置内存布局和链接脚本，
  使用 clangd、悬停调用/引用树、芯片支持包、工程资源报告，以及把软件安装到 Windows 并添加文件夹右键菜单。
  当用户询问 Simple embedded 的界面操作、按钮、快捷键、工程配置、构建、烧录、调试、SVD/寄存器、
  MicroPython、资源报告、安装/卸载或常见报错时使用。
---

# Simple embedded 使用指南

## 核心模型

- **工程文件夹就是工程**。不要创建 `.eide`、`.mini-ide` 等专有工程格式，也不要修改 Keil `.uvprojx`。
- 工程级配置在 `<工程>/.simple-embedded/settings.json`，保存 Build/Clean/Rebuild/Flash、SSH、工具链、烧录、MicroPython 串口/固件、并行构建数、C/C++ 属性、构建选项、内存布局和调试程序。
- 全局偏好和最近工程在 `%APPDATA%\SimpleEmbedded\settings.json`。
- 工作区文件是 VS Code 风格 JSON：`<工程名>.se-workspace`，只使用第一个 folder。
- 界面默认中文、暗色主题；支持 50 套明暗主题。

## 快速开始

1. 启动程序，或在命令行直接带工程目录：
   ```powershell
   simple-embedded.exe D:\firmware\my-board
   simple-embedded.exe D:\firmware\my-board src\main.c
   simple-embedded.exe D:\firmware\my-board\my-board.se-workspace
   ```
2. 也可以点左上角 **导入**、按 `Ctrl+O`，或把文件夹拖进窗口。
3. 打开 **配置**（`Ctrl+,`）→ **构建**，确认工具链和 Build / Clean / Rebuild / Flash 命令。
4. 构建前会自动保存所有脏 Tab；构建输出在底部输出面板。
5. 右侧操作栏：Build、Clean/重构、Flash、调试、检测，最底部是 **设置**。

## 常用快捷键

| 操作 | 快捷键 |
|---|---|
| 打开工程 / 保存 / 另存为 | `Ctrl+O` / `Ctrl+S` / `Ctrl+Shift+S` |
| 构建 / 重新构建 | `Ctrl+B` / `Ctrl+Shift+B` |
| 设置 | `Ctrl+,` |
| 查找 / 替换 / 下一个 | `Ctrl+F` / `Ctrl+H` / `F3` |
| 跳行 | `Ctrl+G` |
| 全工程搜索 | `Ctrl+Shift+F` |
| 符号列表 | `Ctrl+Shift+O` |
| 寄存器浏览器 | `Ctrl+Shift+R` |
| TODO 列表 | `Ctrl+Shift+T` |
| Git 状态 | `Ctrl+Shift+G` |
| 十六进制查看 / C 数组生成 | `Ctrl+Shift+H` / `Ctrl+Shift+A` |
| 跳转定义 / 查找引用 | `F12` / `Shift+F12` |
| 重命名符号（clangd） | `Ctrl+Shift+F2` |
| 开始/继续调试 / 停止调试 | `F5` / `Shift+F5` |
| 下一步 / 单步进入 / 跳出 | `F10` / `F11` / `Shift+F11` |
| 切换断点 | `F9` |
| 切换书签 / 上一个 / 下一个 | `Ctrl+F2` / `Shift+F2` / `F2` |
| 折叠当前块 / 展开全部 | `Ctrl+Shift+[` / `Ctrl+Shift+]` |
| 向右拆分 | `Ctrl+\` |
| 关闭当前标签 | `Ctrl+F4` |
| 沉浸式编码 | `Ctrl+Shift+Enter` |
| 多光标 / 列选择 | `Ctrl+D`、`Alt+点击`、`Ctrl+Alt+↑/↓`、`Ctrl+Alt+左键拖动` |

## 常见工作流

### 构建 Makefile 工程

1. 确认工程根目录有 `Makefile`，配置里的工具链选 **Make · GCC**（或 Clang/AC5/AC6）。
2. 点 **自动检测** 让程序先看根目录构建文件，再查 PATH 和常见安装目录。
3. Build/Clean/Rebuild 命令通常为：
   ```text
   make -j
   make clean
   make clean && make -j
   ```
4. 没有 make 时程序会尝试 `gmake`、`mingw32-make`；本地 Make 构建会自动探测 Git for Windows / MSYS / Cygwin 的 `usr\bin`，保证 `rm` 等 POSIX 命令可用。
5. 只使用 Make + GCC/Clang 时，构建选项、C/C++ 属性和 `C_SOURCES` 的修改才会按差异同步到 Makefile；修改前会备份。

### 构建 Keil 工程

- 工具链选择 **Keil MDK · AC5/AC6** 后，需要把构建命令配置成 UV4/UV5 或自定义包装脚本。
- `.uvprojx` 不会被修改；Keil 工程用于读取芯片、CPU、FPU、包含目录、宏、输出目录和调试产物。
- 工程信息里的工具链只读显示，修改入口在 **配置 → 构建**。

### 烧录

- **工程信息 → 烧录配置 → 编辑**。
- 工具可选：
  - **自定义命令**：在 Flash 命令中填写自己的烧录命令。
  - **OpenOCD**：填写 OpenOCD 路径、程序文件、芯片配置和接口配置；接口可通过 **自动检测** 按 USB VID 识别 ST-Link / CMSIS-DAP / DAPLink / J-Link / XDS110 / ULINK。
  - **pyOCD**：选择目标芯片和可选 Pack，填写程序文件。
- 程序文件支持 `${ExecutableName}`、`${ProjectName}`、`${workspaceFolder}`、`${workspaceRoot}`。
- 找不到程序文件时 Flash 会直接报错，不会自动构建。
- `.bin` 使用 **工程信息 → 烧录配置 → 内存布局** 里的下载起始地址；`.hex` / `.elf` / `.axf` 使用文件内地址。

### 调试和外设寄存器

1. 确认 **配置 → 调试** 中的调试程序和 gdb 路径。
   - ARM ELF/AXF 会自动复用烧录配置的 OpenOCD / pyOCD 参数启动 GDB server，再用 `arm-none-eabi-gdb` 连接并 `load`。
   - PE `.exe` 使用普通 gdb。
2. `F5` 启动/继续；首次启动会先停在 `main` 并打开入口文件。
3. `F10` 下一步、`F11` 单步、`Shift+F11` 跳出、`Shift+F5` 停止；`F9` 或点击行号左侧切换断点。
4. 调试控制浮层：继续/暂停、定位当前行、停止、单步；`F5` 在运行中可请求暂停。
5. 输出面板 **调试输出** 页可以输入任意 gdb 命令。
6. 右侧调试侧栏：
   - **变量**：Live Watch。
   - **寄存器**：CPU 寄存器。
   - **外设**：SVD 外设 → 寄存器 → 位域树；目标暂停时可点 **改** 写入寄存器/位域，位域使用读-改-写。
   - 没有 SVD 时可把 `.svd` 放到工程根目录，或使用 **导入 SVD / 重新扫描**。
7. 构建输出右侧的 **内存可视化** 显示 `.data` / `.bss` 全局变量、栈区、FreeRTOS 堆和 SP 估算。

### MicroPython / ESP32

1. 工具链选 **MicroPython**。
2. 在 **配置 → 构建** 的串口面板选择端口和波特率（默认 115200），可点 **自动检测**。
3. 右侧操作栏变为 **运行 / 上传 / 固件 / 复位**：
   - **运行**：把当前 `.py` 上传为 `main.py` 并 Ctrl-D 软复位。
   - **上传**：把当前文件写回芯片原路径。
   - **固件**：通过 esptool 烧录 MicroPython `.bin`（需系统安装 esptool）。
   - **复位**：只发送 Ctrl-D。
4. 左栏 **芯片文件** 可读取开发板根目录和两层子目录，点击文件下载到工程并打开；刷新会暂停板上程序。
5. 固件烧录：ESP32/ESP32-S2 默认地址 `0x1000`，S3/C3/C6/H2 默认 `0x0`；整片擦除会清除板上文件系统。esptool 4.x/5.x 命令差异由程序处理。
6. 上传/读取上限 128 KB；不支持交互式 REPL、目录上传和自动下载固件。

### 工程资源报告

- 入口：**工程信息 → 资源报告 → 生成资源报告**，或 **工作区菜单 → 生成资源报告...**。
- 后台分析，保存到工程根目录：
  ```text
  <工程名>.resources.json
  ```
- 包含 `application`、`project`、`hardware`、`software`、`system`、`warnings`、`notes`：
  - 硬件：芯片、CPU、FPU、Flash/RAM、使用率、固件 ELF 栈堆/全局变量、SVD 外设/寄存器、`.ioc` 引脚/组件/时钟。
  - 软件：源码/头文件清单、行数、链接脚本、启动文件、库、FreeRTOS/HAL/CMSIS 等组件、符号和调用图统计、支持包。
  - 系统：OS、架构、app 版本、报告文件、扫描统计。
- 报告使用已保存文件和当前索引，未保存编辑不会进入清单；跳过 `build` / `Debug` / `Release` / `out` / `target` / `tools` / `dist` 等目录。
- 报告是静态启发式分析，不能替代原理图、BOM 或链接映射文件；缺少 SVD/`.ioc`/固件时看 `warnings`。

### 安装到 Windows / 文件夹右键菜单

- **配置 → 其他 → 系统集成 → 安装到系统**。
- 默认安装到：
  ```text
  %LOCALAPPDATA%\Programs\SimpleEmbedded
  ```
- 会创建开始菜单快捷方式、Windows 应用列表卸载项、文件夹右键 **用 Simple embedded 打开**、文件夹空白处右键，并复制/硬链接自带 `tools/`。
- 也可以直接运行：
  ```powershell
  simple-embedded.exe --install-system
  simple-embedded.exe --uninstall-system
  ```
- 卸载：Windows 设置 → 应用 → Simple embedded，或运行 `--uninstall-system`；用户设置和最近工程保留。
- Windows 11 的右键菜单可能在 **显示更多选项** 中。
- 安装是 HKCU 用户级，不需要管理员权限，不写入 `Program Files`。

## 配置文件与索引

- 工程配置：`<工程>/.simple-embedded/settings.json`
- 全局设置：`%APPDATA%\SimpleEmbedded\settings.json`
- clangd 编译数据库：`%APPDATA%\SimpleEmbedded\clangd\<hash>\compile_commands.json`
- Makefile 备份：`%APPDATA%\SimpleEmbedded\backups\<工程名>-<hash>\`
- 工作区：`<工程名>.se-workspace`（只使用第一个 folder）
- 不要用专有格式替代以上 JSON。

## 详细参考

- `references/project-build-flash.md`：工程、工具链、构建、SSH、烧录、内存布局。
- `references/debug-peripherals.md`：调试、SVD 外设/寄存器、内存可视化和动态监测。
- `references/micropython.md`：MicroPython 串口、芯片文件、运行/上传/固件烧录。
- `references/integration-report.md`：系统安装、右键菜单和资源报告字段/限制。
- `references/troubleshooting.md`：make/rm、undefined reference、SSH、gdb、SVD、MicroPython 和报告问题。

## 输出建议

当用户询问怎么操作时：

1. 先判断是 **打开工程、构建、烧录、调试、MicroPython、资源报告、安装** 中的哪一类。
2. 给出界面路径和必要的快捷键，再给配置示例。
3. 涉及配置修改时先说明会写入 `.simple-embedded/settings.json` 还是用户目录；不要建议修改 `.uvprojx`。
4. 涉及硬件/芯片/引脚时，提醒 SVD 和 `.ioc` 是静态信息，实际连接以原理图为准。
5. 遇到报错时按 `references/troubleshooting.md` 的顺序排查，不要伪造成功结果。
