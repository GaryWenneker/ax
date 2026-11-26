# ax-admin installer for Windows (x64 and arm64).
# ax-admin is the standalone admin application for a remote Ax. It is a separate
# program: this script never installs, replaces or removes ax itself.
#
#   irm https://getax.wenneker.io/install-admin.ps1 | iex
#
# With options, download the script first:
#   .\install-admin.ps1 -Version v10.1.2
#   .\install-admin.ps1 -Uninstall
#
# Environment: AX_VERSION, AX_ADMIN_INSTALL_DIR (default %LOCALAPPDATA%\ax-admin),
# AX_GITHUB_REPO, AX_ADMIN_DOWNLOAD_BASE, AX_GITHUB_API.
#
# Installs %LOCALAPPDATA%\ax-admin\bin\ax-admin.exe and adds that folder to the
# user PATH. Saved connections (%USERPROFILE%\.ax-admin) and keys (Windows
# Credential Manager) are left in place on uninstall. Run again to upgrade.
param(
  [string]$Version = $env:AX_VERSION,
  [switch]$Uninstall
)

$ErrorActionPreference = 'Stop'
$ProgressPreference = 'SilentlyContinue'

$repo = if ($env:AX_GITHUB_REPO) { $env:AX_GITHUB_REPO } else { 'GaryWenneker/ax' }
$downloadBase = if ($env:AX_ADMIN_DOWNLOAD_BASE) { $env:AX_ADMIN_DOWNLOAD_BASE } else { "https://github.com/$repo/releases/download" }
$apiBase = if ($env:AX_GITHUB_API) { $env:AX_GITHUB_API } else { 'https://api.github.com' }
$installDir = if ($env:AX_ADMIN_INSTALL_DIR) { $env:AX_ADMIN_INSTALL_DIR } else { Join-Path $env:LOCALAPPDATA 'ax-admin' }
$binDir = Join-Path $installDir 'bin'
$exe = Join-Path $binDir 'ax-admin.exe'

function Say([string]$message) { Write-Host "ax-admin: $message" }

# The user PATH is read and written unexpanded, so entries such as
# %USERPROFILE%\bin stay as they are.
function Get-UserPathEntries {
  $key = [Microsoft.Win32.Registry]::CurrentUser.OpenSubKey('Environment')
  if (-not $key) { return @() }
  try {
    $path = $key.GetValue('Path', '', [Microsoft.Win32.RegistryValueOptions]::DoNotExpandEnvironmentNames)
  } finally {
    $key.Dispose()
  }
  if (-not $path) { return @() }
  return @(([string]$path).Split(';') | Where-Object { $_ })
}

function Set-UserPathEntries([string[]]$entries) {
  $key = [Microsoft.Win32.Registry]::CurrentUser.CreateSubKey('Environment')
  try {
    $key.SetValue('Path', ($entries -join ';'), [Microsoft.Win32.RegistryValueKind]::ExpandString)
  } finally {
    $key.Dispose()
  }
  # A direct registry write is not announced. Tell running programs (Explorer
  # among them) that the environment changed, so a new terminal sees the PATH.
  # The PATH is already changed at this point; a system that does not allow
  # this call (constrained language mode) only misses the announcement.
  try {
    if (-not ('AxAdmin.Environment' -as [type])) {
      Add-Type -Namespace AxAdmin -Name Environment -MemberDefinition @'
[DllImport("user32.dll", SetLastError = true, CharSet = CharSet.Auto)]
public static extern IntPtr SendMessageTimeout(IntPtr hWnd, uint Msg, UIntPtr wParam, string lParam, uint fuFlags, uint uTimeout, out UIntPtr lpdwResult);
'@
    }
    $result = [UIntPtr]::Zero
    # HWND_BROADCAST, WM_SETTINGCHANGE, SMTO_ABORTIFHUNG, 5 seconds
    [void][AxAdmin.Environment]::SendMessageTimeout([IntPtr]0xffff, 0x1A, [UIntPtr]::Zero, 'Environment', 2, 5000, [ref]$result)
  } catch {
    Say 'the PATH change could not be announced; sign out and in again if a new terminal does not see it'
  }
}

$onWindows = $env:OS -eq 'Windows_NT'


if ($Uninstall) {
  if (Test-Path -LiteralPath $exe) {
    Remove-Item -LiteralPath $exe -Force
    Say "removed $exe"
  } else {
    Say "nothing to remove at $exe"
  }
  if ($onWindows) {
    $entries = Get-UserPathEntries
    $kept = @($entries | Where-Object { $_.TrimEnd('\') -ne $binDir.TrimEnd('\') })
    if ($kept.Count -ne $entries.Count) {
      Set-UserPathEntries $kept
      Say "removed $binDir from the user PATH (open a new terminal to see it)"
    }
  }
  Say 'saved connections and keys in Windows Credential Manager were left in place'
  return
}

$arch = if ($env:PROCESSOR_ARCHITECTURE -eq 'ARM64') { 'arm64' } else { 'x64' }
$name = "ax-admin-win32-$arch"
$asset = "$name.zip"
$headers = @{ 'User-Agent' = 'ax-install-admin' }

function Get-ReleaseTagsWithAsset($releases) {
  foreach ($release in @($releases)) {
    if ($release.draft -or $release.prerelease) { continue }
    if (@($release.assets | Where-Object { $_.name -eq $asset }).Count -gt 0) { $release.tag_name }
  }
}

function Resolve-Tag {
  if ($Version) {
    if ($Version -notmatch '^v') { $script:Version = "v$Version" }
    return $Version
  }
  try {
    $latest = Invoke-RestMethod -Uri "$apiBase/repos/$repo/releases/latest" -Headers $headers
    $tag = Get-ReleaseTagsWithAsset $latest | Select-Object -First 1
    if ($tag) { return $tag }
    $releases = Invoke-RestMethod -Uri "$apiBase/repos/$repo/releases?per_page=30" -Headers $headers
    $tag = Get-ReleaseTagsWithAsset $releases |
      Sort-Object { [version](($_ -replace '^v', '') -replace '[^0-9.].*$', '') } -Descending |
      Select-Object -First 1
    if ($tag) { return $tag }
  } catch {
    throw "Cannot reach the GitHub API to find the newest release. Pass -Version vX.Y.Z. ($($_.Exception.Message))"
  }
  throw "No published release of $repo contains $asset yet."
}

$tag = Resolve-Tag
Say "installing ax-admin $tag (win32-$arch) to $exe"

$temp = Join-Path ([IO.Path]::GetTempPath()) ("ax-admin-" + [Guid]::NewGuid().ToString('N'))
New-Item -ItemType Directory -Path $temp -Force | Out-Null
try {
  $zip = Join-Path $temp $asset
  $sums = Join-Path $temp 'SHA256SUMS'
  Invoke-WebRequest -Uri "$downloadBase/$tag/$asset" -OutFile $zip -Headers $headers -UseBasicParsing
  Invoke-WebRequest -Uri "$downloadBase/$tag/SHA256SUMS" -OutFile $sums -Headers $headers -UseBasicParsing

  $expected = $null
  foreach ($line in [IO.File]::ReadAllLines($sums)) {
    $parts = $line -split '\s+', 2
    if ($parts.Count -eq 2 -and $parts[1].TrimStart('*') -eq $asset) { $expected = $parts[0].ToLowerInvariant(); break }
  }
  if (-not $expected) { throw "SHA256SUMS of $tag has no entry for $asset" }
  $actual = (Get-FileHash -LiteralPath $zip -Algorithm SHA256).Hash.ToLowerInvariant()
  if ($actual -ne $expected) { throw "checksum mismatch for $asset (expected $expected, got $actual)" }

  Expand-Archive -LiteralPath $zip -DestinationPath $temp -Force
  $source = Join-Path (Join-Path $temp $name) 'ax-admin.exe'
  if (-not (Test-Path -LiteralPath $source)) { throw "archive $asset does not contain $name\ax-admin.exe" }

  # The working program is replaced only by one that runs on this computer.
  $installed = & $source version
  if ($LASTEXITCODE -ne 0) { throw "the downloaded ax-admin does not run on this computer; $exe was left as it was" }

  New-Item -ItemType Directory -Path $binDir -Force | Out-Null
  $staged = "$exe.new"
  Copy-Item -LiteralPath $source -Destination $staged -Force
  Move-Item -LiteralPath $staged -Destination $exe -Force
} finally {
  Remove-Item -LiteralPath $temp -Recurse -Force -ErrorAction SilentlyContinue
}
Say "installed $installed"

if (-not $onWindows) {
  Say "run it as: $exe"
  return
}
$entries = Get-UserPathEntries
if (-not ($entries | Where-Object { $_.TrimEnd('\') -eq $binDir.TrimEnd('\') })) {
  Set-UserPathEntries ($entries + $binDir)
  Say "added $binDir to the user PATH; open a new terminal, then run: ax-admin"
} else {
  Say 'start it with: ax-admin'
}
