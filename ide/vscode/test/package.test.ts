import { test } from 'node:test';
import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import { dirname, join } from 'node:path';
import { fileURLToPath } from 'node:url';

const manifest = JSON.parse(
  readFileSync(join(dirname(fileURLToPath(import.meta.url)), '..', 'package.json'), 'utf8'),
) as { extensionKind?: string[] };

test('command center runs in the local UI host so every window sees it', () => {
  assert.deepEqual(manifest.extensionKind, ['ui']);
});
