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
  // Unused since the docs redesign in 0d013fcf while the site design is still being
  // worked on; wire each one up again or delete it, then drop it from this list.
  const parked = [
    'src/components/DeckBackdrop.astro',
    'src/components/MotionControl.astro',
    'src/components/ScrollReveal.astro',
    'src/scripts/deck-backdrop.ts',
    'src/scripts/motion.js',
    'src/scripts/scroll-reveal.js',
  ];
  const stale = parked.filter(f => !unused.includes(f));
  assert.deepEqual(stale, [], `parked files are in use again or gone; remove them from the list: ${stale.join(', ')}`);
  const unexpected = unused.filter(f => !parked.includes(f));
  assert.deepEqual(unexpected, [], `unused site sources: ${unexpected.join(', ')}`);
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
