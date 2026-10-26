# preview-delete.ps1
# READ-ONLY dry-run preview for a planned delete.
# This script NEVER deletes, moves, or modifies anything.
# It validates the target path, reports impact, and prints the exact command
# that a human may run later, after replying with the confirmation token.
#
# Usage:
#   pwsh -NoProfile -File preview-delete.ps1 -Path "D:\project\dist" -ProjectRoot "D:\project"
#
# Exit codes: 0 = preview printed, 2 = refused or invalid input.

[CmdletBinding()]
param(
    [Parameter(Mandatory = $true, Position = 0)]
    [string]$Path,

    [string]$ProjectRoot,

    [int]$MaxTopLevel = 20
)

Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'

# Keep the Chinese confirmation token readable on legacy Windows console hosts.
try {
    $utf8 = New-Object System.Text.UTF8Encoding $false
    [Console]::OutputEncoding = $utf8
    $OutputEncoding = $utf8
}
catch {
    # Encoding is cosmetic only; never fail the preview because of it.
}

$SensitiveExtensions = @(
    '.rs', '.toml', '.lock', '.cs', '.fs', '.ts', '.tsx', '.js', '.jsx',
    '.py', '.java', '.go', '.c', '.h', '.cpp', '.hpp', '.ps1', '.sh',
    '.json', '.yaml', '.yml', '.xml', '.db', '.sqlite', '.sql',
    '.png', '.jpg', '.jpeg', '.gif', '.svg', '.ico', '.psd'
)

$ProtectedTopLevel = @(
    'src', 'assets', 'crates', 'tests', 'benches', 'docs', '.git', '.github'
)

function Exit-Refused {
    param([string]$Message, [string[]]$Reasons = @())
    Write-Host ''
    Write-Host '============================================================' -ForegroundColor Red
    Write-Host ' REFUSED - no preview generated, nothing was deleted' -ForegroundColor Red
    Write-Host '============================================================' -ForegroundColor Red
    Write-Host " Reason: $Message" -ForegroundColor Red
    foreach ($reason in $Reasons) {
        Write-Host "   - $reason" -ForegroundColor Yellow
    }
    Write-Host ''
    exit 2
}

function Get-NormalizedPath {
    param([Parameter(Mandatory = $true)][string]$InputPath)
    $full = [System.IO.Path]::GetFullPath($InputPath)
    if (Test-Path -LiteralPath $full) {
        $full = (Get-Item -LiteralPath $full -Force).FullName
    }
    return $full.TrimEnd('\', '/')
}

function Format-Size {
    param([long]$Bytes)
    if ($Bytes -ge 1GB) { return ('{0:N2} GB' -f ($Bytes / 1GB)) }
    if ($Bytes -ge 1MB) { return ('{0:N2} MB' -f ($Bytes / 1MB)) }
    if ($Bytes -ge 1KB) { return ('{0:N2} KB' -f ($Bytes / 1KB)) }
    return "$Bytes B"
}

# Resolve project root. Default: four levels above this script
# (scripts -> safe-shell -> skills -> .opencode -> project root).
if (-not $ProjectRoot -or $ProjectRoot.Trim().Length -eq 0) {
    $ProjectRoot = Join-Path $PSScriptRoot '..\..\..\..'
}

$root = Get-NormalizedPath $ProjectRoot
if (-not (Test-Path -LiteralPath $root -PathType Container)) {
    Exit-Refused "ProjectRoot does not exist: $root"
}
if ($root -match '^[A-Za-z]:\\?$' -or $root -match '^[\\/]+$') {
    Exit-Refused "ProjectRoot must not be a drive root: $root"
}

$full = Get-NormalizedPath $Path
$separator = [System.IO.Path]::DirectorySeparatorChar
$reasons = New-Object System.Collections.Generic.List[string]

if ($full -match '^[A-Za-z]:\\?$' -or $full -match '^[\\/]+$' -or $full -match '^\\\\[^\\]+\\[^\\]+\\?$') {
    $reasons.Add('Target is a filesystem root.')
}
if ($full -eq $root) {
    $reasons.Add('Target is the project root itself.')
}
if ($full -ne $root -and -not $full.StartsWith($root + $separator, [System.StringComparison]::OrdinalIgnoreCase)) {
    $reasons.Add("Target is outside ProjectRoot ($root).")
}
if ($full -match '[*?]') {
    $reasons.Add('Target contains wildcard characters; pass a literal path.')
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
    $reasons.Add('Target path is longer than 240 characters.')
}

# Walk ancestors: a junction/symlink inside the project must not point outside it.
$cursor = $full
while ($cursor -and $cursor.Length -gt $root.Length) {
    if (Test-Path -LiteralPath $cursor) {
        $item = Get-Item -LiteralPath $cursor -Force
        if ($item.Attributes -band [System.IO.FileAttributes]::ReparsePoint) {
            $links = @()
            $targetProp = $item.PSObject.Properties['Target']
            if ($targetProp -and $item.Target) {
                $links = @($item.Target)
            }
            if ($links.Count -eq 0) {
                $reasons.Add("Cannot resolve reparse point target, refusing to continue: $cursor")
            }
            foreach ($linkTarget in $links) {
                $resolved = [System.IO.Path]::GetFullPath([string]$linkTarget)
                if (-not $resolved.StartsWith($root + $separator, [System.StringComparison]::OrdinalIgnoreCase)) {
                    $reasons.Add("Reparse point escapes ProjectRoot: $cursor -> $resolved")
                }
            }
        }
    }
    $cursor = Split-Path -Parent $cursor
}

if ($reasons.Count -gt 0) {
    Exit-Refused 'Static path validation failed.' @($reasons)
}

if (-not (Test-Path -LiteralPath $full)) {
    Write-Host "Nothing to do: target does not exist." -ForegroundColor Green
    Write-Host "  Target: $full"
    exit 0
}

$relative = $full.Substring($root.Length).TrimStart('\', '/')
$isFile = Test-Path -LiteralPath $full -PathType Leaf
$targetItem = Get-Item -LiteralPath $full -Force

$fileCount = 0
$dirCount = 0
[long]$totalBytes = 0
$sensitiveCount = 0
$linkItems = New-Object System.Collections.Generic.List[string]

if ($isFile) {
    $fileCount = 1
    $totalBytes = [long]$targetItem.Length
    if ($SensitiveExtensions -contains $targetItem.Extension.ToLowerInvariant()) { $sensitiveCount = 1 }
}
else {
    $children = Get-ChildItem -LiteralPath $full -Recurse -Force -ErrorAction SilentlyContinue
    foreach ($child in $children) {
        if ($child.PSIsContainer) {
            $dirCount++
        }
        else {
            $fileCount++
            $totalBytes += [long]$child.Length
            if ($SensitiveExtensions -contains $child.Extension.ToLowerInvariant()) { $sensitiveCount++ }
        }
        if ($child.Attributes -band [System.IO.FileAttributes]::ReparsePoint) {
            if ($linkItems.Count -lt 10) { $linkItems.Add($child.FullName) }
        }
    }
}

$topLevel = @()
if (-not $isFile) {
    $topLevel = @(Get-ChildItem -LiteralPath $full -Force -ErrorAction SilentlyContinue)
}

# Git awareness (read-only queries).
$gitTracked = 0
$gitDirty = 0
$gitState = 'unavailable'
$gitCommand = Get-Command git -ErrorAction SilentlyContinue
if ($gitCommand) {
    $inRepo = $false
    try {
        $null = & git -C $root rev-parse --is-inside-work-tree 2>$null
        $inRepo = ($LASTEXITCODE -eq 0)
    }
    catch {
        $inRepo = $false
    }

    if ($inRepo) {
        $gitState = 'ok'
        try {
            $tracked = & git -C $root ls-files -- $relative 2>$null
            if ($tracked) { $gitTracked = @($tracked).Count }
            $dirty = & git -C $root status --porcelain -- $relative 2>$null
            if ($dirty) { $gitDirty = @($dirty).Count }
        }
        catch {
            $gitState = 'query failed'
        }
    }
    else {
        $gitState = 'not a git repo'
    }
}

$firstSegment = ($relative -split '[\\/]')[0].ToLowerInvariant()
$risk = 'MEDIUM'
$riskNotes = New-Object System.Collections.Generic.List[string]

if ($gitTracked -gt 0) {
    $risk = 'HIGH'
    $riskNotes.Add("$gitTracked git-tracked file(s) would be lost (not reproducible).")
}
if ($gitDirty -gt 0) {
    $risk = 'HIGH'
    $riskNotes.Add("$gitDirty uncommitted change(s) inside the target.")
}
if ($sensitiveCount -gt 0) {
    $risk = 'HIGH'
    $riskNotes.Add("$sensitiveCount source/config/asset file(s) inside the target.")
}
if ($ProtectedTopLevel -contains $firstSegment) {
    $risk = 'HIGH'
    $riskNotes.Add("Target is a protected top-level directory: $firstSegment")
}
if ($linkItems.Count -gt 0) {
    $risk = 'HIGH'
    $riskNotes.Add("$($linkItems.Count) reparse point(s) found inside the target; verify they do not leave the project.")
}
if ($isFile) {
    $risk = 'HIGH'
    $riskNotes.Add('Target is a single file; deleting it is not a rebuild.')
}

$token = -join @([char]0x786E, [char]0x8BA4, [char]0x5220, [char]0x9664) # confirmation token

Write-Host ''
Write-Host '============================================================' -ForegroundColor Cyan
Write-Host ' SAFE DELETE PREVIEW - READ ONLY - NOTHING WAS DELETED' -ForegroundColor Cyan
Write-Host '============================================================' -ForegroundColor Cyan
Write-Host ''
Write-Host " [TARGET]       `"$full`"" -ForegroundColor Yellow
Write-Host " [PROJECT ROOT] `"$root`""
Write-Host " [RELATIVE]     $relative"
Write-Host " [RISK]         $risk"
Write-Host " [TYPE]         $(if ($isFile) { 'file' } else { 'directory' })"
Write-Host ''
Write-Host ' ---------- IMPACT ----------'
Write-Host " Files:          $fileCount"
Write-Host " Directories:    $dirCount"
Write-Host " Total size:     $(Format-Size $totalBytes)"
Write-Host " Source/config:  $sensitiveCount"
Write-Host " Git tracked:    $(if ($gitState -eq 'ok') { $gitTracked } else { $gitState })"
Write-Host " Uncommitted:    $(if ($gitState -eq 'ok') { $gitDirty } else { $gitState })"
Write-Host " Reparse points: $($linkItems.Count)"
Write-Host ''
Write-Host " ---------- TOP LEVEL (max $MaxTopLevel) ----------"
if ($isFile) {
    Write-Host " $($targetItem.Name)  ($(Format-Size ([long]$targetItem.Length)))"
}
elseif ($topLevel.Count -eq 0) {
    Write-Host ' (empty directory)'
}
else {
    $shown = 0
    foreach ($entry in $topLevel) {
        if ($shown -ge $MaxTopLevel) { break }
        $kind = if ($entry.PSIsContainer) { '<dir> ' } else { '      ' }
        Write-Host " $kind$($entry.Name)"
        $shown++
    }
    if ($topLevel.Count -gt $MaxTopLevel) {
        Write-Host " ... and $($topLevel.Count - $MaxTopLevel) more item(s)"
    }
}
if ($riskNotes.Count -gt 0) {
    Write-Host ''
    Write-Host ' ---------- RISK NOTES ----------'
    foreach ($note in $riskNotes) { Write-Host " ! $note" -ForegroundColor Yellow }
}
Write-Host ''
Write-Host ' ---------- DRY RUN (safe, prints only) ----------'
Write-Host " Remove-Item -LiteralPath `"$full`" -Recurse -Force -WhatIf"
Write-Host ''
Write-Host ' ---------- AWAITING HUMAN CONFIRMATION ----------'
Write-Host " Reply exactly: [$token]$full" -ForegroundColor Yellow
Write-Host ' No command was executed. Run the final command yourself after confirming.'
Write-Host ''
exit 0
