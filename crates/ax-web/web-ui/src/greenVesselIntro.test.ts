import { test } from 'node:test';
import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import {
  INTRO_MS,
  REMOVE_FALLBACK_MS,
  greenVesselCssAccepts,
  scheduleChime,
  startGreenVesselIntro,
} from './greenVesselIntro.ts';

const cssUrl = new URL('./greenVesselIntro.css', import.meta.url);
const htmlUrl = new URL('../index.html', import.meta.url);
const sourceUrl = new URL('./greenVesselIntro.ts', import.meta.url);

type Listener = (ev?: unknown) => void;

function fakeOverlay() {
  const listeners = new Map<string, Set<Listener>>();
  return {
    id: 'ax-green-vessel',
    dataset: {} as Record<string, string>,
    style: { pointerEvents: '' },
    classes: new Set<string>(),
    classList: {
      add(name: string) {
        this.classes.add(name);
      },
      contains(name: string) {
        return this.classes.has(name);
      },
    },
    removed: false,
    setAttribute() {},
    addEventListener(type: string, fn: Listener) {
      const set = listeners.get(type) ?? new Set();
      set.add(fn);
      listeners.set(type, set);
    },
    removeEventListener(type: string, fn: Listener) {
      listeners.get(type)?.delete(fn);
    },
    children: [] as { className: string }[],
    appendChild(node: { className: string }) {
      this.children.push(node);
    },
    remove() {
      this.removed = true;
    },
    dispatch(type: string) {
      for (const fn of [...(listeners.get(type) ?? [])]) fn();
    },
  };
}

function fakeDocument(reduce = false) {
  const overlay = fakeOverlay();
  overlay.classList = {
    add: (name: string) => overlay.classes.add(name),
    contains: (name: string) => overlay.classes.has(name),
  };
  const docListeners = new Map<string, Set<Listener>>();
  const storage = {
    getItem() {
      throw new Error('storage read');
    },
    setItem() {
      throw new Error('storage write');
    },
  };
  const doc = {
    overlay,
    getElementById(id: string) {
      return id === overlay.id ? overlay : null;
    },
    createElement() {
      const node = {
        className: '',
        dataset: {} as Record<string, string>,
        style: {} as Record<string, string>,
        children: [] as { className: string; children?: { className: string }[] }[],
        appendChild(child: { className: string }) {
          this.children.push(child);
        },
      };
      return node;
    },
    body: { appendChild() {} },
    addEventListener(type: string, fn: Listener) {
      const set = docListeners.get(type) ?? new Set();
      set.add(fn);
      docListeners.set(type, set);
    },
    removeEventListener(type: string, fn: Listener) {
      docListeners.get(type)?.delete(fn);
    },
    dispatch(type: string) {
      for (const fn of [...(docListeners.get(type) ?? [])]) fn();
    },
    defaultView: {
      localStorage: storage,
      sessionStorage: storage,
      matchMedia(query: string) {
        return { matches: reduce && query.includes('prefers-reduced-motion') };
      },
    },
  };
  return doc;
}

test('stylesheet keeps the green vessel contract', () => {
  const css = readFileSync(cssUrl, 'utf8');
  const result = greenVesselCssAccepts(css);
  assert.equal(result.ok, true, result.reason);
  assert.ok(INTRO_MS >= 2000 && INTRO_MS <= 4200);
  assert.ok(REMOVE_FALLBACK_MS > INTRO_MS);
});

test('checker fails when the neon shadow is missing', () => {
  const bad = readFileSync(cssUrl, 'utf8').replaceAll('#00ed82', '#ffffff');
  assert.equal(greenVesselCssAccepts(bad).ok, false);
});

test('the dark center stays open and the rim is an ellipse', () => {
  const css = readFileSync(cssUrl, 'utf8');
  assert.match(css, /mask-image:[^;]*ellipse at center/);
  assert.match(css, /mask-image:[^;]*transparent 74%/);
  assert.match(css, /opacity:\s*0\.78/);
});

test('the glow is a teal top and green sides, brighter on the right', () => {
  const css = readFileSync(cssUrl, 'utf8');
  assert.match(css, /radial-gradient/);
  assert.match(css, /filter:\s*blur\((?:3[6-9]|[4-9]\d)px\)/);
  assert.match(css, /70,\s*230,\s*220/);
  assert.match(css, /40,\s*210,\s*130,\s*0\.42/);
  assert.match(css, /40,\s*220,\s*140,\s*0\.72/);
  assert.doesNotMatch(css, /255,\s*70,\s*190/);
  assert.doesNotMatch(css, /255,\s*170,\s*40/);
});

test('checker fails when the duration is not a finite number of seconds', () => {
  const bad = readFileSync(cssUrl, 'utf8').replace(
    'animation: ax-green-vessel 3.6s ease-in-out forwards;',
    'animation: ax-green-vessel ..s ease-out forwards;',
  );
  assert.equal(greenVesselCssAccepts(bad).ok, false);
});

test('the page stays covered until halfway, then the veil fades', () => {
  const css = readFileSync(cssUrl, 'utf8');
  const veil = css.slice(css.indexOf('@keyframes ax-green-vessel {'));
  assert.match(veil, /50%\s*\{[^}]*background-color:\s*#000/);
  assert.match(veil, /100%\s*\{[^}]*background-color:\s*transparent/);
});

test('index.html mounts a full-screen overlay', () => {
  const html = readFileSync(htmlUrl, 'utf8');
  assert.match(html, /id="ax-green-vessel"/);
  assert.match(html, /position:\s*fixed/);
  assert.match(html, /z-index:\s*9999/);
  assert.match(html, /pointer-events:\s*none/);
});

test('start adds the playing class and does not touch storage', () => {
  const doc = fakeDocument();
  let plays = 0;
  startGreenVesselIntro(
    doc as unknown as Document,
    () => {
      plays += 1;
      return Promise.resolve();
    },
    () => {},
  );
  assert.equal(plays, 1);
  assert.equal(doc.overlay.classes.has('is-playing'), true);
  assert.equal(doc.overlay.style.pointerEvents, 'none');
  startGreenVesselIntro(doc as unknown as Document, () => {
    plays += 1;
    return Promise.resolve();
  });
  assert.equal(plays, 1);
});

test('a rejected play still starts the glow and retries once on click', async () => {
  const doc = fakeDocument();
  let plays = 0;
  startGreenVesselIntro(
    doc as unknown as Document,
    () => {
      plays += 1;
      return plays === 1 ? Promise.reject(new Error('blocked')) : Promise.resolve();
    },
    () => {},
  );
  await Promise.resolve();
  assert.equal(doc.overlay.classes.has('is-playing'), true);
  assert.equal(plays, 1);
  doc.dispatch('pointerdown');
  assert.equal(plays, 2);
  doc.dispatch('keydown');
  assert.equal(plays, 2);
});

test('animationend and the fallback timer each remove the overlay', () => {
  const doc = fakeDocument();
  let scheduled: (() => void) | null = null;
  let delay = 0;
  startGreenVesselIntro(
    doc as unknown as Document,
    () => Promise.resolve(),
    (fn, ms) => {
      scheduled = fn;
      delay = ms;
    },
  );
  assert.equal(delay, REMOVE_FALLBACK_MS);
  doc.overlay.dispatch('animationend');
  assert.equal(doc.overlay.removed, true);
  assert.doesNotThrow(() => scheduled?.());

  const second = fakeDocument();
  let fallback: (() => void) | null = null;
  startGreenVesselIntro(
    second as unknown as Document,
    () => Promise.resolve(),
    (fn) => {
      fallback = fn;
    },
  );
  assert.equal(second.overlay.removed, false);
  fallback?.();
  assert.equal(second.overlay.removed, true);
});

test('reduced motion removes the overlay without the playing class', () => {
  const doc = fakeDocument(true);
  let plays = 0;
  startGreenVesselIntro(doc as unknown as Document, () => {
    plays += 1;
    return Promise.resolve();
  });
  assert.equal(plays, 1);
  assert.equal(doc.overlay.removed, true);
  assert.equal(doc.overlay.classes.has('is-playing'), false);
});

test('scheduleChime starts three oscillators on the destination', () => {
  const events: string[] = [];
  const gain = () => ({
    gain: {
      setValueAtTime() {},
      exponentialRampToValueAtTime() {},
    },
    connect(target: unknown) {
      events.push(target === 'dest' ? 'master-dest' : 'gain-connect');
    },
  });
  const ctx = {
    currentTime: 1,
    destination: 'dest',
    createGain: gain,
    createOscillator() {
      return {
        type: '',
        frequency: {
          setValueAtTime(_v: number) {
            events.push('freq');
          },
          exponentialRampToValueAtTime() {},
        },
        connect() {
          events.push('osc');
        },
        start() {
          events.push('start');
        },
        stop() {
          events.push('stop');
        },
      };
    },
  };
  scheduleChime(ctx as unknown as AudioContext);
  assert.equal(events.filter((e) => e === 'start').length, 3);
  assert.equal(events.includes('master-dest'), true);
});

test('intro source does not mention web storage', () => {
  const source = readFileSync(sourceUrl, 'utf8');
  assert.equal(source.includes('localStorage'), false);
  assert.equal(source.includes('sessionStorage'), false);
});
