import assert from 'node:assert/strict';
import { test } from 'node:test';
import { parseInline, parseMarkdown } from './markdownModel.ts';

test('inline code and strong text are separated from plain text', () => {
  assert.deepEqual(parseInline('call `ax_sync` then **wait** here'), [
    { kind: 'text', text: 'call ' },
    { kind: 'code', text: 'ax_sync' },
    { kind: 'text', text: ' then ' },
    { kind: 'strong', text: 'wait' },
    { kind: 'text', text: ' here' },
  ]);
  assert.deepEqual(parseInline(''), []);
  assert.deepEqual(parseInline('a * b ` c'), [{ kind: 'text', text: 'a * b ` c' }]);
});

test('markup in tool output stays text', () => {
  const blocks = parseMarkdown('<script>alert(1)</script>\n<ax_policy note="x">');
  assert.deepEqual(blocks, [
    { type: 'text', inline: [{ kind: 'text', text: '<script>alert(1)</script>\n<ax_policy note="x">' }] },
  ]);
});

test('headings, lists and paragraphs', () => {
  const blocks = parseMarkdown('# Status\n\nIndexed 12 files.\nUp to date.\n\n- one\n- two `x`\n\n1. first\n2) second\n');
  assert.deepEqual(blocks.map((b) => b.type), ['heading', 'text', 'list', 'list']);
  assert.deepEqual(blocks[0], { type: 'heading', level: 1, inline: [{ kind: 'text', text: 'Status' }] });
  assert.deepEqual(blocks[1], { type: 'text', inline: [{ kind: 'text', text: 'Indexed 12 files.\nUp to date.' }] });
  assert.equal(blocks[2].type === 'list' && blocks[2].ordered, false);
  assert.equal(blocks[2].type === 'list' && blocks[2].items.length, 2);
  assert.equal(blocks[3].type === 'list' && blocks[3].ordered, true);
  assert.equal(blocks[3].type === 'list' && blocks[3].items.length, 2);
});

test('fenced code keeps its text, also when the fence is never closed', () => {
  assert.deepEqual(parseMarkdown('before\n```rust\nfn a() {}\n# not a heading\n```\nafter'), [
    { type: 'text', inline: [{ kind: 'text', text: 'before' }] },
    { type: 'code', language: 'rust', text: 'fn a() {}\n# not a heading' },
    { type: 'text', inline: [{ kind: 'text', text: 'after' }] },
  ]);
  assert.deepEqual(parseMarkdown('```\nopen'), [{ type: 'code', language: '', text: 'open' }]);
});

test('numbered source lines become one source block', () => {
  const blocks = parseMarkdown('crates/a.rs\n  10 | fn a() {\n  11 |     b();\n  12 | }\ndone');
  assert.deepEqual(blocks, [
    { type: 'text', inline: [{ kind: 'text', text: 'crates/a.rs' }] },
    { type: 'code', language: 'source', text: '  10 | fn a() {\n  11 |     b();\n  12 | }' },
    { type: 'text', inline: [{ kind: 'text', text: 'done' }] },
  ]);
  // One numbered line alone is ordinary text ("3: see below").
  assert.equal(parseMarkdown('3: see below\nnext')[0].type, 'text');
});

test('tables need a rule line under the header', () => {
  const blocks = parseMarkdown('| Tool | ms |\n|---|---|\n| ax_sync | 12 |\n| `ax_index` | 900 |\n\ntail');
  assert.equal(blocks.length, 2);
  const table = blocks[0];
  assert.equal(table.type, 'table');
  if (table.type !== 'table') return;
  assert.deepEqual(table.header, [[{ kind: 'text', text: 'Tool' }], [{ kind: 'text', text: 'ms' }]]);
  assert.equal(table.rows.length, 2);
  assert.deepEqual(table.rows[1][0], [{ kind: 'code', text: 'ax_index' }]);
  assert.equal(parseMarkdown('| not | a table |')[0].type, 'text');
});

test('empty and whitespace-only text produce no blocks', () => {
  assert.deepEqual(parseMarkdown(''), []);
  assert.deepEqual(parseMarkdown('\n \n\r\n'), []);
});
