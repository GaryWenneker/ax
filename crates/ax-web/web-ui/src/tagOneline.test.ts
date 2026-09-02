import assert from 'node:assert/strict';
import { describe, it } from 'node:test';
import * as policyListUtils from './components/ui/policyListUtils.ts';

describe('table tags are not truncated', () => {
  it('T1 compactTagItems is not part of the list utils', () => {
    assert.equal('compactTagItems' in policyListUtils, false);
  });
});
