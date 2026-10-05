export const INTRO_MS = 3600;
export const REMOVE_FALLBACK_MS = 4000;

const OVERLAY_ID = 'ax-green-vessel';

type ChimeContext = {
  currentTime: number;
  destination: AudioNode;
  state?: AudioContextState;
  resume?: () => Promise<void>;
  createGain: () => GainNode;
  createOscillator: () => OscillatorNode;
};

const PARTIALS = [
  { freq: 392, gain: 0.45 },
  { freq: 587.33, gain: 0.32 },
  { freq: 784, gain: 0.22 },
];

export function greenVesselCssAccepts(css: string): { ok: boolean; reason: string } {
  const required = [
    'position: fixed',
    'inset: 0',
    'z-index: 9999',
    '#00ed82',
    'box-shadow: inset 0 0 60px #00ed82, 0 0 40px #00ed82',
    '@keyframes ax-green-vessel',
    'opacity: 0',
    'pointer-events: none',
    'transition: opacity',
    '#1ce783',
  ];
  for (const needle of required) {
    if (!css.includes(needle)) return { ok: false, reason: `missing ${needle}` };
  }
  const duration = css.match(/animation:\s*ax-green-vessel\s+([\d.]+)s/);
  if (!duration) return { ok: false, reason: 'missing duration' };
  const seconds = Number(duration[1]);
  if (!Number.isFinite(seconds) || seconds < 2 || seconds > 4.2) {
    return { ok: false, reason: `duration ${seconds}` };
  }
  return { ok: true, reason: '' };
}

export function scheduleChime(ctx: ChimeContext): void {
  const now = ctx.currentTime;
  const master = ctx.createGain();
  master.gain.setValueAtTime(0.0001, now);
  master.gain.exponentialRampToValueAtTime(0.2, now + 0.03);
  master.gain.exponentialRampToValueAtTime(0.0001, now + 0.85);
  master.connect(ctx.destination);

  for (const partial of PARTIALS) {
    const osc = ctx.createOscillator();
    osc.type = 'sine';
    osc.frequency.setValueAtTime(partial.freq, now);
    osc.frequency.exponentialRampToValueAtTime(partial.freq * 1.015, now + 0.35);
    const voice = ctx.createGain();
    voice.gain.setValueAtTime(partial.gain, now);
    osc.connect(voice);
    voice.connect(master);
    osc.start(now);
    osc.stop(now + 0.9);
  }
}

// One context for this page load. A hard refresh evaluates the module again.
let sharedCtx: AudioContext | null = null;
let chimeStarted = false;

type AudioHost = typeof globalThis & {
  webkitAudioContext?: typeof AudioContext;
};

export function startWebAudioChime(host: AudioHost = globalThis): Promise<void> {
  try {
    const ctor = host.AudioContext ?? host.webkitAudioContext;
    if (!ctor) return Promise.reject(new Error('Web Audio unavailable'));
    const ctx = sharedCtx ?? new ctor();
    sharedCtx = ctx;
    const ready = ctx.state === 'suspended' ? ctx.resume() : Promise.resolve();
    return ready.then(() => {
      if (ctx.state === 'suspended') throw new Error('autoplay blocked');
      if (chimeStarted) return;
      chimeStarted = true;
      scheduleChime(ctx);
    });
  } catch (err) {
    return Promise.reject(err instanceof Error ? err : new Error('autoplay blocked'));
  }
}

function prefersReducedMotion(doc: Document): boolean {
  try {
    return doc.defaultView?.matchMedia('(prefers-reduced-motion: reduce)').matches ?? false;
  } catch {
    // matchMedia can throw in locked-down documents; play the full intro.
    return false;
  }
}

function overlayOf(doc: Document): HTMLElement {
  const found = doc.getElementById(OVERLAY_ID);
  if (found) return found;
  const el = doc.createElement('div');
  el.id = OVERLAY_ID;
  el.setAttribute('aria-hidden', 'true');
  doc.body.appendChild(el);
  return el;
}

function armUnlock(doc: Document, play: () => Promise<void>): void {
  const once = () => {
    doc.removeEventListener('pointerdown', once, true);
    doc.removeEventListener('keydown', once, true);
    void Promise.resolve(play()).catch(() => {
      // A second rejection still must not reject the intro; the glow does not depend on audio.
    });
  };
  doc.addEventListener('pointerdown', once, true);
  doc.addEventListener('keydown', once, true);
}

function attemptPlay(doc: Document, play: () => Promise<void>): void {
  try {
    void Promise.resolve(play()).catch(() => armUnlock(doc, play));
  } catch {
    armUnlock(doc, play);
  }
}

export function startGreenVesselIntro(
  doc: Document,
  play: () => Promise<void>,
  schedule: (fn: () => void, ms: number) => void = (fn, ms) => {
    setTimeout(fn, ms);
  },
): void {
  const el = overlayOf(doc);
  if (el.dataset.axVessel === 'started') return;
  el.dataset.axVessel = 'started';
  el.style.pointerEvents = 'none';
  el.setAttribute('aria-hidden', 'true');
  attemptPlay(doc, play);

  if (prefersReducedMotion(doc)) {
    el.remove();
    return;
  }

  const remove = () => {
    el.remove();
  };
  el.classList.add('is-playing');
  el.addEventListener('animationend', remove, { once: true });
  schedule(remove, REMOVE_FALLBACK_MS);
}
