$ErrorActionPreference = 'Stop'

$packageName = 'ax-agent'
$version = $env:ChocolateyPackageVersion
if (-not $version) { throw "ChocolateyPackageVersion is missing" }
$tag = "v$version"

$osArch = [System.Runtime.InteropServices.RuntimeInformation]::OSArchitecture.ToString()
switch ($osArch) {
  'X64' { $arch = 'x64'; $checksum = '__AX_SHA256_WIN32_X64__' }
  'Arm64' { $arch = 'arm64'; $checksum = '__AX_SHA256_WIN32_ARM64__' }
  default { throw "Unsupported Windows architecture: $osArch (need x64 or arm64)" }
}

if ($checksum -notmatch '^[0-9a-fA-F]{64}$') {
  throw "Package checksum placeholders were not stamped. Run scripts/chocolatey-prepare.ps1 before choco pack."
}

$asset = "ax-win32-$arch.zip"
$url = "https://github.com/GaryWenneker/ax/releases/download/$tag/$asset"

$installRoot = if ($env:AX_INSTALL_DIR) { $env:AX_INSTALL_DIR } else { Join-Path $env:LOCALAPPDATA 'ax' }
$dest = Join-Path $installRoot 'current'
$tmp = Join-Path $env:TEMP ("ax-choco-" + [guid]::NewGuid().ToString())
New-Item -ItemType Directory -Force -Path $tmp | Out-Null
$zipPath = Join-Path $tmp $asset

try {
  Get-Process -Name 'ax' -ErrorAction SilentlyContinue | Stop-Process -Force -ErrorAction SilentlyContinue

  Get-ChocolateyWebFile -PackageName $packageName -FileFullPath $zipPath -Url $url `
    -Checksum $checksum -ChecksumType 'sha256'

  if (Test-Path $dest) {
    Remove-Item -Recurse -Force $dest -ErrorAction SilentlyContinue
  }
  New-Item -ItemType Directory -Force -Path $dest | Out-Null
  Get-ChocolateyUnzip -FileFullPath $zipPath -Destination $dest -PackageName $packageName

  $inner = Join-Path $dest "ax-win32-$arch"
  if (Test-Path $inner) {
    Get-ChildItem -Force $inner | Move-Item -Destination $dest -Force
    Remove-Item -Recurse -Force $inner
  }

  $exe = Join-Path $dest 'ax.exe'
  if (-not (Test-Path $exe)) { throw "ax.exe not found after unzip (expected in $dest)" }

  $binDir = Join-Path $dest 'bin'
  New-Item -ItemType Directory -Force -Path $binDir | Out-Null
  Copy-Item -Force $exe (Join-Path $binDir 'ax.exe')

  # Match install.ps1 canonical copies (current root + bin; cargo bin when present).
  $targets = @(
    (Join-Path $binDir 'ax.exe'),
    (Join-Path $dest 'ax.exe')
  )
  $cargoBin = Join-Path $env:USERPROFILE '.cargo\bin\ax.exe'
  if (Test-Path (Split-Path $cargoBin -Parent)) {
    $targets += $cargoBin
    Copy-Item -Force $exe $cargoBin
  }

  Install-ChocolateyPath -PathToInstall $binDir -PathType 'User'

  $reported = (& (Join-Path $binDir 'ax.exe') version 2>&1 | Out-String).Trim()
  if ($reported -notmatch [regex]::Escape($version)) {
    throw "ax version mismatch: got '$reported', expected $version"
  }

  Write-Host "ax $version installed to $dest (bin on User PATH: $binDir)" -ForegroundColor Green
  Write-Host "Open a new shell, then run: ax init" -ForegroundColor DarkGray
}
finally {
  if (Test-Path $tmp) { Remove-Item -Recurse -Force $tmp -ErrorAction SilentlyContinue }
}
