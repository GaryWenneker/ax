import assert from 'node:assert/strict';
import { readdirSync, readFileSync } from 'node:fs';
import { matchesGlob } from 'node:path';
import test from 'node:test';

const ROOT = new URL('../', import.meta.url);

function testFiles(dir: string): string[] {
  const out: string[] = [];
  for (const entry of readdirSync(new URL(dir, ROOT), { withFileTypes: true })) {
    const rel = `${dir}${entry.name}`;
    if (entry.isDirectory()) out.push(...testFiles(`${rel}/`));
    else if (/\.test\.ts$/.test(entry.name)) out.push(rel);
  }
  return out;
}

test('npm test runs every unit test file under src', () => {
  const pkg = JSON.parse(readFileSync(new URL('package.json', ROOT), 'utf8')) as { scripts: Record<string, string> };
  const args = pkg.scripts.test.split(/\s+/).map((a) => a.replace(/^["']|["']$/g, ''));
  const patterns = args.filter((a) => a.startsWith('src/'));
  const missed = testFiles('src/').filter((file) => !patterns.some((p) => p === file || matchesGlob(file, p)));
  assert.deepEqual(missed, []);
});
