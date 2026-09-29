import { test } from 'node:test';
import assert from 'node:assert/strict';
import { mountError, mountSummary, openLabel, validMountName, folderSyncSummary, folderNameFromPath } from './vaultMount.ts';

test('openLabel names the file manager per OS', () => {
  assert.equal(openLabel('mac'), 'Open in Finder');
  assert.equal(openLabel('windows'), 'Open in Explorer');
  assert.equal(openLabel('linux'), 'Open in Files');
});

test('mountSummary reports where the vault is connected', () => {
  const base = { os: 'mac' as const, url: 'http://127.0.0.1:7070/ax/', name: 'ax', autostart: false };
  assert.equal(
    mountSummary({ ...base, mounted: true, mountPoint: '/Users/g/ax' }),
    'Connected as "ax" at /Users/g/ax',
  );
  assert.equal(mountSummary({ ...base, mounted: false, mountPoint: null }), 'Not connected');
  assert.equal(
    mountSummary({ ...base, mounted: false, mountPoint: '/Users/g/ax' }),
    'Not connected (last used /Users/g/ax)',
  );
});

test('validMountName mirrors the server rule', () => {
  assert.equal(validMountName('ax'), true);
  assert.equal(validMountName('My Vault_1-a'), true);
  assert.equal(validMountName(''), false);
  assert.equal(validMountName('a'.repeat(33)), false);
  assert.equal(validMountName('ax;rm'), false);
});

test('mountError keeps the copyable command', () => {
  assert.deepEqual(mountError(502, { error: 'boom', command: 'gio mount dav://x' }), {
    message: 'boom',
    command: 'gio mount dav://x',
  });
  assert.deepEqual(mountError(403, {}), {
    message: 'Only the local browser on this machine can connect the vault (HTTP 403).',
    command: null,
  });
  assert.deepEqual(mountError(500, null), { message: 'HTTP 500', command: null });
});

test('folder sync summary reads as plain text', () => {
  assert.equal(folderSyncSummary({ name: 'n', path: '/n', index: false }), 'Not indexed');
  assert.equal(folderSyncSummary({ name: 'n', path: '/n', index: true }), 'Not synced yet');
  assert.equal(
    folderSyncSummary({
      name: 'n',
      path: '/n',
      index: true,
      lastSync: { atMs: 0, added: 2, updated: 1, removed: 0, skipped: 3 },
    }),
    'Synced: 2 added, 1 updated, 0 removed, 3 skipped',
  );
});

test('a picked path suggests a valid folder name', () => {
  assert.equal(folderNameFromPath('/Users/me/notes'), 'notes');
  assert.equal(folderNameFromPath('C:\\Users\\me\\Work Notes'), 'Work Notes');
  assert.equal(folderNameFromPath('/Users/me/my.notes'), 'my-notes');
  assert.equal(folderNameFromPath('/Users/me/' + 'x'.repeat(40)), 'x'.repeat(32));
  assert.equal(folderNameFromPath('/'), '');
  assert.equal(folderNameFromPath('C:\\'), '');
});
