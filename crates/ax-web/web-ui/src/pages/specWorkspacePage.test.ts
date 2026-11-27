import assert from 'node:assert/strict';
import test from 'node:test';
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
