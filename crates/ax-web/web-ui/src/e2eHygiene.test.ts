import assert from 'node:assert/strict';
import { readdirSync, readFileSync } from 'node:fs';
import test from 'node:test';

const e2eDir = new URL('../e2e/', import.meta.url);
const specs = readdirSync(e2eDir)
  .filter((name) => name.endsWith('.spec.ts'))
  .map((name) => ({ name, text: readFileSync(new URL(name, e2eDir), 'utf8') }));

test('e2e specs are found', () => {
  assert.ok(specs.length > 0);
});

test('no e2e spec appends to a file on disk', () => {
  // Playwright runs against the developer's own Command Center by default, so a
  // spec that appends to a file changes real project data. Specs serve fixture
  // data through page.route or write into their own temp directory instead.
  for (const { name, text } of specs) {
    assert.equal(/\bappendFileSync\b|\bappendFile\(/.test(text), false, name);
  }
});

test('no e2e spec writes into a path the live server reports', () => {
  for (const { name, text } of specs) {
    const asksForPath = /request\.get\(\s*['"`][^'"`]*\/path['"`]/.test(text);
    const writes = /\bfs\.(write|append)\w*\(/.test(text);
    assert.equal(asksForPath && writes, false, name);
  }
});
