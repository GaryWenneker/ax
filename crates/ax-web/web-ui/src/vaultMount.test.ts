import { test } from 'node:test';
import assert from 'node:assert/strict';
import { connectVault, fetchMountStatus, mountError, mountSummary, openLabel, pickFolderPath, validMountName, folderSyncSummary, folderNameFromPath } from './vaultMount.ts';

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

type Fetch = typeof globalThis.fetch;

async function withFetch(impl: Fetch, run: () => Promise<void>): Promise<void> {
  const saved = globalThis.fetch;
  globalThis.fetch = impl;
  try {
    await run();
  } finally {
    globalThis.fetch = saved;
  }
}

test('a failed mount request rejects with an Error that keeps the copyable command', async () => {
  const reply: Fetch = async () =>
    new Response(JSON.stringify({ error: 'mount failed', command: 'gio mount dav://x' }), { status: 502 });
  await withFetch(reply, async () => {
    const err: unknown = await connectVault('ax', false).then(
      () => null,
      (e: unknown) => e,
    );
    assert.ok(err instanceof Error, 'the rejection is an Error');
    assert.equal(err.message, 'mount failed');
    assert.match(String(err), /: mount failed$/);
    assert.equal((err as Error & { command: string | null }).command, 'gio mount dav://x');
  });
});

test('a mount request whose 2xx reply is not JSON rejects with an Error', async () => {
  const reply: Fetch = async () => new Response('<html>login</html>', { status: 200 });
  await withFetch(reply, async () => {
    const err: unknown = await fetchMountStatus().then(
      () => null,
      (e: unknown) => e,
    );
    assert.ok(err instanceof Error);
    assert.match(err.message, /not JSON/);
  });
});

test('pickFolderPath reports an unreadable 2xx reply as an Error instead of a TypeError', async () => {
  for (const body of ['<html>login</html>', 'null', JSON.stringify({ path: 7 })]) {
    const reply: Fetch = async () => new Response(body, { status: 200 });
    await withFetch(reply, async () => {
      const err: unknown = await pickFolderPath().then(
        () => null,
        (e: unknown) => e,
      );
      assert.ok(err instanceof Error, `rejects for ${body}`);
      assert.ok(!(err instanceof TypeError), `not a TypeError for ${body}`);
      assert.match(err.message, /folder dialog/i);
    });
  }
});

test('pickFolderPath returns the picked path, null on cancel, and undefined without a dialog', async () => {
  const replies: Array<[Response, string | null | undefined]> = [
    [new Response(JSON.stringify({ path: '/work/notes' }), { status: 200 }), '/work/notes'],
    [new Response(JSON.stringify({ path: null }), { status: 200 }), null],
    [new Response(JSON.stringify({ error: 'no dialog' }), { status: 501 }), undefined],
  ];
  for (const [res, want] of replies) {
    await withFetch(async () => res, async () => {
      assert.equal(await pickFolderPath(), want);
    });
  }
});
