import assert from 'node:assert/strict';
import { spawnSync } from 'node:child_process';
import { mkdtempSync, readFileSync, rmSync, writeFileSync } from 'node:fs';
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
