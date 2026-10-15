#requires -Version 5.1
<#
.SYNOPSIS
  Build release binaries and pack them as dist\splinely-vX.Y.Z-<arch>-windows.zip (+ .sha256).
.DESCRIPTION
  Windows counterpart of scripts/package-release.sh. Run it in PowerShell on Windows with the
  MSVC Rust toolchain. Prints the archive path.
  NOTE: configured and type-checked from Linux only; not yet run on real Windows hardware.
.PARAMETER NoBuild
  Skip `cargo build` and package what is already in target\release.
.PARAMETER Target
  Optional Rust triple (default: host, e.g. x86_64-pc-windows-msvc).
#>
param([switch]$NoBuild, [string]$Target = '')
$ErrorActionPreference = 'Stop'
Set-Location (Join-Path $PSScriptRoot '..')

$version = (Select-String -Path 'Cargo.toml' -Pattern '^version = "(.*)"' | Select-Object -First 1).Matches[0].Groups[1].Value
$triple = $Target
if (-not $triple) { $triple = (rustc -vV | Select-String '^host: (.*)').Matches[0].Groups[1].Value }
$arch = $triple.Split('-')[0]
if ($arch -eq 'amd64') { $arch = 'x86_64' }
$name = "splinely-v$version-$arch-windows"
$targetRoot = if ($env:CARGO_TARGET_DIR) { $env:CARGO_TARGET_DIR } else { 'target' }
$out = if ($Target) { Join-Path (Join-Path $targetRoot $Target) 'release' } else { Join-Path $targetRoot 'release' }

if (-not $NoBuild) {
  $cargoArgs = @('build', '--release', '--locked', '--workspace')
  if ($Target) { $cargoArgs += @('--target', $Target) }
  & cargo @cargoArgs
  if ($LASTEXITCODE -ne 0) { throw "cargo build failed ($LASTEXITCODE)" }
}

$stage = Join-Path 'dist' $name
$zip = Join-Path 'dist' "$name.zip"
Remove-Item -Recurse -Force -ErrorAction SilentlyContinue $stage, $zip, "$zip.sha256"
New-Item -ItemType Directory -Force -Path "$stage\bin", "$stage\lib", "$stage\include" | Out-Null

# splinely.exe is the desktop editor (the cargo binary is called `splinely`).
foreach ($bin in 'splinely', 'splinely-cli', 'splinely-viewer', 'splinely-runtime') {
  Copy-Item (Join-Path $out "$bin.exe") "$stage\bin\"
}
# C library: DLL for the loader, import library (.dll.lib on MSVC, .dll.a on GNU) for linking.
Copy-Item (Join-Path $out 'splinely_runtime.dll') "$stage\lib\"
foreach ($import in 'splinely_runtime.dll.lib', 'libsplinely_runtime.dll.a') {
  $path = Join-Path $out $import
  if (Test-Path $path) { Copy-Item $path "$stage\lib\" }
}
Copy-Item 'crates\splinely_runtime\include\splinely_runtime.h' "$stage\include\"
Copy-Item 'LICENSE', 'llms.txt', 'LLM.md', 'CLI.md', 'RUNTIME_API.md', 'CORE_API.md', 'docs\INSTALL-BINARIES.md' $stage
Copy-Item (Get-ChildItem 'README*.md') $stage
Copy-Item -Recurse 'prompts', 'examples' $stage
Remove-Item -Recurse -Force -ErrorAction SilentlyContinue "$stage\examples\native-runtime\__pycache__"

# ZipFile (not Compress-Archive): entries use "/" separators, so the zip also unpacks on Linux/macOS.
Add-Type -AssemblyName System.IO.Compression.FileSystem
[System.IO.Compression.ZipFile]::CreateFromDirectory(
  (Resolve-Path $stage).Path, (Join-Path (Resolve-Path 'dist').Path "$name.zip"),
  [System.IO.Compression.CompressionLevel]::Optimal, $true)

# Same "<hash>  <file>" format as sha256sum, lowercase hex, LF line ending.
$hash = (Get-FileHash -Algorithm SHA256 $zip).Hash.ToLowerInvariant()
[System.IO.File]::WriteAllText((Join-Path (Resolve-Path 'dist').Path "$name.zip.sha256"), "$hash  $name.zip`n")
Write-Output $zip
