---
name: Safe Shell & Destructive File Ops
description: 高危 Shell 命令与破坏性文件操作（删除、递归清理、覆盖写入、磁盘/仓库重置）的强制安全规则。凡是涉及 rm / del / rd / rmdir / Remove-Item、清理脚本、构建打包脚本、cargo clean、git clean、磁盘操作，或用户要求"清空/删掉/重置/重新生成"目录时，按「路径展开 → 静态校验 → 风险分级 → Dry-run 预览 → 人工确认」流程处理；禁止自动执行任何删除。本规则已通过 instructions 常驻注入，通常无需重复加载。
---

# 安全 Shell 与破坏性文件操作规则

> 违反本规则 = 直接拒绝执行，没有"这次特殊"的例外。

## 0. 第一原则：代码生成 ≠ 执行

| 可以做 | 不可以做 |
| --- | --- |
| 输出源码、构建脚本、清理脚本的**内容** | 直接调用 shell 在本地跑删除命令 |
| 输出 Dry-run 预览命令 | 把删除藏进构建/打包/测试脚本里静默执行 |
| 说明风险和影响范围 | 用"先移动再删""让用户双击脚本"绕过确认 |
| 给出可复制、路径已展开的命令 | 后台、静默、隐藏窗口方式执行破坏性写入 |

能把事做完，不等于允许直接动手。**破坏性动作必须由用户按下最后一次确认。**

## 1. 绝对禁止（无例外，不询问、不执行）

**删除类**

- `rm -rf`、`rm -r`、`rm -fr`、`rm -f *`
- `rmdir /s /q`、`rd /s /q`、`del /f /s /q`、`del /q`、`del *.*`、`erase`
- `Remove-Item -Recurse -Force`、`ri -r -fo`、`Remove-Item * -Recurse`
- 任何带通配符 `*` / `?` 的递归删除
- `find ... -delete`、`xargs rm`、`Get-ChildItem ... | Remove-Item -Recurse`

**覆盖 / 重置 / 磁盘类**

- `Format-Volume`、`Clear-Disk`、`Initialize-Disk`、`diskpart clean`、`mkfs`
- `robocopy /MIR`、`xcopy /s /y`、`rsync --delete`（会镜像删除对端文件）
- `git clean -fdx`、`git reset --hard`、`git checkout -- .`（丢弃工作区改动）
- `git push --force`、`git branch -D`（除非用户明确指定分支并要求）
- `docker system prune -a`、`docker volume prune`
- `cargo clean`（只删 `target/`，但属于破坏性，必须走确认流程）
- 直接 `>` / `Set-Content` 覆盖已有源码、配置、数据库、用户数据文件

**路径类（任何一条命中即拒绝）**

- 目标是磁盘根：`C:\`、`D:\`、`/`、`//`
- 目标是用户主目录或其一级子目录：`C:\Users\<name>`、`~`、`$HOME`
- 目标是系统目录：`C:\Windows`、`C:\Program Files`、`C:\ProgramData`
- 目标在项目根之外（清理/构建场景）
- 路径含未展开变量、`..` 逃逸、未加引号、动态拼接无法静态确定

**执行方式类**

- `Start-Process -WindowStyle Hidden`、后台 `&`、计划任务、服务方式执行删除
- 把删除命令写进 `build.ps1` / `package.ps1` / `Makefile` 让后续流程自动触发
- 在用户只看得到结果、看不到命令的情况下执行

## 2. 风险分级

| 等级 | 典型场景 | 处理 |
| --- | --- | --- |
| LOW | 只读：`Get-ChildItem`、`git status`、`cargo check`、读取源码 | 直接执行 |
| MEDIUM | 新增文件、构建产物、项目内可完全重建的缓存（`target/`、`dist/`、`node_modules/`、`.cache/`） | Dry-run + 确认 |
| HIGH | 删除源码、配置、`Cargo.toml`、`.git`、用户数据、egui 持久化配置、未提交改动 | Dry-run + 确认 + 备份建议 + 二次复核 |
| CRITICAL | 盘根、系统目录、项目上级目录、批量脚本、`/MIR`、`diskpart clean` | **拒绝执行**，只给替代方案 |

判定原则：
- 目录能否一条命令重建？能 → 最多 MEDIUM；不能 → HIGH 起。
- 是否在项目根内？否 → CRITICAL。
- 是否包含 `.git`、源码扩展名（`.rs`/`.toml`/`.png` 源素材等）、用户数据？是 → 升级一级。

## 3. 破坏性操作七步流程

**Step 1 — 停。** 不调用任何执行类工具，不"先跑一下看看"。

**Step 2 — 展开真实路径。** 打印最终绝对路径（不简写、不用变量、不省略引号），并高亮标记：

```text
【目标路径】D:\gif_pic\dist
【项目根】  D:\gif_pic
【在项目内】是
```

路径展开只允许用只读方式：

```powershell
# 只读，安全
[System.IO.Path]::GetFullPath($p)
(Resolve-Path -LiteralPath $p).Path
```

**Step 3 — 静态校验。** 逐条过 `.opencode/skills/safe-shell/references/path-validation.md` 的检查清单，任一条不通过 → 停止并说明原因。

**Step 4 — 影响评估。** 必须给出：
- 待删除文件数、总大小；
- 一级子项列表（最多 20 项，超出注明"另有 N 项"）；
- 是否含 git 跟踪文件 / 未提交改动 / 源码 / 用户数据；
- 是否含符号链接、junction、挂载点（可能指向项目外）。

**Step 5 — 给 Dry-run 预览（只读，绝不删）。** 优先使用本技能自带脚本：

```powershell
powershell -NoProfile -ExecutionPolicy Bypass -File ".opencode/skills/safe-shell/scripts/preview-delete.ps1" -Path "D:\gif_pic\dist" -ProjectRoot "D:\gif_pic"
```

> 装有 PowerShell 7 的机器可把 `powershell -NoProfile -ExecutionPolicy Bypass -File` 换成 `pwsh -NoProfile -File`。

或 PowerShell 原生模拟：

```powershell
# 仅列出，不删除
Get-ChildItem -LiteralPath "D:\gif_pic\dist" -Recurse -Force -ErrorAction SilentlyContinue |
  Select-Object FullName, Length, LastWriteTime |
  Format-Table -AutoSize
Remove-Item -LiteralPath "D:\gif_pic\dist" -Recurse -Force -WhatIf
```

**Step 6 — 请求人工确认。** 使用 `.opencode/skills/safe-shell/references/confirmation-template.md` 的模板，等待用户**手动回复**：

```text
【确认删除】D:\gif_pic\dist
```

- 确认口令必须与展示的路径逐字符一致；范围变了就重新确认。
- 用户说"删吧""可以"不算确认，必须出现 `【确认删除】` + 完整路径。
- 未收到确认前，只能继续提供预览、替代方案或等待。

**Step 7 — 确认后仍不自动执行。** 输出可复制命令块，由用户自行运行；同时说明回滚方式（备份位置、重建命令）。执行权留给用户。

## 4. 路径静态校验（硬性要求）

完整清单和可直接复用的函数见 `.opencode/skills/safe-shell/references/path-validation.md`，核心红线：

1. **绝对路径 + 完整展开**，变量必须加引号，优先 `-LiteralPath`（避免 `[`、`]`、`*` 被当通配符）。
2. **拒绝盘根**：`^[A-Za-z]:[\\/]?$`、`^[\\/]+$`。
3. **必须在项目根之下**：`[IO.Path]::GetFullPath($p).StartsWith($root + [IO.Path]::DirectorySeparatorChar)`，且 `$p -ne $root`。
4. **禁止 `..` 逃逸**：规范化之后不允许残留 `..`。
5. **通配符**：只允许最后一段、只允许 `-Path` 非递归列举；删除动作一律不允许通配符。
6. **禁止简写与根变量**：不允许 `%TEMP%`、`$env:USERPROFILE` 直接作为删除目标，必须先展开成绝对路径再校验。
7. **链接检查**：对路径祖先链做 reparse point 检查，任何一级是 junction/symlink 且指向项目外 → 拒绝。
8. **路径长度**：超过 240 字符给出警告（Windows 长路径/工具兼容问题）。

## 5. 清理脚本必须"项目本地限定"

所有清理、构建、临时文件逻辑：

- 脚本顶部定义并校验项目根：

```powershell
$ErrorActionPreference = 'Stop'
$ProjectRoot = (Resolve-Path -LiteralPath (Join-Path $PSScriptRoot '..')).Path
if (-not $ProjectRoot -or $ProjectRoot -match '^[A-Za-z]:\\?$') {
    throw "拒绝执行：项目根无效 ($ProjectRoot)"
}
```

- 清理目标使用**白名单**，只允许项目内子目录：

```text
target/  dist/  build/  out/  .cache/  tmp/  temp/  node_modules/  .trash/
```

- 禁止 `Set-Location ..` 后删除、禁止相对路径 `..\..`、禁止"清空上级缓存"。
- 保护名单永不删除：`.git/`、`.gitignore`、`Cargo.toml`、`Cargo.lock`、`src/`、`assets/`、`README*`、用户配置。
- 脚本默认行为是**预览**；真正删除必须显式传 `-Confirm` 且由人执行。
- 清理逻辑里不得出现无提示的 `Remove-Item`、`del`、`rd`、`rm`。

## 6. 非破坏性替代方案（优先推荐）

| 想删除 | 更安全的做法 |
| --- | --- |
| 旧构建产物 | 输出到新的 `dist/<版本号>/`，保留旧的；或先 `Compress-Archive` 备份 |
| 临时文件 | 移到项目内 `.trash/<yyyyMMdd-HHmmss>/`，人工确认后再清 |
| 重置 egui 配置 | 先复制到 `.backup/`，再说明将重置哪些键 |
| 清理 git 工作区 | `git status` + `git stash push -u`，不用 `git clean -fdx` |
| `cargo clean` | 先说明只删 `target/`，需要彻底干净时用 `cargo clean -p <crate>` 缩小范围 |
| 删除大目录 | 先 `Get-ChildItem | Measure-Object` 估算，再决定是否备份 |

## 7. Rust / egui / 打包 exe 专项

- `cargo build --release` 的输出在 `target\release\*.exe`；打包只**复制**到 `dist\`，不删除旧 `dist`。
- 打包脚本禁止 `del /s /q *.exe`、`rd /s /q dist`；使用显式文件名：

```powershell
# 安全示例：只新建/覆盖单个目标文件，不做递归删除
$dist = Join-Path $ProjectRoot 'dist'
New-Item -ItemType Directory -Path $dist -Force | Out-Null
$src = Join-Path $ProjectRoot 'target\release\app.exe'
$dst = Join-Path $dist 'app.exe'
if (Test-Path -LiteralPath $dst) {
    Copy-Item -LiteralPath $dst -Destination "$dst.bak" -Force
}
Copy-Item -LiteralPath $src -Destination $dst -Force
Write-Host "已输出: $dst"
```

- 不用 `rd /s /q target` 代替 `cargo clean`；不用通配符清理中间产物。
- egui/eframe 的持久化数据（`%APPDATA%\<app>`、`%LOCALAPPDATA%\<app>`）属于用户数据（HIGH），重置前必须备份并确认。
- UPX、代码签名、安装包工具覆盖 exe 前先留 `.bak`，并在输出里说明覆盖了哪个文件。
- 构建/打包脚本里若必须清理，只能清理项目内白名单子目录，且默认走 `-WhatIf` 预览。

## 8. 输出格式规范（提破坏性命令时强制）

一次回答里必须包含且分块清晰：

```text
【目标路径】D:\gif_pic\dist            ← 高亮、完整、加引号
【项目根】  D:\gif_pic
【风险等级】MEDIUM
【影响范围】删除 128 个文件 / 42.7 MB；含 3 个 .exe；无 git 跟踪文件；无符号链接
【Dry-run】  <只读预览命令>
【待确认】  回复 【确认删除】D:\gif_pic\dist 后，我再给出可执行命令（我不会自动运行）
```

规则：
- **预览块和删除块必须分开**，不能出现在同一个可复制代码块里。
- 不省略路径、不简写为 `.\dist`、不用 `<项目目录>` 之类占位符。
- 风险等级、影响范围、回滚方式缺一不可。

## 9. 例外、复用与升级

- 用户已在本会话确认过**完全相同**的路径和范围 → 可复用，但每次执行前仍需重述路径与范围；任何变化（路径、通配符、范围扩大）→ 重新确认。
- 用户说"以后都不用问了"→ 不接受为无限授权；最多接受"本项目内、指定白名单子目录、本次会话"的限定授权，且仍需打印路径。
- 路径无法静态确定（运行时变量、网络盘、用户输入拼接）→ 拒绝执行，改为输出需要人工填入并校验的脚本。
- 校验不通过但任务确实需要 → 停下来解释冲突，给出更小范围的替代方案，由用户决定。
- 拿不准 → 选择不删，并询问。

## 10. 输出前自检清单

- [ ] 没有自动执行任何执行类工具
- [ ] 路径是完整绝对路径、加引号、无变量、无通配符
- [ ] 已做盘根 / 系统目录 / 项目根逃逸校验
- [ ] 已给出风险等级、文件数、总大小、一级子项
- [ ] 已检查 git 跟踪、源码、用户数据、链接
- [ ] 已提供只读 Dry-run
- [ ] 预览与删除命令分离
- [ ] 已请求用户手动回复 `【确认删除】<完整路径>`
- [ ] 给出了回滚/重建方式

## 参考资料

- `.opencode/skills/safe-shell/references/path-validation.md` — 路径校验清单与 PowerShell 校验函数
- `.opencode/skills/safe-shell/references/confirmation-template.md` — 确认请求模板与口令规则
- `.opencode/skills/safe-shell/scripts/preview-delete.ps1` — 只读 Dry-run 预览脚本（永不删除）
