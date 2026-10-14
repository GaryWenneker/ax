#!/usr/bin/env node
import assert from 'node:assert/strict';
import { spawnSync } from 'node:child_process';
import { createHash } from 'node:crypto';
import { readFileSync, writeFileSync } from 'node:fs';
import { fileURLToPath } from 'node:url';

const root = fileURLToPath(new URL('../', import.meta.url));
const paths = ['install.sh', 'site/public/install.sh'];
const originals = paths.map(path => readFileSync(new URL(`../${path}`, import.meta.url), 'utf8'));
assert.equal(originals[0], originals[1], 'installer copies must match before mutation');
const source = originals[0];
const hash = value => createHash('sha256').update(value).digest('hex');
const replaceOnce = (before, after) => {
  assert.equal(source.split(before).length - 1, 1, 'mutation anchor must occur exactly once');
  return source.replace(before, after);
};
const mutations = process.argv.includes('--negative-control') ? [{
  name: 'unchanged source', test: 'persists PATH for a fresh zsh', source,
}] : [
  {
    name: 'omit persistent setup', test: 'persists PATH for a fresh zsh',
    source: replaceOnce('if ! persist_shell_path; then', 'if ! true; then'),
  },
  {
    name: 'omit apostrophe escaping', test: 'shell-sensitive binary paths',
    source: source.replace(/quote_shell_path\(\) \{\n[\s\S]*?\n\}/, `quote_shell_path() {\n  printf "'%s'" "$1"\n}`),
  },
  {
    name: 'persist before verification', test: 'failed binary verification',
    source: replaceOnce('confirm_ax_install "$version" || exit 1', 'persist_shell_path\nconfirm_ax_install "$version" || exit 1'),
  },
  {
    name: 'wrong fallback for macOS', test: 'unset login shell',
    source: replaceOnce('if [ "$os" = darwin ]; then ax_login_shell=zsh; else ax_login_shell=sh; fi', 'ax_login_shell=sh'),
  },
  {
    name: 'persist a relative path', test: 'relative binary directory',
    source: replaceOnce('case "$BIN_DIR" in /*) ;; *) BIN_DIR="$(pwd)/$BIN_DIR" ;; esac', ':'),
  },
];

const results = [];
try {
  for (const mutant of mutations) {
    if (!process.argv.includes('--negative-control')) assert.notEqual(mutant.source, source, 'mutant must change source');
    for (const path of paths) writeFileSync(new URL(`../${path}`, import.meta.url), mutant.source);
    for (const path of paths) assert.equal(hash(readFileSync(new URL(`../${path}`, import.meta.url))), hash(mutant.source));
    const run = spawnSync(process.execPath, [
      '--experimental-strip-types', '--test', '--test-reporter=tap',
      `--test-name-pattern=${mutant.test}`, 'site/tests/install-script.test.ts',
    ], { cwd: root, encoding: 'utf8', timeout: 60_000 });
    assert.ifError(run.error);
    assert.equal(run.signal, null, 'mutant test must finish without a signal');
    assert.equal(run.status, 1, `${mutant.name} survived (exit ${run.status})\n${run.stdout}\n${run.stderr}`);
    const failed = run.stdout.split('\n').filter(line => /^not ok \d+ - /.test(line));
    assert.ok(failed.some(line => line.includes(mutant.test)), `expected named assertion did not fail\n${run.stdout}`);
    assert.match(run.stdout, /# fail [1-9]\d*/);
    results.push({ name: mutant.name, sha256: hash(mutant.source), exit: run.status, failed });
  }
} finally {
  for (const [index, path] of paths.entries()) {
    writeFileSync(new URL(`../${path}`, import.meta.url), originals[index]);
    assert.equal(hash(readFileSync(new URL(`../${path}`, import.meta.url))), hash(originals[index]), 'source restore failed');
  }
}
console.log(JSON.stringify({ sourceSha256: hash(source), killed: results.length, results }, null, 2));
