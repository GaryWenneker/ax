const PREFIX = 'ax-web-';

export function loadString(key: string, fallback = ''): string {
  try {
    return localStorage.getItem(PREFIX + key) ?? fallback;
  } catch {
    return fallback;
  }
}

export function saveString(key: string, value: string) {
  try {
    localStorage.setItem(PREFIX + key, value);
  } catch {
    /* quota */
  }
}

export function loadJson<T>(key: string, fallback: T): T {
  try {
    const raw = localStorage.getItem(PREFIX + key);
    if (raw) return JSON.parse(raw) as T;
  } catch {
    /* corrupt */
  }
  return fallback;
}

export function saveJson(key: string, value: unknown) {
  try {
    localStorage.setItem(PREFIX + key, JSON.stringify(value));
  } catch {
    /* quota */
  }
}

export function loadNumber(key: string, fallback: number, min: number, max: number): number {
  const raw = loadString(key, '');
  const n = raw ? parseFloat(raw) : fallback;
  if (Number.isNaN(n)) return fallback;
  return Math.min(max, Math.max(min, n));
}

export function saveNumber(key: string, value: number) {
  saveString(key, String(value));
}
