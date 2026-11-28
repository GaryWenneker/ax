import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import test from 'node:test';
import { fileURLToPath } from 'node:url';

// The saver's own behavior is tested in ../lib/latestSaver.test.ts. This pins
// how Settings owns it: React's StrictMode runs an effect's cleanup and then the
// effect again, so a saver made during render and disposed in a cleanup would
// stay disposed and never save the slider again.

const source = readFileSync(fileURLToPath(new URL('./Settings.tsx', import.meta.url)), 'utf8');

test('Settings makes the context-budget saver in an effect and disposes that same one', () => {
  const effect = source.match(/useEffect\(\(\) => \{\s*const saver = createLatestSaver<number>\([\s\S]*?\n {2}\}, \[\]\);/);
  assert.ok(effect, 'the saver is created inside a mount effect');
  assert.match(effect[0], /budgetSaver\.current = saver;/);
  assert.match(effect[0], /return \(\) => \{\s*saver\.dispose\(\);/);
  assert.equal(source.match(/createLatestSaver</g)?.length, 1, 'no second saver is made during render');
});
