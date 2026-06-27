# Reinstall ax CLI to ~/.cargo/bin after a local build.
# Kills every running ax instance first so cargo install can replace ax.exe.
# Usage: .\scripts\reinstall-cli.ps1

$ErrorActionPreference = "Stop"
$root = Split-Path -Parent (Split-Path -Parent $MyInvocation.MyCommand.Path)
Set-Location $root

function Stop-AllAxProcesses {
    Write-Host "Stopping ax daemon (if reachable)..." -ForegroundColor Cyan
    $axCmd = Get-Command ax -ErrorAction SilentlyContinue
    if ($axCmd) {
        & $axCmd.Source daemon stop 2>$null
        Start-Sleep -Milliseconds 400
    }

    Write-Host "Killing all ax.exe processes..." -ForegroundColor Cyan
    $selfPid = $PID
    $parentPid = $PID

    $procs = @(
        Get-CimInstance Win32_Process -Filter "Name = 'ax.exe'" -ErrorAction SilentlyContinue
    ) | Where-Object { $_ -and $_.ProcessId -ne $selfPid -and $_.ProcessId -ne $parentPid }

    foreach ($p in $procs) {
        $line = if ($p.CommandLine) { $p.CommandLine.Trim() } else { "(no cmdline)" }
        Write-Host "  PID $($p.ProcessId): $line" -ForegroundColor DarkGray
        Stop-Process -Id $p.ProcessId -Force -ErrorAction SilentlyContinue
    }

    Start-Sleep -Milliseconds 600

    # Second pass — catch respawns / stragglers
    Get-Process -Name ax -ErrorAction SilentlyContinue |
        Where-Object { $_.Id -ne $selfPid } |
        ForEach-Object {
            Write-Host "  Force stop PID $($_.Id)" -ForegroundColor DarkGray
            Stop-Process -Id $_.Id -Force -ErrorAction SilentlyContinue
        }

    Start-Sleep -Milliseconds 400

    $remaining = @(Get-Process -Name ax -ErrorAction SilentlyContinue)
    if ($remaining.Count -gt 0) {
        Write-Host "Warning: $($remaining.Count) ax process(es) still running:" -ForegroundColor Yellow
        $remaining | ForEach-Object { Write-Host "  PID $($_.Id)" -ForegroundColor Yellow }
    }
}

Stop-AllAxProcesses

Write-Host "Installing ax-cli to cargo bin..." -ForegroundColor Cyan
cargo install --path crates/ax-cli --force
if ($LASTEXITCODE -ne 0) { exit $LASTEXITCODE }

$bin = (Get-Command ax -ErrorAction Stop).Source
$ver = & ax --version
Write-Host "$ver" -ForegroundColor Green
Write-Host "Installed: $bin" -ForegroundColor Green
