param(
    [string]$ReleaseId = "local",
    [string]$ReleaseVersion = ""
)

$ErrorActionPreference = "Stop"
$RepoRoot = Split-Path -Parent $PSScriptRoot
Set-Location $RepoRoot
if (-not $IsWindows) { throw "Windows packaging requires Windows, Python 3, MSVC x64, and .NET 8 SDK." }

$MetadataArgs = @("scripts/release-metadata.py", "--release-id", $ReleaseId)
if ($ReleaseVersion) { $MetadataArgs += @("--release-version", $ReleaseVersion) }
$MetadataJson = & python @MetadataArgs
if ($LASTEXITCODE -ne 0) { throw "Invalid Windows release metadata." }
$Metadata = $MetadataJson | ConvertFrom-Json
$ReleaseVersion = $Metadata.release_version
$BinaryName = "advanced-show-control"
$Target = "x86_64-pc-windows-msvc"
$BuildDir = Join-Path $RepoRoot "dist/build/windows"
$PackRoot = Join-Path $RepoRoot "dist/pack/windows"
$PackageDir = Join-Path $PackRoot "app"
$VpkOutput = Join-Path $PackRoot "releases"
$StageDir = Join-Path $RepoRoot "dist/release/windows"

# Explicit --target keeps these release-only CRT flags off host build scripts and proc macros.
$BuildEnvironment = @{
    CARGO_TARGET_X86_64_PC_WINDOWS_MSVC_RUSTFLAGS = "$env:CARGO_TARGET_X86_64_PC_WINDOWS_MSVC_RUSTFLAGS -C target-feature=+crt-static"
    ASC_RELEASE_ID = $ReleaseId
    ASC_RELEASE_VERSION = $ReleaseVersion
}
if ($env:RUSTFLAGS -or $env:CARGO_ENCODED_RUSTFLAGS) {
    throw "Unset RUSTFLAGS and CARGO_ENCODED_RUSTFLAGS so the Windows release can select the static CRT."
}
$PreviousEnvironment = @{}
try {
    foreach ($Name in $BuildEnvironment.Keys) {
        $PreviousEnvironment[$Name] = [Environment]::GetEnvironmentVariable($Name, "Process")
        [Environment]::SetEnvironmentVariable($Name, $BuildEnvironment[$Name], "Process")
    }
    cargo build --manifest-path app/Cargo.toml --release --target $Target --target-dir $BuildDir --bin $BinaryName
    if ($LASTEXITCODE -ne 0) { throw "Windows release build failed." }
} finally {
    foreach ($Name in $PreviousEnvironment.Keys) {
        [Environment]::SetEnvironmentVariable($Name, $PreviousEnvironment[$Name], "Process")
    }
}

Remove-Item -Recurse -Force $PackRoot -ErrorAction SilentlyContinue
Remove-Item -Recurse -Force $StageDir -ErrorAction SilentlyContinue
New-Item -ItemType Directory -Force -Path $PackageDir | Out-Null
Copy-Item (Join-Path $BuildDir "$Target/release/$BinaryName.exe") (Join-Path $PackageDir "$BinaryName.exe")
Copy-Item "LICENSE" (Join-Path $PackageDir "LICENSE.txt")
@"
Advanced Show Control ($ReleaseId / $ReleaseVersion)

Requires 64-bit Windows 10 version 1703 (build 15063) or newer. Packages are unsigned.
Use Setup.exe or the per-user MSI to install. For portable use, extract the ZIP and run
"Advanced Show Control.exe". Do not run installed and portable copies together.
Applying an update will close and restart Advanced Show Control.
Settings, diagnostic logs, and .ascs show files belong outside the installation directory.
"@ | Set-Content -Encoding UTF8 (Join-Path $PackageDir "README.txt")

& python scripts/installer/velopack.py pack --msi --msiVersion $Metadata.msi_version `
    --instLocation PerUser --runtime win10.0.15063-x64 --channel $Metadata.windows_channel `
    --packId com.advancedshowcontrol.app --packTitle "Advanced Show Control" `
    --mainExe "$BinaryName.exe" --packVersion $ReleaseVersion --packAuthors "Advanced Show Control" `
    --packDir $PackageDir --outputDir $VpkOutput --icon (Join-Path $RepoRoot "app/icons/icon.ico") `
    --instLicense (Join-Path $PackageDir "LICENSE.txt") --shortcuts StartMenuRoot
if ($LASTEXITCODE -ne 0) { throw "Windows Velopack packaging failed." }
& python scripts/installer/validate-release.py --directory $VpkOutput --platform windows `
    --release-id $ReleaseId --release-version $ReleaseVersion --stage $StageDir
if ($LASTEXITCODE -ne 0) { throw "Windows release payload validation failed." }
Write-Output "Windows release $ReleaseVersion staged at $StageDir"
