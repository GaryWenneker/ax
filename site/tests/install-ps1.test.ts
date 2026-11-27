import assert from 'node:assert/strict';
import { spawnSync } from 'node:child_process';
import { mkdirSync, mkdtempSync, readFileSync, rmSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { fileURLToPath } from 'node:url';
import test from 'node:test';

const installer = fileURLToPath(new URL('../../install.ps1', import.meta.url));
const published = fileURLToPath(new URL('../public/install.ps1', import.meta.url));
const MAIN_MARKER = '# ---- install steps ----';
const pwsh = spawnSync('pwsh', ['-NoProfile', '-Command', '$PSVersionTable.PSVersion.Major'], { encoding: 'utf8' });
const skip = pwsh.status === 0 ? false : 'pwsh is not installed';

test('published and repository PowerShell installers match', () => {
  assert.equal(readFileSync(installer, 'utf8'), readFileSync(published, 'utf8'));
});

/** Runs `body` after the installer's function definitions, with test doubles defined last. */
function runDefinitions(body: string) {
  const text = readFileSync(installer, 'utf8');
  assert.ok(text.includes(MAIN_MARKER), 'install.ps1 keeps its definitions above the main marker');
  const root = mkdtempSync(join(tmpdir(), 'ax-ps1-test-'));
  try {
    const script = join(root, 'run.ps1');
    writeFileSync(script, `${text.split(MAIN_MARKER)[0]}\n${body}\n`);
    return spawnSync('pwsh', ['-NoProfile', '-NonInteractive', '-File', script], {
      encoding: 'utf8', timeout: 60_000,
      env: { ...process.env, AX_INSTALL_DIR: join(root, 'install'), LOCALAPPDATA: root, GITHUB_TOKEN: 'valid-token', GH_TOKEN: '' },
    });
  } finally { rmSync(root, { recursive: true, force: true }); }
}

const releasesDouble = `
function Get-AxReleasesFromGitHubApi {
  param([hashtable]$Headers)
  if ($Headers.Authorization) { throw $script:failure }
  @([pscustomobject]@{ tag_name = 'v8.1.0'; draft = $false; prerelease = $false; assets = @([pscustomobject]@{ name = $assetName }) })
}`;

test('a rate-limit 403 keeps the GitHub token and retries anonymously', { skip }, () => {
  const result = runDefinitions(`${releasesDouble}
$script:failure = 'Response status code does not indicate success: 403 (Forbidden). API rate limit exceeded for 203.0.113.9.'
$v = Resolve-AxVersionFromGitHubApi -AssetName $assetName
Write-Output "VERSION=$v"
Write-Output "TOKEN=$env:GITHUB_TOKEN"
Write-Output "AUTH=$((Get-GitHubApiHeaders).ContainsKey('Authorization'))"`);
  assert.equal(result.status, 0, result.stderr);
  assert.match(result.stdout, /VERSION=v8\.1\.0/);
  assert.match(result.stdout, /TOKEN=valid-token/, 'the session token must survive');
  assert.match(result.stdout, /rate limit/i);
  assert.doesNotMatch(result.stdout, /cleared stale/);
});

test('a rejected token is skipped for this install and the cleanup is only suggested', { skip }, () => {
  const result = runDefinitions(`${releasesDouble}
$script:failure = 'Response status code does not indicate success: 401 (Unauthorized). Bad credentials'
$v = Resolve-AxVersionFromGitHubApi -AssetName $assetName
Write-Output "VERSION=$v"
Write-Output "TOKEN=$env:GITHUB_TOKEN"
Write-Output "AUTH=$((Get-GitHubApiHeaders).ContainsKey('Authorization'))"`);
  assert.equal(result.status, 0, result.stderr);
  assert.match(result.stdout, /VERSION=v8\.1\.0/);
  assert.match(result.stdout, /TOKEN=valid-token/);
  assert.match(result.stdout, /AUTH=False/, 'later requests in this install skip the rejected token');
  assert.match(result.stdout, /SetEnvironmentVariable\('GITHUB_TOKEN', \$null, 'User'\)/, 'the user gets the command to run');
});

test('the installer never writes GitHub tokens to the User or Machine environment', () => {
  const text = readFileSync(installer, 'utf8');
  assert.doesNotMatch(text, /ClearUserScope/);
  for (const line of text.split('\n').filter((l) => /SetEnvironmentVariable\(/.test(l))) {
    assert.doesNotMatch(line.replace(/Write-Host.*$/, ''), /\$name|GITHUB_TOKEN|GH_TOKEN/, line);
  }
});

/** Runs the whole installer with network, process and PATH doubles defined before its main steps. */
function runInstall(doubles: string, after: string, setup?: (root: string) => void) {
  const text = readFileSync(installer, 'utf8');
  const [definitions, main] = text.split(MAIN_MARKER);
  const root = mkdtempSync(join(tmpdir(), 'ax-ps1-install-'));
  try {
    setup?.(root);
    const script = join(root, 'run.ps1');
    writeFileSync(script, `${definitions}
function Stop-AxProcesses { param([int[]]$ExcludePid = @()) Write-Host 'STOP-AX-PROCESSES' }
function Set-UserPathFirst([string]$entry) { }
${doubles}
try {
${main}
} catch { Write-Host "INSTALL-FAILED: $_" }
${after}
`);
    return spawnSync('pwsh', ['-NoProfile', '-NonInteractive', '-File', script], {
      encoding: 'utf8', timeout: 120_000,
      env: {
        ...process.env, AX_INSTALL_DIR: join(root, 'install'), LOCALAPPDATA: root, TEMP: root, USERPROFILE: join(root, 'profile'),
        AX_SKIP_PROJECT_INIT: '1', AX_KEEP_CARGO_BIN: '1', GITHUB_TOKEN: '', GH_TOKEN: '', AX_VERSION: '', AX_TEST_ROOT: root,
      },
    });
  } finally { rmSync(root, { recursive: true, force: true }); }
}

const offline = `
function Invoke-RestMethod { throw 'offline' }
function Invoke-WebRequest { throw 'offline' }`;

test('an offline Windows install keeps the working program and running ax processes', { skip }, () => {
  const result = runInstall(offline, `
Write-Host "CURRENT=$(Test-Path (Join-Path $env:AX_INSTALL_DIR 'current/ax.exe'))"`, (root) => {
    const current = join(root, 'install', 'current');
    mkdirSync(current, { recursive: true });
    writeFileSync(join(current, 'ax.exe'), 'working program');
  });
  assert.equal(result.status, 0, result.stderr);
  assert.match(result.stdout, /INSTALL-FAILED/);
  assert.match(result.stdout, /CURRENT=True/, 'current\\ax.exe must survive a failed download');
  assert.doesNotMatch(result.stdout, /STOP-AX-PROCESSES/, 'ax processes are only stopped once the new program is ready');
});

const bundle = `win32-${process.arch === 'arm64' ? 'arm64' : 'x64'}`;

/** A release zip whose ax.exe is a script that reports version 8.1.0. */
function writeReleaseZip(root: string) {
  const inner = join(root, 'payload', `ax-${bundle}`);
  mkdirSync(inner, { recursive: true });
  writeFileSync(join(inner, 'ax.exe'), '#!/bin/sh\necho "ax 8.1.0"\n', { mode: 0o755 });
  const zipped = spawnSync('zip', ['-qr', join(root, 'release.zip'), `ax-${bundle}`], { cwd: join(root, 'payload'), encoding: 'utf8' });
  assert.equal(zipped.status, 0, zipped.stderr);
}

const online = (sums: string) => `
function Invoke-RestMethod {
  param($Uri, $Headers, $TimeoutSec)
  if ("$Uri" -match 'releases\\?per_page') {
    return @([pscustomobject]@{ tag_name = 'v8.1.0'; draft = $false; prerelease = $false; assets = @([pscustomobject]@{ name = $assetName }) })
  }
  throw 'unexpected request'
}
function Invoke-WebRequest {
  param($Uri, $OutFile, $Headers, $TimeoutSec, [switch]$UseBasicParsing)
  Add-Content -Path (Join-Path $env:AX_TEST_ROOT 'requests.txt') -Value "$Uri"
  if ("$Uri" -like '*/ax-win32-*.zip') { Copy-Item (Join-Path $env:AX_TEST_ROOT 'release.zip') $OutFile; return }
  if ("$Uri" -like '*/SHA256SUMS') { ${sums}; return }
  throw 'not found'
}
function Expand-Archive {
  param($Path, $DestinationPath, [switch]$Force)
  New-Item -ItemType Directory -Force -Path $DestinationPath | Out-Null
  & unzip -qo $Path -d $DestinationPath
}`;

const goodSums = `$h = (Get-FileHash (Join-Path $env:AX_TEST_ROOT 'release.zip') -Algorithm SHA256).Hash.ToLowerInvariant(); Set-Content -Path $OutFile -Value "$h  ax-${bundle}.zip"`;

function seedWorkingInstall(root: string) {
  writeReleaseZip(root);
  const current = join(root, 'install', 'current');
  mkdirSync(current, { recursive: true });
  writeFileSync(join(current, 'ax.exe'), 'previous program');
}

const reportTree = `
$installRoot = $env:AX_INSTALL_DIR
Write-Host "CURRENT=$(Get-Content -Raw (Join-Path $installRoot 'current/ax.exe'))"
Write-Host "LEFTOVERS=$(@(Get-ChildItem -Force $installRoot | Where-Object { $_.Name -ne 'current' }).Count)"`;

test('a Windows install swaps the new program in and leaves no staging folders', { skip }, () => {
  const result = runInstall(online(goodSums), reportTree, seedWorkingInstall);
  assert.equal(result.status, 0, result.stderr);
  assert.doesNotMatch(result.stdout, /INSTALL-FAILED/, result.stdout);
  assert.match(result.stdout, /CURRENT=#!\/bin\/sh/);
  assert.match(result.stdout, /LEFTOVERS=0/);
  assert.match(result.stdout, /STOP-AX-PROCESSES/);
});

for (const [name, sums, expected] of [
  ['a wrong checksum', `Set-Content -Path $OutFile -Value "${'0'.repeat(64)}  ax-${bundle}.zip"`, /checksum mismatch/],
  ['no entry for the archive', `Set-Content -Path $OutFile -Value "${'0'.repeat(64)}  ax-other.zip"`, /no entry/],
  ['no SHA256SUMS at all', `throw 'not found'`, /SHA256SUMS/],
] as const) {
  test(`a Windows archive with ${name} is never installed`, { skip }, () => {
    const result = runInstall(online(sums), reportTree, seedWorkingInstall);
    assert.equal(result.status, 0, result.stderr);
    assert.match(result.stdout, /INSTALL-FAILED/);
    assert.match(result.stdout, expected);
    assert.match(result.stdout, /CURRENT=previous program/);
    assert.match(result.stdout, /LEFTOVERS=0/);
    assert.doesNotMatch(result.stdout, /STOP-AX-PROCESSES/);
  });
}
