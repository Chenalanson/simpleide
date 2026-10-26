# 路径静态校验清单

删除前逐条检查，任一条不通过 → 停止。只允许用只读方式解析路径。

## 1. 一次性检查表

| # | 检查项 | 通过条件 | 不通过处理 |
| --- | --- | --- | --- |
| 1 | 是否绝对路径 | `[IO.Path]::IsPathRooted($p)` 为真 | 先转绝对路径再校验 |
| 2 | 是否完整展开 | 无 `%VAR%`、`$env:`、`~`、`..` 残留 | 展开后重新校验 |
| 3 | 是否盘根 / UNC 根 | 不匹配 `^[A-Za-z]:[\\/]?$`、`^[\\/]{1,2}[^\\/]+[\\/]?$` | 拒绝 |
| 4 | 是否系统目录 | 不在 `Windows`、`Program Files*`、`ProgramData`、`System32` 下 | 拒绝 |
| 5 | 是否用户主目录 | 不等于 `$HOME` / `C:\Users\<name>`，也不在其一级子目录 | 拒绝 |
| 6 | 是否在项目根内 | `$full.StartsWith($root + '\')` 且 `$full -ne $root` | 拒绝 |
| 7 | 是否通配符 | 不含 `*`、`?`；删除只用 `-LiteralPath` | 拒绝，要求给字面路径 |
| 8 | 是否含 `.git` | 路径分段中无 `.git` | 拒绝 |
| 9 | 链接祖先 | 祖先链无指向项目外的 junction / symlink | 拒绝 |
| 10 | 路径长度 | ≤ 240 字符 | 警告并缩短 |
| 11 | 是否存在 | `Test-Path -LiteralPath` 为真 | 提示无需删除 |
| 12 | 是否项目根本身 | `$full -ne $root` | 拒绝 |

## 2. 可复用校验函数

```powershell
$ErrorActionPreference = 'Stop'

function Get-NormalizedPath {
    param([Parameter(Mandatory)][string]$Path)
    $full = [System.IO.Path]::GetFullPath($Path)
    if (Test-Path -LiteralPath $full) {
        $full = (Get-Item -LiteralPath $full -Force).FullName
    }
    return $full.TrimEnd('\', '/')
}

function Test-SafeDeleteTarget {
    param(
        [Parameter(Mandatory)][string]$Path,
        [Parameter(Mandatory)][string]$ProjectRoot
    )

    $reasons = New-Object System.Collections.Generic.List[string]
    $root = Get-NormalizedPath $ProjectRoot
    $full = Get-NormalizedPath $Path

    if ($root -match '^[A-Za-z]:\\?$' -or $root -match '^\\\\[^\\]+\\[^\\]+\\?$') {
        throw "Invalid project root: $root"
    }
    if ($full -match '^[A-Za-z]:\\?$' -or $full -match '^[\\/]+$') {
        $reasons.Add('Target is a drive/filesystem root.')
    }
    if ($full -eq $root) {
        $reasons.Add('Target is the project root itself.')
    }
    if (-not $full.StartsWith($root + [System.IO.Path]::DirectorySeparatorChar)) {
        $reasons.Add("Target is outside the project root: $root")
    }
    if ($full -match '[*?]') {
        $reasons.Add('Target contains wildcard characters.')
    }
    if ($full -match '(^|[\\/])\.git([\\/]|$)') {
        $reasons.Add('Target contains .git.')
    }
    foreach ($blocked in @('C:\Windows', 'C:\Program Files', 'C:\Program Files (x86)', 'C:\ProgramData')) {
        if ($full.StartsWith($blocked, [System.StringComparison]::OrdinalIgnoreCase)) {
            $reasons.Add("Target is a system directory: $blocked")
        }
    }
    if ($full.Length -gt 240) {
        $reasons.Add('Target path exceeds 240 characters.')
    }

    # Ancestor reparse points (junction/symlink) must not escape the project root.
    $cursor = $full
    while ($cursor -and $cursor.Length -gt $root.Length) {
        if (Test-Path -LiteralPath $cursor) {
            $item = Get-Item -LiteralPath $cursor -Force
            if ($item.Attributes -band [System.IO.FileAttributes]::ReparsePoint) {
                $resolved = $item.ResolvedTarget
                if ($resolved -and -not ($resolved.FullName.StartsWith($root, [System.StringComparison]::OrdinalIgnoreCase))) {
                    $reasons.Add("Reparse point escapes project root: $cursor -> $($resolved.FullName)")
                }
            }
        }
        $cursor = Split-Path -Parent $cursor
    }

    [pscustomobject]@{
        Target = $full
        ProjectRoot = $root
        IsSafe = ($reasons.Count -eq 0)
        Reasons = $reasons
    }
}
```

调用：

```powershell
$r = Test-SafeDeleteTarget -Path 'D:\gif_pic\dist' -ProjectRoot 'D:\gif_pic'
if (-not $r.IsSafe) { $r.Reasons; throw 'Refused by static path validation.' }
```

## 3. 只读预览（永远不删）

```powershell
# 递归列出，但不删除
Get-ChildItem -LiteralPath $full -Recurse -Force -ErrorAction SilentlyContinue |
    Select-Object FullName, Length, LastWriteTime

# PowerShell 原生模拟删除（只打印将受影响的项）
Remove-Item -LiteralPath $full -Recurse -Force -WhatIf

# 统计
$items = Get-ChildItem -LiteralPath $full -Recurse -Force -ErrorAction SilentlyContinue
[pscustomobject]@{
    Files = ($items | Where-Object { -not $_.PSIsContainer }).Count
    Dirs  = ($items | Where-Object { $_.PSIsContainer }).Count
    Bytes = ($items | Where-Object { -not $_.PSIsContainer } | Measure-Object Length -Sum).Sum
}
```

## 4. 影响面检查

```powershell
# git 跟踪文件（不可重建，风险升级）
git -C $root ls-files -- $relativePath

# 未提交改动
git -C $root status --porcelain -- $relativePath

# 源码 / 配置类文件（风险升级）
Get-ChildItem -LiteralPath $full -Recurse -Force -Include *.rs,*.toml,*.lock,*.cs,*.ts,*.js,*.py,*.json,*.yaml,*.yml,*.ps1,*.sh -ErrorAction SilentlyContinue

# 符号链接 / junction（可能指向项目外）
Get-ChildItem -LiteralPath $full -Recurse -Force -ErrorAction SilentlyContinue |
    Where-Object { $_.Attributes -band [System.IO.FileAttributes]::ReparsePoint }
```

## 5. 常见踩坑

- `Remove-Item C:\foo\[1].txt` 会被当成通配符 → 一律用 `-LiteralPath`。
- `rd /s /q "%~dp0"` 中 `%~dp0` 带尾反斜杠；若脚本位置变动会变成上级目录 → 先展开并校验再使用。
- `Remove-Item .\target\* -Recurse` 在空目录/变量为空时会退化为当前目录 → 禁止通配符递归。
- 网络盘 / 映射盘符（`Z:\`）可能是生产共享目录 → 视为 CRITICAL，不自动处理。
- 在 OneDrive / 同步目录删除会同步删除云端 → 先在输出里说明。
- 路径以空格结尾、含 Unicode 全角字符 → 打印时用引号包裹并提示用户核对。
