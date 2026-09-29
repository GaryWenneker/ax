import assert from 'node:assert/strict';
import { describe, it } from 'node:test';
import {
  formatTraceTime,
  priorityMeta,
  ruleRowSubtitle,
  skillRowSubtitle,
  traceDirection,
  traceDirectionLabel,
  traceRowSubtitle,
} from './calmRows.ts';

describe('trace time', () => {
  it('drops milliseconds', () => {
    assert.equal(formatTraceTime('2026-09-28 14:03:21.005'), '2026-09-28 14:03:21');
  });
  it('keeps a time without milliseconds', () => {
    assert.equal(formatTraceTime('2026-09-28 14:03:21'), '2026-09-28 14:03:21');
  });
});

describe('trace direction', () => {
  it('maps inbound to prompt in', () => {
    assert.equal(traceDirection('inbound'), 'in');
  });
  it('maps outbound and preview to returned', () => {
    assert.equal(traceDirection('outbound'), 'out');
    assert.equal(traceDirection('preview'), 'out');
  });
  it('maps everything else to internal', () => {
    for (const k of ['enrich', 'internal', 'memory', 'policy', 'cli', 'error']) {
      assert.equal(traceDirection(k), 'internal');
    }
  });
  it('labels each direction', () => {
    assert.equal(traceDirectionLabel('in'), 'Prompt in');
    assert.equal(traceDirectionLabel('out'), 'Returned to agent');
    assert.equal(traceDirectionLabel('internal'), '');
  });
});

describe('rule row subtitle', () => {
  it('lists apply mode, globs, triggers and storage', () => {
    assert.equal(
      ruleRowSubtitle({ alwaysApply: true, globs: ['a', 'b'], triggers: ['x', 'y', 'z'] }, 'files'),
      'always apply · 2 globs · 3 triggers · MD',
    );
  });

  it('uses singular nouns and skips empty counts', () => {
    assert.equal(ruleRowSubtitle({ globs: ['a'], triggers: [] }, 'files'), 'conditional · 1 glob · MD');
    assert.equal(ruleRowSubtitle({ triggers: ['t'] }, 'files'), 'conditional · 1 trigger · MD');
  });

  it('prefers the item storage and marks an override', () => {
    assert.equal(
      ruleRowSubtitle({ effectiveStorage: 'database', storageIsOverride: true }, 'files'),
      'conditional · DB override',
    );
    assert.equal(ruleRowSubtitle({}, 'database'), 'conditional · DB');
  });
});

describe('skill row subtitle', () => {
  it('starts with the first description line', () => {
    assert.equal(
      skillRowSubtitle({ description: 'Review Rust code.\nMore text', triggers: ['a'] }, 'files'),
      'Review Rust code. · 1 trigger · MD',
    );
  });

  it('works without a description', () => {
    assert.equal(skillRowSubtitle({ description: '  ', triggers: ['a', 'b'] }, 'database'), '2 triggers · DB');
  });
});

describe('priority meta', () => {
  it('prints the priority or nothing', () => {
    assert.equal(priorityMeta(90), 'p90');
    assert.equal(priorityMeta(0), 'p0');
    assert.equal(priorityMeta(undefined), '');
  });
});

describe('trace row subtitle', () => {
  it('joins tool, kind and meta without blanks', () => {
    assert.equal(traceRowSubtitle('ax_explore', 'Inbound', '12 ms'), 'ax_explore · Inbound · 12 ms');
    assert.equal(traceRowSubtitle(undefined, 'Internal', ''), 'Internal');
  });
});
