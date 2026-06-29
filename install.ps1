# ax standalone installer for Windows (PowerShell).
#
#   irm https://raw.githubusercontent.com/GaryWenneker/ax/main/install.ps1 | iex
#
# Upgrade: ax upgrade  (or re-run this script)
# Uninstall: remove %LOCALAPPDATA%\ax and its bin entry from user PATH.

$ErrorActionPreference = 'Stop'
$repo = if ($env:AX_GITHUB_REPO) { $env:AX_GITHUB_REPO } else { 'GaryWenneker/ax' }
$installDir = if ($env:AX_INSTALL_DIR) { $env:AX_INSTALL_DIR } else { Join-Path $env:LOCALAPPDATA 'ax' }

$arch = if ([System.Runtime.InteropServices.RuntimeInformation]::OSArchitecture -eq 'Arm64') { 'arm64' } else { 'x64' }
$target = "win32-$arch"

$version = $env:AX_VERSION
if (-not $version) {
  $version = (Invoke-RestMethod "https://api.github.com/repos/$repo/releases/latest").tag_name
}
if (-not $version) { throw "ax: could not resolve latest version; set AX_VERSION." }

$url = "https://github.com/$repo/releases/download/$version/ax-$target.zip"
Write-Host "Installing ax $version ($target)..."
$tmp = Join-Path $env:TEMP ("ax-" + [guid]::NewGuid().ToString())
New-Item -ItemType Directory -Force -Path $tmp | Out-Null
$zip = Join-Path $tmp 'ax.zip'
Invoke-WebRequest -Uri $url -OutFile $zip

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

$userPath = [Environment]::GetEnvironmentVariable('Path', 'User')
if (($userPath -split ';') -notcontains $binDir) {
  [Environment]::SetEnvironmentVariable('Path', "$binDir;$userPath", 'User')
  Write-Host "Added $binDir to your PATH (restart terminal)."
}

Write-Host "Installed to $dest"
Write-Host "Run: ax version"
