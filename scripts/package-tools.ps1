param(
    [string]$Compiler = 'C:\complier\arm-gnu-toolchain-15.2.rel1-mingw-w64-i686-arm-none-eabi\bin\arm-none-eabi-gcc.exe',
    [string]$Make = 'C:\complier\mingw64\bin\mingw32-make.exe',
    [string]$Destination = 'dist\simple-embedded',
    [string]$SourceExe = 'target\release\simple-embedded.exe'
)

$ErrorActionPreference = 'Stop'

foreach ($path in @($Compiler, $Make, $SourceExe)) {
    if (-not (Test-Path -LiteralPath $path -PathType Leaf)) {
        throw "File not found: $path"
    }
}

$compiler = (Resolve-Path -LiteralPath $Compiler).Path
$make = (Resolve-Path -LiteralPath $Make).Path
$sourceExe = (Resolve-Path -LiteralPath $SourceExe).Path
$compilerBin = Split-Path -Parent $compiler
$compilerRoot = if ((Split-Path -Leaf $compilerBin) -ieq 'bin') {
    Split-Path -Parent $compilerBin
} else {
    $compilerBin
}
$makeDir = Split-Path -Parent $make
$destination = [System.IO.Path]::GetFullPath((Join-Path (Get-Location) $Destination))
$tools = Join-Path $destination 'tools'
$compilerDest = Join-Path $tools 'compiler'
$makeDest = Join-Path $tools 'make'

New-Item -ItemType Directory -Force -Path $compilerDest, $makeDest | Out-Null
Copy-Item -LiteralPath $sourceExe -Destination (Join-Path $destination 'simple-embedded.exe') -Force

Write-Output "Copying compiler: $compilerRoot"
& robocopy $compilerRoot $compilerDest /E /MT:8 /NFL /NDL /NJH /NJS /NP | Out-Null
if ($LASTEXITCODE -ge 8) {
    throw "Compiler copy failed, robocopy exit code $LASTEXITCODE"
}

Write-Output "Copying make: $make"
$makeName = Split-Path -Leaf $make
Copy-Item -LiteralPath $make -Destination (Join-Path $makeDest $makeName) -Force
if ($makeName -ine 'make.exe') {
    Copy-Item -LiteralPath $make -Destination (Join-Path $makeDest 'make.exe') -Force
}
Get-ChildItem -LiteralPath $makeDir -Filter '*.dll' -File | ForEach-Object {
    Copy-Item -LiteralPath $_.FullName -Destination (Join-Path $makeDest $_.Name) -Force
}

$compilerRelative = $compiler.Substring($compilerRoot.Length).TrimStart('\', '/') -replace '\\', '/'
$manifest = [ordered]@{
    compiler = "compiler/$compilerRelative"
    make = 'make/make.exe'
} | ConvertTo-Json
$utf8 = New-Object System.Text.UTF8Encoding($false)
[System.IO.File]::WriteAllText((Join-Path $tools 'toolchain.json'), $manifest, $utf8)

$total = (Get-ChildItem -LiteralPath $tools -Recurse -File | Measure-Object Length -Sum).Sum
Write-Output ("Done: {0}" -f $destination)
Write-Output ("Bundled tools size: {0:N0} MB" -f ($total / 1MB))
Write-Output 'Run dist\simple-embedded\simple-embedded.exe to use the bundled toolchain.'
