import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import test from 'node:test';

const FILES = ['ReplicationSetup.tsx', 'RemoteHostSetup.tsx', 'RemoteMcpPublishSection.tsx', 'ui/DataTable.tsx'];
const MAX_LINE = 500;

function read(file: string) {
  return readFileSync(new URL(`./${file}`, import.meta.url), 'utf8');
}

test('settings dialogs are not packed into single very long lines', () => {
  for (const file of FILES) {
    read(file).split('\n').forEach((line, index) => {
      assert.ok(line.length <= MAX_LINE, `${file}:${index + 1} is ${line.length} characters`);
    });
  }
});

test('dialog actions are named functions, not inline async handlers', () => {
  const replication = read('ReplicationSetup.tsx');
  assert.match(replication, /async function removeConfiguration\(\)/);
  assert.match(replication, /async function stopNode\(\)/);
  assert.match(replication, /onClick=\{\(\) => void removeConfiguration\(\)\}/);
  assert.match(replication, /onClick=\{\(\) => void stopNode\(\)\}/);
  const publish = read('RemoteMcpPublishSection.tsx');
  assert.match(publish, /onClick=\{\(\) => void verifyEndpoint\(\)\}/);
  assert.match(publish, /onClick=\{\(\) => void removeRegistration\(\)\}/);
  assert.match(read('RemoteHostSetup.tsx'), /onClick=\{\(\) => void removeConnection\(removeRequest\)\}/);
});
