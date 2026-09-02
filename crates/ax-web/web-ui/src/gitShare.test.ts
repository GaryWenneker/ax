import assert from 'node:assert/strict';
import { describe, it } from 'node:test';
import { isGitShared } from './components/ui/policyListUtils.ts';

describe('isGitShared', () => {
  it('G1 project enabled is git-shared', () => {
    assert.equal(isGitShared('project', true), true);
    assert.equal(isGitShared(undefined, true), true);
  });

  it('G2 workspace enabled is git-shared', () => {
    assert.equal(isGitShared('Workspace', true), true);
  });

  it('G3 private, company, and user are not git-shared', () => {
    assert.equal(isGitShared('private', true), false);
    assert.equal(isGitShared('company', true), false);
    assert.equal(isGitShared('user', true), false);
  });

  it('G4 disabled is never git-shared even for project', () => {
    assert.equal(isGitShared('project', false), false);
    assert.equal(isGitShared('workspace', false), false);
  });
});
