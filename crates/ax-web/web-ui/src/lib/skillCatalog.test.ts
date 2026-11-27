import assert from 'node:assert/strict';
import { test } from 'node:test';
import { canSearchCatalog, parseCatalogHits, settingsAfterSave } from './skillCatalog.ts';

test('Enter and the Search button share one guard', () => {
  assert.equal(canSearchCatalog(false, 'git'), true);
  assert.equal(canSearchCatalog(true, 'git'), false, 'no second search while one runs');
  assert.equal(canSearchCatalog(false, ' g '), false, 'a query under two characters is refused');
  assert.equal(canSearchCatalog(false, ''), false);
});

test('a catalog row without installs shows zero instead of crashing the panel', () => {
  const hits = parseCatalogHits(JSON.stringify([{ name: 'git', source: 'acme/skills' }]));
  assert.equal(hits.length, 1);
  assert.equal(hits[0].installs, 0);
  assert.equal(hits[0].installs.toLocaleString(), '0');
});

test('catalog rows keep their fields and drop rows without a name', () => {
  const hits = parseCatalogHits(
    JSON.stringify([
      { name: 'git', source: 'acme/skills', installs: 1200, stars: 5, audits: [{ name: 'a', status: 'pass', url: 'u' }] },
      { source: 'nameless' },
      null,
      'text',
      { name: 'web', installs: '12' },
    ]),
  );
  assert.deepEqual(
    hits.map((h) => [h.name, h.source, h.installs]),
    [
      ['git', 'acme/skills', 1200],
      ['web', '', 0],
    ],
  );
  assert.equal(hits[0].stars, 5);
  assert.equal(hits[0].audits?.length, 1);
});

test('a reply that is not a list is a clear error, not a render crash', () => {
  assert.throws(() => parseCatalogHits('{"error":"x"}'), /unexpected reply/i);
  assert.throws(() => parseCatalogHits('not json'), /unexpected reply/i);
});

test('the report-installs toggle goes back when saving the setting fails', () => {
  const before = { reportInstalls: false, publishRemote: 'origin' };
  const wanted = { reportInstalls: true, publishRemote: 'origin' };
  assert.deepEqual(settingsAfterSave(before, wanted, true), wanted);
  assert.deepEqual(settingsAfterSave(before, wanted, false), before);
});
