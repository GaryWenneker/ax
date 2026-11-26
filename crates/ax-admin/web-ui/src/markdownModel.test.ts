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
  assert.deepEqual(blocks[2], {
    type: 'list',
    ordered: false,
    items: [[{ kind: 'text', text: 'one' }], [{ kind: 'text', text: 'two ' }, { kind: 'code', text: 'x' }]],
  });
  assert.deepEqual(blocks[3], {
    type: 'list',
    ordered: true,
    items: [[{ kind: 'text', text: 'first' }], [{ kind: 'text', text: 'second' }]],
  });
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
  assert.equal(parseMarkdown('3: see below\nnext')[0]?.type, 'text');
});

test('tables need a rule line under the header', () => {
  const blocks = parseMarkdown('| Tool | ms |\n|---|---|\n| ax_sync | 12 |\n| `ax_index` | 900 |\n\ntail');
  assert.equal(blocks.length, 2);
  assert.deepEqual(blocks[0], {
    type: 'table',
    header: [[{ kind: 'text', text: 'Tool' }], [{ kind: 'text', text: 'ms' }]],
    rows: [
      [[{ kind: 'text', text: 'ax_sync' }], [{ kind: 'text', text: '12' }]],
      [[{ kind: 'code', text: 'ax_index' }], [{ kind: 'text', text: '900' }]],
    ],
  });
  assert.equal(parseMarkdown('| not | a table |')[0]?.type, 'text');
});

test('empty and whitespace-only text produce no blocks', () => {
  assert.deepEqual(parseMarkdown(''), []);
  assert.deepEqual(parseMarkdown('\n \n\r\n'), []);
});

test('a long line of blanks under a table row is handled in linear time', () => {
  const started = performance.now();
  const blocks = parseMarkdown(`| a | b |\n${' '.repeat(400_000)}\n| c | d |`);
  assert.ok(performance.now() - started < 500, 'parsing must not backtrack');
  assert.equal(blocks.length, 2);
  assert.ok(blocks.every((block) => block.type === 'text'));
});

test('table rule lines with alignment marks and missing outer bars are recognised', () => {
  for (const rule of ['|---|---|', '| :--- | ---: |', '---|---', ':---:|:---:']) {
    assert.equal(parseMarkdown(`| a | b |\n${rule}\n| 1 | 2 |`)[0]?.type, 'table', rule);
  }
  for (const notRule of ['| -- | -- |', '| a | b |', '|   |   |', '']) {
    assert.notEqual(parseMarkdown(`| a | b |\n${notRule}\n| 1 | 2 |`)[0]?.type, 'table', notRule);
  }
});
