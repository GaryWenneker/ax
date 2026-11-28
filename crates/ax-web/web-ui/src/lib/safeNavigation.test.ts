import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import test from 'node:test';

import { signInTarget } from './safeNavigation.ts';

test('an HTTPS sign-in address is allowed', () => {
  assert.equal(signInTarget('https://issuer.example.com/authorize?state=1'), 'https://issuer.example.com/authorize?state=1');
});

test('plain HTTP is allowed only for this computer', () => {
  assert.equal(signInTarget('http://127.0.0.1:8080/authorize'), 'http://127.0.0.1:8080/authorize');
  assert.equal(signInTarget('http://localhost/authorize'), 'http://localhost/authorize');
  assert.equal(signInTarget('http://[::1]:9000/authorize'), 'http://[::1]:9000/authorize');
  assert.equal(signInTarget('http://issuer.example.com/authorize'), null);
});

test('script, data and malformed addresses are refused', () => {
  assert.equal(signInTarget('javascript:alert(document.cookie)'), null);
  assert.equal(signInTarget(' JavaScript:alert(1)'), null);
  assert.equal(signInTarget('data:text/html,<script>alert(1)</script>'), null);
  assert.equal(signInTarget('not a url'), null);
  assert.equal(signInTarget(undefined), null);
  assert.equal(signInTarget(42), null);
});

test('the remote MCP sign-in only navigates to a checked address', () => {
  const source = readFileSync(new URL('../components/RemoteMcpSettingsSection.tsx', import.meta.url), 'utf8');
  const connect = source.slice(source.indexOf('async function connect('), source.indexOf('async function cancel('));
  assert.ok(connect.length > 0);
  assert.match(connect, /signInTarget\(value\.authorization_url\)/);
  assert.doesNotMatch(connect, /popup\.location\.href = value\.authorization_url/);
  assert.doesNotMatch(connect, /setAuthorizationUrl\(value\.authorization_url\)/);
});
