import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import test from 'node:test';

const page = [
  readFileSync(new URL('./Specs.tsx', import.meta.url), 'utf8'),
  readFileSync(new URL('./SpecsDetail.tsx', import.meta.url), 'utf8'),
].join('\n');
const routes = readFileSync(new URL('../lib/routes.ts', import.meta.url), 'utf8');
const nav = readFileSync(new URL('../lib/nav.ts', import.meta.url), 'utf8');
const css = readFileSync(new URL('../index.css', import.meta.url), 'utf8');

test('the specs page lists path, status, project, and session through the calm list', () => {
  assert.match(page, /className="calm-list specs-list"/);
  assert.match(page, /listId="specs"/);
  assert.match(page, /aria-label="Open version"/);
  assert.doesNotMatch(page, /listId="spec-revisions"/);
  assert.doesNotMatch(page, /listId="specs" rows/);
  assert.match(page, /label: 'Version'/);
  assert.match(page, /label: 'Logging'/);
  assert.match(page, /label: 'Memory'/);
  assert.match(page, /fetch\('\/api\/specs\?refresh=1'\)/);
  assert.match(page, /spec\.path \|\| 'No file'/);
  assert.match(page, /page-item-badge/);
  assert.match(page, /memory-blade specs-blade/);
  assert.match(page, /aria-label="Close"/);
  assert.match(css, /\.memory-blade\.specs-blade \{[^}]*policy-blade-slide-in/);
  assert.doesNotMatch(page, /specs-workspace/);
  assert.doesNotMatch(page, /\$\{spec\.path\} \$\{spec\.projectId\} \$\{spec\.sessions\}/);
  assert.match(page, /Approve v/);
  assert.doesNotMatch(page, /Test /);
  assert.match(routes, /specs: '\/specs'/);
  assert.match(nav, /id: 'specs', label: 'Specs & Plans'/);
});

test('the specs page can filter, edit, and walk history', () => {
  assert.match(page, /aria-label="Search specs"/);
  assert.match(page, /Plans/);
  assert.match(page, /Specs/);
  assert.match(page, /Clear filters/);
  assert.match(page, /Save\s*<\/button>/);
  assert.match(page, /disabled=\{!dirty\}/);
  assert.match(page, /loadToken\.current/);
  assert.match(page, /catch/);
  assert.doesNotMatch(page, /Revise/);
  assert.doesNotMatch(page, /function lineDiff/);
  assert.match(page, /MarkdownEditor/);
  assert.match(page, /label: 'Markdown'/);
  assert.match(page, /label: 'WYSIWYG'/);
  assert.match(page, /label: 'Split'/);
  assert.match(page, /label: 'Diff'/);
  assert.match(page, /\{item\.label\}/);
  assert.match(page, /No previous version/);
  assert.match(page, /No changes from v/);
  assert.doesNotMatch(page, /Previous\s*<\/button>/);
  assert.doesNotMatch(page, /Next\s*<\/button>/);
  assert.doesNotMatch(page, /<select/);
  assert.match(page, /specs-version-chip/);
  assert.match(page, /Unsaved changes/);
  assert.match(page, /basedOn: openVersion/);
  assert.match(page, /page: 'logging'/);
  assert.match(page, /page: 'memory'/);
  assert.match(page, /className="specs-jump"/);
  assert.match(page, />\s*Logging\s*</);
  assert.match(page, />\s*Memory\s*</);
  assert.match(page, /specs-scroll-sentinel/);
  assert.match(page, /IntersectionObserver/);
  assert.match(css, /\.specs-page \.specs-list-pane \{[^}]*overflow:\s*auto/);
  assert.match(css, /\.specs-page \.specs-list :is\(\.column-header, \.portal-data-row\)/);
  assert.match(css, /@container ax-list \(max-width: 720px\)/);
  assert.match(page, /sortSpecs\(visible, 'newest'\)/);
  assert.match(page, /formatSpecDate\(spec\.updatedAt\)/);
  assert.match(page, /specVersionLabel\(spec\)/);
  assert.match(page, /className="specs-col-version"/);
  assert.match(page, /label: 'Version', minWidth: 260/);
  assert.match(css, /\.specs-col-version \{[^}]*flex-wrap:\s*nowrap/);
  assert.match(page, /className="specs-col-logging"/);
  assert.match(page, /className="specs-col-memory"/);
  assert.match(css, /\.memory-blade\.specs-blade \{[^}]*left:\s*8rem/);
  assert.doesNotMatch(css, /memory-blade\.specs-blade\) > \.container/);
});
