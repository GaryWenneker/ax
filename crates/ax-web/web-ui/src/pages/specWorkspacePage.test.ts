import assert from 'node:assert/strict';
import test from 'node:test';
import { readFileSync } from 'node:fs';
import { fileURLToPath } from 'node:url';
import { readGlobalStyles } from '../../test-support/globalStyles.ts';

// Workspace behaviour is tested in ../lib/specWorkspace.test.ts and the
// Playwright specs. This check only pins the responsive layout rules, which
// have no runtime test in this suite.

const css = readGlobalStyles();

test('the workspace becomes drawers and then one pane on small screens', () => {
  assert.match(css, /\.specws \{[^}]*grid-template-columns/);
  assert.match(css, /@media \(max-width: 1280px\) \{[^@]*\.specws-context \{[^}]*position: absolute/);
  assert.match(css, /@media \(max-width: 899px\) \{[^@]*\.specws-explorer \{[^}]*position: absolute/);
  assert.match(css, /@media \(max-width: 720px\) \{[^@]*\.specws-mobile-bar/);
});

test('the drawer toggle asks the same widths the stylesheet uses for its drawers', () => {
  const source = readFileSync(fileURLToPath(new URL('./SpecWorkspace.tsx', import.meta.url)), 'utf8');
  const query = (name: string) => {
    const found = source.match(new RegExp(`${name} = '(\\(max-width: \\d+px\\))'`));
    assert.ok(found, `SpecWorkspace.tsx defines ${name}`);
    return found[1];
  };
  // The @media bound of the block that opens each panel as a drawer.
  const cssBound = (drawerClass: string) => {
    const blocks = [...css.matchAll(/@media (\(max-width: \d+px\)) \{([^@]*)/g)];
    const owner = blocks.find((block) => block[2].includes(drawerClass));
    assert.ok(owner, `a @media block shows ${drawerClass}`);
    return owner[1];
  };
  assert.equal(query('EXPLORER_DRAWER_QUERY'), cssBound('.specws--drawer-explorer'));
  assert.equal(query('CONTEXT_DRAWER_QUERY'), cssBound('.specws--drawer-context'));
  assert.match(source, /panel === 'explorer' \? EXPLORER_DRAWER_QUERY : CONTEXT_DRAWER_QUERY/);
});
