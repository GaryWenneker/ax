/** Live updates: React hooks over `/api/changes` (docs/specs/live-updates.md). */

import { useCallback, useEffect, useRef, useState } from 'react';
import { WORKSPACE_SWITCHED } from '../workspaceEvents';
import { createDebouncer, parseChange, trackNewKeys, type KeyBaseline, type LiveTopic } from './live';
import { subscribeSharedEventSource } from './sharedEventSource';

const CHANGES_URL = '/api/changes';
const RELOAD_DEBOUNCE_MS = 400;
export const GLOW_MS = 2600;

/**
 * Call `reload` when one of `topics` changes on the server. While `paused`
 * (an open editor or form), the reload waits and runs once it is unpaused.
 */
export function useLive(topics: LiveTopic | readonly LiveTopic[], reload: () => void, paused = false): void {
  const reloadRef = useRef(reload);
  reloadRef.current = reload;
  const pausedRef = useRef(paused);
  pausedRef.current = paused;
  const pendingRef = useRef(false);
  const key = typeof topics === 'string' ? topics : topics.join(',');

  useEffect(() => {
    const wanted = new Set(key.split(','));
    const fire = createDebouncer(RELOAD_DEBOUNCE_MS, () => {
      if (pausedRef.current) {
        pendingRef.current = true;
        return;
      }
      reloadRef.current();
    });
    return subscribeSharedEventSource(CHANGES_URL, {
      events: {
        change: (ev) => {
          const change = parseChange(String(ev.data));
          if (change && wanted.has(change.topic)) fire();
        },
      },
    });
  }, [key]);

  useEffect(() => {
    if (!paused && pendingRef.current) {
      pendingRef.current = false;
      reloadRef.current();
    }
  }, [paused]);
}

/**
 * Keys that appeared since the previous `keys` value, for `GLOW_MS`.
 * Pass `null` while loading. The first list, a workspace switch, and a new
 * `scope` (the filter, search, or page that produced `keys`) set a baseline
 * and mark nothing new.
 */
export function useNewKeys(keys: readonly string[] | null, scope = ''): ReadonlySet<string> {
  const prev = useRef<KeyBaseline | null>(null);
  const [fresh, mark] = useFreshMarks();
  const signature = keys === null ? null : keys.join('\u0000');

  useEffect(() => {
    const reset = () => {
      prev.current = null;
    };
    window.addEventListener(WORKSPACE_SWITCHED, reset);
    return () => window.removeEventListener(WORKSPACE_SWITCHED, reset);
  }, []);

  useEffect(() => {
    if (keys === null) return;
    const { state, added } = trackNewKeys(prev.current, scope, keys);
    prev.current = state;
    mark(added);
    // eslint-disable-next-line react-hooks/exhaustive-deps -- `signature` captures `keys`
  }, [signature, scope]);

  return fresh;
}

/**
 * Keys marked fresh for `GLOW_MS`. For streams that know which items arrived
 * live (as opposed to a replayed backlog), call `mark` with those keys.
 */
export function useFreshMarks(): [ReadonlySet<string>, (keys: readonly string[]) => void] {
  const timers = useRef(new Set<ReturnType<typeof setTimeout>>());
  const [fresh, setFresh] = useState<ReadonlySet<string>>(() => new Set());

  useEffect(() => {
    const pending = timers.current;
    return () => {
      for (const t of pending) clearTimeout(t);
      pending.clear();
    };
  }, []);

  const mark = useCallback((keys: readonly string[]) => {
    if (keys.length === 0) return;
    setFresh((cur) => new Set([...cur, ...keys]));
    const t = setTimeout(() => {
      timers.current.delete(t);
      setFresh((cur) => {
        const next = new Set(cur);
        for (const k of keys) next.delete(k);
        return next;
      });
    }, GLOW_MS);
    timers.current.add(t);
  }, []);

  return [fresh, mark];
}

/** `' live-new'` for a fresh key, else `''`; append to a row's className. The class runs a text chase, not a background glow. */
export function liveClass(fresh: ReadonlySet<string>, key: string): string {
  return fresh.has(key) ? ' live-new' : '';
}
