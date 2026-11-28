import assert from 'node:assert/strict';
import { spawnSync } from 'node:child_process';
import { existsSync, mkdirSync, mkdtempSync, readdirSync, readFileSync, rmSync, symlinkSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { fileURLToPath } from 'node:url';
import test from 'node:test';

const installers = [
  fileURLToPath(new URL('../../install.sh', import.meta.url)),
  fileURLToPath(new URL('../public/install.sh', import.meta.url)),
];

test('published and repository shell installers match', () => {
  assert.equal(readFileSync(installers[0], 'utf8'), readFileSync(installers[1], 'utf8'));
});

for (const installer of installers) {
  for (const shell of ['sh', 'bash']) {
    test(`${installer} parses in ${shell}`, () => {
      const result = spawnSync(shell, shell === 'bash' ? ['--posix', '-n', installer] : ['-n', installer], { encoding: 'utf8', timeout: 10_000 });
      assert.equal(result.status, 0, result.stderr);
    });
  }
}

test('every shell run in this suite has a timeout so a waiting installer fails instead of hanging', () => {
  const source = readFileSync(fileURLToPath(import.meta.url), 'utf8');
  const calls = [...source.matchAll(/spawnSync\((?:'(?:sh|bash|zsh)'|shell\b)/g)];
  assert.ok(calls.length > 5, 'found the shell runs');
  for (const call of calls) {
    let depth = 0;
    let end = call.index!;
    for (; end < source.length; end++) {
      if (source[end] === '(') depth++;
      else if (source[end] === ')' && --depth === 0) break;
    }
    const text = source.slice(call.index!, end + 1);
    const line = source.slice(0, call.index!).split('\n').length;
    assert.match(text, /\btimeout:/, `line ${line}: ${text.split('\n')[0]}`);
  }
});

function fixture(options: { os?: string; arch?: string; withoutPython?: boolean; stalePin?: boolean; powershell?: boolean; tokenFailure?: 'rate' | 'auth'; loginShell?: string; binName?: string; binaryVersion?: string } = {}) {
  const root = mkdtempSync(join(tmpdir(), 'ax-installer-test-'));
  const userHome = join(root, 'user-home');
  mkdirSync(userHome);
  const bin = join(root, 'mock-bin');
  const trace = join(root, 'requests.jsonl');
  mkdirSync(bin);
  mkdirSync(join(root, 'payload'));
  writeFileSync(join(root, 'payload/ax'), `#!/bin/sh\necho "ax ${options.binaryVersion ?? '8.1.0'}"\n`, { mode: 0o755 });
  const archive = join(root, 'ax.tar.gz');
  const packed = spawnSync('tar', ['-czf', archive, '-C', root, 'payload'], { encoding: 'utf8' });
  assert.equal(packed.status, 0, packed.stderr);
  mkdirSync(join(root, 'tampered'));
  writeFileSync(join(root, 'tampered/ax'), `#!/bin/sh\necho "ax ${options.binaryVersion ?? '8.1.0'}"\n# tampered\n`, { mode: 0o755 });
  const tampered = join(root, 'tampered.tar.gz');
  assert.equal(spawnSync('tar', ['-czf', tampered, '-C', root, 'tampered'], { encoding: 'utf8' }).status, 0);
  const realSort = spawnSync('sh', ['-c', 'command -v sort'], { encoding: 'utf8', timeout: 10_000 }).stdout.trim();
  const mock = join(bin, 'mock.mjs');
  writeFileSync(mock, `#!${process.execPath}
import { appendFileSync, copyFileSync, readFileSync, writeFileSync } from 'node:fs';
import { createHash } from 'node:crypto';
import { basename } from 'node:path';
import { spawnSync } from 'node:child_process';
const name=basename(process.argv[1]), args=process.argv.slice(2), env=process.env;
appendFileSync(env.AX_TEST_TRACE, JSON.stringify({name,args})+'\\n');
if(name==='uname') console.log(args[0]==='-s'?env.AX_TEST_OS:env.AX_TEST_ARCH);
else if(name==='pkill'||name==='sleep') process.exit(0);
else if(name==='python3') process.exit(1);
else if(name==='sort') {
  if(args.includes('-V')) {console.error('sort: illegal option -- V');process.exit(2);}
  const result=spawnSync(env.AX_TEST_SORT,args,{stdio:'inherit'});process.exit(result.status??1);
} else if(name==='powershell.exe') {
  if(args.at(-1).includes('GetEnvironmentVariable')) console.log('v99.0.0');
} else if(name==='curl') {
  const url=args.find(a=>a.startsWith('https://'));
  const outIdx=args.indexOf('-o');
  const outPath=outIdx>=0?args[outIdx+1]:null;
  const wantCode=args.some(a=>String(a).includes('%{http_code}'));
  const asset='ax-'+(env.AX_TEST_OS==='Darwin'?'darwin':'linux')+'-'+(env.AX_TEST_ARCH==='x86_64'?'x64':'arm64')+'.tar.gz';
  const release=(tag,extra={})=>({tag_name:tag,draft:false,prerelease:false,assets:[{name:asset}],...extra});
  const emit=(body,code=200)=>{
    const text=typeof body==='string'?body:JSON.stringify(body,null,2);
    if(outPath) writeFileSync(outPath, text);
    else if(typeof body==='string') process.stdout.write(body);
    else console.log(text);
    if(wantCode) process.stdout.write(String(code));
  };
  const withToken=args.some(a=>String(a).startsWith('Authorization: Bearer'));
  if(url?.includes('releases?') && withToken && env.AX_TEST_TOKEN_FAILURE==='rate') emit({message:'API rate limit exceeded for user ID 1.'},403);
  else if(url?.includes('releases?') && withToken && env.AX_TEST_TOKEN_FAILURE==='auth') emit({message:'Bad credentials'},401);
  else if(url?.includes('releases?')) emit([release('v7.9.0'),release('v8.1.0'),release('v50.0.0',{prerelease:true}),release('v100.0.0',{draft:true}),release('v20.0.0',{assets:[]})]);
  else if(url?.endsWith('releases/latest')) emit(release('v8.1.0'));
  else if(url?.includes('/releases/tags/')) emit(url.endsWith('v8.1.0')||url.endsWith('v7.9.0')?release('v8.1.0'):{assets:[]});
  else if(url?.endsWith('latest.txt')) emit('8.1.0');
  else if(args.includes('-r')) {if(wantCode) process.stdout.write('404'); process.exit(22);}
  else if(url?.includes('/v8.1.0/') && env.AX_TEST_DOWNLOAD_FAIL==='1') process.exit(22);
  else if(url?.endsWith('/v8.1.0/SHA256SUMS')) {
    const sha=(file)=>createHash('sha256').update(readFileSync(file)).digest('hex');
    const mirror=!url.startsWith('https://github.com/');
    const mode=env.AX_TEST_SUMS||'good';
    if(mode==='github-down' && !mirror) process.exit(22);
    const hash=mode==='bad'?'0'.repeat(64):mode==='mirror-tampered'&&mirror?sha(env.AX_TEST_TAMPERED):sha(env.AX_TEST_ARCHIVE);
    const name=mode==='missing'?'ax-other.tar.gz':asset;
    writeFileSync(args[args.indexOf('-o')+1], hash+'  '+name+'\\n'+'1'.repeat(64)+'  ax-slim-linux-x64.tar.gz\\n');
  }
  else if(url?.includes('/v8.1.0/') && env.AX_TEST_SUMS==='mirror-tampered' && url.startsWith('https://github.com/')) process.exit(22);
  else if(url?.includes('/v8.1.0/') && env.AX_TEST_SUMS==='mirror-tampered') copyFileSync(env.AX_TEST_TAMPERED,args[args.indexOf('-o')+1]);
  else if(url?.includes('/v8.1.0/')) {copyFileSync(env.AX_TEST_ARCHIVE,args[args.indexOf('-o')+1]); if(wantCode) process.stdout.write('200');}
  else process.exit(22);
} else process.exit(1);
`, { mode: 0o755 });
  for (const name of ['uname', 'curl', 'sort', 'pkill', 'sleep', ...(options.withoutPython ? ['python3'] : []), ...(options.stalePin || options.powershell ? ['powershell.exe'] : [])]) {
    symlinkSync(mock, join(bin, name));
  }
  const binaryDir = join(root, options.binName ?? 'linked');
  const env: NodeJS.ProcessEnv = {
    ...process.env,
    HOME: userHome, SHELL: options.loginShell ?? '/bin/zsh',
    PATH: `${bin}:${process.env.PATH ?? '/usr/bin:/bin'}`,
    AX_INSTALL_DIR: join(root, 'install'), AX_BIN_DIR: binaryDir,
    AX_KEEP_CARGO_BIN: '1', AX_SKIP_PROJECT_INIT: '1',
    AX_TEST_OS: options.os ?? 'Darwin', AX_TEST_ARCH: options.arch ?? 'arm64',
    AX_TEST_TRACE: trace, AX_TEST_ARCHIVE: archive, AX_TEST_TAMPERED: tampered, AX_TEST_SORT: realSort,
    AX_TEST_TOKEN_FAILURE: options.tokenFailure ?? '',
  };
  delete env.GITHUB_TOKEN;
  delete env.GH_TOKEN;
  delete env.AX_VERSION;
  delete env.ZDOTDIR;
  if (options.stalePin) env.AX_VERSION = 'v99.0.0';
  return { root, userHome, trace, binaryDir, env };
}

for (const os of ['Darwin', 'Linux']) {
  for (const arch of ['arm64', 'x86_64']) {
    test(`release API chooses the highest stable ${os}/${arch} asset from JSON input`, () => {
      const f = fixture({ os, arch });
      try {
        const definitions = readFileSync(installers[0], 'utf8').split('# ---- install steps ----')[0];
        const result = spawnSync('sh', ['-s'], { env: f.env, input: `${definitions}\nresolve_version_from_api\n`, encoding: 'utf8', timeout: 30_000 });
        assert.equal(result.status, 0, result.stderr);
        assert.equal(result.stdout.trim(), 'v8.1.0');
      } finally { rmSync(f.root, { recursive: true, force: true }); }
    });
  }
}

function runInstaller(installer: string, env: NodeJS.ProcessEnv) {
  return spawnSync('sh', ['-s'], { env, input: readFileSync(installer, 'utf8'), encoding: 'utf8', timeout: 30_000 });
}

function freshZsh(f: ReturnType<typeof fixture>) {
  const result = spawnSync('zsh', ['-i', '-c', 'command -v ax && ax version'], {
    cwd: f.root, env: { ...f.env, PATH: '/usr/bin:/bin' }, encoding: 'utf8', timeout: 10_000,
  });
  if (result.error) throw result.error;
  return result;
}

for (const installer of installers) {
  test(`${installer} persists PATH for a fresh zsh and preserves existing configuration`, () => {
    const f = fixture();
    try {
      const rc = join(f.userHome, '.zshrc');
      const original = '# café user configuration\nexport AX_USER_SETTING=kept';
      writeFileSync(rc, original);
      const installed = runInstaller(installer, f.env);
      assert.equal(installed.status, 0, installed.stderr);
      assert.ok(readFileSync(rc, 'utf8').startsWith(original));
      const fresh = freshZsh(f);
      assert.equal(fresh.status, 0, fresh.stderr);
      assert.ok(fresh.stdout.includes(`${f.env.AX_BIN_DIR}/ax`));
      assert.match(fresh.stdout, /ax 8\.1\.0/);
      assert.match(installed.stdout, /this terminal/);
    } finally { rmSync(f.root, { recursive: true, force: true }); }
  });
}

test('reinstall keeps a single PATH block and honors ZDOTDIR', () => {
  const f = fixture();
  try {
    const dotdir = join(f.root, 'zsh configuration');
    mkdirSync(dotdir);
    f.env.ZDOTDIR = dotdir;
    for (let run = 0; run < 2; run++) {
      const installed = runInstaller(installers[0], f.env);
      assert.equal(installed.status, 0, installed.stderr);
    }
    const rc = readFileSync(join(dotdir, '.zshrc'), 'utf8');
    assert.equal(rc.split('# ax PATH').length - 1, 1);
    assert.equal(freshZsh(f).status, 0);
    assert.throws(() => readFileSync(join(f.userHome, '.zshrc')));
  } finally { rmSync(f.root, { recursive: true, force: true }); }
});

test('shell-sensitive binary paths are literal data in the profile', () => {
  for (const binName of ['bin with spaces', "bin'quote", 'bin[$cash]*', 'bin`touch AX_INJECTION`', 'bin$(touch AX_INJECTION)']) {
    const f = fixture({ binName });
    try {
      const installed = runInstaller(installers[0], f.env);
      assert.equal(installed.status, 0, installed.stderr);
      const fresh = freshZsh(f);
      assert.equal(fresh.status, 0, fresh.stderr);
      assert.ok(fresh.stdout.includes(`${f.env.AX_BIN_DIR}/ax`));
      assert.throws(() => readFileSync(join(f.root, 'AX_INJECTION')));
    } finally { rmSync(f.root, { recursive: true, force: true }); }
  }
});

test('Bash configures interactive and the first applicable login profile', () => {
  for (const existing of ['.bash_profile', '.bash_login', '.profile']) {
    const f = fixture({ loginShell: '/bin/bash' });
    try {
      writeFileSync(join(f.userHome, existing), '# existing configuration\n');
      assert.equal(runInstaller(installers[0], f.env).status, 0);
      for (const filename of ['.bashrc', existing]) {
        const configured = spawnSync('bash', ['--noprofile', '--norc', '-c', 'source "$1"; command -v ax; ax version', 'path-test', join(f.userHome, filename)], {
          env: { ...f.env, PATH: '/usr/bin:/bin' }, encoding: 'utf8', timeout: 10_000,
        });
        assert.equal(configured.status, 0, configured.stderr);
        assert.match(configured.stdout, /ax 8\.1\.0/);
      }
    } finally { rmSync(f.root, { recursive: true, force: true }); }
  }
});

test('POSIX shells receive a profile while unknown shells receive manual guidance', () => {
  for (const loginShell of ['/bin/sh', '/bin/dash', '/bin/fish']) {
    const f = fixture({ loginShell });
    try {
      const installed = runInstaller(installers[0], f.env);
      assert.equal(installed.status, 0, installed.stderr);
      if (loginShell.endsWith('fish')) {
        assert.match(installed.stderr, /manual PATH/);
        assert.throws(() => readFileSync(join(f.userHome, '.profile')));
      } else {
        const fresh = spawnSync('sh', ['-c', '. "$1"; command -v ax; ax version', 'path-test', join(f.userHome, '.profile')], {
          env: { ...f.env, PATH: '/usr/bin:/bin' }, encoding: 'utf8', timeout: 10_000,
        });
        assert.equal(fresh.status, 0, fresh.stderr);
      }
    } finally { rmSync(f.root, { recursive: true, force: true }); }
  }
});

test('failed profile writes are reported honestly while the binary remains installed', () => {
  for (const blocked of ['zshrc', 'zsh-parent', 'bashrc']) {
    const f = fixture({ loginShell: blocked === 'bashrc' ? '/bin/bash' : '/bin/zsh' });
    try {
      if (blocked === 'zsh-parent') {
        const parent = join(f.root, 'blocked-parent');
        writeFileSync(parent, 'not a directory');
        f.env.ZDOTDIR = join(parent, 'child');
      } else {
        mkdirSync(join(f.userHome, blocked === 'bashrc' ? '.bashrc' : '.zshrc'));
      }
      const installed = runInstaller(installers[0], f.env);
      assert.equal(installed.status, 0, installed.stderr);
      assert.match(installed.stderr, /manual PATH/);
      assert.doesNotMatch(installed.stdout, /PATH configured/);
      assert.equal(spawnSync(join(f.binaryDir, 'ax'), ['version'], { encoding: 'utf8' }).status, 0);
    } finally { rmSync(f.root, { recursive: true, force: true }); }
  }
});

test('invalid PATH components are rejected before install side effects', () => {
  for (const binName of ['bin:colon', 'bin\nnewline']) {
    const f = fixture({ binName });
    try {
      const installed = runInstaller(installers[0], f.env);
      assert.notEqual(installed.status, 0);
      assert.match(installed.stderr, /unsupported AX_BIN_DIR/);
      assert.throws(() => readFileSync(f.trace));
    } finally { rmSync(f.root, { recursive: true, force: true }); }
  }
});

test('failed binary verification never updates shell configuration', () => {
  const f = fixture({ binaryVersion: '7.0.0' });
  try {
    const installed = runInstaller(installers[0], f.env);
    assert.notEqual(installed.status, 0);
    assert.throws(() => readFileSync(join(f.userHome, '.zshrc')));
  } finally { rmSync(f.root, { recursive: true, force: true }); }
});

test('an unset login shell uses the platform startup file', () => {
  for (const os of ['Darwin', 'Linux']) {
    const f = fixture({ os });
    try {
      f.env.SHELL = '';
      assert.equal(runInstaller(installers[0], f.env).status, 0);
      const filename = os === 'Darwin' ? '.zshrc' : '.profile';
      assert.match(readFileSync(join(f.userHome, filename), 'utf8'), /# ax PATH/);
    } finally { rmSync(f.root, { recursive: true, force: true }); }
  }
});

test('a relative binary directory is resolved before persisting PATH', () => {
  const f = fixture();
  try {
    f.env.AX_BIN_DIR = 'relative-bin';
    const installed = spawnSync('sh', ['-s'], {
      cwd: f.root, env: f.env, input: readFileSync(installers[0], 'utf8'), encoding: 'utf8', timeout: 30_000,
    });
    assert.equal(installed.status, 0, installed.stderr);
    const fresh = spawnSync('zsh', ['-i', '-c', 'command -v ax && ax version'], {
      cwd: f.userHome, env: { ...f.env, PATH: '/usr/bin:/bin' }, encoding: 'utf8', timeout: 10_000,
    });
    assert.equal(fresh.status, 0, fresh.stderr);
    assert.ok(fresh.stdout.includes(join(f.root, 'relative-bin/ax')));
  } finally { rmSync(f.root, { recursive: true, force: true }); }
});

function runUninstall(env: NodeJS.ProcessEnv, ...flags: string[]) {
  return spawnSync('sh', ['-s', '--', '--uninstall', ...flags], { env, input: readFileSync(installers[0], 'utf8'), encoding: 'utf8', timeout: 30_000 });
}

function seedUserData(installDir: string) {
  mkdirSync(join(installDir, 'global_policy/rules'), { recursive: true });
  mkdirSync(join(installDir, 'auth'), { recursive: true });
  writeFileSync(join(installDir, 'global.db'), 'database');
  writeFileSync(join(installDir, 'global_policy/rules/team.mdc'), 'rule');
  writeFileSync(join(installDir, 'auth/token.json'), '{}');
  writeFileSync(join(installDir, 'config.json'), '{}');
  mkdirSync(join(installDir, 'versions/v8.0.0'), { recursive: true });
  mkdirSync(join(installDir, 'upgrade-staging-3'), { recursive: true });
  mkdirSync(join(installDir, 'current.old'), { recursive: true });
}

test('uninstall removes the program and PATH block but keeps ~/.ax data', () => {
  const f = fixture();
  try {
    const rc = join(f.userHome, '.zshrc');
    const original = '# user configuration\nexport AX_USER_SETTING=kept\n';
    writeFileSync(rc, original);
    assert.equal(runInstaller(installers[0], f.env).status, 0);
    const installDir = f.env.AX_INSTALL_DIR as string;
    seedUserData(installDir);
    const removed = runUninstall(f.env);
    assert.equal(removed.status, 0, removed.stderr);
    for (const kept of ['global.db', 'global_policy/rules/team.mdc', 'auth/token.json', 'config.json']) {
      assert.ok(existsSync(join(installDir, kept)), `${kept} must survive --uninstall`);
    }
    for (const gone of ['current', 'versions', 'upgrade-staging-3', 'current.old']) {
      assert.ok(!existsSync(join(installDir, gone)), `${gone} must be removed`);
    }
    assert.ok(!existsSync(join(f.binaryDir, 'ax')));
    assert.equal(readFileSync(rc, 'utf8'), original);
    assert.match(removed.stdout, /--purge/);
  } finally { rmSync(f.root, { recursive: true, force: true }); }
});

test('uninstall --purge needs confirmation and refuses unsafe directories', () => {
  const f = fixture();
  try {
    const installDir = join(f.userHome, '.ax');
    f.env.AX_INSTALL_DIR = installDir;
    seedUserData(installDir);
    const unconfirmed = runUninstall(f.env, '--purge');
    assert.notEqual(unconfirmed.status, 0);
    assert.match(unconfirmed.stderr, /--yes/);
    assert.ok(existsSync(join(installDir, 'global.db')));

    for (const unsafe of [f.userHome, join(f.root, 'projects')]) {
      mkdirSync(join(f.root, 'projects'), { recursive: true });
      const refused = runUninstall({ ...f.env, AX_INSTALL_DIR: unsafe }, '--purge', '--yes');
      assert.notEqual(refused.status, 0, `purge of ${unsafe} must be refused`);
      assert.ok(existsSync(join(installDir, 'global.db')));
      assert.ok(existsSync(join(f.root, 'projects')));
    }

    const purged = runUninstall(f.env, '--purge', '--yes');
    assert.equal(purged.status, 0, purged.stderr);
    assert.ok(!existsSync(installDir));
  } finally { rmSync(f.root, { recursive: true, force: true }); }
});

test('a failed download keeps the working install and running ax processes', () => {
  const f = fixture();
  try {
    assert.equal(runInstaller(installers[0], f.env).status, 0);
    const current = join(f.env.AX_INSTALL_DIR as string, 'current/ax');
    assert.ok(existsSync(current));
    assert.deepEqual(readdirSync(f.env.AX_INSTALL_DIR as string), ['current'], 'no staging or parked folders stay behind');
    writeFileSync(f.trace, '');
    const failed = runInstaller(installers[0], { ...f.env, AX_TEST_DOWNLOAD_FAIL: '1' });
    assert.notEqual(failed.status, 0);
    assert.match(failed.stderr, /download failed/);
    assert.ok(existsSync(current), 'the working program in current/ is kept');
    assert.equal(spawnSync(current, ['version'], { encoding: 'utf8' }).stdout.trim(), 'ax 8.1.0');
    assert.equal(spawnSync(join(f.binaryDir, 'ax'), ['version'], { encoding: 'utf8' }).status, 0);
    const trace = readFileSync(f.trace, 'utf8');
    assert.ok(!trace.includes('"name":"pkill"'), 'running ax processes are only stopped once the new program is ready');
  } finally { rmSync(f.root, { recursive: true, force: true }); }
});

for (const [sums, expected] of [['bad', /checksum mismatch/], ['missing', /no entry/], ['mirror-tampered', /checksum mismatch/]] as const) {
  test(`an archive that fails the SHA256SUMS check (${sums}) is never installed`, () => {
    const f = fixture();
    try {
      assert.equal(runInstaller(installers[0], f.env).status, 0);
      const current = join(f.env.AX_INSTALL_DIR as string, 'current/ax');
      const before = readFileSync(current, 'utf8');
      writeFileSync(f.trace, '');
      const result = runInstaller(installers[0], { ...f.env, AX_TEST_SUMS: sums });
      assert.notEqual(result.status, 0, 'the install must fail');
      assert.match(result.stderr, expected);
      assert.equal(readFileSync(current, 'utf8'), before);
      assert.ok(!readFileSync(f.trace, 'utf8').includes('"name":"pkill"'));
    } finally { rmSync(f.root, { recursive: true, force: true }); }
  });
}

test('the mirror checksum list is used when GitHub is unreachable', () => {
  const f = fixture();
  try {
    const result = runInstaller(installers[0], { ...f.env, AX_TEST_SUMS: 'github-down' });
    assert.equal(result.status, 0, result.stderr);
    assert.match(result.stderr, /mirror/);
  } finally { rmSync(f.root, { recursive: true, force: true }); }
});

for (const tokenFailure of ['rate', 'auth'] as const) {
  test(`a GitHub ${tokenFailure} failure never changes the stored Windows tokens`, () => {
    const f = fixture({ tokenFailure, powershell: true });
    try {
      f.env.GITHUB_TOKEN = 'valid-token';
      const result = runInstaller(installers[0], f.env);
      assert.equal(result.status, 0, result.stderr);
      const writes = readFileSync(f.trace, 'utf8').split('\n').filter((l) => l.includes('powershell.exe') && l.includes('SetEnvironmentVariable') && /GITHUB_TOKEN|GH_TOKEN/.test(l));
      assert.deepEqual(writes, []);
      if (tokenFailure === 'rate') assert.match(result.stderr, /rate limit/i);
      else assert.match(result.stderr, /SetEnvironmentVariable\('GITHUB_TOKEN', \$null, 'User'\)/);
    } finally { rmSync(f.root, { recursive: true, force: true }); }
  });
}

for (const options of [{}, { withoutPython: true }, { stalePin: true }]) {
  test(`piped Apple Silicon installation completes with ${JSON.stringify(options)}`, () => {
    const f = fixture(options);
    try {
      const result = runInstaller(installers[1], f.env);
      assert.equal(result.error, undefined, 'installer finished within the timeout');
      assert.equal(result.status, 0, result.stderr);
      assert.match(result.stdout, /Installing ax v8\.1\.0 \(darwin-arm64\)/);
      assert.match(result.stdout, /Active: ax 8\.1\.0/);
      assert.equal(spawnSync(join(f.root, 'linked/ax'), ['version'], { encoding: 'utf8' }).stdout.trim(), 'ax 8.1.0');
      const trace = readFileSync(f.trace, 'utf8');
      assert.ok(!trace.includes('"-V"'), 'macOS does not provide GNU version sort');
      if ('stalePin' in options) {
        assert.ok(trace.includes(JSON.stringify({ name: 'powershell.exe', args: ['-NoProfile', '-Command', '[Environment]::SetEnvironmentVariable("AX_VERSION", $null, "User")'] })));
      }
    } finally { rmSync(f.root, { recursive: true, force: true }); }
  });
}

test('a release tag that is not plain x.y.z does not break version resolution', () => {
  const text = readFileSync(installers[0], 'utf8');
  const fn = text.match(/^parse_latest_installable_tag\(\) \{\n[\s\S]*?\n\}\n/m);
  assert.ok(fn, 'install.sh defines parse_latest_installable_tag');
  const asset = 'ax-darwin-arm64.tar.gz';
  const release = (tag: string) => ({ tag_name: tag, draft: false, prerelease: false, assets: [{ name: asset }] });
  const releases = [release('v10.1.0'), release('v10.2.0-hotfix1'), release('nightly'), release('v10.3'), release('v9.9.9'),
    { ...release('v11.0.0'), prerelease: true }];
  const result = spawnSync('sh', ['-c', `${fn[0]}\nparse_latest_installable_tag "$1"`, 'sh', asset], {
    encoding: 'utf8', input: JSON.stringify(releases), timeout: 10_000,
  });
  assert.equal(result.status, 0, result.stderr);
  assert.equal(result.stdout, 'v10.3\n');
});
