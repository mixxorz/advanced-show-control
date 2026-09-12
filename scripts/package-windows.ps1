param(
    [string]$ReleaseId = "local"
)

$ErrorActionPreference = "Stop"
$RepoRoot = Split-Path -Parent $PSScriptRoot
Set-Location $RepoRoot

$BinaryName = "advanced-show-control"
$Target = "x86_64-pc-windows-msvc"
$PackageDir = Join-Path $RepoRoot "dist/release/windows/Advanced Show Control"
$Archive = Join-Path $RepoRoot "dist/release/Advanced-Show-Control_${ReleaseId}_Windows_x64.zip"

cargo build --manifest-path app/Cargo.toml --release --target $Target --bin $BinaryName
if ($LASTEXITCODE -ne 0) {
    throw "Windows release build failed"
}

Remove-Item -Recurse -Force $PackageDir -ErrorAction SilentlyContinue
New-Item -ItemType Directory -Force -Path $PackageDir | Out-Null
Copy-Item "target/$Target/release/$BinaryName.exe" (Join-Path $PackageDir "Advanced Show Control.exe")
Copy-Item "LICENSE" (Join-Path $PackageDir "LICENSE.txt")
@"
Advanced Show Control

Run "Advanced Show Control.exe". This package is unsigned and requires Windows 10 or newer.
"@ | Set-Content -Encoding UTF8 (Join-Path $PackageDir "README.txt")

Remove-Item -Force $Archive -ErrorAction SilentlyContinue
Compress-Archive -Path $PackageDir -DestinationPath $Archive -Force
Write-Output $Archive
