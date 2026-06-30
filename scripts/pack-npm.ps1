# Pack @garywenneker/ax npm launcher (Windows)
$ErrorActionPreference = 'Stop'
$Root = Split-Path -Parent (Split-Path -Parent $MyInvocation.MyCommand.Path)
$Version = if ($args[0]) { $args[0] } else {
  (Select-String -Path (Join-Path $Root 'crates\ax-cli\Cargo.toml') -Pattern '^version = "(.*)"').Matches[0].Groups[1].Value
}
& node (Join-Path $Root 'scripts\sync-npm-docs.js')
$Npm = Join-Path $Root 'release\npm\main'
if (Test-Path $Npm) { Remove-Item -Recurse -Force $Npm }
New-Item -ItemType Directory -Force -Path $Npm | Out-Null
Copy-Item (Join-Path $Root 'scripts\npm-shim.js') (Join-Path $Npm 'npm-shim.js')
Copy-Item (Join-Path $Root 'docs\npm\README.md') (Join-Path $Npm 'README.md')
$pkg = @{
  name = '@garywenneker/ax'
  version = $Version
  description = 'Native code-intelligence CLI for AI agents (MCP). Thin npm launcher — downloads the ax binary from GitHub Releases.'
  bin = @{ ax = 'npm-shim.js' }
  files = @('npm-shim.js', 'README.md')
  repository = @{ type = 'git'; url = 'git+https://github.com/GaryWenneker/ax.git' }
  homepage = 'https://getax.wenneker.io'
  bugs = @{ url = 'https://github.com/GaryWenneker/ax/issues' }
  keywords = @('mcp', 'code-intelligence', 'tree-sitter', 'ai-agents', 'cursor', 'claude')
  license = 'MIT'
  engines = @{ node = '>=18' }
}
$utf8 = New-Object System.Text.UTF8Encoding $false
[System.IO.File]::WriteAllText((Join-Path $Npm 'package.json'), ($pkg | ConvertTo-Json -Depth 6) + "`n", $utf8)
Write-Host "[pack-npm] @garywenneker/ax@$Version -> $Npm"
