# Copies the Visual C++ runtime DLLs (app-local deployment, which Microsoft allows) into the
# resources the Windows installer carries:
#   resources/llama  beside llama-server.exe, which looks in its own folder first
#   resources/vcrt   added to the DLL search path at startup, for whisper.cpp (see `lib.rs`)
# Usage: scripts/copy-vc-runtime.ps1 -Arch x64|arm64
param([Parameter(Mandatory = $true)][ValidateSet("x64", "arm64")][string]$Arch)
$ErrorActionPreference = "Stop"

$vswhere = "${env:ProgramFiles(x86)}\Microsoft Visual Studio\Installer\vswhere.exe"
$vs = & $vswhere -latest -products * -property installationPath
$crt = Get-ChildItem "$vs\VC\Redist\MSVC\*\$Arch\Microsoft.VC14*.CRT" -Directory |
  Sort-Object FullName -Descending | Select-Object -First 1
if (-not $crt) { throw "No Visual C++ redistributable for $Arch under $vs" }

$names = @("msvcp140.dll", "msvcp140_1.dll", "msvcp140_2.dll", "vcruntime140.dll", "vcruntime140_1.dll")
foreach ($target in @("src-tauri\resources\llama", "src-tauri\resources\vcrt")) {
  New-Item -ItemType Directory -Force -Path $target | Out-Null
  foreach ($name in $names) {
    $from = Join-Path $crt.FullName $name
    if (Test-Path $from) { Copy-Item $from $target -Force }
  }
}
Write-Host "Copied the VC runtime from $($crt.FullName)"
