import assert from 'node:assert/strict';
import { readdirSync, readFileSync } from 'node:fs';
import test from 'node:test';

import { globalStyleFiles } from '../test-support/globalStyles.ts';

test('index.css holds only the ordered imports of the style files', () => {
  const index = readFileSync(new URL('./index.css', import.meta.url), 'utf8');
  const rules = index.replace(/\/\*[\s\S]*?\*\//g, '').split('\n').map((l) => l.trim()).filter(Boolean);
  assert.ok(rules.length > 0);
  for (const rule of rules) assert.match(rule, /^@import '\.\/styles\/\d{2}-[a-z-]+\.css';$/);
});

test('every file in styles/ is imported once, in numbered order', () => {
  const imported = globalStyleFiles().map((u) => u.pathname.split('/').pop());
  const onDisk = readdirSync(new URL('./styles/', import.meta.url)).filter((f) => f.endsWith('.css')).sort();
  assert.deepEqual(imported, onDisk);
});
