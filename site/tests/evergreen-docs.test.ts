import assert from 'node:assert/strict';
import test from 'node:test';
import { readdirSync, readFileSync } from 'node:fs';
import path from 'node:path';
import { fileURLToPath } from 'node:url';

const root = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '../src/content/docs');
test('website documentation has no per-version update sections', () => {
  const violations: string[] = [];
  function scan(dir: string) {
    for (const entry of readdirSync(dir,{withFileTypes:true})) {
      const file = path.join(dir,entry.name);
      if (entry.isDirectory()) scan(file);
      else if (/\.mdx?$/.test(entry.name)) {
        const content = readFileSync(file,'utf8');
        const headings = content.match(/^#{1,6}\s+.*(?:what['’]?s new|earlier releases|release notes|changelog|\bv\d+\.\d+\.\d+\b).*$/gmi);
        if (headings) violations.push(`${path.relative(root,file)}: ${headings.join(', ')}`);
      }
    }
  }
  scan(root);
  assert.deepEqual(violations,[],'Document current behavior in place; do not add version-by-version updates');
});

test('README describes current behavior without a per-version update list', () => {
  const content = readFileSync(path.resolve(root, '../../../../README.md'), 'utf8');
  assert.doesNotMatch(content, /^\*\*(?:v\d+\.\d+\.\d+|Unreleased:)/m);
  assert.doesNotMatch(content, /\[What's new\]/i);
});
