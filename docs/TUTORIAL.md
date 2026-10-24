# Simple embedded 使用教程

这是一份从打开软件到完成第一次构建、烧录的完整教程。Simple embedded 的目标很简单：**工程文件夹就是工程**，你不需要先创建 `.eide` 文件、选择芯片或配置内存布局。

---

## 1. 启动软件

### 使用已构建的 exe

```powershell
& "D:\embeed\target\release\simple-embedded.exe"
```

双击 `target\release\simple-embedded.exe` 也可以。

### 从源码运行

```powershell
cd D:\embeed
cargo run --release
```

启动时也可以直接带上工程目录，以及一个想顺便打开的文件：

```powershell
cargo run --release -- D:\firmware\my-board src\main.c
```

启动后，左上角会显示 **导入工程** 卡片，中间只显示一行提示：

- 点击 **选择工程文件夹**
- 或按 `Ctrl+O`
- 或直接把文件夹拖进窗口

第一次打开时会自动记住这个工程；下次启动会尝试重新打开它。已经打开工程时，左上角工程卡片右侧的 **更换** 也可以切换目录。

---

## 2. 认识主界面

主界面分为三块：

```text
┌──────────────────────┬────────────────────────────────┐
│ 左侧：工程卡片 + 文件树 │ 中间：多标签代码编辑器 / 空状态   │
├──────────────────────┴────────────────────────────────┤
│ 底部：构建输出（彩色编译日志 / 错误 / 警告）              │
├───────────────────────────────────────────────────────┤
│ 右侧操作栏：构建 / 清理 / 重构建 / 烧录 / 设置            │
└───────────────────────────────────────────────────────┘
```

- 左侧点文件夹展开，点文件在中间打开
- 文件有修改时，Tab 上会出现黄色圆点，窗口标题也会标记为未保存
- 底部输出面板可以拖动上边缘调整高度，也可以点标题左侧的 **收起/展开**

---

## 3. 打开一个已有工程

假设你的固件目录是：

```text
D:\firmware\my-board
├── Makefile
├── src\
│   └── main.c
└── include\
    └── board.h
```

操作：

1. 点击左上角 **选择工程文件夹**
2. 选择 `D:\firmware\my-board`
3. 在左侧展开 `src`，点击 `main.c`

Simple embedded 会自动检查工程根目录：

| 根目录存在 | 自动使用的构建系统 |
| --- | --- |
| `Cargo.toml` | Cargo |
| `Makefile` / `makefile` / `GNUmakefile` | Make（`make`、`gmake` 或 Windows 的 `mingw32-make`） |
| `CMakeLists.txt` | CMake |

如果根目录没有这些文件，构建按钮会被禁用。点击左上角工程卡片里的 **配置**，手动填写命令即可；设置里也可以重新点击 **自动检测**。

> Simple embedded 不会在工程里创建任何配置文件。工程文件夹可以直接提交到 Git、拷贝给同事，仍然是一个普通工程。

---

## 4. 编辑与保存

### 基础操作

- 点击左侧文件打开新 Tab
- 点击 Tab 切换文件
- 点 Tab 上的 `×` 或按 `Ctrl+W` 关闭文件
- 有未保存修改时，关闭前会弹出确认框
- `Ctrl+S` 保存当前文件
- `Ctrl+Shift+S` 保存所有文件

### 编辑辅助

- 语法高亮：C/C++/ASM/Rust/Makefile/CMake/TOML/JSON 等
- Tab 缩进
- 撤销 / 重做 / 复制 / 粘贴使用 egui 编辑器默认快捷键
- 工具栏上的 **自动换行** 可切换长行折行
- 设置里可以调整代码字号，`Ctrl+=` / `Ctrl+-` 也可以快速缩放
- 打开和保存时会保持原来的 CRLF / LF 行尾
- 读取和保存 UTF-8 文件，保留 UTF-8 BOM（如果有）

### 还没保存时直接构建？

不用担心：点击 Build / Clean / Rebuild / Flash 之前，编辑器会自动保存所有未保存文件。

---

## 5. 构建、清理与重新构建

以 `Makefile` 工程为例，自动检测通常会得到：

- Build：`mingw32-make`（Windows）
- Clean：`mingw32-make clean`
- Rebuild：`mingw32-make clean && mingw32-make`

点击右侧操作栏的 **构建** 或按 `Ctrl+B`。

构建期间：

- UI 不会冻结，可以继续切文件或编辑
- 右侧操作栏会显示 **停止**
- 输出面板实时显示命令的 stdout / stderr
- 含 `error` 的行显示为红色，含 `warning` 的行显示为黄色
- 同时支持编译器的 ANSI 颜色

构建结束后，底部会显示：

- 构建成功：绿色
- 构建失败：红色，并显示退出码
- 被停止：黄色

输出面板标题右侧还可以：

- **清空**：清掉当前日志
- **复制**：把全部日志复制到剪贴板
- **自动滚动**：关闭后可以回看之前的日志
- **停止**：强制结束 Windows 进程树

### 修改构建命令

点击右侧操作栏底部的 **设置**，或按 `Ctrl+,`：

- **Build**：例如 `make -j8`、`cargo build --release`
- **Clean**：例如 `make clean`
- **Rebuild**：例如 `make clean && make`
- **Flash**：烧录命令，可留空

点击 **自动检测** 可以重新读取根目录的构建文件。

设置只保存在：

```text
%APPDATA%\SimpleEmbedded\settings.json
```

并且按工程分别保存。修改后点击 **保存** 才会生效。

---

## 6. 配置烧录

烧录命令**始终在本地工程目录执行**，这样 J-Link 等工具可以直接访问 USB。

在 **设置 → 构建 → Flash** 填类似：

```text
JLink.exe -CommanderScript flash.jlink
```

或者 OpenOCD：

```text
openocd -f interface/cmsis-dap.cfg -f target/stm32f4x.cfg -c "program build/firmware.elf verify reset exit"
```

Flash 命令留空时，右侧操作栏的 **烧录** 按钮会禁用。

> 当前版本没有内置 J-Link MCP 客户端。烧录就是一个可配置的本地命令，因此你可以接任意工具链或封装脚本。

---

## 7. 远程 SSH 构建

适合同一套代码在 Linux 虚拟机里编译。Simple embedded 直接调用系统 OpenSSH。

### 前置条件

1. 本机已经安装 `ssh`（Windows 10/11 自带 OpenSSH 客户端）
2. 能免密登录：
   ```powershell
   ssh -o BatchMode=yes user@192.168.1.20 true
   ```
   如果这条命令不需要密码，就配置好了。
3. 虚拟机上已经有工程目录，例如 `/home/user/firmware`

### 在 Simple embedded 中配置

打开 **设置 → 构建**：

1. 构建位置选择 **远程 SSH**
2. 填写：
   - SSH 主机：`user@192.168.1.20` 或 `~/.ssh/config` 中的别名
   - SSH 端口：默认 `22`
   - 远程目录：`/home/user/firmware`
3. 命令仍然填 Make/Cargo 命令，例如 `make -j8`
4. 保存后点击构建

实际执行逻辑类似：

```text
ssh -o BatchMode=yes user@host "cd '/home/user/firmware' && make -j8"
```

注意：

- 必须提前配置 SSH key，远程模式不支持交互式密码输入
- 右侧操作栏的 **停止** 会结束本地 `ssh` 进程；远端编译进程是否退出取决于远端 shell/环境
- Flash 仍在本地执行；远程构建产物需要你自己同步回本地再烧录

---

## 8. 设置与持久化

设置文件位置：

```text
%APPDATA%\SimpleEmbedded\settings.json
```

里面保存：

- 最近工程列表
- 每个工程各自的 Build/Clean/Rebuild/Flash/SSH 配置
- 代码字号
- 自动换行
- 输出自动滚动
- 是否显示隐藏文件

删除这个文件即可恢复首次启动状态。

工程目录本身**不会**被写入配置。

---

## 9. 用自带示例试一遍

仓库包含一个可以直接编译的示例：

```powershell
& "D:\embeed\target\release\simple-embedded.exe" "D:\embeed\examples\blink" "D:\embeed\examples\blink\src\main.c"
```

然后：

1. 左侧应能看到 `include`、`src`、`Makefile`
2. 点击 `src/main.c`
3. 按 `Ctrl+B`
4. 底部应显示：

```text
▶ 构建 · mingw32-make
cc -std=c11 -Wall -Wextra -O2 -Iinclude src/main.c src/board.c -o build/firmware.exe
构建完成 · 用时 ...
```

前提是 `gcc` 和 `mingw32-make` 在 `PATH` 中。Windows 上通常来自 MinGW：

```text
C:\complier\mingw64\bin
```

可以在 PowerShell 里验证：

```powershell
gcc --version
mingw32-make --version
```

---

## 10. 快捷键速查

| 快捷键 | 功能 |
| --- | --- |
| `Ctrl+O` | 打开工程文件夹 |
| `Ctrl+S` | 保存当前文件 |
| `Ctrl+Shift+S` | 保存全部文件 |
| `Ctrl+B` | Build |
| `Ctrl+Shift+B` | Rebuild |
| `Ctrl+W` | 关闭当前 Tab |
| `Ctrl+,` | 打开设置 |
| `Ctrl+=` / `Ctrl+-` | 放大/缩小代码字号 |

---

## 11. 常见问题

### 提示 `make` / `mingw32-make` 找不到

把 MinGW 的 `bin` 目录加入系统 `PATH`，然后重新启动 Simple embedded：

```powershell
where.exe mingw32-make
```

如果找不到，安装 MinGW-w64，或把 Build 命令改成你实际使用的 make 程序。

### 清理时报 `rm` 找不到 / `CreateProcess failed`

这是 POSIX 风格 Makefile 在 Windows 上缺少 Unix 工具导致的。新版本运行本地 Make 构建时会自动寻找 Git for Windows、MSYS2 或 Cygwin 的 `usr\bin` 并加入 PATH。

如果仍然失败，可以在设置里把对应的 `usr\bin` 加到 **工具链路径 / 额外 PATH**，例如：

```text
D:\Program Files\Git\usr\bin
```

### 自动检测不到构建系统

确认工程根目录直接存在 `Makefile`、`Cargo.toml` 或 `CMakeLists.txt`。如果构建脚本在子目录，打开设置手动填写：

```text
cd firmware && make
```

（在 Windows 本地模式下命令会通过 `cmd /S /C` 执行，因此 `&&`、`cd` 都可以用。）

### 日志太长，右边被截断

当前日志采用固定行高虚拟化显示，超长行会横向裁剪。可以：

- 把鼠标悬停在长行上查看完整内容
- 点击输出面板的 **复制**，粘贴到文本编辑器里查看全部日志

### SSH 提示 `Permission denied`

先在本机终端测试：

```powershell
ssh -o BatchMode=yes user@host true
```

如果仍然要求输密码，说明 key 未配置成功。检查 `%USERPROFILE%\.ssh\id_ed25519.pub` 是否已加入远端 `~/.ssh/authorized_keys`。

### 保存失败

编辑器只支持 UTF-8 文本文件，且限制为 16 MB。保存失败通常是：

- 文件只读
- 文件被其他程序占用
- 路径权限不足

### 修改没有生效

设置需要点击 **保存**；构建前会自动保存所有未保存的源码 Tab。构建使用的是磁盘上的文件。

---

## 12. 从源码开发

```powershell
cd D:\embeed
cargo fmt --all
cargo test
cargo clippy --all-targets -- -D warnings
cargo run --release
cargo build --release
```

生成文件：

```text
target\release\simple-embedded.exe
```

项目结构与设计取舍见根目录的 `README.md` 和 `AGENTS.md`。
