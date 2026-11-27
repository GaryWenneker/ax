import assert from 'node:assert/strict';
import { beforeEach, test } from 'node:test';

/** A fake EventSource: records every instance and lets a test drive readyState and events. */
class FakeEventSource extends EventTarget {
  static CONNECTING = 0;
  static OPEN = 1;
  static CLOSED = 2;
  static all: FakeEventSource[] = [];
  readyState = FakeEventSource.CONNECTING;
  onmessage: ((ev: MessageEvent) => void) | null = null;
  onopen: (() => void) | null = null;
  onerror: (() => void) | null = null;
  readonly url: string;
  constructor(url: string) {
    super();
    this.url = url;
    FakeEventSource.all.push(this);
  }
  close() {
    this.readyState = FakeEventSource.CLOSED;
  }
  open() {
    this.readyState = FakeEventSource.OPEN;
    this.onopen?.();
  }
  emit(type: string, data: string) {
    // A closed EventSource never dispatches again.
    if (this.readyState === FakeEventSource.CLOSED) return;
    const ev = new MessageEvent(type, { data });
    if (type === 'message') this.onmessage?.(ev);
    else this.dispatchEvent(ev);
  }
  fail(hardClose: boolean) {
    if (hardClose) this.readyState = FakeEventSource.CLOSED;
    this.onerror?.();
  }
}

const fakeWindow = new EventTarget();
const g = globalThis as unknown as { EventSource: unknown; window: unknown };
g.EventSource = FakeEventSource;
g.window = fakeWindow;

const { subscribeSharedEventSource, closeAllSharedEventSources } = await import('./sharedEventSource.ts');

function live(url: string): FakeEventSource[] {
  return FakeEventSource.all.filter((es) => es.url === url && es.readyState !== FakeEventSource.CLOSED);
}

function pageEvent(type: 'pagehide' | 'pageshow', persisted: boolean) {
  const ev = new Event(type) as Event & { persisted: boolean };
  ev.persisted = persisted;
  fakeWindow.dispatchEvent(ev);
}

beforeEach(() => {
  closeAllSharedEventSources();
  FakeEventSource.all = [];
});

test('subscribers of one URL share a single connection and the last one closes it', () => {
  const got: string[] = [];
  const a = subscribeSharedEventSource('/api/a', { events: { tick: (ev) => got.push(`a:${ev.data}`) } });
  const b = subscribeSharedEventSource('/api/a', { events: { tick: (ev) => got.push(`b:${ev.data}`) } });
  assert.equal(FakeEventSource.all.length, 1);
  FakeEventSource.all[0].emit('tick', '1');
  assert.deepEqual(got, ['a:1', 'b:1']);
  a();
  assert.equal(live('/api/a').length, 1);
  b();
  assert.equal(live('/api/a').length, 0);
});

test('the default message channel goes through onmessage', () => {
  const got: string[] = [];
  const off = subscribeSharedEventSource('/api/m', { events: { message: (ev) => got.push(ev.data) } });
  FakeEventSource.all[0].emit('message', 'hello');
  assert.deepEqual(got, ['hello']);
  off();
  assert.equal(FakeEventSource.all[0].onmessage, null);
});

test('a soft error lets the browser retry; a hard close reconnects with the listeners rebound', async (t) => {
  t.mock.timers.enable({ apis: ['setTimeout'] });
  const got: string[] = [];
  let errors = 0;
  let opens = 0;
  const off = subscribeSharedEventSource('/api/r', {
    events: { tick: (ev) => got.push(ev.data) },
    onError: () => { errors += 1; },
    onOpen: () => { opens += 1; },
  });
  const first = FakeEventSource.all[0];
  first.open();
  first.fail(false);
  t.mock.timers.tick(60_000);
  assert.equal(FakeEventSource.all.length, 1, 'no second EventSource while the browser retries');
  first.fail(true);
  t.mock.timers.tick(999);
  assert.equal(FakeEventSource.all.length, 1);
  t.mock.timers.tick(1);
  assert.equal(FakeEventSource.all.length, 2);
  const second = FakeEventSource.all[1];
  second.open();
  second.emit('tick', 'after');
  first.emit('tick', 'stale');
  assert.deepEqual(got, ['after']);
  assert.equal(errors, 2);
  assert.equal(opens, 2);
  off();
});

test('reconnect delay doubles up to 15 s and resets after a successful open', (t) => {
  t.mock.timers.enable({ apis: ['setTimeout'] });
  const off = subscribeSharedEventSource('/api/d', { events: { tick: () => {} } });
  const delays: number[] = [];
  for (let i = 0; i < 6; i += 1) {
    const before = FakeEventSource.all.length;
    FakeEventSource.all.at(-1)!.fail(true);
    let waited = 0;
    while (FakeEventSource.all.length === before) {
      t.mock.timers.tick(500);
      waited += 500;
    }
    delays.push(waited);
  }
  assert.deepEqual(delays, [1000, 2000, 4000, 8000, 15000, 15000]);
  FakeEventSource.all.at(-1)!.open();
  const before = FakeEventSource.all.length;
  FakeEventSource.all.at(-1)!.fail(true);
  t.mock.timers.tick(1000);
  assert.equal(FakeEventSource.all.length, before + 1);
  off();
});

test('unsubscribing during a pending reconnect cancels it', (t) => {
  t.mock.timers.enable({ apis: ['setTimeout'] });
  const off = subscribeSharedEventSource('/api/c', { events: { tick: () => {} } });
  FakeEventSource.all[0].fail(true);
  off();
  t.mock.timers.tick(60_000);
  assert.equal(FakeEventSource.all.length, 1);
});

test('a late batch subscriber reopens the stream so it receives the snapshot', () => {
  const offA = subscribeSharedEventSource('/api/b', { events: { batch: () => {} } });
  FakeEventSource.all[0].open();
  const got: string[] = [];
  const offB = subscribeSharedEventSource('/api/b', { events: { batch: (ev) => got.push(ev.data) } });
  assert.equal(FakeEventSource.all.length, 2);
  assert.equal(live('/api/b').length, 1);
  FakeEventSource.all[1].emit('batch', 'snapshot');
  assert.deepEqual(got, ['snapshot']);
  offA();
  offB();
});

test('pagehide closes the streams and a back/forward restore reopens them for mounted subscribers', () => {
  const got: string[] = [];
  const off = subscribeSharedEventSource('/api/p', { events: { tick: (ev) => got.push(ev.data) } });
  pageEvent('pagehide', true);
  assert.equal(live('/api/p').length, 0, 'pagehide closes the socket');
  pageEvent('pageshow', true);
  assert.equal(live('/api/p').length, 1, 'restored page reconnects');
  live('/api/p')[0].emit('tick', 'restored');
  assert.deepEqual(got, ['restored']);
  off();
  assert.equal(live('/api/p').length, 0);
});

test('a normal pageshow does not open extra connections', () => {
  const off = subscribeSharedEventSource('/api/n', { events: { tick: () => {} } });
  pageEvent('pageshow', false);
  assert.equal(FakeEventSource.all.length, 1);
  off();
});

test('a stale unsubscribe never removes a newer hub for the same URL', () => {
  const stale = subscribeSharedEventSource('/api/s', { events: { tick: () => {} } });
  closeAllSharedEventSources();
  const got: string[] = [];
  const fresh = subscribeSharedEventSource('/api/s', { events: { tick: (ev) => got.push(ev.data) } });
  stale();
  stale();
  assert.equal(live('/api/s').length, 1, 'the fresh subscriber keeps its stream');
  const again = subscribeSharedEventSource('/api/s', { events: { tick: () => {} } });
  assert.equal(live('/api/s').length, 1, 'a third subscriber shares it instead of opening a second socket');
  live('/api/s')[0].emit('tick', 'x');
  assert.deepEqual(got, ['x']);
  again();
  fresh();
  assert.equal(live('/api/s').length, 0);
});

test('calling an unsubscribe twice only releases one reference', () => {
  const a = subscribeSharedEventSource('/api/t', { events: { tick: () => {} } });
  const b = subscribeSharedEventSource('/api/t', { events: { tick: () => {} } });
  a();
  a();
  assert.equal(live('/api/t').length, 1);
  b();
  assert.equal(live('/api/t').length, 0);
});
