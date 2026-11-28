import test from 'node:test';
import assert from 'node:assert/strict';
import { readFileSync, readdirSync } from 'node:fs';
import path from 'node:path';
import { fileURLToPath } from 'node:url';

const site = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '..');
const read = (p: string) => readFileSync(path.join(site, p), 'utf8');
const adminPages = readdirSync(path.join(site, 'src/pages/admin')).filter(f => f.endsWith('.astro'));

test('third-party scripts on admin pages carry Subresource Integrity and stay unbundled', () => {
  for (const page of adminPages) {
    const source = read(`src/pages/admin/${page}`);
    for (const [tag] of source.matchAll(/<script\b[^>]*\bsrc="https?:\/\/[^"]+"[^>]*>/g)) {
      // Without is:inline Astro rewrites the tag into a bundled `import`, dropping integrity.
      assert.match(tag, /\bis:inline\b/, `${page}: ${tag} must be is:inline`);
      assert.match(tag, /\bintegrity="sha(256|384|512)-[A-Za-z0-9+/]+=*"/, `${page}: ${tag} needs integrity`);
      assert.match(tag, /\bcrossorigin="anonymous"/, `${page}: ${tag} needs crossorigin`);
    }
  }
});

/** The Content-Security-Policy netlify.toml sets for a path pattern. */
function cspFor(pattern: string): string | undefined {
  const toml = read('netlify.toml');
  const blocks = toml.split('[[headers]]').slice(1);
  const block = blocks.find(b => b.includes(`for = "${pattern}"`));
  return block?.match(/Content-Security-Policy\s*=\s*"([^"]+)"/)?.[1];
}
const directive = (csp: string, name: string) =>
  csp.split(';').map(s => s.trim()).find(s => s.startsWith(name + ' '))?.split(/\s+/).slice(1) ?? [];

test('the admin dashboard sends a CSP that only allows its own scripts and the pinned CDN', () => {
  const csp = cspFor('/admin/dashboard/*');
  assert.ok(csp, 'netlify.toml needs a Content-Security-Policy for /admin/dashboard/*');
  assert.deepEqual(directive(csp, 'script-src').sort(), ["'self'", 'https://cdn.jsdelivr.net'].sort());
  assert.deepEqual(directive(csp, 'connect-src'), ["'self'"]);
  assert.deepEqual(directive(csp, 'object-src'), ["'none'"]);
  assert.deepEqual(directive(csp, 'base-uri'), ["'none'"]);
  assert.deepEqual(directive(csp, 'frame-ancestors'), ["'none'"]);
});

test('the admin login page sends a CSP that blocks third-party scripts', () => {
  const csp = cspFor('/admin/login/*');
  assert.ok(csp, 'netlify.toml needs a Content-Security-Policy for /admin/login/*');
  // Astro inlines the small login script, so 'unsafe-inline' is needed; no other origin is.
  assert.deepEqual(directive(csp, 'script-src').sort(), ["'self'", "'unsafe-inline'"].sort());
  assert.deepEqual(directive(csp, 'connect-src'), ["'self'"]);
  assert.deepEqual(directive(csp, 'frame-ancestors'), ["'none'"]);
});

test('admin pages keep the admin token out of page scripts and let password managers fill the login', () => {
  for (const page of adminPages) {
    const source = read(`src/pages/admin/${page}`);
    assert.doesNotMatch(source, /sessionStorage|localStorage|document\.cookie|Authorization/, `${page}: the HttpOnly cookie carries the token`);
  }
  const loginPage = read('src/pages/admin/login.astro');
  assert.doesNotMatch(loginPage, /autocomplete="off"/);
  assert.match(loginPage, /autocomplete="current-password"/);
});
