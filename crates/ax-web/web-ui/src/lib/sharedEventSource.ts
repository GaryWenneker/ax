/**
 * Shared EventSource hub — one TCP connection per URL per browser tab.
 *
 * Browsers cap ~6 concurrent HTTP/1.1 sockets per host. Command Center opens
 * several long-lived SSE streams (activity, MCP trace, ship, quality). Without
 * sharing, StatusBar + Logging + StrictMode remounts saturate the pool and all
 * `/api/*` fetches hang — the UI looks empty / stuck loading.
 */

type EventHandler = (ev: MessageEvent) => void;

type Hub = {
  url: string;
  es: EventSource | null;
  generation: number;
  refCount: number;
  handlers: Map<string, Set<EventHandler>>;
  errorHandlers: Set<() => void>;
  openHandlers: Set<() => void>;
  reconnectTimer: ReturnType<typeof setTimeout> | null;
  reconnectDelay: number;
  nativeListeners: Map<string, (ev: Event) => void>;
};

const hubs = new Map<string, Hub>();

function bindNative(hub: Hub, type: string) {
  if (hub.nativeListeners.has(type)) return;
  const native = (ev: Event) => {
    const set = hub.handlers.get(type);
    if (!set) return;
    for (const h of [...set]) h(ev as MessageEvent);
  };
  hub.nativeListeners.set(type, native);
  if (!hub.es) return;
  if (type === 'message') {
    hub.es.onmessage = native as (ev: MessageEvent) => void;
  } else {
    hub.es.addEventListener(type, native);
  }
}

function unbindNative(hub: Hub, type: string) {
  const native = hub.nativeListeners.get(type);
  if (!native) return;
  if (hub.es) {
    if (type === 'message') {
      hub.es.onmessage = null;
    } else {
      hub.es.removeEventListener(type, native);
    }
  }
  hub.nativeListeners.delete(type);
}

function tearDownEs(hub: Hub) {
  if (hub.reconnectTimer) {
    clearTimeout(hub.reconnectTimer);
    hub.reconnectTimer = null;
  }
  hub.generation += 1;
  hub.es?.close();
  hub.es = null;
  hub.nativeListeners.clear();
}

function ensureConnected(hub: Hub) {
  if (hub.refCount <= 0) return;
  if (hub.es && hub.es.readyState !== EventSource.CLOSED) return;

  tearDownEs(hub);
  const generation = hub.generation;
  const es = new EventSource(hub.url);
  hub.es = es;

  for (const type of hub.handlers.keys()) {
    bindNative(hub, type);
  }

  es.onopen = () => {
    if (hub.generation !== generation) return;
    hub.reconnectDelay = 1000;
    for (const h of [...hub.openHandlers]) h();
  };

  es.onerror = () => {
    if (hub.generation !== generation) return;
    for (const h of [...hub.errorHandlers]) h();
    // Only recreate after a hard close. While CONNECTING/OPEN the browser
    // already retries — opening a second EventSource would leak sockets.
    if (es.readyState !== EventSource.CLOSED || hub.refCount <= 0) return;
    if (hub.es !== es) return;
    hub.es = null;
    hub.nativeListeners.clear();
    if (hub.reconnectTimer) clearTimeout(hub.reconnectTimer);
    const delay = hub.reconnectDelay;
    hub.reconnectDelay = Math.min(hub.reconnectDelay * 2, 15_000);
    hub.reconnectTimer = setTimeout(() => {
      hub.reconnectTimer = null;
      if (hub.generation !== generation) return;
      ensureConnected(hub);
    }, delay);
  };
}

export type SharedEventSourceOptions = {
  /** Named SSE event types. Use `message` for the default `onmessage` channel. */
  events?: Record<string, EventHandler>;
  onError?: () => void;
  onOpen?: () => void;
};

/** Subscribe to a shared SSE URL. Returns an unsubscribe function. */
export function subscribeSharedEventSource(
  url: string,
  opts: SharedEventSourceOptions,
): () => void {
  let hub = hubs.get(url);
  if (!hub) {
    hub = {
      url,
      es: null,
      generation: 0,
      refCount: 0,
      handlers: new Map(),
      errorHandlers: new Set(),
      openHandlers: new Set(),
      reconnectTimer: null,
      reconnectDelay: 1000,
      nativeListeners: new Map(),
    };
    hubs.set(url, hub);
  }

  hub.refCount += 1;
  const added: Array<[string, EventHandler]> = [];

  if (opts.events) {
    for (const [type, handler] of Object.entries(opts.events)) {
      let set = hub.handlers.get(type);
      const first = !set;
      if (!set) {
        set = new Set();
        hub.handlers.set(type, set);
      }
      set.add(handler);
      added.push([type, handler]);
      if (first) bindNative(hub, type);
    }
  }
  if (opts.onError) hub.errorHandlers.add(opts.onError);
  if (opts.onOpen) hub.openHandlers.add(opts.onOpen);

  ensureConnected(hub);

  return () => {
    for (const [type, handler] of added) {
      const set = hub!.handlers.get(type);
      set?.delete(handler);
      if (set && set.size === 0) {
        hub!.handlers.delete(type);
        unbindNative(hub!, type);
      }
    }
    if (opts.onError) hub!.errorHandlers.delete(opts.onError);
    if (opts.onOpen) hub!.openHandlers.delete(opts.onOpen);

    hub!.refCount -= 1;
    if (hub!.refCount <= 0) {
      tearDownEs(hub!);
      hubs.delete(url);
    }
  };
}

/** Close every shared SSE (e.g. pagehide) so sockets cannot outlive the tab. */
export function closeAllSharedEventSources() {
  for (const [url, hub] of [...hubs.entries()]) {
    hub.refCount = 0;
    tearDownEs(hub);
    hubs.delete(url);
  }
}

if (typeof window !== 'undefined') {
  window.addEventListener('pagehide', () => closeAllSharedEventSources());
}
