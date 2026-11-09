$ErrorActionPreference = 'Stop'

$installRoot = if ($env:AX_INSTALL_DIR) { $env:AX_INSTALL_DIR } else { Join-Path $env:LOCALAPPDATA 'ax' }
$binDir = Join-Path $installRoot 'current\bin'

Get-Process -Name 'ax' -ErrorAction SilentlyContinue | Stop-Process -Force -ErrorAction SilentlyContinue

if (Test-Path $installRoot) {
  Remove-Item -Recurse -Force $installRoot -ErrorAction SilentlyContinue
  Write-Host "Removed $installRoot"
}

# Drop User PATH entry for the bin dir when present.
$userPath = [Environment]::GetEnvironmentVariable('Path', 'User')
if ($userPath) {
  $parts = $userPath -split ';' | Where-Object { $_ -and ($_ -ne $binDir) }
  [Environment]::SetEnvironmentVariable('Path', ($parts -join ';'), 'User')
}

$cargoBin = Join-Path $env:USERPROFILE '.cargo\bin\ax.exe'
if (Test-Path $cargoBin) {
  # Only remove if it looks like our sync target; leave other cargo installs alone
  # when AX_KEEP_CARGO_BIN=1.
  if ($env:AX_KEEP_CARGO_BIN -ne '1') {
    Remove-Item -Force $cargoBin -ErrorAction SilentlyContinue
  }
}

Write-Host "ax-agent uninstalled. Open a new shell so PATH updates apply." -ForegroundColor Green
