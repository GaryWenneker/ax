import { useCallback, useState } from 'react';
import { loadJson, loadNumber, loadString, saveJson, saveNumber, saveString } from '../lib/uiStorage';

export function usePersistedString(key: string, defaultValue = '') {
  const [value, setValue] = useState(() => loadString(key, defaultValue));
  const set = useCallback(
    (next: string) => {
      setValue(next);
      saveString(key, next);
    },
    [key],
  );
  return [value, set] as const;
}

export function usePersistedNumber(key: string, defaultValue: number, min: number, max: number) {
  const [value, setValue] = useState(() => loadNumber(key, defaultValue, min, max));
  const set = useCallback(
    (next: number) => {
      const clamped = Math.min(max, Math.max(min, next));
      setValue(clamped);
      saveNumber(key, clamped);
    },
    [key, min, max],
  );
  return [value, set] as const;
}

export function usePersistedJson<T>(key: string, defaultValue: T) {
  const [value, setValue] = useState(() => loadJson(key, defaultValue));
  const set = useCallback(
    (next: T) => {
      setValue(next);
      saveJson(key, next);
    },
    [key],
  );
  return [value, set] as const;
}
