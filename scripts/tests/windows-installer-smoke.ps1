param(
    [Parameter(Mandatory)][string]$ReleaseId,
    [Parameter(Mandatory)][string]$ReleaseVersion
)

$ErrorActionPreference = "Stop"
if (-not $IsWindows -or $env:CI -cne "true" -or $env:GITHUB_ACTIONS -cne "true" -or
        $env:RUNNER_ENVIRONMENT -cne "github-hosted") {
    throw "Installer smoke requires CI=true on a disposable GitHub-hosted Windows runner."
}
$RepoRoot = Split-Path -Parent (Split-Path -Parent $PSScriptRoot)
$MetadataJson = & python (Join-Path $RepoRoot "scripts/release-metadata.py") --release-id $ReleaseId --release-version $ReleaseVersion
if ($LASTEXITCODE -ne 0) { throw "Invalid installer smoke release metadata." }
$Metadata = $MetadataJson | ConvertFrom-Json
$MsiPath = Join-Path $RepoRoot "dist/release/windows/Advanced-Show-Control_${ReleaseId}_Windows_x64.msi"
$PackageExe = Join-Path $RepoRoot "dist/pack/windows/app/advanced-show-control.exe"
$TestRoot = Join-Path $RepoRoot "dist/installer-smoke"
$InstallDir = Join-Path $TestRoot "install"
$RealInstallDir = Join-Path ([Environment]::GetFolderPath("LocalApplicationData")) "com.advancedshowcontrol.app"
$OldInstallDir = Join-Path ([Environment]::GetFolderPath("LocalApplicationData")) "Programs/Advanced Show Control"
$Shortcut = Join-Path ([Environment]::GetFolderPath("Programs")) "Advanced Show Control.lnk"
$RealDataDir = Join-Path ([Environment]::GetFolderPath("ApplicationData")) "com.advancedshowcontrol.app"
$DataDir = Join-Path $TestRoot "isolated-app-data/com.advancedshowcontrol.app"
$Installer = New-Object -ComObject WindowsInstaller.Installer
$Database = $Installer.OpenDatabase($MsiPath, 0)
$View = $Database.OpenView('SELECT `Property`, `Value` FROM `Property`')
$Properties = @{}
try {
    $View.Execute()
    while ($Record = $View.Fetch()) { $Properties[$Record.StringData(1)] = $Record.StringData(2) }
} finally { $View.Close() }
if ($Properties.ALLUSERS -or $Properties.WixAppFolder -ne "WixPerUserFolder" -or
        $Properties.ApplicationFolderName -ne "com.advancedshowcontrol.app" -or
        $Properties.ProductVersion -ne $Metadata.msi_version -or -not $Properties.UpgradeCode) {
    throw "Velopack MSI has incorrect per-user scope, identity or numeric version."
}
if ($Database.SummaryInformation(0).Property(7) -notmatch '^x64;') { throw "MSI must target x64." }
$ProductCode = $Properties.ProductCode
if ((Test-Path $TestRoot) -or (Test-Path $RealInstallDir) -or (Test-Path $OldInstallDir) -or
        (Test-Path $Shortcut) -or (Test-Path $RealDataDir) -or
        (Test-Path "HKCU:/Software/Microsoft/Windows/CurrentVersion/Uninstall/MSI:com.advancedshowcontrol.app") -or
        $Installer.RelatedProducts($Properties.UpgradeCode).Count -gt 0) {
    throw "Existing installation or production app data detected; refusing to touch this user profile."
}
if ([IO.Path]::GetFullPath($DataDir) -eq [IO.Path]::GetFullPath($RealDataDir)) {
    throw "Smoke fixture must not use production application data."
}
if (Get-Process -Name "advanced-show-control", "Advanced Show Control" -ErrorAction SilentlyContinue) {
    throw "App is running; refusing installer smoke."
}
New-Item -ItemType Directory -Force -Path $DataDir | Out-Null
$Sentinels = @("settings.json", "logs/diagnostics-smoke.jsonl", "shows/smoke.ascs")
foreach ($Relative in $Sentinels) {
    $Path = Join-Path $DataDir $Relative
    New-Item -ItemType Directory -Force -Path (Split-Path $Path) | Out-Null
    [IO.File]::WriteAllText($Path, "installer smoke preserved data")
}

$Imports = & dumpbin /dependents $PackageExe
if ($LASTEXITCODE -ne 0) { throw "dumpbin import inspection failed." }
if (($Imports -join "`n") -match '(?im)^\s*(?:VCRUNTIME\d*[^\s]*|MSVCP\d*[^\s]*|CONCRT\d*[^\s]*|api-ms-win-crt-[^\s]*)\.dll\s*$') {
    throw "Release executable still imports a dynamic MSVC CRT."
}
$Imports | Set-Content (Join-Path $TestRoot "executable-imports.txt")

function Invoke-Msi {
    param([string]$Operation, [string]$PathOrCode, [string]$LogName)
    $Arguments = @($Operation, "`"$PathOrCode`"", "/qn", "/norestart", "/l*v", "`"$(Join-Path $TestRoot "$LogName.log")`"")
    if ($Operation -eq "/i") { $Arguments += "VELOPACK_INSTALLDIR=`"$InstallDir`"" }
    $Process = Start-Process msiexec.exe -ArgumentList $Arguments -Wait -PassThru
    if ($Process.ExitCode -ne 0) { throw "MSI $LogName failed with $($Process.ExitCode). See dist/installer-smoke/$LogName.log" }
}

function Assert-PreservedData {
    foreach ($Relative in $Sentinels) {
        if ([IO.File]::ReadAllText((Join-Path $DataDir $Relative)) -cne "installer smoke preserved data") {
            throw "Installer changed the isolated user data fixture."
        }
    }
    if (Test-Path $RealDataDir) { throw "Installer lifecycle hook unexpectedly created production app data." }
}

try {
    Invoke-Msi "/i" $MsiPath "install"
    if ($Installer.ProductState($ProductCode) -ne 5) { throw "MSI product is not installed for the current user." }
    foreach ($Relative in @("current/advanced-show-control.exe", "current/sq.version", "Update.exe", "Advanced Show Control.exe", ".msi-installed")) {
        if (-not (Test-Path (Join-Path $InstallDir $Relative))) { throw "Missing installed Velopack resource: $Relative" }
    }
    if ((Get-FileHash (Join-Path $InstallDir "current/advanced-show-control.exe")).Hash -ne (Get-FileHash $PackageExe).Hash) {
        throw "Installed executable differs from the packaged application."
    }
    [xml]$VersionXml = [IO.File]::ReadAllText((Join-Path $InstallDir "current/sq.version"))
    $InstalledMetadata = $VersionXml.package.metadata
    if ($InstalledMetadata.id -ne "com.advancedshowcontrol.app" -or $InstalledMetadata.version -ne $ReleaseVersion -or
            $InstalledMetadata.channel -ne $Metadata.windows_channel) {
        throw "Installed Velopack metadata differs from this release."
    }
    if (-not (Test-Path $Shortcut)) { throw "Start menu shortcut missing." }
    $Shell = New-Object -ComObject WScript.Shell
    if ($Shell.CreateShortcut($Shortcut).TargetPath -ne (Join-Path $InstallDir "Advanced Show Control.exe")) {
        throw "Start menu shortcut does not target the Velopack launcher."
    }
    Assert-PreservedData
    Invoke-Msi "/x" $ProductCode "uninstall"
    if ($Installer.ProductState($ProductCode) -eq 5 -or (Test-Path $InstallDir) -or (Test-Path $Shortcut)) {
        throw "Uninstall left installed Velopack resources."
    }
    Assert-PreservedData
    Write-Output "Velopack MSI scope, tables, install/uninstall, updater layout and isolated data smoke passed."
} finally {
    # Only remove the product created by this test, never arbitrary installed products.
    if ($Installer.ProductState($ProductCode) -eq 5) { Invoke-Msi "/x" $ProductCode "cleanup" }
}
