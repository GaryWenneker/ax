# Markdown editor caret/overlay metric gauntlet (Command Center).
#
# Tests this checkout's build: it starts $env:AX_BIN (default target-dev/debug/ax,
# ax.exe on Windows) as `ax web` on a free port, or on $env:AX_WEB_PORT when set,
# and stops it on exit. A port that already answers is an error, so an installed
# ax web is never tested by mistake (and `ax web` would stop whatever listens there).
$ErrorActionPreference = 'Stop'
$root = Split-Path -Parent $PSScriptRoot
Set-Location $root
$webUi = Join-Path $root 'crates/ax-web/web-ui'
$exe = if ($IsWindows -or $PSVersionTable.PSEdition -eq 'Desktop') { 'ax.exe' } else { 'ax' }
$axBin = if ($env:AX_BIN) { $env:AX_BIN } else { Join-Path $root "target-dev/debug/$exe" }
if (-not (Test-Path -LiteralPath $axBin -PathType Leaf)) {
    throw "$axBin is missing; build it with cargo build -p ax-cli"
}
if ($env:AX_WEB_PORT) {
    $port = $env:AX_WEB_PORT
} else {
    $probe = [System.Net.Sockets.TcpListener]::new([System.Net.IPAddress]::Loopback, 0)
    $probe.Start()
    $port = [string]$probe.LocalEndpoint.Port
    $probe.Stop()
}
$url = "http://127.0.0.1:$port"

Write-Host '== tsc =='
Push-Location $webUi
npx tsc --noEmit
if ($LASTEXITCODE -ne 0) { exit $LASTEXITCODE }
Pop-Location

Write-Host "== start this checkout's ax web =="
$answered = $true
try {
    Invoke-WebRequest -Uri "$url/" -UseBasicParsing -TimeoutSec 2 | Out-Null
} catch { $answered = $false }
if ($answered) { throw "something already answers on $url; free the port or set AX_WEB_PORT" }

$start = @{ FilePath = $axBin; ArgumentList = @('web', $root, '--port', $port); WorkingDirectory = $root; PassThru = $true }
# -WindowStyle exists only on Windows.
if ($IsWindows -or $PSVersionTable.PSEdition -eq 'Desktop') { $start.WindowStyle = 'Hidden' }
$web = Start-Process @start
try {
    Write-Host "started $axBin web on $port (pid $($web.Id))"
    $ok = $false
    # A foreach statement, not ForEach-Object: `break` must end the polling.
    foreach ($attempt in 1..20) {
        if ($web.HasExited) { throw "ax web exited with code $($web.ExitCode)" }
        try {
            Invoke-WebRequest -Uri "$url/" -UseBasicParsing -TimeoutSec 1 | Out-Null
            $ok = $true
            break
        } catch { Start-Sleep -Milliseconds 500 }
    }
    if (-not $ok) { throw "ax web did not start on $url" }

    Write-Host '== playwright md-editor-caret (desktop-chrome) =='
    Push-Location $webUi
    $env:AX_WEB_URL = $url
    npx playwright test e2e/md-editor-caret.spec.ts --project=desktop-chrome
    if ($LASTEXITCODE -ne 0) { exit $LASTEXITCODE }
    Pop-Location
} finally {
    if (-not $web.HasExited) {
        Stop-Process -Id $web.Id -Force -ErrorAction SilentlyContinue
        $web.WaitForExit(10000) | Out-Null
    }
}

Write-Host 'gauntlet-md-editor-caret: ok'
