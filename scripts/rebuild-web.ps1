# Rebuild Command Center web UI and install a fresh ax binary.
#
# ax web embeds web-ui/dist at compile time (include_dir!). npm run build alone is NOT
# enough - you must rebuild ax-cli and reinstall before localhost:7070 picks up changes.
#
# Usage:
#   .\scripts\rebuild-web.ps1              # kill ax, build, install, restart ax web
#   .\scripts\rebuild-web.ps1 -SkipKill    # build + install only (ax still running)
#   .\scripts\rebuild-web.ps1 -NoWeb       # build + install, do not start ax web
#   .\scripts\rebuild-web.ps1 -Port 7070   # port for ax web (default 7070)
#
param(
    [switch]$SkipKill,
    [switch]$NoWeb,
    [int]$Port = 7070
)

$ErrorActionPreference = 'Stop'
$root = Split-Path -Parent $PSScriptRoot
Set-Location $root
$env:CARGO_TARGET_DIR = 'target-dev'

function Write-Step {
    param([string]$Message)
    Write-Host ""
    Write-Host "==> $Message" -ForegroundColor Cyan
}

function Get-DistJsBundle {
    $index = Join-Path $root 'crates\ax-web\web-ui\dist\index.html'
    if (-not (Test-Path $index)) { return $null }
    $html = Get-Content $index -Raw
    if ($html -match '/assets/(index-[^"]+\.js)') { return $Matches[1] }
    return $null
}

function Test-WebServesBundle {
    param([string]$Bundle, [int]$WebPort)
    try {
        $response = Invoke-WebRequest -Uri "http://localhost:$WebPort/" -UseBasicParsing -TimeoutSec 5
        return $response.Content -match [regex]::Escape($Bundle)
    } catch {
        return $false
    }
}

function Wait-WebServesBundle {
    param(
        [string]$Bundle,
        [int]$WebPort,
        [int]$MaxAttempts = 20
    )

    for ($attempt = 1; $attempt -le $MaxAttempts; $attempt++) {
        if (Test-WebServesBundle -Bundle $Bundle -WebPort $WebPort) {
            return $true
        }
        Start-Sleep -Seconds 1
    }
    return $false
}

Write-Step "Rebuild Command Center web UI (embed dist + reinstall ax)"

$releaseParams = @{ SkipClean = $true }
if ($SkipKill) { $releaseParams.SkipKill = $true }
& (Join-Path $PSScriptRoot 'release-local.ps1') @releaseParams

$built = Join-Path $root 'target-dev\release\ax.exe'
$distIndex = Join-Path $root 'crates\ax-web\web-ui\dist\index.html'
if (-not (Test-Path $built)) { throw "Missing $built" }
if (-not (Test-Path $distIndex)) { throw "Missing $distIndex - cargo build should have run npm run build" }

$bundle = Get-DistJsBundle
if (-not $bundle) { throw "Could not read JS bundle name from dist/index.html" }

$builtTime = (Get-Item $built).LastWriteTime
$distTime = (Get-Item $distIndex).LastWriteTime
if ($builtTime -lt $distTime) {
    throw "ax.exe ($builtTime) is older than dist/index.html ($distTime) - embed is stale"
}
Write-Host "Embedded bundle: $bundle" -ForegroundColor Green
Write-Host "ax.exe: $builtTime | dist: $distTime" -ForegroundColor Green

if ($NoWeb) {
    Write-Host ""
    Write-Host "Skip ax web. Start manually: ax web --port $Port" -ForegroundColor Yellow
    exit 0
}

Write-Step "Start ax web on port $Port"
$ax = (Get-Command ax -ErrorAction Stop).Source
$webProc = Start-Process -FilePath $ax -ArgumentList @('web', '--port', "$Port") -WorkingDirectory $root -PassThru -WindowStyle Hidden

if (-not (Wait-WebServesBundle -Bundle $bundle -WebPort $Port)) {
    if ($webProc -and -not $webProc.HasExited) {
        Stop-Process -Id $webProc.Id -Force -ErrorAction SilentlyContinue
    }
    throw "http://localhost:$Port/ does not serve $bundle within 20s - check ax web logs (PID $($webProc.Id))"
}

Write-Host ""
Write-Host "OK: http://localhost:$Port/ serves $bundle" -ForegroundColor Green
Write-Host "Hard refresh browser (Ctrl+Shift+R) if UI still looks cached." -ForegroundColor Yellow
