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

export function loadGraphSettings(
  storage: StorageLike | null = defaultStorage(),
  key: string = GRAPH_SETTINGS_KEY,
): GraphSettings {
  const raw = storage?.getItem(key);
  if (!raw) return { ...DEFAULT_GRAPH_SETTINGS };
  try {
    const parsed: unknown = JSON.parse(raw);
    if (!parsed || typeof parsed !== 'object') return { ...DEFAULT_GRAPH_SETTINGS };
    return { ...DEFAULT_GRAPH_SETTINGS, ...(parsed as Partial<GraphSettings>) };
  } catch {
    return { ...DEFAULT_GRAPH_SETTINGS };
  }
}

export function saveGraphSettings(
  settings: GraphSettings,
  storage: StorageLike | null = defaultStorage(),
  key: string = GRAPH_SETTINGS_KEY,
): void {
  storage?.setItem(key, JSON.stringify(settings));
}
