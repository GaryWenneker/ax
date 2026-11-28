import assert from 'node:assert/strict';
import { describe, it } from 'node:test';
import {
  appendMemoryPage,
  fetchMemoryWindow,
  memoryListChanged,
  memorySourceCounts,
  pickSelectedMemory,
} from './memoryRefresh.ts';

const row = (id: string, updated_at = 1, extra: { enabled?: boolean; score?: number } = {}) => ({
  id,
  updated_at,
  ...extra,
});

describe('memory list refresh', () => {
  it('reports no change for the same rows', () => {
    assert.equal(memoryListChanged([row('a'), row('b')], [row('a'), row('b')]), false);
  });

  it('detects a new memory', () => {
    assert.equal(memoryListChanged([row('a')], [row('b'), row('a')]), true);
  });

  it('detects a removed memory', () => {
    assert.equal(memoryListChanged([row('a'), row('b')], [row('a')]), true);
  });

  it('detects an edited memory', () => {
    assert.equal(memoryListChanged([row('a', 1)], [row('a', 2)]), true);
  });

  it('detects a toggled memory', () => {
    assert.equal(
      memoryListChanged([row('a', 1, { enabled: true })], [row('a', 1, { enabled: false })]),
      true,
    );
  });

  it('detects a reordered list', () => {
    assert.equal(memoryListChanged([row('a'), row('b')], [row('b'), row('a')]), true);
  });

  it('detects a changed recall score', () => {
    assert.equal(
      memoryListChanged([row('a', 1, { score: 0.5 })], [row('a', 1, { score: 0.7 })]),
      true,
    );
  });
});

describe('memory vault paging', () => {
  const vault = Array.from({ length: 1200 }, (_, i) => ({ id: `m${i}`, source: i % 3 === 0 ? 'git' : 'manual' }));
  const server = async (limit: number, offset: number) => {
    assert.ok(limit <= 500, 'the server caps one page at 500');
    return { memories: vault.slice(offset, offset + limit), total: vault.length };
  };

  it('refreshes every loaded row, in pages the server accepts', async () => {
    const page = await fetchMemoryWindow(1100, server);
    assert.equal(page.memories.length, 1100);
    assert.equal(page.memories[1099].id, 'm1099');
    assert.equal(page.total, 1200);
  });

  it('stops at the end of the vault', async () => {
    const page = await fetchMemoryWindow(5000, server);
    assert.equal(page.memories.length, 1200);
  });

  it('appends a next page without duplicating rows that shifted', () => {
    const next = appendMemoryPage([{ id: 'a' }, { id: 'b' }], [{ id: 'b' }, { id: 'c' }]);
    assert.deepEqual(next.map((m) => m.id), ['a', 'b', 'c']);
  });

  it('counts sources over the rows it is given', () => {
    assert.deepEqual(memorySourceCounts([{ source: 'git' }, { source: 'cli-turn' }, { source: 'mcp' }, { source: '' }]), {
      git: 1,
      cliTurn: 1,
      manual: 2,
    });
  });

  it('opens a memory that is not in the loaded rows once it is fetched by id', () => {
    const shown = [{ id: 'a' }];
    assert.deepEqual(pickSelectedMemory(shown, 'a', null), { id: 'a' });
    assert.equal(pickSelectedMemory(shown, 'old', null), null);
    assert.deepEqual(pickSelectedMemory(shown, 'old', { id: 'old' }), { id: 'old' });
    assert.equal(pickSelectedMemory(shown, 'old', { id: 'other' }), null);
    assert.equal(pickSelectedMemory(shown, '', { id: 'old' }), null);
  });
});
