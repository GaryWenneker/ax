import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import test from 'node:test';
import { readGlobalStyles } from '../../test-support/globalStyles.ts';

const panel = readFileSync(new URL('./ContinuityPanel.tsx', import.meta.url), 'utf8');
const css = readGlobalStyles();

const fields = [
  'External chat id',
  'External turn id',
  'Ax session id',
  'Ax turn id',
  'Agent provider',
  'Project id',
  'Created at',
  'Parent session',
  'Checkpoint',
  'Mode',
  'Target agent',
  'Task',
];

function rule(selector: string): string {
  const start = css.indexOf(`${selector} {`);
  assert.notEqual(start, -1, selector);
  return css.slice(start, css.indexOf('}', start));
}

test('the new-chat prompt box has a copy control', () => {
  assert.match(panel, /className="continuity-prompt"/);
  assert.match(panel, /aria-label="Copy prompt"/);
  assert.match(panel, /value=\{prompt\}/);
  assert.match(panel, /navigator\.clipboard\.writeText\(prompt\)/);
});

test('every continuity field stays an enabled control', () => {
  for (const field of fields) assert.equal(panel.includes(`>${field}`), true, field);
  assert.equal(panel.includes('readOnly'), false);
  assert.equal(panel.includes('disabled'), false);
  assert.match(panel, /aria-expanded=\{open\}/);
  assert.match(panel, /hidden=\{!open\}/);
});

test('the continuity panel scrolls with the page and does not clip its fields', () => {
  const block = rule('.continuity-panel');
  assert.equal(/position:\s*(fixed|sticky)/.test(block), false);
  assert.match(block, /position:\s*static/);
  assert.match(block, /overflow:\s*visible/);
  assert.match(block, /max-height:\s*none/);
  assert.match(rule('.continuity-grid'), /repeat\(6, minmax\(0, 1fr\)\)/);
  assert.match(rule('.continuity-grid label'), /font-size:\s*10px/);
  assert.match(rule('.continuity-grid input, .continuity-grid select, .continuity-grid textarea'), /font:\s*13px\/1\.3 "Share Tech Mono"/);
  assert.match(rule('.continuity-body'), /padding:\s*8px 10px 10px/);
  assert.equal(panel.includes('Test '), false);
  assert.match(rule('.continuity-actions .continuity-prepare'), /color:\s*var\(--accent-on-fill\)/);
  assert.equal(/statusbar-fg/.test(rule('.continuity-actions .continuity-prepare')), false);
  assert.match(panel, /btn btn-compact mcp-trace-icon-btn continuity-toggle/);
  assert.equal(/\.continuity-toggle[^{]*\{[^}]*(?:background|color)\s*:/.test(css), false);
  assert.match(rule('.mcp-trace-shell--page .mcp-trace-header-actions .btn'), /background:\s*transparent/);
  assert.match(rule('.mcp-trace-shell--page .mcp-trace-header-actions .btn'), /font-weight:\s*400/);
  const page = rule('.logging-page');
  assert.equal(/min-height:\s*calc\(100vh/.test(page), false);
  assert.match(page, /flex:\s*1 1 auto/);
  assert.match(page, /min-height:\s*0/);
  assert.match(page, /overflow:\s*auto/);
  const card = rule('.logging-page .mcp-trace-card--page');
  assert.match(card, /flex:\s*1 1 0/);
  assert.match(card, /min-height:\s*0/);
  assert.match(card, /max-height:\s*none/);
  const openPanel = rule('.logging-page .continuity-panel--docked:not(:has(.continuity-body[hidden]))');
  assert.match(openPanel, /flex:\s*1 1 0/);
  assert.match(openPanel, /min-height:\s*min-content/);
  assert.match(openPanel, /overflow:\s*visible/);
  assert.equal(/max-height:/.test(openPanel), false);
  assert.equal(/@media \(max-width: 900px\) \{\s*\.continuity-grid \{ grid-template-columns: 1fr; \}/.test(css), false);
});
