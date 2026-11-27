import assert from 'node:assert/strict';
import { test } from 'node:test';

// Pin a zone with a daylight-saving change before any Date is built.
process.env.TZ = 'Europe/Amsterdam';

const { computeStreaks } = await import('./activityStreaks.ts');

type Row = Parameters<typeof computeStreaks>[0][number];

function day(date: string, calls = 1): Row {
  return { date, calls, graph_calls: calls, tokens_saved_est: calls } as Row;
}

function everyDay(from: string, to: string): Row[] {
  const rows: Row[] = [];
  const [y, m, d] = from.split('-').map(Number);
  const cur = new Date(y, m - 1, d);
  for (;;) {
    const iso = `${cur.getFullYear()}-${String(cur.getMonth() + 1).padStart(2, '0')}-${String(cur.getDate()).padStart(2, '0')}`;
    rows.push(day(iso));
    if (iso === to) break;
    cur.setDate(cur.getDate() + 1);
  }
  return rows;
}

test('a streak runs across the spring daylight-saving change', () => {
  // Europe/Amsterdam moves to summer time on Sunday 30 March 2025 (a 23-hour day).
  const { longest } = computeStreaks(everyDay('2025-03-01', '2025-03-31'), 'calls', 2025);
  assert.equal(longest, 31);
});

test('a streak runs across the autumn daylight-saving change', () => {
  // Europe/Amsterdam moves back on Sunday 26 October 2025 (a 25-hour day).
  const { longest } = computeStreaks(everyDay('2025-10-20', '2025-10-31'), 'calls', 2025);
  assert.equal(longest, 12);
});

test('a missing day still breaks the streak', () => {
  const rows = [...everyDay('2025-03-25', '2025-03-29'), ...everyDay('2025-03-31', '2025-04-02')];
  assert.equal(computeStreaks(rows, 'calls', 2025).longest, 5);
});

test('a day with zero activity breaks the streak', () => {
  const rows = everyDay('2025-03-28', '2025-04-03').map((r) => (r.date === '2025-04-01' ? day(r.date, 0) : r));
  assert.equal(computeStreaks(rows, 'calls', 2025).longest, 4);
});
