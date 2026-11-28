export interface GraphGroup {
  query: string;
  color: string;
}

export interface GraphSettings {
  search: string;
  labels: boolean;
  attachments: boolean;
  existingOnly: boolean;
  orphans: boolean;
  groups: GraphGroup[];
  arrows: boolean;
  textFadeThreshold: number;
  nodeSize: number;
  linkThickness: number;
  centerForce: number;
  repelForce: number;
  linkForce: number;
  linkDistance: number;
}

export interface StorageLike {
  getItem(key: string): string | null;
  setItem(key: string, value: string): void;
}

export const GRAPH_SETTINGS_KEY = 'ax.graph.settings';

/** Debounce window for persisting slider / filter changes (ms). */
export const GRAPH_SETTINGS_SAVE_MS = 300;

export const DEFAULT_GRAPH_SETTINGS: GraphSettings = {
  search: '',
  labels: false,
  attachments: true,
  existingOnly: false,
  orphans: true,
  groups: [],
  arrows: false,
  textFadeThreshold: 0,
  nodeSize: 1,
  linkThickness: 1,
  centerForce: 0.52,
  repelForce: 10,
  linkForce: 1,
  linkDistance: 250,
};

function defaultStorage(): StorageLike | null {
  return typeof localStorage === 'undefined' ? null : localStorage;
}

type PendingSave = {
  timer: ReturnType<typeof setTimeout> | null;
  settings: GraphSettings;
  storage: StorageLike | null;
  key: string;
};

const pendingByKey = new Map<string, PendingSave>();
const storageIds = new WeakMap<StorageLike, number>();
let nextStorageId = 0;

function pendingKey(storage: StorageLike, key: string): string {
  let id = storageIds.get(storage);
  if (id === undefined) {
    id = ++nextStorageId;
    storageIds.set(storage, id);
  }
  return `${id}\0${key}`;
}

function writeGraphSettingsNow(
  settings: GraphSettings,
  storage: StorageLike | null,
  key: string,
): void {
  try {
    storage?.setItem(key, JSON.stringify(settings));
  } catch {
    // Storage is full or blocked: the settings stay in memory for this page.
  }
}

type NumberKey = {
  [K in keyof GraphSettings]: GraphSettings[K] extends number ? K : never;
}[keyof GraphSettings];
type BooleanKey = {
  [K in keyof GraphSettings]: GraphSettings[K] extends boolean ? K : never;
}[keyof GraphSettings];

/** Slider ranges from the graph settings panel; stored numbers are clamped to them. */
const NUMBER_RANGES: Record<NumberKey, readonly [number, number]> = {
  textFadeThreshold: [-3, 3],
  nodeSize: [0.1, 5],
  linkThickness: [0.1, 5],
  centerForce: [0, 1],
  repelForce: [0, 20],
  linkForce: [0, 1],
  linkDistance: [30, 500],
};

const BOOLEAN_KEYS: readonly BooleanKey[] = ['labels', 'attachments', 'existingOnly', 'orphans', 'arrows'];

function isGroup(value: unknown): value is GraphGroup {
  if (!value || typeof value !== 'object') return false;
  const g = value as Record<string, unknown>;
  return typeof g.query === 'string' && typeof g.color === 'string';
}

/** Build settings from stored JSON, keeping each valid field and defaulting the rest. */
function sanitizeGraphSettings(parsed: Record<string, unknown>): GraphSettings {
  const out: GraphSettings = { ...DEFAULT_GRAPH_SETTINGS, groups: [] };
  if (typeof parsed.search === 'string') out.search = parsed.search;
  for (const k of BOOLEAN_KEYS) {
    const v = parsed[k];
    if (typeof v === 'boolean') out[k] = v;
  }
  for (const k of Object.keys(NUMBER_RANGES) as NumberKey[]) {
    const v = parsed[k];
    if (typeof v !== 'number' || !Number.isFinite(v)) continue;
    const [min, max] = NUMBER_RANGES[k];
    out[k] = Math.min(max, Math.max(min, v));
  }
  if (Array.isArray(parsed.groups)) {
    out.groups = parsed.groups.filter(isGroup).map(g => ({ query: g.query, color: g.color }));
  }
  return out;
}

function flushPending(storage: StorageLike | null, key: string): void {
  if (!storage) return;
  const pk = pendingKey(storage, key);
  const pending = pendingByKey.get(pk);
  if (!pending) return;
  if (pending.timer != null) {
    clearTimeout(pending.timer);
    pending.timer = null;
  }
  writeGraphSettingsNow(pending.settings, pending.storage, pending.key);
  pendingByKey.delete(pk);
}

/** Persist any debounced graph settings for this storage key (e.g. before navigation). */
export function flushGraphSettings(
  storage: StorageLike | null = defaultStorage(),
  key: string = GRAPH_SETTINGS_KEY,
): void {
  flushPending(storage, key);
}

function flushAllPendingGraphSettings(): void {
  for (const pending of [...pendingByKey.values()]) {
    flushPending(pending.storage, pending.key);
  }
}

if (typeof window !== 'undefined') {
  window.addEventListener('pagehide', () => flushAllPendingGraphSettings());
}

export function loadGraphSettings(
  storage: StorageLike | null = defaultStorage(),
  key: string = GRAPH_SETTINGS_KEY,
): GraphSettings {
  if (storage) flushPending(storage, key);
  try {
    const raw = storage?.getItem(key);
    if (!raw) return { ...DEFAULT_GRAPH_SETTINGS, groups: [] };
    const parsed: unknown = JSON.parse(raw);
    if (!parsed || typeof parsed !== 'object' || Array.isArray(parsed)) {
      return { ...DEFAULT_GRAPH_SETTINGS, groups: [] };
    }
    return sanitizeGraphSettings(parsed as Record<string, unknown>);
  } catch {
    return { ...DEFAULT_GRAPH_SETTINGS, groups: [] };
  }
}

export function saveGraphSettings(
  settings: GraphSettings,
  storage: StorageLike | null = defaultStorage(),
  key: string = GRAPH_SETTINGS_KEY,
): void {
  if (!storage) return;

  const pk = pendingKey(storage, key);
  let pending = pendingByKey.get(pk);
  if (!pending) {
    pending = { timer: null, settings, storage, key };
    pendingByKey.set(pk, pending);
  } else {
    pending.settings = settings;
    pending.storage = storage;
    pending.key = key;
  }

  if (typeof setTimeout === 'undefined') {
    writeGraphSettingsNow(settings, storage, key);
    pendingByKey.delete(pk);
    return;
  }

  if (pending.timer != null) clearTimeout(pending.timer);
  pending.timer = setTimeout(() => {
    flushPending(storage, key);
  }, GRAPH_SETTINGS_SAVE_MS);
}