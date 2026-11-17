import assert from 'node:assert/strict';
import { describe, it } from 'node:test';
import { precisionText, promotedText, rankLatencyText, tokenDeltaText } from './neuralMetrics.ts';

describe('Neural metrics', () => {
  it('shows the measured token delta with its preflight count', () => {
    assert.equal(
      tokenDeltaText({ value: -200, kind: 'measured', unit: 'tokens', preflights: 4 }),
      '-200 tokens over 4 preflights',
    );
    assert.equal(tokenDeltaText({ value: null, kind: 'unavailable' }), 'unavailable');
    assert.equal(tokenDeltaText(undefined), 'unavailable');
  });

  it('shows rank latency per preflight', () => {
    assert.equal(
      rankLatencyText({ value: 212.6, kind: 'measured', unit: 'microseconds', preflights: 3 }),
      '213 µs per preflight over 3 preflights',
    );
    assert.equal(rankLatencyText({ value: null, kind: 'unavailable' }), 'unavailable');
  });

  it('names where precision was measured', () => {
    assert.equal(
      precisionText({
        value: 1,
        kind: 'measured',
        source: 'coding fixture, leave-one-out',
        predictions: 5,
      }),
      '100% precision on 5 predictions (coding fixture, leave-one-out)',
    );
    assert.equal(
      precisionText({ value: null, kind: 'measured', source: 'coding fixture, leave-one-out', predictions: 0 }),
      'no predictions to score (coding fixture, leave-one-out)',
    );
    assert.equal(precisionText({ value: null, kind: 'unavailable' }), 'not evaluated; run ax neural evaluate');
  });

  it('reads a missing count as 0 and a missing source as unknown', () => {
    assert.equal(tokenDeltaText({ value: -5, kind: 'measured' }), '-5 tokens over 0 preflights');
    assert.equal(rankLatencyText({ value: 7, kind: 'measured' }), '7 µs per preflight over 0 preflights');
    assert.equal(precisionText({ value: 0.5, kind: 'measured' }), '50% precision on 0 predictions (unknown source)');
  });

  it('shows a checksum of 12 characters or fewer whole and separates models with a comma', () => {
    assert.equal(
      promotedText([
        { modelId: 'm', version: '1', checksum: '0123456789ab' },
        { modelId: 'n', version: '2', checksum: 'abc' },
      ]),
      'm 1 (0123456789ab), n 2 (abc)',
    );
  });

  it('lists promoted models, or says memory stays hash-v1', () => {
    assert.equal(promotedText([]), 'none; memory stays hash-v1');
    assert.equal(promotedText(undefined), 'none; memory stays hash-v1');
    assert.equal(
      promotedText([{ modelId: 'candidate', version: '2', checksum: 'sha256:0123456789abcdef' }]),
      'candidate 2 (sha256:01234…)',
    );
  });
});
