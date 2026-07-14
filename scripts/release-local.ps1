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

function Refresh-Path {
    $machine = [Environment]::GetEnvironmentVariable('Path', 'Machine')
    $user = [Environment]::GetEnvironmentVariable('Path', 'User')
    $env:Path = @($machine, $user, $env:Path) -join ';'
}

function Resolve-AxCommand {
    Refresh-Path
    $cmd = Get-Command ax -ErrorAction SilentlyContinue
    if ($cmd) { return $cmd.Source }

    $candidates = @(
        (Join-Path $env:USERPROFILE '.cargo\bin\ax.exe')
        (Join-Path $env:LOCALAPPDATA 'ax\current\bin\ax.exe')
        (Join-Path $root 'target-dev\release\ax.exe')
        (Join-Path $root 'target-ui\release\ax.exe')
    ) | Where-Object { $_ -and (Test-Path $_) }

    foreach ($path in $candidates) {
        $dir = Split-Path -Parent $path
        if ($dir -and ($env:Path -split ';' -notcontains $dir)) {
            $env:Path = "$dir;$env:Path"
        }
        if (Test-Path $path) { return $path }
    }

    return $null
}

function Stop-AllAxProcesses {
    param(
        [string]$Reason = 'shutdown'
    )

    if ($Reason -eq 'shutdown') {
        Write-Step 'Graceful ax daemon stop'
    } else {
        Write-Host "  Stopping ax ($Reason)..." -ForegroundColor DarkGray
    }

    $axCandidates = @(
        $(Resolve-AxCommand)
        (Join-Path $root 'target-dev\release\ax.exe')
        (Join-Path $root 'target-ui\release\ax.exe')
        (Join-Path $env:USERPROFILE '.cargo\bin\ax.exe')
        (Join-Path $env:LOCALAPPDATA 'ax\current\bin\ax.exe')
        (Join-Path $env:LOCALAPPDATA 'ax\current\ax.exe')
    ) | Where-Object { $_ -and (Test-Path $_) } | Select-Object -Unique

    foreach ($axPath in $axCandidates) {
        & $axPath daemon stop 2>$null
    }
    Start-Sleep -Milliseconds 400

    if ($Reason -eq 'shutdown') {
        Write-Step 'Hard shutdown - killing all ax.exe'
    }

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

    if ($Reason -eq 'shutdown') {
        Write-Host "All ax processes stopped." -ForegroundColor Green
    }
}

function Copy-AxReleaseBinary {
    param(
        [Parameter(Mandatory = $true)][string]$Source,
        [Parameter(Mandatory = $true)][string]$Destination,
        [int]$MaxAttempts = 5
    )

    for ($attempt = 1; $attempt -le $MaxAttempts; $attempt++) {
        try {
            Copy-Item -LiteralPath $Source -Destination $Destination -Force -ErrorAction Stop
            return
        } catch {
            if ($attempt -ge $MaxAttempts) {
                throw "Could not copy ax.exe to $Destination after $MaxAttempts attempts: $($_.Exception.Message)"
            }
            Write-Host "  Locked: $Destination (attempt $attempt/$MaxAttempts)" -ForegroundColor Yellow
            Stop-AllAxProcesses -Reason "unlock copy target (attempt $attempt)"
            Start-Sleep -Milliseconds (400 * $attempt)
        }
    }
}

function Use-BuildPath {
    # Avoid PATH truncation on Windows when Refresh-Path inflates User PATH (>20KB).
    $machine = [Environment]::GetEnvironmentVariable('Path', 'Machine')
    $cargo = Join-Path $env:USERPROFILE '.cargo\bin'
    $npmDir = $env:ProgramFiles
    if ($npmDir) {
        $env:Path = "$cargo;$npmDir\nodejs;$machine"
    } else {
        $env:Path = "$cargo;$machine"
    }
}

function Verify-AxBinarySync {
    param(
        [Parameter(Mandatory = $true)][string]$Source,
        [Parameter(Mandatory = $true)][string[]]$Targets
    )
    if (-not (Test-Path $Source)) {
        throw "Source binary missing: $Source"
    }
    $srcHash = (Get-FileHash -Algorithm SHA256 $Source).Hash
    $srcSize = (Get-Item $Source).Length
    $checked = 0
    $failed = @()

    foreach ($target in $Targets) {
        if (-not (Test-Path $target)) {
            Write-Host "  skip (missing): $target" -ForegroundColor DarkYellow
            continue
        }
        $checked++
        $item = Get-Item $target
        $hash = (Get-FileHash -Algorithm SHA256 $target).Hash
        if ($hash -ne $srcHash) {
            $failed += $target
            Write-Host "  STALE: $target ($($item.Length) bytes, $($item.LastWriteTime))" -ForegroundColor Red
        } else {
            Write-Host "  OK:    $target" -ForegroundColor Green
        }
    }

    if ($failed.Count -gt 0) {
        throw @(
            "ax.exe hash mismatch - copy manually from:",
            "  $Source",
            "  ($srcSize bytes, SHA256 $($srcHash.Substring(0, 16))...)",
            "Stale:",
            ($failed | ForEach-Object { "  $_" })
        ) -join "`n"
    }

    Write-Host "All $checked ax.exe copy/copies match release build (SHA256 $($srcHash.Substring(0, 16)))" -ForegroundColor Green
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
    Use-BuildPath
    cargo build --release -p ax-cli
    if ($LASTEXITCODE -ne 0) { exit $LASTEXITCODE }

    $built = Join-Path $root 'target-dev\release\ax.exe'
    if (-not (Test-Path $built)) {
        throw "Build failed - ax.exe not found at $built"
    }
    Write-Host "Built: $built" -ForegroundColor Green
}

if (-not $SkipInstall) {
    # MCP / ax web can respawn during the long cargo build - kill again before install/copy.
    Stop-AllAxProcesses -Reason 'pre-install sync'

    Write-Step 'cargo install --path crates/ax-cli --force'
    Use-BuildPath
    cargo install --path crates/ax-cli --force
    if ($LASTEXITCODE -ne 0) { exit $LASTEXITCODE }

    $built = Join-Path $root 'target-dev\release\ax.exe'
    $appDataRoot = Join-Path $env:LOCALAPPDATA 'ax\current'
    $appDataBin = Join-Path $appDataRoot 'bin\ax.exe'
    $appDataExe = Join-Path $appDataRoot 'ax.exe'
    $cargoBin = Join-Path $env:USERPROFILE '.cargo\bin\ax.exe'
    if (Test-Path $built) {
        foreach ($pair in @(
            @{ Label = $appDataBin; Path = $appDataBin; NeedParent = $true }
            @{ Label = $appDataExe; Path = $appDataExe; NeedParent = $false }
            @{ Label = $cargoBin; Path = $cargoBin; NeedParent = $true }
        )) {
            $dest = $pair.Path
            $parentOk = if ($pair.NeedParent) { Test-Path (Split-Path $dest -Parent) } else { Test-Path $appDataRoot }
            if ($parentOk) {
                Write-Step "Sync release build -> $dest"
                Copy-AxReleaseBinary -Source $built -Destination $dest
            }
        }
    }

    $bin = Resolve-AxCommand
    if (-not $bin) {
        throw 'cargo install finished but ax is not on PATH. Open a new shell or add ~/.cargo/bin to PATH.'
    }
    $ver = & $bin --version
    Write-Host $ver -ForegroundColor Green
    Write-Host "Installed: $bin" -ForegroundColor Green

    if (Test-Path $built) {
        Write-Step 'Verify all ax.exe copies match release build'
        Verify-AxBinarySync -Source $built -Targets @(
            $cargoBin
            $appDataBin
            $appDataExe
        )
    }
}

Write-Host ""
Write-Host "Restart ax MCP in Cursor (Settings -> MCP) to pick up the new binary." -ForegroundColor Yellow
