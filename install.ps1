# install.ps1 — One-line installer for G-Type on Windows.
# Usage: irm https://raw.githubusercontent.com/IntelligenzaArtificiale/g-type/main/install.ps1 | iex

$ErrorActionPreference = "Stop"

$Repo = "IntelligenzaArtificiale/g-type"
$BinName = "g-type.exe"
$InstallDir = "$env:LOCALAPPDATA\g-type"
$ConfigDir = "$env:APPDATA\g-type"
$ConfigFile = "$ConfigDir\config.toml"

function Write-Info($msg)  { Write-Host "[INFO]  $msg" -ForegroundColor Cyan }
function Write-Ok($msg)    { Write-Host "[OK]    $msg" -ForegroundColor Green }
function Write-Warn($msg)  { Write-Host "[WARN]  $msg" -ForegroundColor Yellow }
function Write-Fail($msg)  { Write-Host "[FAIL]  $msg" -ForegroundColor Red; exit 1 }

# Detect architecture
function Get-Platform {
    $arch = [System.Runtime.InteropServices.RuntimeInformation]::OSArchitecture
    switch ($arch) {
        "X64"   { return "windows-x86_64" }
        "Arm64" { return "windows-aarch64" }
        default { Write-Fail "Unsupported architecture: $arch" }
    }
}

# Get latest release version
function Get-LatestVersion {
    $url = "https://api.github.com/repos/$Repo/releases/latest"
    try {
        $response = Invoke-RestMethod -Uri $url -Method Get
        return $response.tag_name
    }
    catch {
        Write-Fail "Could not fetch latest release. Check https://github.com/$Repo/releases"
    }
}

# Download binary
function Install-Binary($version, $platform) {
    $assetName = "g-type-${platform}.exe"
    $url = "https://github.com/$Repo/releases/download/$version/$assetName"

    Write-Info "Downloading G-Type $version for $platform..."

    if (-not (Test-Path $InstallDir)) {
        New-Item -ItemType Directory -Path $InstallDir -Force | Out-Null
    }

    $outPath = Join-Path $InstallDir $BinName

    try {
        Invoke-WebRequest -Uri $url -OutFile $outPath -UseBasicParsing
    }
    catch {
        Write-Fail "Download failed. URL: $url"
    }

    Write-Ok "Binary installed to $outPath"
}

# Add to PATH
function Add-ToPath {
    $currentPath = [Environment]::GetEnvironmentVariable("PATH", "User")
    if ($currentPath -notlike "*$InstallDir*") {
        [Environment]::SetEnvironmentVariable("PATH", "$InstallDir;$currentPath", "User")
        $env:PATH = "$InstallDir;$env:PATH"
        Write-Ok "Added $InstallDir to user PATH"
    }
    else {
        Write-Ok "$InstallDir already in PATH"
    }
}

# Setup config — delegates to the binary's built-in wizard
function Setup-Config {
    if (Test-Path $ConfigFile) {
        Write-Ok "Config already exists at $ConfigFile"
        return
    }

    Write-Host ""
    Write-Host "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━" -ForegroundColor Cyan
    Write-Host "  Running first-time setup..." -ForegroundColor Cyan
    Write-Host "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━" -ForegroundColor Cyan
    Write-Host ""

    # Use the binary itself to run the interactive setup wizard
    $binPath = Join-Path $InstallDir $BinName
    & $binPath setup
}

# Main
function Main {
    Write-Host ""
    Write-Host "╔══════════════════════════════════════╗" -ForegroundColor Green
    Write-Host "║       G-Type Installer v1.0          ║" -ForegroundColor Green
    Write-Host "║  Global Voice Dictation Daemon       ║" -ForegroundColor Green
    Write-Host "╚══════════════════════════════════════╝" -ForegroundColor Green
    Write-Host ""

    $platform = Get-Platform
    Write-Info "Detected platform: $platform"

    $version = Get-LatestVersion
    Write-Info "Latest version: $version"

    Install-Binary $version $platform
    Add-ToPath
    Setup-Config

    Write-Host ""
    Write-Host "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━" -ForegroundColor Green
    Write-Host "  Installation complete!" -ForegroundColor Green
    Write-Host "  Run 'g-type' to start the daemon." -ForegroundColor Green
    Write-Host "  Hold your hotkey (default: CTRL+SHIFT+SPACE) to dictate." -ForegroundColor Green
    Write-Host "" -ForegroundColor Green
    Write-Host "  Useful commands:" -ForegroundColor Green
    Write-Host "    g-type setup     Re-run setup wizard" -ForegroundColor Green
    Write-Host "    g-type set-key   Update API key" -ForegroundColor Green
    Write-Host "    g-type config    Show config path" -ForegroundColor Green
    Write-Host "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━" -ForegroundColor Green
    Write-Host ""
}

Main
