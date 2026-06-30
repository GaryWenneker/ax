# Hard-stop ax, force a clean release build, install to ~/.cargo/bin
#
# Usage:
#   .\scripts\release-local.ps1                 # kill + clean + build + install
#   .\scripts\release-local.ps1 -SkipClean      # kill + build + install (no cargo clean)
#   .\scripts\release-local.ps1 -SkipInstall    # kill + clean + build only
#   .\scripts\release-local.ps1 -SkipBuild      # kill + install only (same as reinstall-cli.ps1)
#
param(
    [switch]$SkipBuild,
    [switch]$SkipClean,
    [switch]$SkipInstall,
    [switch]$SkipKill
)

$ErrorActionPreference = 'Stop'
$root = Split-Path -Parent $PSScriptRoot
Set-Location $root

# Respect repo .cargo/config.toml (target-dev); ignore shell override to target-ui.
if ($env:CARGO_TARGET_DIR -and $env:CARGO_TARGET_DIR -ne 'target-dev') {
    Write-Host "Clearing CARGO_TARGET_DIR=$($env:CARGO_TARGET_DIR) -> target-dev" -ForegroundColor Yellow
}
$env:CARGO_TARGET_DIR = 'target-dev'

function Write-Step {
    param([string]$Message)
    Write-Host ""
    Write-Host "==> $Message" -ForegroundColor Cyan
}

function Stop-AllAxProcesses {
    Write-Step 'Graceful ax daemon stop'
    $axCmd = Get-Command ax -ErrorAction SilentlyContinue
    $axCandidates = @(
        $(if ($axCmd) { $axCmd.Source })
        (Join-Path $root 'target-dev\release\ax.exe')
        (Join-Path $root 'target-ui\release\ax.exe')
        (Join-Path $env:USERPROFILE '.cargo\bin\ax.exe')
    ) | Where-Object { $_ -and (Test-Path $_) } | Select-Object -Unique

    foreach ($axPath in $axCandidates) {
        & $axPath daemon stop 2>$null
    }
    Start-Sleep -Milliseconds 400

    Write-Step 'Hard shutdown - killing all ax.exe'
    $selfPid = $PID
    $procs = @(
        Get-CimInstance Win32_Process -Filter "Name = 'ax.exe'" -ErrorAction SilentlyContinue
    ) | Where-Object { $_ -and $_.ProcessId -ne $selfPid }

    foreach ($p in $procs) {
        $line = if ($p.CommandLine) { $p.CommandLine.Trim() } else { '(no cmdline)' }
        Write-Host "  Stop-Process -Force PID $($p.ProcessId): $line" -ForegroundColor DarkGray
        Stop-Process -Id $p.ProcessId -Force -ErrorAction SilentlyContinue
    }

    Start-Sleep -Milliseconds 600

    Get-Process -Name ax -ErrorAction SilentlyContinue |
        Where-Object { $_.Id -ne $selfPid } |
        ForEach-Object {
            Write-Host "  Second pass PID $($_.Id)" -ForegroundColor DarkGray
            Stop-Process -Id $_.Id -Force -ErrorAction SilentlyContinue
        }

    Start-Sleep -Milliseconds 400

    $remaining = @(Get-Process -Name ax -ErrorAction SilentlyContinue | Where-Object { $_.Id -ne $selfPid })
    if ($remaining.Count -gt 0) {
        throw "Could not stop ax (still running: $($remaining.Id -join ', ')). Close Cursor MCP manually and retry."
    }
    Write-Host "All ax processes stopped." -ForegroundColor Green
}

if (-not $SkipKill) {
    Stop-AllAxProcesses
}

if (-not $SkipBuild) {
    if (-not $SkipClean) {
        Write-Step 'cargo clean -p ax-cli (forced rebuild)'
        cargo clean -p ax-cli
        if ($LASTEXITCODE -ne 0) { exit $LASTEXITCODE }
    }

    Write-Step 'cargo build --release -p ax-cli'
    cargo build --release -p ax-cli
    if ($LASTEXITCODE -ne 0) { exit $LASTEXITCODE }

    $built = Join-Path $root 'target-dev\release\ax.exe'
    if (-not (Test-Path $built)) {
        throw "Build failed - ax.exe not found at $built"
    }
    Write-Host "Built: $built" -ForegroundColor Green
}

if (-not $SkipInstall) {
    Write-Step 'cargo install --path crates/ax-cli --force'
    cargo install --path crates/ax-cli --force
    if ($LASTEXITCODE -ne 0) { exit $LASTEXITCODE }

    $bin = (Get-Command ax -ErrorAction Stop).Source
    $ver = & ax --version
    Write-Host $ver -ForegroundColor Green
    Write-Host "Installed: $bin" -ForegroundColor Green
}

Write-Host ""
Write-Host "Restart ax MCP in Cursor (Settings -> MCP) to pick up the new binary." -ForegroundColor Yellow
