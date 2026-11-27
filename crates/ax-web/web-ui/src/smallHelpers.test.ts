import assert from 'node:assert/strict';
import { describe, it } from 'node:test';
import { formatLogLine, liveActivityLabel } from './logFormat.ts';
import { displayPath, pathBreadcrumbs } from './pathDisplay.ts';
import { formatInstantInZone, resolveTimeZone } from './lib/timeZone.ts';
import { instantMs } from './memoryHeadline.ts';

describe('formatLogLine', () => {
  it('rewrites a leading unix-second stamp to a local date', () => {
    const out = formatLogLine('[1795771193] ▶ index');
    assert.equal(out, `[${new Date(1795771193 * 1000).toLocaleString()}] ▶ index`);
  });
  it('leaves lines that already carry a date alone', () => {
    assert.equal(formatLogLine('[2026-11-27 10:00:00] ▶ index'), '[2026-11-27 10:00:00] ▶ index');
    assert.equal(formatLogLine('no stamp'), 'no stamp');
  });
});

describe('liveActivityLabel', () => {
  it('prefers the live sonar project, then the live step label', () => {
    assert.equal(liveActivityLabel([], 'tia', 'org.k', 'ax-core'), 'Sonar · ax-core');
    assert.equal(liveActivityLabel([], 'tia', 'org.k'), 'Sonar · org.k');
    assert.equal(liveActivityLabel([], 'tia', null), 'Test impact');
    assert.equal(liveActivityLabel([], 'custom', null), 'custom');
  });
  it('falls back to the last started line in the log', () => {
    const lines = ['[2026-11-27 10:00:00] ▶ index', '[2026-11-27 10:00:01] ▶ sonar:org.k — ax-web (1/2)'];
    assert.equal(liveActivityLabel(lines, null, null), 'Sonar · ax-web');
    assert.equal(liveActivityLabel(lines.slice(0, 1), null, null), 'index');
    assert.equal(liveActivityLabel(['plain'], null, null), null);
    assert.equal(liveActivityLabel(undefined, null, null), null);
  });
});

describe('displayPath', () => {
  it('strips the Windows extended-length prefix and trailing separators', () => {
    assert.equal(displayPath('\\\\?\\C:\\Users\\gary\\'), 'C:\\Users\\gary');
    assert.equal(displayPath('/Users/gary/io/'), '/Users/gary/io');
    assert.equal(displayPath('/'), '/');
    assert.equal(displayPath('C:/Users/gary'), 'C:/Users/gary');
    assert.equal(displayPath('\\\\server\\share\\dir'), '\\\\server\\share\\dir');
  });
});

describe('pathBreadcrumbs', () => {
  it('splits a Unix path', () => {
    assert.deepEqual(pathBreadcrumbs('/Users/gary'), [
      { label: 'Users', path: '/Users' },
      { label: 'gary', path: '/Users/gary' },
    ]);
    assert.deepEqual(pathBreadcrumbs('/'), [{ label: '/', path: '/' }]);
    assert.deepEqual(pathBreadcrumbs(''), []);
  });
  it('splits a drive path and a UNC path', () => {
    assert.deepEqual(pathBreadcrumbs('C:\\Users\\gary'), [
      { label: 'C:', path: 'C:\\' },
      { label: 'Users', path: 'C:\\Users' },
      { label: 'gary', path: 'C:\\Users\\gary' },
    ]);
    assert.deepEqual(pathBreadcrumbs('\\\\srv\\share\\dir'), [
      { label: '\\\\srv\\share', path: '\\\\srv\\share' },
      { label: 'dir', path: '\\\\srv\\share\\dir' },
    ]);
  });
});

describe('timeZone', () => {
  it('resolves empty, local and browser to the browser zone', () => {
    const browser = Intl.DateTimeFormat().resolvedOptions().timeZone || 'UTC';
    for (const value of [undefined, null, '', ' local ', 'browser']) assert.equal(resolveTimeZone(value), browser);
    assert.equal(resolveTimeZone('Europe/Amsterdam'), 'Europe/Amsterdam');
  });
  it('formats an instant in the given zone with milliseconds', () => {
    const ms = Date.UTC(2026, 10, 27, 23, 30, 5, 42);
    assert.deepEqual(formatInstantInZone(ms, 'UTC'), { time: '2026-11-27 23:30:05.042', day: '2026-11-27' });
    assert.deepEqual(formatInstantInZone(ms, 'Europe/Amsterdam'), { time: '2026-11-28 00:30:05.042', day: '2026-11-28' });
    assert.deepEqual(formatInstantInZone(Date.UTC(2026, 0, 1, 0, 0, 0), 'UTC').time, '2026-01-01 00:00:00.000');
  });
  it('falls back to UTC for an unknown zone and a dash for an invalid instant', () => {
    const ms = Date.UTC(2026, 10, 27, 1, 2, 3, 4);
    assert.deepEqual(formatInstantInZone(ms, 'Not/AZone'), { time: '2026-11-27 01:02:03.004', day: '2026-11-27' });
    assert.deepEqual(formatInstantInZone(Number.NaN, 'UTC'), { time: '—', day: '' });
  });
});

describe('instantMs', () => {
  it('reads unix seconds, unix milliseconds and numeric strings', () => {
    assert.equal(instantMs(1795771193), 1795771193000);
    assert.equal(instantMs(1795771193795), 1795771193795);
    assert.equal(instantMs('1795771193'), 1795771193000);
  });
  it('reads ISO and SQLite UTC text, treating a zone-less value as UTC', () => {
    assert.equal(instantMs('2026-11-27 10:00:00'), Date.UTC(2026, 10, 27, 10, 0, 0));
    assert.equal(instantMs('2026-11-27T10:00:00Z'), Date.UTC(2026, 10, 27, 10, 0, 0));
    assert.equal(instantMs('2026-11-27T10:00:00+01:00'), Date.UTC(2026, 10, 27, 9, 0, 0));
  });
  it('returns null for empty, zero, negative, non-finite and unparsable input', () => {
    for (const value of [null, undefined, '', 0, -5, Number.NaN, Number.POSITIVE_INFINITY, 'yesterday']) {
      assert.equal(instantMs(value), null, String(value));
    }
  });
});
