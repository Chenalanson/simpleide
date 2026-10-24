# 烧录配置教程

这篇教程专门讲 Simple embedded 里的 **烧录配置（Flash）**，包括它到底执行什么、命令写在哪里、STM32 常用的几种烧录工具怎么填，以及烧录失败时先查什么。

---

## 1. 先理解：这里的“烧录”是什么

Simple embedded 不自带烧录算法，也不接管 J-Link / ST-Link 的协议。右侧操作栏的 **烧录** 按钮实际做的是：

1. 保存所有未保存的编辑器文件；
2. 在**本地工程根目录**执行你在设置里填写的 **Flash** 命令；
3. 把命令的 stdout/stderr 显示在底部构建输出面板。

它**不会**自动先构建。正确顺序永远是：

```text
先点“构建” → 确认构建成功 → 再点“烧录”
```

另外两条容易忽略的规则：

- **Flash 始终在本机执行**。即使构建目标选的是“远程 SSH”，烧录命令也不会通过 SSH 发到远端。
- 左侧“工程信息 → 烧录配置”只是**显示**：烧录命令、芯片名称、Keil 的烧录驱动、产物文件。真正修改命令的地方是 **配置**。

---

## 2. 配置入口

1. 点击左上角工程卡片里的 **配置**（或按 `Ctrl+,`）。
2. 找到 **Flash** 一行。
3. 填入完整的烧录命令，例如：

   ```text
   "C:\Program Files\STMicroelectronics\STM32Cube\STM32CubeProgrammer\bin\STM32_Programmer_CLI.exe" -c port=SWD -w build\test32.hex -v -rst
   ```

4. 点击 **保存**。
5. 右侧操作栏的 **烧录** 按钮会从灰色变为可用。

> 命令的工作目录是**工程根目录**，所以 `build\test32.hex` 这种相对路径可以直接用。
> 命令由 Windows 的 `cmd /S /C` 执行；路径有空格时要用英文双引号包起来。

---

## 3. 先找到要烧录的文件

以 CubeMX 生成的 Makefile 工程为例，构建成功后产物通常在：

```text
build\<TARGET>.elf
build\<TARGET>.hex
build\<TARGET>.bin
```

`TARGET` 来自 Makefile。当前示例工程 `test32` 的 `TARGET = test32`，所以文件是：

```text
build\test32.elf
build\test32.hex
build\test32.bin
```

选择建议：

- **优先用 `.hex` 或 `.elf`**：文件里已经包含地址信息，烧录命令不需要再写起始地址。
- **用 `.bin` 时必须写起始地址**，STM32 通常是 `0x08000000`。
- 构建输出面板会显示最终链接出来的文件名；以那里为准，不要凭感觉猜。

---

## 4. 常用烧录工具怎么填

下面命令里的 `build\test32.hex` 请按你的实际工程名替换。

### 4.1 STM32CubeProgrammer CLI（推荐，ST-Link）

安装 STM32CubeProgrammer 后，CLI 通常在：

```text
C:\Program Files\STMicroelectronics\STM32Cube\STM32CubeProgrammer\bin\STM32_Programmer_CLI.exe
```

Flash 命令：

```text
"C:\Program Files\STMicroelectronics\STM32Cube\STM32CubeProgrammer\bin\STM32_Programmer_CLI.exe" -c port=SWD -w build\test32.hex -v -rst
```

说明：

- `-c port=SWD`：使用 ST-Link 的 SWD 接口。
- `-w`：写入文件；`.hex` / `.elf` 不需要地址。
- `-v`：写入后校验。
- `-rst`：烧录完成后复位运行。

如果是 `.bin`：

```text
"...\STM32_Programmer_CLI.exe" -c port=SWD -w build\test32.bin 0x08000000 -v -rst
```

如果是 DFU / USB 下载（芯片进入 DFU 模式）：

```text
"...\STM32_Programmer_CLI.exe" -c port=USB1 -w build\test32.hex -v -rst
```

先确认电脑能识别到探针，可以单独运行：

```text
"...\STM32_Programmer_CLI.exe" -l
```

### 4.2 OpenOCD

适用于 ST-Link、J-Link、CMSIS-DAP 等。命令：

```text
openocd -f interface/stlink.cfg -f target/stm32f1x.cfg -c "program build/test32.elf verify reset exit"
```

说明：

- `interface/stlink.cfg`：ST-Link。老版本 OpenOCD 可能叫 `interface/stlink-v2.cfg`。
- `target/stm32f1x.cfg`：STM32F1 系列。STM32G4 用 `target/stm32g4x.cfg`。
- J-Link 把接口文件换成 `interface/jlink.cfg`；CMSIS-DAP 换成 `interface/cmsis-dap.cfg`。
- 最后一段 `program ... verify reset exit` 是 OpenOCD 的命令序列，必须整体放进引号。
- 如果连接不稳定，可以在 `program` 前加 `adapter speed 1000`。

### 4.3 pyOCD

安装：

```powershell
pip install pyocd
```

Flash 命令：

```text
pyocd flash -t stm32f103xb build/test32.elf
```

说明：

- `-t stm32f103xb` 是 pyOCD 的目标名，必须和芯片匹配。
- `pyocd list` 查看已连接的探针。
- `pyocd list --targets` 查看支持的目标名。
- 如果提示找不到目标包，先安装对应的 CMSIS Pack，例如：

  ```text
  pyocd pack install stm32f103xb
  ```

### 4.4 ST-LINK_CLI（旧版 ST-LINK Utility）

如果电脑上装的是旧的 STM32 ST-LINK Utility：

```text
"C:\Program Files (x86)\STMicroelectronics\STM32 ST-LINK Utility\ST-LINK Utility\ST-LINK_CLI.exe" -c SWD UR -P build\test32.hex 0x08000000 -V -Rst
```

说明：

- `-P`：烧录文件。
- `0x08000000`：起始地址；`.hex` 也可以省略。
- `-V`：校验。
- `-Rst`：复位运行。
- 旧版工具对新型号（例如 STM32G4）支持有限，新芯片优先用 STM32CubeProgrammer。

### 4.5 J-Link

J-Link 可以用 Commander 脚本。先在工程根目录准备一个 `flash.jlink`：

```text
loadfile build/test32.hex
r
g
exit
```

Flash 命令：

```text
"C:\Program Files\SEGGER\JLink\JLink.exe" -device STM32F103xB -if SWD -speed 4000 -autoconnect 1 -CommanderScript flash.jlink
```

说明：

- `-device` 必须和芯片型号匹配；STM32G474 可能是 `STM32G474RE` 之类的具体型号，以 J-Link 支持列表为准。
- `flash.jlink` 是脚本文件；如果你不想把它放进工程目录，可以在命令里写脚本的绝对路径。
- 也可以直接用 J-Link 的 `-CommanderScript` 指向任意位置的脚本。

### 4.6 Keil uVision（`.uvprojx` 工程）

Keil 工程通常有自己的下载配置。可以用 UV4 的命令行：

```text
"C:\Keil_v5\UV4\UV4.exe" -f "D:\path\to\project.uvprojx" -j0 -o "%TEMP%\keil-flash.log"
```

说明：

- `-f`：执行 Flash Download。
- `-j0`：不显示 Keil 主界面。
- `-o`：把日志写到文件，烧录失败时先看这个日志。
- UV4 的返回码不一定能完整表示烧录结果，务必检查日志和 Keil 的 Flash 配置。
- 如果你的电脑上已经有一个封装好的 `keil-uv4.cmd`，直接调用它也可以。

> 对于 Keil 工程，左侧“工程信息 → 烧录配置”里的“烧录驱动 / 芯片名称”来自 `.uvprojx`，但它们只是显示；真正执行什么仍由你填写的 Flash 命令决定。

### 4.7 Makefile 里已经有 flash 目标

有些工程在 Makefile 里自己定义了 `flash` 目标，例如：

```make
flash:
	openocd -f interface/stlink.cfg -f target/stm32f1x.cfg -c "program build/$(TARGET).elf verify reset exit"
```

这种情况下 Flash 命令可以直接写：

```text
mingw32-make flash
```

先确认目标真的存在，可以运行：

```text
mingw32-make -n flash
```

CubeMX 默认生成的 Makefile **没有** `flash` 目标，不要直接照抄。

---

## 5. 工具不在 PATH 里怎么办

两种办法，任选一种：

### 办法一：写绝对路径

上面的示例都是绝对路径，最省事。路径有空格时一定加双引号。

### 办法二：在软件里加 PATH

打开 **配置**，找到“附加 PATH”（额外工具路径），一行一个目录，例如：

```text
C:\Program Files\SEGGER\JLink
C:\Program Files\STMicroelectronics\STM32Cube\STM32CubeProgrammer\bin
```

保存后，构建和烧录任务都会把这些目录放到 PATH 前面。

> 如果你是在 Windows 系统环境变量里刚改的 PATH，建议重启 Simple embedded；写在软件“附加 PATH”里则不需要重启。

---

## 6. 常见问题

### 烧录按钮是灰色的

- 还没有打开工程；
- Flash 命令为空；
- 当前有构建/清理/烧录任务正在运行。
- 命令填好后必须点 **保存**。

### 提示 `不是内部或外部命令` / `'openocd' 不是可识别的命令`

命令不在 PATH 里。用绝对路径，或者把工具目录加到“附加 PATH”。

### 提示找不到 ST-Link / J-Link

- 检查 USB 线是不是只能充电的线；
- 关闭 STM32CubeProgrammer、Keil、STM32CubeIDE 等可能占用探针的软件；
- 运行 `STM32_Programmer_CLI -l`、`pyocd list` 或 J-Link 自带的工具确认探针能被识别；
- 检查驱动是否安装。

### 提示 `Error: init mode failed` / 无法连接目标

- 检查 SWDIO、SWCLK、GND、3V3 接线；
- 确认目标板有电；
- 降低 SWD 速度（CubeProgrammer 的 `freq=`，OpenOCD 的 `adapter speed`）；
- 检查芯片是否被读保护，必要时先解除保护/全片擦除。

### 烧录成功但没有运行

- CubeProgrammer 加 `-rst`；
- OpenOCD 用 `reset run` / `reset exit`；
- 检查 BOOT0/BOOT1 引脚和启动模式；
- 确认烧录地址正确。`.bin` 文件必须指定 `0x08000000`（或芯片对应的 Flash 起始地址）。

### 提示找不到 `build\test32.hex`

- 先构建成功；
- 检查 Makefile 的 `TARGET` 是不是 `test32`；
- Clean 之后产物会被删除，需要重新构建；
- 输出面板里会打印最终产物路径。

### 远程 SSH 构建后，本地烧录找不到文件

构建在远端执行，产物在远端机器上，本地 Flash 命令看不到它。要本地烧录，需要：

- 改成在本地构建；或者
- 手动把远端的 `build\...` 产物复制回本地（例如 `scp`）。

软件不会自动同步构建产物。

---

## 7. 当前 `test32` 的最短配置

假设你已经安装 STM32CubeProgrammer，并且工程构建成功：

1. 打开 **配置**；
2. Flash 填：

   ```text
   "C:\Program Files\STMicroelectronics\STM32Cube\STM32CubeProgrammer\bin\STM32_Programmer_CLI.exe" -c port=SWD -w build\test32.hex -v -rst
   ```

3. 保存；
4. 点 **构建**，确认输出面板显示成功并生成 `build\test32.hex`；
5. 点 **烧录**，观察输出面板。

---

## 8. 关于 AXF 和 ELF

这是两个容易混淆的点：

- **Keil 工程**的原生产物通常是 `.axf`。
- **Make/GCC 工程**的原生产物是 `.elf`，CubeMX 的 Makefile 还会额外生成 `.hex` 和 `.bin`。
- `.axf` 和 `.elf` 本质上都是 ELF 容器，区别更多是扩展名和工具链习惯。
- 在 Make/GCC 工程里把“输出格式”选成 `AXF` 并保存时，软件会在 Makefile 末尾添加一段受管理的规则，重新构建后生成 `build\test32.axf`；它的内容仍然是 ELF，只是扩展名不同。
- 具体步骤：打开 **构建选项 → 链接器 → 输出格式**，选择 `AXF`，保存，然后重新构建。构建日志最后会多出一行 `objcopy ... build/test32.axf`。
- 保存时会自动备份 Makefile；切回 `ELF` 再保存，会移除这段受管理的 `.axf` 规则。
- 如果 Makefile 不是可识别的结构，或者工程没有 `$(TARGET).elf` 产物，软件不会强行修改，并会提示同步失败。
- 不用 AXF 时，直接烧录 `.elf` 或 `.hex` 即可，CubeProgrammer / OpenOCD / pyOCD 一般都能识别。

---

## 9. 安全提示

- 烧录前确认芯片型号和产物文件，不要拿 A 工程的 `.hex` 烧到 B 板子上。
- 不要随意修改 Option Bytes / 读保护；一旦开启读保护，解除保护通常会全片擦除。
- 保留一份已知可用的固件和构建产物，出问题时可以快速回退。
- 烧录失败时先看完整输出，不要连续重复点击；有些探针在异常后需要重新插拔。
