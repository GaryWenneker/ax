import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import test from 'node:test';
import { readGlobalStyles } from '../../test-support/globalStyles.ts';

const ws = readFileSync(new URL('./SpecWorkspace.tsx', import.meta.url), 'utf8');
const page = readFileSync(new URL('./Specs.tsx', import.meta.url), 'utf8');
const css = readGlobalStyles();

test('the workspace has an explorer, a document pane, and a context panel', () => {
  assert.match(ws, /aria-label="Specification explorer"/);
  assert.match(ws, /aria-label="Document"/);
  assert.match(ws, /aria-label="Context and evidence"/);
});

test('the explorer nests plans under specs and keeps Needs review apart', () => {
  assert.match(ws, /ItemList rows className="calm-list specws-tree"/);
  assert.match(ws, /ItemGroupHeader/);
  assert.match(ws, /label="Specifications"/);
  assert.match(ws, /label="Needs review"/);
  assert.match(ws, /Unclassified/);
  assert.match(ws, /aria-expanded=\{/);
  assert.doesNotMatch(ws, /specs-jump/);
});

test('selecting a spec shows its plans in the explorer and at the top of its overview', () => {
  assert.match(ws, /setExpanded\(\(current\) => expandOnOpen\(current, doc\)\)/);
  assert.match(ws, /className="specws-overview-plans"/);
  assert.match(ws, /Plans \(\{openSpec\.plans\.length\}\)/);
  assert.ok(ws.indexOf('specws-overview-plans') < ws.indexOf('Latest text, not approved yet'), 'plans come before the text');
});

test('the document pane has the spec tabs and hosts the existing editor', () => {
  for (const label of ['Overview', 'Content', 'Plans', 'History', 'Validation', 'Dependencies']) {
    assert.match(ws, new RegExp(`label: '${label}'`));
  }
  assert.match(ws, /role="tablist"/);
  assert.match(ws, /\{editor\}/);
  assert.match(ws, /checklistLineCount/);
  assert.match(ws, /not tracked yet/);
});

test('the context panel names every evidence tab and never reads missing data as no history', () => {
  for (const label of ['Activity', 'Sessions', 'Turns', 'Memory', 'Logging', 'Decisions', 'Rules & Skills', 'Context Manifest']) {
    assert.match(ws, new RegExp(`label: '${label}'`));
  }
  assert.match(ws, /Not indexed yet/);
  assert.match(ws, /does not mean there is no history/);
  assert.match(ws, /page: 'logging', session/);
  assert.match(ws, /page: 'memory', session/);
});

test('new plans are created in a modal, not an inline form', () => {
  assert.match(ws, /<ModalShell/);
  assert.match(ws, /title="New plan"/);
  assert.doesNotMatch(page, /className="specs-alt-form"/);
});

test('the page opens on the workspace and keeps the current list as All documents', () => {
  assert.match(page, /<h1>Specs &amp; Plans<\/h1>/);
  assert.match(page, /useState<SpecsView>\(initialView\(\)\)/);
  assert.match(page, /'All documents'/);
  assert.match(page, /<SpecWorkspace/);
});

test('the workspace becomes drawers and then one pane on small screens', () => {
  assert.match(css, /\.specws \{[^}]*grid-template-columns/);
  assert.match(css, /@media \(max-width: 1280px\) \{[^@]*\.specws-context \{[^}]*position: absolute/);
  assert.doesNotMatch(css, /@media \(max-width: 1280px\) \{[^@]*\.specws-explorer \{[^}]*position: absolute/);
  assert.match(css, /@media \(max-width: 900px\) \{[^@]*\.specws-explorer \{[^}]*position: absolute/);
  assert.match(css, /@media \(max-width: 720px\) \{[^@]*\.specws-mobile-bar/);
  assert.match(ws, /\(max-width: 900px\)/);
  assert.match(ws, /\(max-width: 1280px\)/);
});
