# install.ps1 resolver schema: 2 (GitHub API assets first)
# ax standalone installer for Windows x64 and arm64 (PowerShell).
# macOS / Linux / WSL2: install.sh
#
#   irm https://getax.wenneker.io/install.ps1 | iex
#
# Always installs the latest published release (highest semver with assets).
# Stops running ax processes and replaces any previous install under %LOCALAPPDATA%\ax.
# Pin a version: $env:AX_VERSION = 'v2.0.14'; irm ... | iex
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
  for ($i = 0; $i -lt 15; $i++) {
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

function Clear-AxInstallState {
  Stop-AxProcesses
  if (Test-Path $installDir) {
    Get-ChildItem -Path $installDir -Directory -Filter 'upgrade-staging-*' -ErrorAction SilentlyContinue |
      ForEach-Object { Remove-Item -Recurse -Force $_.FullName -ErrorAction SilentlyContinue }
    $current = Join-Path $installDir 'current'
    if (Test-Path $current) {
      try {
        Remove-Item -Recurse -Force $current
      } catch {
        Start-Sleep -Seconds 1
        Stop-AxProcesses
        Remove-Item -Recurse -Force $current
      }
    }
  }
}

function Copy-AxExeForce {
  param(
    [Parameter(Mandatory = $true)][string]$Source,
    [Parameter(Mandatory = $true)][string]$Destination
  )
  Stop-AxProcesses
  $destDir = Split-Path -Parent $Destination
  if ($destDir -and -not (Test-Path $destDir)) {
    New-Item -ItemType Directory -Force -Path $destDir | Out-Null
  }
  for ($i = 0; $i -lt 8; $i++) {
    try {
      if (Test-Path $Destination) { Remove-Item -Force $Destination -ErrorAction Stop }
      Copy-Item -Force $Source $Destination
      return
    } catch {
      if ($i -ge 7) { throw }
      Start-Sleep -Milliseconds 400
      Stop-AxProcesses
    }
  }
}

$arch = if ([System.Runtime.InteropServices.RuntimeInformation]::OSArchitecture -eq 'Arm64') { 'arm64' } else { 'x64' }
$target = "win32-$arch"
$assetName = "ax-$target.zip"

function Get-GitHubApiHeaders {
  $h = @{
    'User-Agent' = 'ax-install'
    Accept         = 'application/vnd.github+json'
  }
  if ($env:GITHUB_TOKEN) { $h.Authorization = "Bearer $($env:GITHUB_TOKEN)" }
  elseif ($env:GH_TOKEN) { $h.Authorization = "Bearer $($env:GH_TOKEN)" }
  return $h
}

function Resolve-AxVersionFromGitHubApi {
  param([string]$AssetName)
  try {
    $rels = Invoke-RestMethod `
      -Uri "https://api.github.com/repos/$repo/releases?per_page=30" `
      -Headers (Get-GitHubApiHeaders) `
      -TimeoutSec 15
    $installable = [System.Collections.Generic.List[string]]::new()
    foreach ($r in $rels) {
      if ($r.draft -or $r.prerelease) { continue }
      $tag = "$($r.tag_name)".Trim()
      if ($tag -notmatch '^v') { $tag = "v$tag" }
      foreach ($a in $r.assets) {
        if ($a.name -eq $AssetName) {
          $installable.Add($tag)
          break
        }
      }
    }
    if ($installable.Count -gt 0) {
      return ($installable |
        Sort-Object { [version]($_.TrimStart('v')) } -Descending |
        Select-Object -First 1)
    }
  } catch { }
  return $null
}

function Test-AxReleaseAsset {
  param([string]$Tag)
  try {
    $rel = Invoke-RestMethod `
      -Uri "https://api.github.com/repos/$repo/releases/tags/$Tag" `
      -Headers (Get-GitHubApiHeaders) `
      -TimeoutSec 15
    foreach ($a in $rel.assets) {
      if ($a.name -eq $assetName) { return $true }
    }
  } catch { }
  foreach ($base in @(
      "https://github.com/$repo/releases/download/$Tag/$assetName",
      "$downloadBase/$Tag/$assetName"
    )) {
    try {
      # Fallback when API is unreachable — 1-byte Range GET (GitHub S3 rejects HEAD).
      $resp = Invoke-WebRequest -Uri $base -Headers @{ Range = 'bytes=0-0' } -TimeoutSec 15 -UseBasicParsing
      if ($resp.StatusCode -eq 200 -or $resp.StatusCode -eq 206) { return $true }
    } catch { }
  }
  return $false
}

function Resolve-AxVersion {
  if ($env:AX_VERSION) {
    $v = $env:AX_VERSION.Trim()
    if ($v -notmatch '^v') { $v = "v$v" }
    if (-not (Test-AxReleaseAsset -Tag $v)) {
      throw "ax: AX_VERSION $v has no downloadable $assetName on GitHub or getax"
    }
    return $v
  }

  # Prefer GitHub API asset list — no HTTP probe (HEAD/Range fail on some networks).
  $fromApi = Resolve-AxVersionFromGitHubApi -AssetName $assetName
  if ($fromApi) { return $fromApi }

  # Fallback: semver-sorted tags + Range probe, then getax latest.txt pointer.
  $candidates = [System.Collections.Generic.List[string]]::new()
  foreach ($source in @(
      { (Invoke-RestMethod "https://api.github.com/repos/$repo/releases/latest" -Headers (Get-GitHubApiHeaders) -TimeoutSec 15).tag_name },
      {
        $rels = Invoke-RestMethod "https://api.github.com/repos/$repo/releases?per_page=30" -Headers (Get-GitHubApiHeaders) -TimeoutSec 15
        foreach ($r in $rels) {
          if (-not $r.draft -and -not $r.prerelease) { $r.tag_name }
        }
      },
      { (Invoke-RestMethod "$downloadBase/latest.txt" -TimeoutSec 15).Trim() }
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

function Get-AxInstallTargets {
  param(
    [Parameter(Mandatory = $true)][string]$BinDir,
    [Parameter(Mandatory = $true)][string]$InstallRoot
  )
  @(
    (Join-Path $BinDir 'ax.exe'),
    (Join-Path $InstallRoot 'ax.exe'),
    (Join-Path $env:USERPROFILE '.cargo\bin\ax.exe')
  ) | Select-Object -Unique
}

function Sync-LocalAxInstances {
  param(
    [Parameter(Mandatory = $true)][string]$SourceExe,
    [Parameter(Mandatory = $true)][string[]]$Targets
  )
  foreach ($dest in $Targets) {
    $srcResolved = (Resolve-Path $SourceExe -ErrorAction SilentlyContinue)
    $destResolved = (Resolve-Path $dest -ErrorAction SilentlyContinue)
    if ($srcResolved -and $destResolved -and ($srcResolved.Path -eq $destResolved.Path)) {
      continue
    }
    Copy-AxExeForce -Source $SourceExe -Destination $dest
  }
}

function Confirm-AxInstall {
  param(
    [Parameter(Mandatory = $true)][string]$ExpectedTag,
    [Parameter(Mandatory = $true)][string[]]$Targets
  )
  $expected = $ExpectedTag.TrimStart('v')
  foreach ($path in $Targets) {
    if (-not (Test-Path $path)) {
      throw "ax: install incomplete - missing $path"
    }
    $ver = (& $path version 2>&1 | Out-String).Trim()
    if ($ver -notmatch [regex]::Escape($expected)) {
      throw "ax: $path reports '$ver', expected $expected - close ax MCP/web/IDE terminals and re-run install"
    }
  }
}

function Update-SessionPath {
  param([Parameter(Mandatory = $true)][string]$BinDir)
  Set-UserPathFirst -entry $BinDir
  $userPath = [Environment]::GetEnvironmentVariable('Path', 'User')
  $machinePath = [Environment]::GetEnvironmentVariable('Path', 'Machine')
  $merged = @($BinDir) +
    ($userPath -split ';' | Where-Object { $_ -and ($_ -ne $BinDir) }) +
    ($machinePath -split ';' | Where-Object { $_ -and ($_ -ne $BinDir) })
  $env:Path = ($merged | Select-Object -Unique) -join ';'
}

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

# Kill stale ax and wipe previous install before resolving/downloading.
Clear-AxInstallState

$version = Resolve-AxVersion

$getaxUrl = "$downloadBase/$version/ax-$target.zip"
$githubUrl = "https://github.com/$repo/releases/download/$version/ax-$target.zip"
Write-Host "Installing ax $version ($target) - latest available..."
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

Stop-AxProcesses

$dest = Join-Path $installDir 'current'
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

$installTargets = Get-AxInstallTargets -BinDir $binDir -InstallRoot $dest
if ($env:AX_KEEP_CARGO_BIN -eq '1') {
  $installTargets = $installTargets | Where-Object { $_ -notlike '*\.cargo\bin\ax.exe' }
}
Sync-LocalAxInstances -SourceExe $exe -Targets $installTargets
Update-SessionPath -BinDir $binDir
Confirm-AxInstall -ExpectedTag $version -Targets $installTargets

$installedVer = (& (Join-Path $binDir 'ax.exe') version 2>&1 | Out-String).Trim()
Write-Host "Installed to $dest (replaced previous install)" -ForegroundColor Green
Write-Host "Active: $installedVer ($binDir\ax.exe)" -ForegroundColor Green
Write-Host "Synced local instances:" -ForegroundColor DarkGray
foreach ($path in $installTargets) {
  Write-Host "  $path" -ForegroundColor DarkGray
}

$shadow = Get-Command ax -All -ErrorAction SilentlyContinue | Where-Object { $_.Source -notin $installTargets }
if ($shadow) {
  Write-Host "Other ax on PATH (install updated canonical paths above; new shells prefer $binDir):" -ForegroundColor Yellow
  foreach ($cmd in $shadow) {
    Write-Host "  $($cmd.Source)" -ForegroundColor DarkGray
  }
}
