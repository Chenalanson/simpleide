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

启动后，左上角显示构建卡片。未导入工程时：

- 名称显示为“未导入工程”
- 构建器显示“未配置”
- 点击 **导入**
- 或按 `Ctrl+O`
- 或直接把文件夹拖进窗口

第一次打开时会自动记住这个工程；下次启动会尝试重新打开它。导入后按钮会变成 **更换 / 配置**。

---

## 2. 认识主界面

主界面分为四块：

```text
┌──────────────┬──────────────────────────┬────────────┐
│ 左侧：构建卡片 │ 中间：代码编辑             │ 右侧操作栏  │
│ 资源管理器    │                          │            │
│ 工程信息      ├──────────────────────────┤            │
│              │ 底部：构建输出             │            │
└──────────────┴──────────────────────────┴────────────┘
```

- 左侧构建卡片顶部显示当前工程名和完整路径
- 构建卡片里的 **配置** 用于修改构建命令，**更换** 用于切换工程
- 资源管理器第一行是工程根目录，点击可展开/收起，下面才是工程内容
- 文件树较长时，拖动资源管理器与工程信息之间的分割线，可以调整两块区域的高度；位置会记住
- 拖动左栏右侧边缘，可以调整整个左栏的宽度
- 收起或展开工程信息时标题位置保持不变；文件树较短时标题紧跟最后一个文件，展开状态也会记住
- 文件树下方是“工程信息”，点击分组或列表整行可选中并收起/展开，选中行显示淡蓝色背景
- 工程信息中可查看构建配置、输出目录、CPU、浮点、链接脚本、烧录配置、包含目录、库搜索目录和宏定义
- 构建配置里的 **构建选项 → 编辑** 分为全局选项、C/C++ 编译器、汇编器和链接器四个页面
- Make 工程保存构建选项时会按差异写回 Makefile；链接器输出格式选 `AXF` 后，重新构建会生成 `.axf` 副本
- 包含目录、库搜索目录和预处理定义标题左侧也有折叠箭头，可单独收起
- 这些列表右侧有绿色 `+`，点击可直接添加一项；底部的 **批量编辑** 可一次修改全部内容
- Makefile 工程保存这些修改时会同步到 Makefile（修改前自动备份）；Keil `.uvprojx` 不会被修改
- 左侧点文件夹展开，点文件在中间打开
- 文件有修改时，Tab 上会出现黄色圆点，窗口标题也会标记为未保存
- 底部输出面板可以拖动顶部手柄调整高度；收起后也能直接向上拉出，高度会记住；也可以点标题左侧的 **收起/展开**

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
- 左侧行号，当前行更亮；设置里可打开 **相对行号**
- 设置里可选 80 / 120 列 **换行标尺**
- 设置里可打开 **显示空白字符**，用点、箭头和折角显示空格、Tab 和换行
- 括号匹配：光标靠近 `() [] {}` 时自动高亮配对，`Ctrl+]` 跳到配对括号
- 书签：`Ctrl+F2` 在当前行添加/移除，`F2` 跳到下一个，`Shift+F2` 跳到上一个
- 撤销 / 重做 / 复制 / 粘贴使用 egui 编辑器默认快捷键
- 工具栏上的 **自动换行** 可切换长行折行
- `Tab` 插入缩进，`Shift+Tab` 减少缩进；选中多行时整块缩进 / 反缩进
- 回车自动保持当前行缩进，在 `{` / `[` / `(` 后额外增加一级
- 设置中可选择 **空格缩进** 和 **缩进宽度**（1–8）
- 设置中可开启 **自动保存**，停止输入指定毫秒后自动保存所有修改
- 工具栏 **只读** 可锁定当前文件，锁定后不能编辑、替换、切换编码或保存；再次点击恢复
- 工具栏 **格式化** 或 `Ctrl+Shift+I` 使用 clang-format 格式化当前 C/C++ 文件；可在设置中指定 clang-format 路径，或开启 **保存时 clang-format**
- 未安装 clang-format 时会提示安装 LLVM 或填写路径，不会阻止普通保存
- 资源管理器右上角 **Git** 或 `Ctrl+Shift+G` 打开 Git 状态：显示分支和已修改 / 新增 / 删除文件，点击文件直接打开
- 文件树里 `M` 为已修改、`U` 为未跟踪、`A` 为新增、`D` 为删除、`R` 为重命名；当前文件已修改的行在行号栏左侧显示黄色竖条
- Git 集成只读取本机 `git` 命令状态，不自动提交、不弹出凭据窗口
- 资源管理器右上角 **TODO** 或 `Ctrl+Shift+T` 扫描工程中的 `TODO` / `FIXME` / `HACK` / `NOTE` 注释，可按类型过滤，点击跳到对应行
- 工具栏 **符号** 或 `Ctrl+Shift+O` 搜索 C/C++ 函数、结构体、宏和汇编标签，支持按名称或路径过滤，点击跳转
- 查找 / 替换：工具栏 **查找** 或 `Ctrl+F`；`Ctrl+H` 打开替换；`Aa` 区分大小写，`.*` 正则；`F3` / `Shift+F3` 上下一个；支持全部替换
- 查找匹配会在编辑器中高亮，当前匹配使用不同底色
- 全工程搜索：`Ctrl+Shift+F` 或查找栏 **全工程**；支持大小写 / 正则，结果显示文件、行号和预览，点击直接跳转
- 资源管理器默认遵循工程根目录的 `.gitignore`；设置里可以关闭，或用分号追加额外忽略规则（如 `build/; *.tmp`）
- 构建日志中带 `文件:行:列` 的错误 / 警告行可以直接点击，跳转到对应代码行
- 有错误的行在行号栏右侧显示红色竖条，警告显示黄色竖条
- 输出面板标题栏可过滤 **全部 / 错误 / 警告**
- 设置里可以调整代码字号，`Ctrl+=` / `Ctrl+-` 也可以快速缩放
- 打开和保存时会保持原来的 CRLF / LF 行尾
- 自动识别 UTF-8 / GBK；工具栏编码下拉可切换，保存时按所选编码写回
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

点击左上角构建卡片的 **配置**，或按 `Ctrl+,`：

- **Build**：例如 `make -j8`、`cargo build --release`
- **Clean**：例如 `make clean`
- **Rebuild**：例如 `make clean && make`
- **Flash**：烧录命令，可留空

点击 **自动检测** 可以重新读取根目录的构建文件。

设置和自动备份位于：

```text
%APPDATA%\SimpleEmbedded\settings.json
%APPDATA%\SimpleEmbedded\backups\
```

设置按工程分别保存。Makefile 工程在保存构建选项或 C/C++ 属性时会同时写入 Makefile；Keil 工程只保存在本机设置。修改后点击 **保存** 才会生效。

---

## 6. 配置烧录

烧录命令**始终在本地工程目录执行**，这样 J-Link / ST-Link 等工具可以直接访问 USB；即使构建目标选的是远程 SSH，也不会把烧录命令发到远端。

在 **配置 → Flash** 填写命令，例如：

```text
JLink.exe -CommanderScript flash.jlink
```

或者 OpenOCD：

```text
openocd -f interface/cmsis-dap.cfg -f target/stm32f4x.cfg -c "program build/firmware.elf verify reset exit"
```

Flash 命令留空时，右侧操作栏的 **烧录** 按钮会禁用。烧录前需要先构建成功；**烧录不会自动构建**。

ST-Link、STM32CubeProgrammer、pyOCD、J-Link、Keil、`.bin` 地址、PATH 配置和常见错误，见单独的详细教程：

**[docs/FLASH.md](FLASH.md)**

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
