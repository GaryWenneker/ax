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

function Stop-AxProcesses {
  param([int[]]$ExcludePid = @())
  $procs = @(Get-Process -Name 'ax' -ErrorAction SilentlyContinue | Where-Object { $ExcludePid -notcontains $_.Id })
  if ($procs.Count -eq 0) { return }
  Write-Host "Stopping $($procs.Count) running ax process(es)..."
  foreach ($p in $procs) {
    Stop-Process -Id $p.Id -Force -ErrorAction SilentlyContinue
  }
  for ($i = 0; $i -lt 10; $i++) {
    Start-Sleep -Milliseconds 300
    $left = @(Get-Process -Name 'ax' -ErrorAction SilentlyContinue | Where-Object { $ExcludePid -notcontains $_.Id })
    if ($left.Count -eq 0) { return }
    foreach ($p in $left) {
      Stop-Process -Id $p.Id -Force -ErrorAction SilentlyContinue
    }
  }
  $survivors = @(Get-Process -Name 'ax' -ErrorAction SilentlyContinue | Where-Object { $ExcludePid -notcontains $_.Id })
  if ($survivors.Count -gt 0) {
    throw "ax: could not stop running ax process(es). Close ax web, MCP, or IDE terminals using ax, then retry."
  }
}

function Remove-AxInstallTree {
  param([string]$Path)
  if (-not (Test-Path $Path)) { return }
  Stop-AxProcesses
  try {
    Remove-Item -Recurse -Force $Path
  } catch {
    Start-Sleep -Seconds 1
    Stop-AxProcesses
    Remove-Item -Recurse -Force $Path
  }
}

$arch = if ([System.Runtime.InteropServices.RuntimeInformation]::OSArchitecture -eq 'Arm64') { 'arm64' } else { 'x64' }
$target = "win32-$arch"

function Test-AxReleaseAsset {
  param([string]$Tag)
  $url = "https://github.com/$repo/releases/download/$Tag/ax-$target.zip"
  try {
    $resp = Invoke-WebRequest -Uri $url -Method Head -TimeoutSec 15 -UseBasicParsing
    return $resp.StatusCode -eq 200
  } catch {
    return $false
  }
}

function Resolve-AxVersion {
  if ($env:AX_VERSION) {
    $v = $env:AX_VERSION.Trim()
    if ($v -notmatch '^v') { $v = "v$v" }
    return $v
  }

  $candidates = [System.Collections.Generic.List[string]]::new()
  foreach ($source in @(
      { (Invoke-RestMethod "$downloadBase/latest.txt" -TimeoutSec 15).Trim() },
      { (Invoke-RestMethod "https://api.github.com/repos/$repo/releases/latest" -TimeoutSec 15).tag_name },
      {
        $rels = Invoke-RestMethod "https://api.github.com/repos/$repo/releases?per_page=30" -TimeoutSec 15
        foreach ($r in $rels) {
          if (-not $r.draft -and -not $r.prerelease) { $r.tag_name }
        }
      }
    )) {
    try {
      $value = & $source
      if ($value -is [array]) {
        foreach ($item in $value) { if ($item) { $candidates.Add($item.Trim()) } }
      } elseif ($value) {
        $candidates.Add($value.Trim())
      }
    } catch {
      # try next source
    }
  }

  $sorted = $candidates |
    Select-Object -Unique |
    Sort-Object { [version]($_.TrimStart('v')) } -Descending

  foreach ($candidate in $sorted) {
    $tag = if ($candidate -match '^v') { $candidate } else { "v$candidate" }
    if (Test-AxReleaseAsset -Tag $tag) { return $tag }
  }

  throw "ax: could not resolve a release with downloadable assets; set AX_VERSION or publish releases to GitHub ($repo)"
}

$version = Resolve-AxVersion

$getaxUrl = "$downloadBase/$version/ax-$target.zip"
$githubUrl = "https://github.com/$repo/releases/download/$version/ax-$target.zip"
Write-Host "Installing ax $version ($target)..."
$tmp = Join-Path $env:TEMP ("ax-" + [guid]::NewGuid().ToString())
New-Item -ItemType Directory -Force -Path $tmp | Out-Null
$zip = Join-Path $tmp 'ax.zip'

$downloaded = $false
$downloadFrom = $null
foreach ($url in @($githubUrl, $getaxUrl)) {
  try {
    Invoke-WebRequest -Uri $url -OutFile $zip -TimeoutSec 120
    $downloaded = $true
    $downloadFrom = $url
    break
  } catch {
    Write-Host "  download failed: $url" -ForegroundColor DarkGray
  }
}
if (-not $downloaded) {
  throw "ax: download failed. Try: cargo install --git https://github.com/$repo ax-cli"
}
Write-Host "  downloaded from: $downloadFrom" -ForegroundColor DarkGray

$dest = Join-Path $installDir 'current'
Remove-AxInstallTree -Path $dest
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
    Stop-AxProcesses
    Copy-Item -Force $exe $cargoAx
    Write-Host "Updated $cargoAx (was: $($oldVer.Trim()))" -ForegroundColor DarkGray
  } catch {
    Write-Host "Note: could not update $cargoAx — use a new terminal or run:" -ForegroundColor Yellow
    Write-Host "  `$env:Path = '$binDir;' + `$env:Path" -ForegroundColor Yellow
  }
}

$installedVer = (& (Join-Path $binDir 'ax.exe') version 2>&1 | Out-String).Trim()
$expectedVer = $version.TrimStart('v')
if ($installedVer -notmatch [regex]::Escape($expectedVer)) {
  Write-Warning "Tag $version was installed but binary reports: $installedVer (release may have been built from wrong Cargo.toml — try again after CI republish)"
}
Write-Host "Installed to $dest"
Write-Host "Active: $installedVer ($binDir\ax.exe)" -ForegroundColor Green

$shadow = Get-Command ax -All -ErrorAction SilentlyContinue | Select-Object -Skip 1
if ($shadow) {
  Write-Host "Other ax on PATH (ignored if $binDir is first in new terminals):" -ForegroundColor DarkGray
  foreach ($cmd in $shadow) {
    Write-Host "  $($cmd.Source)" -ForegroundColor DarkGray
  }
}
