import test from 'node:test';
import assert from 'node:assert/strict';
import { readFileSync, readdirSync, existsSync } from 'node:fs';
import { dirname, join, resolve, relative } from 'node:path';
import { fileURLToPath } from 'node:url';

const site = resolve(dirname(fileURLToPath(import.meta.url)), '..');
const walk = (dir: string): string[] =>
  readdirSync(dir, { withFileTypes: true }).flatMap(e =>
    e.isDirectory() ? walk(join(dir, e.name)) : [join(dir, e.name)]);

const sourceExt = /\.(astro|ts|js|mjs|mdx?)$/;

function localImports(file: string): string[] {
  const text = readFileSync(file, 'utf8');
  const specs = [...text.matchAll(/(?:import|from)\s*\(?\s*['"](\.{1,2}\/[^'"]+)['"]/g)].map(m => m[1]);
  return specs.flatMap(spec => {
    const base = resolve(dirname(file), spec);
    const candidates = [base, `${base}.ts`, `${base}.js`, `${base}.astro`];
    const hit = candidates.find(c => existsSync(c) && sourceExt.test(c));
    return hit ? [hit] : [];
  });
}

test('every component and client script is reachable from a page, layout or the Starlight config', () => {
  const config = readFileSync(join(site, 'astro.config.mjs'), 'utf8');
  const roots = [
    ...walk(join(site, 'src/pages')).filter(f => sourceExt.test(f)),
    ...walk(join(site, 'src/content')).filter(f => /\.mdx?$/.test(f)),
    ...[...config.matchAll(/['"]\.\/(src\/[^'"]+\.astro)['"]/g)].map(m => join(site, m[1])),
  ];
  const seen = new Set<string>();
  const queue = [...roots];
  while (queue.length) {
    const file = queue.pop()!;
    if (seen.has(file)) continue;
    seen.add(file);
    queue.push(...localImports(file));
  }
  const candidates = [
    ...walk(join(site, 'src/components')).filter(f => f.endsWith('.astro')),
    // Compiled .js/.d.ts siblings of a .ts source are build output, not sources.
    ...walk(join(site, 'src/scripts')).filter(f =>
      /\.(ts|js)$/.test(f) && !(/(\.d\.ts|\.js)$/.test(f) && existsSync(f.replace(/(\.d\.ts|\.js)$/, '.ts')))),
  ];
  const unused = candidates.filter(f => !seen.has(f)).map(f => relative(site, f)).sort();
  assert.deepEqual(unused, [], `unused site sources: ${unused.join(', ')}`);
});

const exportedNames = (file: string): Set<string> => new Set(
  [...readFileSync(file, 'utf8').matchAll(
    /export\s+(?:declare\s+)?(?:async\s+)?(?:function\*?|const|let|var|class|type|interface|enum)\s+([A-Za-z_$][\w$]*)/g,
  )].map(m => m[1]));

test('no compiled .js or .d.ts sibling exports a symbol its .ts source no longer has', () => {
  const stale: string[] = [];
  for (const file of walk(join(site, 'src'))) {
    if (!/\.(js|d\.ts)$/.test(file)) continue;
    const source = file.replace(/(\.d\.ts|\.js)$/, '.ts');
    if (!existsSync(source)) continue;
    const current = exportedNames(source);
    for (const name of exportedNames(file)) {
      if (!current.has(name)) stale.push(`${relative(site, file)}: ${name}`);
    }
  }
  assert.deepEqual(stale.sort(), [], `compiled siblings export removed symbols: ${stale.join(', ')}`);
});
