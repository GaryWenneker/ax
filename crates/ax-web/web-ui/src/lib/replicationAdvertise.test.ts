import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import test from 'node:test';

import { savedAdvertise } from './replicationAdvertise.ts';

test('a saved LAN address fills the IP picker and leaves the proxy URL empty', () => {
  assert.deepEqual(savedAdvertise('http://192.168.1.4:7001'), { ip: '192.168.1.4', advertise: '' });
  assert.deepEqual(savedAdvertise('http://[fe80::1]:7001'), { ip: 'fe80::1', advertise: '' });
});

test('a saved public proxy URL fills the proxy field', () => {
  assert.deepEqual(savedAdvertise('https://node.example.com'), { ip: null, advertise: 'https://node.example.com' });
});

test('a missing or malformed saved value never throws', () => {
  assert.deepEqual(savedAdvertise(null), { ip: null, advertise: '' });
  assert.deepEqual(savedAdvertise(undefined), { ip: null, advertise: '' });
  assert.deepEqual(savedAdvertise('not a url'), { ip: null, advertise: 'not a url' });
});

test('the replication poll reads the saved address through the guarded helper', () => {
  const source = readFileSync(new URL('../components/ReplicationSetup.tsx', import.meta.url), 'utf8');
  assert.doesNotMatch(source, /new URL\(saved\.advertise\)/);
  assert.match(source, /savedAdvertise\(saved\.advertise\)/);
});
