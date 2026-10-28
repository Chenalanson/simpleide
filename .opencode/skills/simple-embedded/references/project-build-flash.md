# 工程、构建与烧录

## 工程模型

- 一个文件夹就是一个工程；打开文件夹即打开工程。
- 工程配置：
  ```text
  <工程>/.simple-embedded/settings.json
  ```
  保存构建、烧录、调试、MicroPython、C/C++ 属性和内存布局。
- 全局设置：
  ```text
  %APPDATA%\SimpleEmbedded\settings.json
  ```
- 工作区文件：
  ```text
  <工程名>.se-workspace
  ```
  只使用第一个 folder，不实现多根工作区、任务和扩展。

## 打开工程

- 启动时传目录：
  ```powershell
  simple-embedded.exe D:\firmware\my-board
  simple-embedded.exe D:\firmware\my-board src\main.c
  ```
- 也可以传 `.se-workspace` / `.code-workspace`。
- 界面里用 **导入**、`Ctrl+O`、拖入文件夹，或 **工作区 → 打开工作区文件…**。

## 工具链选择

配置 → 构建 → **工具链**：

- 自动检测
- Make · GCC
- Make · Clang
- Make · AC5 / AC6
- Keil MDK · AC5 / AC6
- Cargo
- CMake
- MicroPython
- 自定义

工具链只改变元数据、内存布局、构建选项展示和可用按钮，**不会自动改写用户的 Build / Clean / Rebuild 命令**。选择后要确认命令与工具链一致；不一致时界面显示黄色提示。

## 自动检测顺序

1. 先看工程根目录的 `Cargo.toml` / `Makefile` / `CMakeLists.txt` / `*.uvprojx`，确定期望的构建系统。
2. 再在 PATH 和常见安装目录查找对应工具。
3. 根目录只有 `main.py` / `boot.py` 时识别为 MicroPython。
4. 没有构建文件时回退到 PATH 中的 cargo / cmake / make。

自动检测只是初值，修改后点 **保存**。

## Make 工程

常用命令：

```text
Build:    make -j
Clean:    make clean
Rebuild:  make clean && make -j
Flash:    make flash
```

- 程序按 `make` → `gmake` → `mingw32-make` 探测。
- 本地 Make 构建会把 Git for Windows / MSYS / Cygwin 的 `usr\bin` 加入 PATH，保证 `rm` 等命令可用。
- 构建前自动保存所有脏 Tab。
- 并行任务数（0 = 不启用）会注入 `MAKEFLAGS=-jN`；命令里已有 `-j` 时不重复添加。

## Keil 工程

- 读取 `.uvprojx` 中的芯片、CPU、FPU、包含目录、宏、输出目录、Scatter 文件和调试产物。
- 选择 Keil AC5/AC6 后，构建命令需要用户配置成 UV4/UV5 或外部包装脚本；程序不会修改 `.uvprojx`。
- Keil 构建选项只写入 `.simple-embedded/settings.json`。

## Cargo / CMake

- Cargo 注入 `CARGO_BUILD_JOBS`。
- CMake 注入 `CMAKE_BUILD_PARALLEL_LEVEL`。
- 构建命令示例：
  ```text
  cargo build --release
  cmake --build build
  ```

## 包含目录、库目录、宏

- **工程信息 → C/C++ 属性** 可编辑。
- **自动检测包含目录** 只扫描 `.h/.hpp/.hh/.hxx/.inl`，追加头文件父目录，并清理只含 `.c` 的目录。
- **同步源文件到 Makefile** 扫描 `.c/.cc/.cpp/.cxx`，把未加入的源文件追加到 `C_SOURCES`，跳过 `Drivers` / `Middlewares` / `CMSIS` / `third_party` / `vendor`。
- 仅 Make 工程保存时按差异写回 `C_INCLUDES` / `LIBDIR` / `C_DEFS`；修改前备份。
- 删除 `.c` / `.cpp` 文件或目录后，会同步从 `C_SOURCES` 移除对应条目；Makefile 有未保存修改时跳过并提示。

## 构建选项

**工程信息 → 构建配置 → 构建选项**：

- 全局选项
- C/C++ 编译器
- 汇编器
- 链接器

GCC/Clang 下可配置 newlib-nano 浮点、LTO、函数分段等；Keil AC5/AC6 使用 Use MicroLIB / Split Load/Store 等字段。链接器输出选 `AXF` 时会受管理地生成 `.axf` 副本。

## SSH 远程构建

配置 → 构建：

- 构建位置选 **远程 SSH**
- 填写 host、port、远端工程目录
- 命令会构造为：
  ```text
  ssh -o BatchMode=yes -o ConnectTimeout=10 [-p port] host "cd '<remote_dir>' && <command>"
  ```

要求本机 OpenSSH 和 SSH key 免交互登录；不支持交互式密码。停止远程构建只保证结束本地 ssh 进程。

## 烧录配置

工程信息 → 烧录配置 → 编辑：

### 自定义命令

- 在 Flash 命令中写自己的命令。
- 可以引用程序路径变量：
  - `${ExecutableName}`
  - `${ProjectName}`
  - `${workspaceFolder}`
  - `${workspaceRoot}`

### OpenOCD

填写：

- OpenOCD 路径
- 程序文件
- 芯片配置（target cfg）
- 接口配置（interface cfg）

接口配置有 ST-Link / CMSIS-DAP / DAPLink / J-Link / XDS110 / ULINK 预设；**自动检测** 按 USB VID 识别已连接探针。不是所有 OpenOCD 适配器都能穷举识别，必要时手动选择。

### pyOCD

填写：

- pyOCD 路径
- 目标芯片
- 可选 Pack
- 程序文件

切换到 pyOCD 时会尝试由芯片名推断 target，并优先使用工程根目录的 `.pack`。

### 程序文件和 .bin

- 找不到程序文件时 Flash 直接报错，不会自动构建。
- `.hex` / `.elf` / `.axf` 使用文件内地址。
- `.bin` 使用 **内存布局** 中的下载起始地址。
- 内存布局保存到工程配置，不改写链接脚本。

## 其他路径设置

- **工具链路径**：配置 → 构建 → 工具链路径，对应 `BuildConfig.extra_paths`，构建时前置到 PATH。
- 软件自带 `tools/` 存在时优先使用；不完整时回退到旧路径和系统 PATH。
- 远程 SSH 不注入本地 PATH。
