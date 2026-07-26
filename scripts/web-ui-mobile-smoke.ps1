# Run Command Center Playwright mobile smoke against a live ax web.
# Usage: .\scripts\web-ui-mobile-smoke.ps1
# Optional: $env:AX_WEB_URL = 'http://127.0.0.1:7070'

$ErrorActionPreference = 'Stop'
$root = Split-Path -Parent $PSScriptRoot
$webUi = Join-Path $root 'crates\ax-web\web-ui'
$url = if ($env:AX_WEB_URL) { $env:AX_WEB_URL } else { 'http://127.0.0.1:7070' }
$env:AX_WEB_URL = $url

function Test-AxWeb {
  try {
    $r = Invoke-WebRequest -Uri "$url/api/stats" -UseBasicParsing -TimeoutSec 3
    return $r.StatusCode -eq 200
  } catch {
    return $false
  }
}

Write-Host "==> Mobile smoke against $url"

$startedWeb = $false
if (-not (Test-AxWeb)) {
  Write-Host "==> Starting ax web on port 7070"
  Start-Process -FilePath 'ax' -ArgumentList @('web', '--port', '7070') -WindowStyle Hidden
  $startedWeb = $true
  $deadline = (Get-Date).AddSeconds(45)
  while ((Get-Date) -lt $deadline) {
    if (Test-AxWeb) { break }
    Start-Sleep -Seconds 1
  }
  if (-not (Test-AxWeb)) {
    throw "ax web did not become ready at $url"
  }
}

Push-Location $webUi
try {
  if (-not (Test-Path (Join-Path $webUi 'node_modules\@playwright\test'))) {
    Write-Host '==> npm install (incl. Playwright)'
    npm install
    if ($LASTEXITCODE -ne 0) { throw 'npm install failed' }
  }
  Write-Host '==> Ensure Chromium for Playwright'
  npx playwright install chromium
  if ($LASTEXITCODE -ne 0) { throw 'playwright install chromium failed' }

  $shotDir = Join-Path $webUi 'test-results\mobile-shots'
  New-Item -ItemType Directory -Force -Path $shotDir | Out-Null

  Write-Host '==> npx playwright test --project=mobile-chrome'
  npx playwright test --project=mobile-chrome
  $code = $LASTEXITCODE

  Write-Host ''
  Write-Host "Screenshots: $shotDir"
  if (Test-Path $shotDir) {
    Get-ChildItem $shotDir -Filter '*.png' | ForEach-Object { Write-Host "  $($_.FullName)" }
  }

  if ($code -ne 0) { throw "Playwright exited with $code" }
  Write-Host 'OK: mobile smoke passed'
} finally {
  Pop-Location
}

if ($startedWeb) {
  Write-Host 'Note: left ax web running (started by this script).'
}
