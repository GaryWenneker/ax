import assert from 'node:assert/strict';
import { test } from 'node:test';
import { EMPTY_INPUT, addressError, inputError, suggestedName } from './connectionInput.ts';

test('an address must be a project MCP address over https, or http on this computer', () => {
  for (const ok of [
    'https://ax.example.com/projects/my-app/mcp',
    ' https://ax.example.com:8443/projects/a_b/mcp ',
    'http://127.0.0.1:9000/projects/alpha/mcp',
    'http://localhost:9000/projects/alpha/mcp',
    'http://[::1]:9000/projects/alpha/mcp',
  ]) assert.equal(addressError(ok), null, ok);
  assert.equal(addressError(''), 'Enter the address of the Ax environment.');
  assert.equal(addressError('   '), 'Enter the address of the Ax environment.');
  assert.equal(addressError('ax.example.com'), 'The address is not a valid URL.');
  for (const bad of [
    'http://ax.example.com/projects/a/mcp',
    'http://127.0.0.1.example.com/projects/a/mcp',
    'ftp://ax.example.com/projects/a/mcp',
  ]) assert.match(addressError(bad) ?? '', /https:\/\//, bad);
  for (const bad of [
    'https://user:pw@ax.example.com/projects/a/mcp',
    'https://ax.example.com/projects/a/mcp?x=1',
    'https://ax.example.com/projects/a/mcp#x',
  ]) assert.match(addressError(bad) ?? '', /without a user name/, bad);
  for (const bad of [
    'https://ax.example.com/',
    'https://ax.example.com/projects/a',
    'https://ax.example.com/projects//mcp',
    'https://ax.example.com/projects/a/mcp/extra',
  ]) assert.match(addressError(bad) ?? '', /project MCP address/, bad);
});

test('an address and credentials are the minimum', () => {
  const url = 'https://ax.example.com/projects/app/mcp';
  assert.equal(inputError(EMPTY_INPUT), 'Enter the address of the Ax environment.');
  assert.equal(inputError({ ...EMPTY_INPUT, url }), 'Enter the API key for this environment.');
  assert.equal(inputError({ ...EMPTY_INPUT, url, key: '  ' }), 'Enter the API key for this environment.');
  assert.equal(inputError({ ...EMPTY_INPUT, url, key: 'two words' }), 'The key contains spaces.');
  assert.equal(inputError({ ...EMPTY_INPUT, url, key: 'secret' }), null);
  assert.equal(inputError({ ...EMPTY_INPUT, url, key: 'secret', name: 'x'.repeat(81) }), 'Use a name of at most 80 characters.');
  // Editing a saved connection: an empty key keeps the stored one.
  assert.equal(inputError({ ...EMPTY_INPUT, url }, true), null);
  assert.equal(inputError({ ...EMPTY_INPUT, url, key: 'a b' }, true), 'The key contains spaces.');
});

test('browser sign-in needs a client ID and https', () => {
  const url = 'https://ax.example.com/projects/app/mcp';
  assert.match(inputError({ ...EMPTY_INPUT, url, mode: 'oauth' }) ?? '', /public client ID/);
  assert.equal(inputError({ ...EMPTY_INPUT, url, mode: 'oauth', clientId: 'native' }), null);
  assert.equal(
    inputError({ ...EMPTY_INPUT, url: 'http://127.0.0.1:1/projects/a/mcp', mode: 'oauth', clientId: 'native' }),
    'Browser sign-in needs an https:// address.',
  );
});

test('the suggested name is host and project', () => {
  assert.equal(suggestedName('https://ax.example.com/projects/my-app/mcp'), 'ax.example.com / my-app');
  assert.equal(suggestedName('https://ax.example.com/'), 'ax.example.com');
  assert.equal(suggestedName('nope'), '');
});
