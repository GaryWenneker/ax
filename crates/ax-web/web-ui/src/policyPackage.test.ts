import assert from 'node:assert/strict';
import { describe, it } from 'node:test';
import { defaultRestoreAction, isShareablePolicyItem } from './policyTypes.ts';
import {
  allIdsSelected,
  compareBadgeClass,
  compareLabel,
  compareStatusClass,
  compareSummary,
  emptyDiffCopy,
  newerBadgeClass,
  newerLabel,
  pickDroppedPolicyZipFile,
  policyItemDescription,
  restoreDecisionLabels,
  restoreFileActionLabel,
  toRestoreApiDecision,
  setHunkTake,
  hunkTakeFromChecks,
  numberedHunkLines,
  changeNavLabel,
  toggleSelectAll,
  unifiedDiffLines,
} from './policyPackage.ts';

describe('policy zip package helpers', () => {
  it('shareable is project/workspace enabled only', () => {
    assert.equal(isShareablePolicyItem('project', true), true);
    assert.equal(isShareablePolicyItem('workspace', true), true);
    assert.equal(isShareablePolicyItem('company', true), false);
    assert.equal(isShareablePolicyItem('private_project', true), false);
    assert.equal(isShareablePolicyItem('project', false), false);
  });

  it('conflict defaults to skip, new to overwrite, local newer stays skip', () => {
    assert.equal(defaultRestoreAction('new'), 'overwrite');
    assert.equal(defaultRestoreAction('conflict'), 'skip');
    assert.equal(defaultRestoreAction('conflict', 'local'), 'skip');
    assert.equal(defaultRestoreAction('invalid'), null);
  });

  it('newerLabel distinguishes local vs package', () => {
    assert.equal(newerLabel('local'), 'Local newer');
    assert.equal(newerLabel('package'), 'Package newer');
    assert.equal(newerLabel('none'), null);
    assert.match(newerBadgeClass('local'), /policy-pack-badge--newer-local/);
  });

  it('toggleSelectAll selects every id then clears', () => {
    const ids = ['a', 'b'];
    const all = toggleSelectAll(ids, new Set());
    assert.deepEqual([...all].sort(), ['a', 'b']);
    assert.equal(allIdsSelected(ids, all), true);
    const none = toggleSelectAll(ids, all);
    assert.equal(none.size, 0);
  });

  it('compare badges distinguish changed from identical', () => {
    assert.equal(compareLabel('changed'), 'Different');
    assert.equal(compareLabel('identical'), 'Identical');
    assert.match(compareBadgeClass('changed'), /policy-pack-badge--changed/);
    assert.match(compareBadgeClass('identical'), /policy-pack-badge--identical/);
  });

  it('compareSummary is one line of status and optional age', () => {
    assert.equal(compareSummary('changed', 'package'), 'Different · Package newer');
    assert.equal(compareSummary('changed', 'local'), 'Different · Local newer');
    assert.equal(compareSummary('identical', 'equal'), 'Identical · Same age');
    assert.equal(compareSummary('new', 'none'), 'New');
    assert.equal(compareSummary('invalid'), 'Invalid');
  });

  it('compareStatusClass is red for local newer, orange for package newer, green for identical', () => {
    assert.match(compareStatusClass('changed', 'local'), /policy-pack-compare-status--local/);
    assert.match(compareStatusClass('changed', 'package'), /policy-pack-compare-status--changed/);
    assert.match(compareStatusClass('identical', 'equal'), /policy-pack-compare-status--identical/);
  });

  it('emptyDiffCopy does not claim a match when compare is changed', () => {
    assert.match(emptyDiffCopy('identical'), /matches the package/);
    assert.match(emptyDiffCopy('changed'), /line endings or encoding/);
    assert.doesNotMatch(emptyDiffCopy('changed'), /matches the package/);
  });

  it('restoreDecisionLabels are Accept and Reject', () => {
    assert.deepEqual(restoreDecisionLabels(), { reject: 'Reject', accept: 'Accept' });
  });

  it('restoreFileActionLabel is partial when some hunks are accepted', () => {
    assert.equal(restoreFileActionLabel('skip', 2), 'reject');
    assert.equal(restoreFileActionLabel('overwrite', 2), 'accept');
    assert.equal(restoreFileActionLabel({ action: 'merge', acceptHunks: [] }, 2), 'reject');
    assert.equal(restoreFileActionLabel({ action: 'merge', acceptHunks: [0, 1] }, 2), 'accept');
    assert.equal(restoreFileActionLabel({ action: 'merge', acceptHunks: [0] }, 2), 'partial');
    assert.deepEqual(toRestoreApiDecision({ action: 'merge', acceptHunks: [0] }, 2), {
      action: 'merge',
      acceptHunks: [0],
      hunks: [
        { index: 0, take: 'package' },
        { index: 1, take: 'local' },
      ],
    });
    assert.equal(toRestoreApiDecision({ action: 'merge', acceptHunks: [0, 1] }, 2), 'overwrite');
    assert.equal(toRestoreApiDecision({ action: 'merge', acceptHunks: [] }, 2), 'skip');
    assert.equal(restoreFileActionLabel(setHunkTake('skip', 0, 2, 'package'), 2), 'partial');
    assert.equal(setHunkTake({ action: 'merge', acceptHunks: [0] }, 0, 2, 'local'), 'skip');
    assert.equal(hunkTakeFromChecks(true, true), 'both');
    assert.equal(hunkTakeFromChecks(true, false), 'local');
    assert.deepEqual(numberedHunkLines(['b', 'c'], 2), [
      { n: 2, text: 'b' },
      { n: 3, text: 'c' },
    ]);
  });

  it('changeNavLabel is 1-based Change N of M', () => {
    assert.equal(changeNavLabel(0, 2), 'Change 1 of 2');
    assert.equal(changeNavLabel(1, 2), 'Change 2 of 2');
  });

  it('unifiedDiffLines classifies git-style add and delete', () => {
    const lines = unifiedDiffLines('--- local\n+++ package\n@@ -1,1 +1,1 @@\n-old\n+new\n context');
    assert.equal(lines[0].kind, 'meta');
    assert.equal(lines[3].kind, 'del');
    assert.equal(lines[4].kind, 'add');
    assert.equal(lines[5].kind, 'ctx');
    assert.deepEqual(unifiedDiffLines(''), []);
  });

  it('policyItemDescription prefers explicit then body then id', () => {
    assert.equal(
      policyItemDescription({ id: 'utf8-no-bom', description: '  UTF-8 without BOM  ', body: 'ignored' }),
      'UTF-8 without BOM',
    );
    assert.equal(
      policyItemDescription({ id: 'english-only', body: '# English only\n\nAll agent-facing text MUST be English.' }),
      'English only',
    );
    assert.equal(
      policyItemDescription({ id: 'x', body: 'Never use UTF-16.\n\nMore detail.' }),
      'Never use UTF-16.',
    );
    assert.equal(policyItemDescription({ id: 'utf8-no-bom', body: '' }), 'utf8 no bom');
  });

  it('pickDroppedPolicyZipFile takes the first zip from a drop and ignores other types', () => {
    const zip = new File(['pk'], 'team.ax-policy.zip', { type: 'application/zip' });
    const txt = new File(['no'], 'readme.txt', { type: 'text/plain' });
    assert.equal(pickDroppedPolicyZipFile([txt, zip])?.name, 'team.ax-policy.zip');
    assert.equal(pickDroppedPolicyZipFile([txt]), null);
    assert.equal(pickDroppedPolicyZipFile([]), null);
    assert.equal(pickDroppedPolicyZipFile(null), null);
    const typed = new File(['pk'], 'pack.bin', { type: 'application/x-zip-compressed' });
    assert.equal(pickDroppedPolicyZipFile([typed])?.name, 'pack.bin');
  });
});
