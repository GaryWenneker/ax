/**
 * Pages and code grammars load as separate chunks. After an upgrade an open tab still
 * asks for the old chunk names, which the server no longer has. Vite reports that as
 * `vite:preloadError`; the page then reloads once to pick up the new index.html.
 */

const STORAGE_KEY = 'ax-chunk-reload-at';
/** A second failure inside this window shows the error instead of reloading in a loop. */
const GUARD_MS = 60_000;

export function shouldReloadForChunkError(lastReloadAt: number | null, now: number): boolean {
  if (lastReloadAt === null || !Number.isFinite(lastReloadAt)) return true;
  return now - lastReloadAt > GUARD_MS;
}

export interface ChunkReloadEnv {
  addEventListener(type: string, listener: (event: { preventDefault(): void }) => void): void;
  storage: { getItem(key: string): string | null; setItem(key: string, value: string): void } | null;
  reload(): void;
  now(): number;
}

export function installChunkReload(env: ChunkReloadEnv): void {
  let memoryLast: number | null = null;
  env.addEventListener('vite:preloadError', (event) => {
    let last = memoryLast;
    try {
      const stored = env.storage?.getItem(STORAGE_KEY);
      if (stored != null) last = Number(stored);
    } catch {
      // Storage blocked: the in-memory value still stops a loop within this page load.
    }
    const now = env.now();
    if (!shouldReloadForChunkError(last, now)) return;
    memoryLast = now;
    try {
      env.storage?.setItem(STORAGE_KEY, String(now));
    } catch {
      // See above.
    }
    event.preventDefault();
    env.reload();
  });
}

export function installBrowserChunkReload(): void {
  let storage: ChunkReloadEnv['storage'] = null;
  try {
    storage = window.sessionStorage;
  } catch {
    storage = null;
  }
  installChunkReload({
    addEventListener: (type, listener) => window.addEventListener(type, listener as EventListener),
    storage,
    reload: () => window.location.reload(),
    now: () => Date.now(),
  });
}
