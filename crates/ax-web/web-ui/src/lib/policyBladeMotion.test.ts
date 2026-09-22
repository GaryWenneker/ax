import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import { dirname, join } from 'node:path';
import { fileURLToPath } from 'node:url';
import { describe, it } from 'node:test';
import {
  POLICY_BLADE_DISMISS_MS,
  policyDetailOpen,
  policyWorkspaceHostClass,
} from './policyBladeMotion.ts';

describe('policy blade dismiss motion', () => {
  it('B2 open while selected or closing', () => {
    assert.equal(policyDetailOpen('rule-a', false), true);
    assert.equal(policyDetailOpen(null, true), true);
    assert.equal(policyDetailOpen(null, false), false);
    assert.equal(policyDetailOpen('', false), false);
  });

  it('B2 host class marks closing', () => {
    assert.equal(policyWorkspaceHostClass(false), 'policy-inline-host');
    assert.equal(policyWorkspaceHostClass(true), 'policy-inline-host policy-inline-host--closing');
  });

  it('B1 CSS defines slide-out matching slide-in duration', () => {
    assert.equal(POLICY_BLADE_DISMISS_MS, 320);
    const css = readFileSync(join(dirname(fileURLToPath(import.meta.url)), '../index.css'), 'utf8');
    assert.match(css, /@keyframes policy-blade-slide-out/);
    assert.match(css, /policy-inline-host--closing[\s\S]{0,400}policy-blade-slide-out 0\.32s/);
  });
});
