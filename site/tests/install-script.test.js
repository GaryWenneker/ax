import assert from 'node:assert/strict';
import { spawnSync } from 'node:child_process';
import { mkdirSync, mkdtempSync, readFileSync, rmSync, symlinkSync, writeFileSync } from 'node:fs';
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
            const result = spawnSync(shell, shell === 'bash' ? ['--posix', '-n', installer] : ['-n', installer], { encoding: 'utf8' });
            assert.equal(result.status, 0, result.stderr);
        });
    }
}
function fixture(options = {}) {
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
    const realSort = spawnSync('sh', ['-c', 'command -v sort'], { encoding: 'utf8' }).stdout.trim();
    const mock = join(bin, 'mock.mjs');
    writeFileSync(mock, `#!${process.execPath}
import { appendFileSync, copyFileSync } from 'node:fs';
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
  const asset='ax-'+(env.AX_TEST_OS==='Darwin'?'darwin':'linux')+'-'+(env.AX_TEST_ARCH==='x86_64'?'x64':'arm64')+'.tar.gz';
  const release=(tag,extra={})=>({tag_name:tag,draft:false,prerelease:false,assets:[{name:asset}],...extra});
  if(url?.includes('releases?')) console.log(JSON.stringify([release('v7.9.0'),release('v8.1.0'),release('v50.0.0',{prerelease:true}),release('v100.0.0',{draft:true}),release('v20.0.0',{assets:[]})],null,2));
  else if(url?.endsWith('releases/latest')) console.log(JSON.stringify(release('v8.1.0')));
  else if(url?.includes('/releases/tags/')) console.log(JSON.stringify(url.endsWith('v8.1.0')||url.endsWith('v7.9.0')?release('v8.1.0'):{assets:[]}));
  else if(url?.endsWith('latest.txt')) console.log('8.1.0');
  else if(args.includes('-r')) {process.stdout.write('404');process.exit(22);}
  else if(url?.includes('/v8.1.0/')) copyFileSync(env.AX_TEST_ARCHIVE,args[args.indexOf('-o')+1]);
  else process.exit(22);
} else process.exit(1);
`, { mode: 0o755 });
    for (const name of ['uname', 'curl', 'sort', 'pkill', 'sleep', ...(options.withoutPython ? ['python3'] : []), ...(options.stalePin ? ['powershell.exe'] : [])]) {
        symlinkSync(mock, join(bin, name));
    }
    const binaryDir = join(root, options.binName ?? 'linked');
    const env = {
        ...process.env,
        HOME: userHome, SHELL: options.loginShell ?? '/bin/zsh',
        PATH: `${bin}:${process.env.PATH ?? '/usr/bin:/bin'}`,
        AX_INSTALL_DIR: join(root, 'install'), AX_BIN_DIR: binaryDir,
        AX_KEEP_CARGO_BIN: '1', AX_SKIP_PROJECT_INIT: '1',
        AX_TEST_OS: options.os ?? 'Darwin', AX_TEST_ARCH: options.arch ?? 'arm64',
        AX_TEST_TRACE: trace, AX_TEST_ARCHIVE: archive, AX_TEST_SORT: realSort,
    };
    delete env.AX_VERSION;
    delete env.ZDOTDIR;
    if (options.stalePin)
        env.AX_VERSION = 'v99.0.0';
    return { root, userHome, trace, binaryDir, env };
}
for (const os of ['Darwin', 'Linux']) {
    for (const arch of ['arm64', 'x86_64']) {
        test(`release API chooses the highest stable ${os}/${arch} asset from JSON input`, () => {
            const f = fixture({ os, arch });
            try {
                const definitions = readFileSync(installers[0], 'utf8').split('# Kill stale ax and remove previous install before download.')[0];
                const result = spawnSync('sh', ['-s'], { env: f.env, input: `${definitions}\nresolve_version_from_api\n`, encoding: 'utf8' });
                assert.equal(result.status, 0, result.stderr);
                assert.equal(result.stdout.trim(), 'v8.1.0');
            }
            finally {
                rmSync(f.root, { recursive: true, force: true });
            }
        });
    }
}
function runInstaller(installer, env) {
    return spawnSync('sh', ['-s'], { env, input: readFileSync(installer, 'utf8'), encoding: 'utf8', timeout: 30_000 });
}
function freshZsh(f) {
    const result = spawnSync('zsh', ['-i', '-c', 'command -v ax && ax version'], {
        cwd: f.root, env: { ...f.env, PATH: '/usr/bin:/bin' }, encoding: 'utf8', timeout: 10_000,
    });
    if (result.error)
        throw result.error;
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
        }
        finally {
            rmSync(f.root, { recursive: true, force: true });
        }
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
    }
    finally {
        rmSync(f.root, { recursive: true, force: true });
    }
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
        }
        finally {
            rmSync(f.root, { recursive: true, force: true });
        }
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
        }
        finally {
            rmSync(f.root, { recursive: true, force: true });
        }
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
            }
            else {
                const fresh = spawnSync('sh', ['-c', '. "$1"; command -v ax; ax version', 'path-test', join(f.userHome, '.profile')], {
                    env: { ...f.env, PATH: '/usr/bin:/bin' }, encoding: 'utf8', timeout: 10_000,
                });
                assert.equal(fresh.status, 0, fresh.stderr);
            }
        }
        finally {
            rmSync(f.root, { recursive: true, force: true });
        }
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
            }
            else {
                mkdirSync(join(f.userHome, blocked === 'bashrc' ? '.bashrc' : '.zshrc'));
            }
            const installed = runInstaller(installers[0], f.env);
            assert.equal(installed.status, 0, installed.stderr);
            assert.match(installed.stderr, /manual PATH/);
            assert.doesNotMatch(installed.stdout, /PATH configured/);
            assert.equal(spawnSync(join(f.binaryDir, 'ax'), ['version'], { encoding: 'utf8' }).status, 0);
        }
        finally {
            rmSync(f.root, { recursive: true, force: true });
        }
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
        }
        finally {
            rmSync(f.root, { recursive: true, force: true });
        }
    }
});
test('failed binary verification never updates shell configuration', () => {
    const f = fixture({ binaryVersion: '7.0.0' });
    try {
        const installed = runInstaller(installers[0], f.env);
        assert.notEqual(installed.status, 0);
        assert.throws(() => readFileSync(join(f.userHome, '.zshrc')));
    }
    finally {
        rmSync(f.root, { recursive: true, force: true });
    }
});
test('an unset login shell uses the platform startup file', () => {
    for (const os of ['Darwin', 'Linux']) {
        const f = fixture({ os });
        try {
            f.env.SHELL = '';
            assert.equal(runInstaller(installers[0], f.env).status, 0);
            const filename = os === 'Darwin' ? '.zshrc' : '.profile';
            assert.match(readFileSync(join(f.userHome, filename), 'utf8'), /# ax PATH/);
        }
        finally {
            rmSync(f.root, { recursive: true, force: true });
        }
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
    }
    finally {
        rmSync(f.root, { recursive: true, force: true });
    }
});
for (const options of [{}, { withoutPython: true }, { stalePin: true }]) {
    test(`piped Apple Silicon installation completes with ${JSON.stringify(options)}`, () => {
        const f = fixture(options);
        try {
            const result = spawnSync('sh', ['-s'], { env: f.env, input: readFileSync(installers[1], 'utf8'), encoding: 'utf8' });
            assert.equal(result.status, 0, result.stderr);
            assert.match(result.stdout, /Installing ax v8\.1\.0 \(darwin-arm64\)/);
            assert.match(result.stdout, /Active: ax 8\.1\.0/);
            assert.equal(spawnSync(join(f.root, 'linked/ax'), ['version'], { encoding: 'utf8' }).stdout.trim(), 'ax 8.1.0');
            const trace = readFileSync(f.trace, 'utf8');
            assert.ok(!trace.includes('"-V"'), 'macOS does not provide GNU version sort');
            if ('stalePin' in options) {
                assert.ok(trace.includes(JSON.stringify({ name: 'powershell.exe', args: ['-NoProfile', '-Command', '[Environment]::SetEnvironmentVariable("AX_VERSION", $null, "User")'] })));
            }
        }
        finally {
            rmSync(f.root, { recursive: true, force: true });
        }
    });
}
//# sourceMappingURL=install-script.test.js.map