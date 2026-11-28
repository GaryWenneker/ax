import assert from 'node:assert/strict';
import { readdirSync, readFileSync } from 'node:fs';
import test from 'node:test';

const SRC = new URL('./', import.meta.url);

function walk(dir: URL, out: URL[] = []): URL[] {
  for (const entry of readdirSync(dir, { withFileTypes: true })) {
    const url = new URL(entry.name + (entry.isDirectory() ? '/' : ''), dir);
    if (entry.isDirectory()) walk(url, out);
    else out.push(url);
  }
  return out;
}

const files = walk(SRC);
const cssFiles = files.filter((u) => u.pathname.endsWith('.css'));
const codeFiles = files.filter((u) => /\.tsx?$/.test(u.pathname) && !/\.test\.tsx?$/.test(u.pathname));

/** Custom properties a stylesheet declares or the code sets (applyTheme, inline style objects). */
function definedProperties(): Set<string> {
  const defined = new Set<string>();
  for (const file of cssFiles) {
    for (const m of readFileSync(file, 'utf8').matchAll(/(--[a-zA-Z0-9-]+)\s*:/g)) defined.add(m[1]);
  }
  for (const file of codeFiles) {
    const text = readFileSync(file, 'utf8');
    for (const m of text.matchAll(/['"`](--[a-zA-Z0-9-]+)['"`]\s*[:,\]]/g)) defined.add(m[1]);
    for (const m of text.matchAll(/setProperty\(\s*['"`](--[a-zA-Z0-9-]+)/g)) defined.add(m[1]);
  }
  return defined;
}

test('every var(--token) in the stylesheets names a token that something defines', () => {
  const defined = definedProperties();
  const missing: string[] = [];
  for (const file of cssFiles) {
    const lines = readFileSync(file, 'utf8').split('\n');
    lines.forEach((line, i) => {
      for (const m of line.matchAll(/var\((--[a-zA-Z0-9-]+)/g)) {
        if (!defined.has(m[1])) missing.push(`${file.pathname.split('/src/').pop()}:${i + 1} ${m[1]}`);
      }
    });
  }
  // An undefined token makes the declaration fall back to its hard-coded (dark) value or to
  // transparent, which breaks the light theme and see-through overlays.
  assert.deepEqual(missing, []);
});
