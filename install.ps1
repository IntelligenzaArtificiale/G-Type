# G-Type one-command installer for Windows x86_64.
# Usage: irm https://raw.githubusercontent.com/IntelligenzaArtificiale/G-Type/main/install.ps1 | iex

Set-StrictMode -Version 2.0
$ErrorActionPreference = "Stop"
$ProgressPreference = "SilentlyContinue"

try {
    [Net.ServicePointManager]::SecurityProtocol = [Net.SecurityProtocolType]::Tls12 -bor [Net.ServicePointManager]::SecurityProtocol
} catch {}

$Repo = "IntelligenzaArtificiale/G-Type"
$BinName = "g-type.exe"
$InstallDir = Join-Path $env:LOCALAPPDATA "g-type"
$BinPath = Join-Path $InstallDir $BinName
$UserAgent = "g-type-installer"
$MinBinaryBytes = 1000000

function Write-Info($msg) { Write-Host "[INFO]  $msg" -ForegroundColor Cyan }
function Write-Ok($msg) { Write-Host "[OK]    $msg" -ForegroundColor Green }
function Write-Warn($msg) { Write-Host "[WARN]  $msg" -ForegroundColor Yellow }
function Write-Fail($msg, $err = $null) {
    Write-Host "[FAIL]  $msg" -ForegroundColor Red
    if ($err) { Write-Host "        $($err.Exception.Message)" -ForegroundColor DarkRed }
    throw $msg
}

function Get-Platform {
    $arch = [System.Runtime.InteropServices.RuntimeInformation]::OSArchitecture
    if ($arch -ne "X64") {
        Write-Fail "La release Windows precompilata è disponibile al momento solo per x86_64. Architettura rilevata: $arch"
    }
    return "windows-x86_64"
}

function Invoke-Download($url, $outPath) {
    $lastError = $null
    for ($attempt = 1; $attempt -le 5; $attempt++) {
        try {
            $params = @{
                Uri = $url
                OutFile = $outPath
                TimeoutSec = 120
                UserAgent = $UserAgent
            }
            if ($PSVersionTable.PSVersion.Major -lt 6) { $params.UseBasicParsing = $true }
            Invoke-WebRequest @params | Out-Null
            return
        } catch {
            $lastError = $_
            Remove-Item $outPath -Force -ErrorAction SilentlyContinue
            if ($attempt -lt 5) {
                Write-Warn "Download non riuscito (tentativo $attempt/5). Nuovo tentativo tra pochi secondi..."
                Start-Sleep -Seconds ([Math]::Min(2 * $attempt, 8))
            }
        }
    }
    Write-Fail "Download fallito dopo 5 tentativi: $url" $lastError
}

function Install-Binary($platform) {
    $assetName = "g-type-${platform}.exe"
    $url = "https://github.com/$Repo/releases/latest/download/$assetName"

    New-Item -ItemType Directory -Path $InstallDir -Force | Out-Null
    $tmpPath = Join-Path $InstallDir (".g-type-install-" + [Guid]::NewGuid().ToString("N") + ".tmp")
    $backupPath = "$BinPath.bak"

    try {
        Write-Info "Download dell'ultima release di G-Type per $platform..."
        Invoke-Download $url $tmpPath

        $size = (Get-Item $tmpPath).Length
        if ($size -lt $MinBinaryBytes) { throw "Download incompleto: $size byte" }

        Remove-Item $backupPath -Force -ErrorAction SilentlyContinue
        if (Test-Path $BinPath) { Move-Item $BinPath $backupPath -Force }

        try {
            Move-Item $tmpPath $BinPath -Force
        } catch {
            if ((Test-Path $backupPath) -and -not (Test-Path $BinPath)) { Move-Item $backupPath $BinPath -Force }
            throw
        }

        Remove-Item $backupPath -Force -ErrorAction SilentlyContinue
        Write-Ok "Installato in $BinPath"
    } catch {
        Remove-Item $tmpPath -Force -ErrorAction SilentlyContinue
        Write-Fail "Installazione del binario non riuscita. La versione precedente è stata preservata quando possibile." $_
    }
}

function Add-ToPath {
    $currentPath = [Environment]::GetEnvironmentVariable("PATH", "User")
    if ([string]::IsNullOrWhiteSpace($currentPath)) { $currentPath = "" }

    $entries = $currentPath -split ';' | Where-Object { $_ -and $_.Trim() }
    if ($entries -notcontains $InstallDir) {
        $newPath = if ([string]::IsNullOrWhiteSpace($currentPath)) { $InstallDir } else { "$InstallDir;$currentPath" }
        [Environment]::SetEnvironmentVariable("PATH", $newPath, "User")
        Write-Ok "Aggiunto $InstallDir al PATH utente"
    } else {
        Write-Ok "$InstallDir è già nel PATH"
    }

    if (($env:PATH -split ';') -notcontains $InstallDir) { $env:PATH = "$InstallDir;$env:PATH" }
}

function Test-Dashboard {
    try {
        $null = Invoke-WebRequest -Uri "http://127.0.0.1:9741/api/state" -UseBasicParsing -TimeoutSec 1
        return $true
    } catch { return $false }
}

function Start-GType {
    if (Test-Dashboard) {
        Write-Info "G-Type è già in esecuzione; non avvio una seconda istanza."
        return
    }

    Write-Info "Avvio G-Type..."
    Start-Process -FilePath $BinPath -WindowStyle Hidden
    Start-Sleep -Milliseconds 800
    Write-Ok "G-Type avviato. Al primo utilizzo si apre la configurazione nel browser."
}

function Main {
    Write-Host ""
    Write-Host "G-Type · installer" -ForegroundColor Green

    $platform = Get-Platform
    Write-Info "Piattaforma: $platform"

    Install-Binary $platform
    Add-ToPath
    Start-GType

    Write-Host ""
    Write-Host "Installazione completata." -ForegroundColor Green
    Write-Host "Dashboard: http://127.0.0.1:9741/"
    Write-Host "Aggiornamenti futuri: g-type upgrade"
    Write-Host ""
}

Main
