import assert from 'node:assert/strict';
import test from 'node:test';
import { mountScrollReveal } from '../src/scripts/scroll-reveal.js';

test('reveal handles intersection, focus, Pause, Play and teardown', () => {
  const saved = { window: globalThis.window, document: globalThis.document, observer: globalThis.IntersectionObserver };
  const classes = new Set();
  const item = { classList: { add: (x: string) => classes.add(x), remove: (x: string) => classes.delete(x) }, contains: (target: unknown) => target === item };
  const events = new Map();
  let callback;
  let observed = false;
  class Observer {
    constructor(cb) { callback = cb; }
    observe() { observed = true; }
    unobserve() { observed = false; }
    disconnect() { observed = false; }
  }
  const preference = { matches: false, addEventListener: () => {}, removeEventListener: () => {} };
  const document = { documentElement: { dataset: {} }, querySelectorAll: (selector: string) => selector.includes('[data-ax-reveal]') ? [item] : [], addEventListener: (name, cb) => events.set(name, cb), removeEventListener: name => events.delete(name) };
  globalThis.window = { IntersectionObserver: Observer, matchMedia: () => preference, addEventListener: (name, cb) => events.set(name, cb), removeEventListener: name => events.delete(name) };
  globalThis.document = document;
  globalThis.IntersectionObserver = Observer;
  try {
    const cleanup = mountScrollReveal();
    assert.ok(classes.has('ax-reveal-pending'));
    assert.ok(observed);
    document.documentElement.dataset.axMotion = 'paused';
    events.get('ax:motion')();
    assert.equal(classes.has('ax-reveal-pending'), false);
    document.documentElement.dataset.axMotion = 'running';
    events.get('ax:motion')();
    assert.ok(classes.has('ax-reveal-pending'), 'Play resumes unseen sections');
    callback([{ target: item, isIntersecting: false }]);
    assert.ok(classes.has('ax-reveal-pending'));
    events.get('focusin')({ target: {} });
    assert.ok(classes.has('ax-reveal-pending'), 'Unrelated focus does not reveal this section');
    events.get('focusin')({ target: item });
    assert.equal(classes.has('ax-reveal-pending'), false);
    events.get('ax:motion')();
    assert.equal(classes.has('ax-reveal-pending'), false, 'Seen sections stay visible');
    cleanup();
    assert.equal(events.size, 0);
    assert.equal(observed, false);
    const cleanupAgain = mountScrollReveal();
    callback([{ target: item, isIntersecting: true }]);
    assert.equal(classes.has('ax-reveal-pending'), false, 'An intersecting section becomes visible');
    cleanupAgain();
  } finally {
    globalThis.window = saved.window;
    globalThis.document = saved.document;
    globalThis.IntersectionObserver = saved.observer;
  }
});

test('unsupported observer leaves content visible without touching the document', () => {
  const saved = globalThis.window;
  globalThis.window = {};
  try { assert.doesNotThrow(() => mountScrollReveal()()); }
  finally { globalThis.window = saved; }
});
