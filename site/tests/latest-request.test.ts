import test from 'node:test';
import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import { latestRequests } from '../src/lib/latest-request.ts';

test('starting a request aborts the previous one and makes it stale', () => {
  const loads = latestRequests();
  const first = loads.begin();
  assert.equal(first.isCurrent(), true);
  const second = loads.begin();
  assert.equal(first.signal.aborted, true, 'the superseded fetch is aborted');
  assert.equal(first.isCurrent(), false, 'a late response from it is ignored');
  assert.equal(second.isCurrent(), true);
  assert.equal(second.signal.aborted, false);
});

test('aborting the current request (timeout) keeps it current so its error is shown', () => {
  const loads = latestRequests();
  const only = loads.begin();
  only.abort();
  assert.equal(only.signal.aborted, true);
  assert.equal(only.isCurrent(), true);
});

test('the admin dashboard routes every load through latestRequests', () => {
  const page = readFileSync(new URL('../src/pages/admin/dashboard.astro', import.meta.url), 'utf8');
  assert.match(page, /import \{ latestRequests \} from '\.\.\/\.\.\/lib\/latest-request\.ts';/);
  assert.doesNotMatch(page, /new AbortController\(\)/, 'load() must not create its own unshared controller');
  // Stale responses must stop before touching the stat cards or charts.
  const load = page.slice(page.indexOf('async function load('));
  const afterFetch = load.slice(load.indexOf('await apiFetch('));
  assert.ok(afterFetch.indexOf('isCurrent()') < afterFetch.indexOf("getElementById('stat-installs')"));
});
