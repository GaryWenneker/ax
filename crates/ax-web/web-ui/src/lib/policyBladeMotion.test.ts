import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import { dirname, join } from 'node:path';
import { fileURLToPath } from 'node:url';
import { describe, it } from 'node:test';
import {
  POLICY_BLADE_DISMISS_MS,
  pointerKeepsPolicyBlade,
  policyDetailOpen,
  policyWorkspaceHostClass,
  selectionHidden,
} from './policyBladeMotion.ts';

describe('policy blade dismiss motion', () => {
  it('B2 open while selected or closing', () => {
    assert.equal(policyDetailOpen('rule-a', false), true);
    assert.equal(policyDetailOpen(null, true), true);
    assert.equal(policyDetailOpen(null, false), false);
    assert.equal(policyDetailOpen('', false), false);
  });

  it('U1 a linked item stays selected while the list loads', () => {
    assert.equal(selectionHidden('link-rule', [], true), false);
    assert.equal(selectionHidden('link-rule', ['other'], true), false);
  });

  it('U1 a loaded list hides a selection it does not show', () => {
    assert.equal(selectionHidden('link-rule', ['other'], false), true);
    assert.equal(selectionHidden('link-rule', [], false), true);
    assert.equal(selectionHidden('link-rule', ['other', 'link-rule'], false), false);
    assert.equal(selectionHidden(null, ['other'], false), false);
  });

  it('B2 host class marks closing', () => {
    assert.equal(policyWorkspaceHostClass(false), 'policy-inline-host');
    assert.equal(policyWorkspaceHostClass(true), 'policy-inline-host policy-inline-host--closing');
  });

  it('a click inside the properties dialog or its select menu keeps the blade open', () => {
    const host = { contains: () => false };
    const outside = { closest: () => null };
    assert.equal(pointerKeepsPolicyBlade(outside, host), false);
    assert.equal(pointerKeepsPolicyBlade(null, host), false);

    const field = { closest: (sel: string) => (sel.includes('.ax-modal') ? field : null) };
    assert.equal(pointerKeepsPolicyBlade(field, host), true);

    const option = { closest: (sel: string) => (sel.includes('.settings-select-menu') ? option : null) };
    assert.equal(pointerKeepsPolicyBlade(option, host), true);

    const text = { parentElement: field };
    assert.equal(pointerKeepsPolicyBlade(text, host), true);

    const inside = { closest: () => null };
    assert.equal(pointerKeepsPolicyBlade(inside, { contains: (node) => node === inside }), true);
  });

  it('B1 CSS defines slide-out matching slide-in duration', () => {
    assert.equal(POLICY_BLADE_DISMISS_MS, 320);
    const css = readFileSync(join(dirname(fileURLToPath(import.meta.url)), '../index.css'), 'utf8');
    assert.match(css, /@keyframes policy-blade-slide-out/);
    assert.match(css, /policy-inline-host--closing[\s\S]{0,400}policy-blade-slide-out 0\.32s/);
  });
});
