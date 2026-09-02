# Markdown editor caret/overlay metric gauntlet (Command Center).
$ErrorActionPreference = 'Stop'
$root = Split-Path -Parent $PSScriptRoot
Set-Location $root
$webUi = Join-Path $root 'crates\ax-web\web-ui'
$port = if ($env:AX_WEB_PORT) { $env:AX_WEB_PORT } else { '7070' }
$url = "http://127.0.0.1:$port"

Write-Host '== tsc =='
Push-Location $webUi
npx tsc --noEmit
if ($LASTEXITCODE -ne 0) { exit $LASTEXITCODE }
Pop-Location

Write-Host '== ensure ax web =='
try {
    Invoke-WebRequest -Uri "$url/" -UseBasicParsing -TimeoutSec 2 | Out-Null
} catch {
    Write-Host "starting ax web on $port"
    Start-Process -FilePath 'ax' -ArgumentList @('web', '--port', $port) -WorkingDirectory $root -WindowStyle Hidden
    $ok = $false
    1..20 | ForEach-Object {
        try {
            Invoke-WebRequest -Uri "$url/" -UseBasicParsing -TimeoutSec 1 | Out-Null
            $ok = $true
            return
        } catch { Start-Sleep -Milliseconds 500 }
    }
    if (-not $ok) { throw "ax web did not start on $url" }
}

Write-Host '== playwright md-editor-caret (desktop-chrome) =='
Push-Location $webUi
$env:AX_WEB_URL = $url
npx playwright test e2e/md-editor-caret.spec.ts --project=desktop-chrome
if ($LASTEXITCODE -ne 0) { exit $LASTEXITCODE }
Pop-Location

Write-Host 'gauntlet-md-editor-caret: ok'
