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
  /**
   * Called when the stream opens again after this subscriber already saw it open
   * (a browser retry or a hub reconnect). Events sent while it was down are not
   * replayed, so a subscriber that tracks server state should reload here.
   */
  onReconnect?: () => void;
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

  // A connection opened for another subscriber already sent its snapshot.
  // Late listeners, such as Logging after the status bar, would otherwise
  // miss that snapshot and fall back to yesterday's file.
  if (opts.events?.batch && hub.es && hub.es.readyState !== EventSource.CLOSED) {
    tearDownEs(hub);
  }
  ensureConnected(hub);

  let reopenHandler: (() => void) | null = null;
  if (opts.onReconnect) {
    const onReconnect = opts.onReconnect;
    // A stream that is already open counts as seen: its next open is a reconnect.
    let seenOpen = hub.es?.readyState === EventSource.OPEN;
    reopenHandler = () => {
      if (seenOpen) onReconnect();
      seenOpen = true;
    };
    hub.openHandlers.add(reopenHandler);
  }

  let released = false;
  return () => {
    // React may call a cleanup twice; release this subscriber's reference only once.
    if (released) return;
    released = true;
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
    if (reopenHandler) hub!.openHandlers.delete(reopenHandler);

    hub!.refCount = Math.max(0, hub!.refCount - 1);
    if (hub!.refCount === 0) {
      tearDownEs(hub!);
      // A newer hub may own this URL by now; only forget the one this subscriber joined.
      if (hubs.get(url) === hub) hubs.delete(url);
    }
  };
}

/** Close every shared SSE and forget all hubs. Subscribers still mounted are not reconnected. */
export function closeAllSharedEventSources() {
  for (const [url, hub] of [...hubs.entries()]) {
    hub.refCount = 0;
    tearDownEs(hub);
    hubs.delete(url);
  }
}

/**
 * Close the sockets on pagehide so they cannot outlive the tab (and the page stays eligible
 * for the back/forward cache), but keep every hub and its subscribers.
 */
function suspendSharedEventSources() {
  for (const hub of hubs.values()) tearDownEs(hub);
}

/** A page restored from the back/forward cache reopens the streams its subscribers still use. */
function resumeSharedEventSources() {
  for (const hub of hubs.values()) {
    hub.reconnectDelay = 1000;
    ensureConnected(hub);
  }
}

if (typeof window !== 'undefined') {
  window.addEventListener('pagehide', () => suspendSharedEventSources());
  window.addEventListener('pageshow', (ev) => {
    if ((ev as PageTransitionEvent).persisted) resumeSharedEventSources();
  });
}
