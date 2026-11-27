import assert from 'node:assert/strict';
import { test } from 'node:test';
import { highlightLine, loadLanguage, mapLanguage } from './sourceHighlight.ts';

test('mapLanguage maps ax language ids to highlight.js names, case-insensitively', () => {
  assert.equal(mapLanguage('rust'), 'rust');
  assert.equal(mapLanguage('Rust'), 'rust');
  assert.equal(mapLanguage('pascal'), 'delphi');
  assert.equal(mapLanguage('unknown'), '');
});

test('an unknown language is escaped, never injected as HTML', () => {
  assert.equal(highlightLine('<img src=x onerror=alert(1)> & co', 'unknown'), '&lt;img src=x onerror=alert(1)&gt; &amp; co');
});

test('a known language is highlighted and still escapes markup in the source', async () => {
  assert.equal(await loadLanguage('rust'), true);
  const html = highlightLine('let s = "<b>";', 'rust');
  assert.match(html, /<span class="hljs-keyword">let<\/span>/);
  assert.ok(!html.includes('<b>'));
  assert.ok(html.includes('&lt;b&gt;'));
});

test('an empty line renders as one space so the row keeps its height', () => {
  assert.equal(highlightLine('', 'rust'), ' ');
});

test('a grammar loads on demand; until then the line is shown escaped', async () => {
  const line = 'local x = "<b>"';
  assert.equal(highlightLine(line, 'luau'), 'local x = "&lt;b&gt;"');
  assert.equal(await loadLanguage('luau'), true);
  assert.match(highlightLine(line, 'luau'), /<span class="hljs-keyword">local<\/span>/);
  assert.equal(await loadLanguage('unknown'), false);
});
