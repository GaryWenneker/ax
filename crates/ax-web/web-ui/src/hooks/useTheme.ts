import { useEffect, useState } from 'react';
import { loadThemeId, themeById, THEME_CHANGED } from '../lib/themes';

/** Keep third-party controls in sync when the user switches a preset. */
export function useTheme() {
  const [id, setId] = useState(loadThemeId);
  useEffect(() => {
    const update = (event: Event) => setId((event as CustomEvent<{ id: string }>).detail.id);
    window.addEventListener(THEME_CHANGED, update);
    return () => window.removeEventListener(THEME_CHANGED, update);
  }, []);
  return themeById(id);
}
