# ax standalone installer for Windows x64 and arm64 (PowerShell).
# macOS / Linux / WSL2: install.sh
#
#   irm https://getax.wenneker.io/install.ps1 | iex
#
# Upgrade: ax upgrade  (or re-run this script)
# Uninstall: remove %LOCALAPPDATA%\ax and its bin entry from user PATH.

$ErrorActionPreference = 'Stop'
$repo = if ($env:AX_GITHUB_REPO) { $env:AX_GITHUB_REPO } else { 'GaryWenneker/ax' }
$downloadBase = if ($env:AX_DOWNLOAD_BASE) { $env:AX_DOWNLOAD_BASE } else { 'https://getax.wenneker.io/releases' }
$installDir = if ($env:AX_INSTALL_DIR) { $env:AX_INSTALL_DIR } else { Join-Path $env:LOCALAPPDATA 'ax' }

$arch = if ([System.Runtime.InteropServices.RuntimeInformation]::OSArchitecture -eq 'Arm64') { 'arm64' } else { 'x64' }
$target = "win32-$arch"

$version = $env:AX_VERSION
if (-not $version) {
  try {
    $version = (Invoke-RestMethod "$downloadBase/latest.txt" -TimeoutSec 15).Trim()
  } catch {
    $version = $null
  }
}
if (-not $version) {
  try {
    $version = (Invoke-RestMethod "https://api.github.com/repos/$repo/releases/latest" -TimeoutSec 15).tag_name
  } catch {
    $version = $null
  }
}
if (-not $version) {
  throw "ax: could not resolve latest version; set AX_VERSION or publish releases to $downloadBase"
}

$getaxUrl = "$downloadBase/$version/ax-$target.zip"
$githubUrl = "https://github.com/$repo/releases/download/$version/ax-$target.zip"
Write-Host "Installing ax $version ($target)..."
$tmp = Join-Path $env:TEMP ("ax-" + [guid]::NewGuid().ToString())
New-Item -ItemType Directory -Force -Path $tmp | Out-Null
$zip = Join-Path $tmp 'ax.zip'

$downloaded = $false
foreach ($url in @($getaxUrl, $githubUrl)) {
  try {
    Invoke-WebRequest -Uri $url -OutFile $zip -TimeoutSec 120
    $downloaded = $true
    break
  } catch {
    Write-Host "  download failed: $url" -ForegroundColor DarkGray
  }
}
if (-not $downloaded) {
  throw "ax: download failed. Try: cargo install --git https://github.com/$repo ax-cli"
}

$dest = Join-Path $installDir 'current'
if (Test-Path $dest) { Remove-Item -Recurse -Force $dest }
New-Item -ItemType Directory -Force -Path $dest | Out-Null
Expand-Archive -Path $zip -DestinationPath $dest -Force
$inner = Join-Path $dest "ax-$target"
if (Test-Path $inner) {
  Get-ChildItem -Force $inner | Move-Item -Destination $dest -Force
  Remove-Item -Recurse -Force $inner
}
Remove-Item -Recurse -Force $tmp

$binDir = Join-Path $dest 'bin'
New-Item -ItemType Directory -Force -Path $binDir | Out-Null
$exe = Join-Path $dest 'ax.exe'
if (-not (Test-Path $exe)) { throw "ax.exe not found in bundle" }
Copy-Item -Force $exe (Join-Path $binDir 'ax.exe')

function Set-UserPathFirst([string]$entry) {
  $userPath = [Environment]::GetEnvironmentVariable('Path', 'User')
  $parts = @()
  if ($userPath) {
    $parts = $userPath -split ';' | Where-Object { $_ -and ($_ -ne $entry) }
  }
  $newPath = @($entry) + $parts
  $joined = ($newPath | Select-Object -Unique) -join ';'
  [Environment]::SetEnvironmentVariable('Path', $joined, 'User')
}

Set-UserPathFirst -entry $binDir
# Apply immediately in this shell (install.ps1 | iex does not reload user PATH).
$env:Path = ($binDir + ';' + (($env:Path -split ';') | Where-Object { $_ -and ($_ -ne $binDir) }) -join ';')

# Replace stale cargo-installed ax so `ax` resolves correctly even before a new terminal.
$cargoAx = Join-Path $env:USERPROFILE '.cargo\bin\ax.exe'
if ((Test-Path $cargoAx) -and ($env:AX_KEEP_CARGO_BIN -ne '1')) {
  try {
    $oldVer = & $cargoAx version 2>&1 | Out-String
    Copy-Item -Force $exe $cargoAx
    Write-Host "Updated $cargoAx (was: $($oldVer.Trim()))" -ForegroundColor DarkGray
  } catch {
    Write-Host "Note: could not update $cargoAx — use a new terminal or run:" -ForegroundColor Yellow
    Write-Host "  `$env:Path = '$binDir;' + `$env:Path" -ForegroundColor Yellow
  }
}

$installedVer = (& (Join-Path $binDir 'ax.exe') version 2>&1 | Out-String).Trim()
Write-Host "Installed to $dest"
Write-Host "Active: $installedVer ($binDir\ax.exe)" -ForegroundColor Green

$shadow = Get-Command ax -All -ErrorAction SilentlyContinue | Select-Object -Skip 1
if ($shadow) {
  Write-Host "Other ax on PATH (ignored if $binDir is first in new terminals):" -ForegroundColor DarkGray
  foreach ($cmd in $shadow) {
    Write-Host "  $($cmd.Source)" -ForegroundColor DarkGray
  }
}
