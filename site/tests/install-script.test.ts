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

function fixture(options: { os?: string; arch?: string; withoutPython?: boolean; stalePin?: boolean } = {}) {
  const root = mkdtempSync(join(tmpdir(), 'ax-installer-test-'));
  const bin = join(root, 'mock-bin');
  const trace = join(root, 'requests.jsonl');
  mkdirSync(bin);
  mkdirSync(join(root, 'payload'));
  writeFileSync(join(root, 'payload/ax'), '#!/bin/sh\necho "ax 8.1.0"\n', { mode: 0o755 });
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
  const env: NodeJS.ProcessEnv = {
    ...process.env,
    PATH: `${bin}:${process.env.PATH ?? '/usr/bin:/bin'}`,
    AX_INSTALL_DIR: join(root, 'install'), AX_BIN_DIR: join(root, 'linked'),
    AX_KEEP_CARGO_BIN: '1', AX_SKIP_PROJECT_INIT: '1',
    AX_TEST_OS: options.os ?? 'Darwin', AX_TEST_ARCH: options.arch ?? 'arm64',
    AX_TEST_TRACE: trace, AX_TEST_ARCHIVE: archive, AX_TEST_SORT: realSort,
  };
  delete env.AX_VERSION;
  if (options.stalePin) env.AX_VERSION = 'v99.0.0';
  return { root, trace, env };
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
      } finally { rmSync(f.root, { recursive: true, force: true }); }
    });
  }
}

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
    } finally { rmSync(f.root, { recursive: true, force: true }); }
  });
}
