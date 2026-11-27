import test from 'node:test';
import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';

const script = (name: string) => readFileSync(new URL(`../scripts/${name}`, import.meta.url), 'utf8');

test('site-design capture writes only into its own folder, never manifest-tracked screenshots', () => {
  const source = script('capture-site-design.mjs');
  // Every screenshot path must be built from `output` (public/screenshots/site-design).
  const bases = [...source.matchAll(/page\.screenshot\(\{\s*path:\s*path\.join\((\w+)/g)].map(m => m[1]);
  const calls = [...source.matchAll(/page\.screenshot\(\{\s*path:/g)].length;
  assert.ok(calls > 0, 'expected screenshot calls');
  assert.equal(bases.length, calls, 'every screenshot path must be a path.join(...)');
  for (const base of bases) assert.equal(base, 'output', `screenshot written outside site-design/ (base ${base})`);
  assert.doesNotMatch(source, /home-install-/);
});

test('the homepage capture that owns home-install screenshots records them in the manifest', () => {
  const source = script('capture-homepage.mjs');
  assert.match(source, /home-install-/);
  assert.match(source, /manifest\.json/);
  assert.match(source, /imageHash/);
});
