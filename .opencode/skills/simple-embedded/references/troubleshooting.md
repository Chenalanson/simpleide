# 常见问题排查

## 构建

### 提示找不到 make / gmake / mingw32-make

- 安装 MinGW-w64、MSYS2 或 Git for Windows，并把 `bin` 加入 PATH。
- 或在 **配置 → 构建 → 工具链路径** 填入 make 和编译器所在目录。
- 软件自带 `tools/` 存在时会优先使用。

### 构建时找不到 `rm` / `cp` / `CreateProcess failed`

本地 Make 构建需要 POSIX 工具。程序会自动探测：

- Git for Windows `usr\bin`
- MSYS2
- Cygwin

仍失败时，把对应 `usr\bin` 加入配置的 **工具链路径**，或安装完整 MSYS2/MinGW。

### `undefined reference to ...`

这是链接错误，通常不是编译器语法错误：

1. 检查缺失函数对应的 `.c` 是否加入 Makefile 的 `C_SOURCES`。
2. 使用 **工程信息 → C/C++ 属性 → 同步源文件到 Makefile**。
3. 检查函数是否实现、是否误加 `static`、是否缺少库。
4. 输出面板会提取缺失符号并给出提示。

### 构建命令与工具链不一致

工程信息的工具链只读显示。到 **配置 → 构建 → 工具链** 修改；程序不会自动改写 Build/Clean/Rebuild 命令，需要同时确认命令与实际工具一致。

### 更新 Makefile 后仍使用旧源文件

- **同步源文件到 Makefile** 只处理 Make + GCC/Clang 工程。
- Makefile 有未保存修改时会跳过并提示。
- 删除源文件后会自动从 `C_SOURCES` 移除；若失败，检查 Makefile 是否被占用或有未保存修改。

## SSH 远程构建

- 确保本机安装 OpenSSH，并且已配置 SSH key。
- 先手动测试：
  ```powershell
  ssh user@host "cd '/remote/dir' && make"
  ```
- 不支持交互式密码；`BatchMode=yes` 时密码提示会直接失败。
- 远端目录必须是实际工程目录。
- 停止远程构建只结束本地 ssh 进程，远端 make 是否退出取决于远端 shell。

## 调试

### `error 193` / `not a valid Win32 application`

通常是用了 MinGW gdb 调 ARM 目标。到配置 → 调试，把 gdb 指向 `arm-none-eabi-gdb.exe`，或点 **自动检测**。

### gdb 找不到 / 无法启动

- 安装 GNU Arm Toolchain 或填写 gdb 路径。
- gdb 路径可以填 exe，也可以填 `bin` 目录。
- 确认调试程序 ELF/AXF 是最新构建产物。

### OpenOCD / pyOCD 启动失败

- 检查烧录配置里的工具路径、芯片配置、接口配置。
- OpenOCD 接口必须与探针匹配（ST-Link / CMSIS-DAP / J-Link 等）。
- pyOCD 需要正确的 target 和可选 Pack。
- 同时连接多个探针时自动检测按优先级取第一个，可能需要在接口下拉手动改。

### 外设寄存器显示“无法读取”

- OpenOCD 对外设未映射地址可能报 `Failed to read memory`；程序会过滤这些行。
- 确认目标已暂停、时钟已开启、地址来自正确 SVD。
- 每次最多读取 160 个寄存器；未读取的显示 `—`。
- 64 位寄存器只读取低 32 位。

### 写寄存器失败

- 目标必须暂停。
- 只读位/外设可能忽略写入，以硬件手册为准。
- 位域写入是读-改-写；如果当前寄存器值未读取成功，会拒绝写入。

## MicroPython

### 串口打不开 / 被占用

- 关闭其他串口工具、烧录工具和 REPL。
- 确认端口和波特率正确。
- 点 **自动检测** 重新扫描。

### 开发板没有运行 MicroPython

- 先烧录 MicroPython 固件。
- 复位后等待启动完成。
- 上传/读取时板子会暂停当前程序。

### esptool 缺失或地址错误

- 安装 esptool 并在固件面板填写路径。
- ESP32 / ESP32-S2 默认 `0x1000`；S3/C3/C6/H2 默认 `0x0`。
- 确认使用的是合并后的固件 `.bin`。
- 整片擦除会清除板上文件系统。

### 上传后程序没有运行

- **运行** 会把当前文件写成 `main.py` 并 Ctrl-D 软复位。
- **上传** 只是写文件，不一定复位。
- 需要运行 `boot.py` 或模块时，使用 **复位** 后再触发。

## SVD / 芯片支持包

- 没有 SVD 时，把 `.svd` 放到工程根目录，或导入 Keil/CMSIS `.pack` / `.pdsc`。
- 支持包只读取元数据、SVD 和内存区域；不导入 Keil 设备树、启动文件、链接脚本或烧录算法。
- 寄存器浏览器和调试外设页共用索引；切换工程后重新扫描。
- 大型 SVD 首次解析需要一点时间，之后复用当前索引。

## 资源报告

- 没有 `.ioc`：引脚为空，warnings 会说明；这不影响 C 工程构建。
- 没有 SVD：硬件外设/寄存器为空。
- 没有固件产物：ELF 栈堆和全局变量为空；先构建或配置调试程序。
- 调用图未就绪：稍后重新生成，或先打开悬停/符号索引。
- 文件列表截断：项目文件超过 2 万或单类列表超过 500；报告仍是部分清单。
- 报告是静态分析，实际引脚、外部器件和连线以原理图和 BOM 为准。

## 界面/文件

- 工程内没有专用工程文件；找不到配置时检查 `.simple-embedded/settings.json`。
- 全局设置损坏时，程序会回退默认值并输出到 stderr；可删除 `%APPDATA%\SimpleEmbedded\settings.json` 重新生成。
- Keil `.uvprojx` 不会被修改；不要期待程序自动写 Keil 工程。
- 移动程序后 `.se-workspace` 关联会失效，需要重新生成/关联。
