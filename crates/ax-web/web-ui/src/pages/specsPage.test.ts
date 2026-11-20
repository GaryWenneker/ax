import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import test from 'node:test';

const page = readFileSync(new URL('./Specs.tsx', import.meta.url), 'utf8');
const routes = readFileSync(new URL('../lib/routes.ts', import.meta.url), 'utf8');
const nav = readFileSync(new URL('../lib/nav.ts', import.meta.url), 'utf8');
const css = readFileSync(new URL('../index.css', import.meta.url), 'utf8');

test('the specs page lists path, status, project, and session through the calm list', () => {
  assert.match(page, /className="calm-list"/);
  assert.match(page, /listId="specs"/);
  assert.match(page, /listId="spec-revisions"/);
  assert.match(page, /fetch\('\/api\/specs'\)/);
  assert.match(page, /label: 'Path'/);
  assert.match(page, /label: 'Status'/);
  assert.match(page, /\$\{spec\.path\} \$\{spec\.projectId\} \$\{spec\.sessions\}/);
  assert.match(page, /spec\.projectId/);
  assert.match(page, /spec\.sessions/);
  assert.match(page, /Approve v/);
  assert.doesNotMatch(page, /Test /);
  assert.match(routes, /specs: '\/specs'/);
  assert.match(nav, /id: 'specs', label: 'Specs'/);
});

test('the specs page can filter, edit, and walk history', () => {
  assert.match(page, /aria-label="Search specs"/);
  assert.match(page, /Plans/);
  assert.match(page, /Specs/);
  assert.match(page, /Clear filters/);
  assert.match(page, /Save\s*<\/button>/);
  assert.match(page, /Revise\s*<\/button>/);
  assert.match(page, /Previous\s*<\/button>/);
  assert.match(page, /Next\s*<\/button>/);
  assert.match(page, /function lineDiff/);
  assert.match(page, /Unsaved changes/);
  assert.match(page, /basedOn: openVersion/);
  assert.match(page, /\/logging\?session=/);
  assert.match(page, /\/memory\?q=/);
  assert.match(page, />\s*Logging\s*</);
  assert.match(page, />\s*Memory\s*</);
  assert.match(css, /\.specs-page \.column-list \{[^}]*overflow:\s*auto/);
});
