export interface ThemePreset {
  id: string;
  label: string;
  accent: string;
  ok: string;
  danger: string;
  warn: string;
  bg: string;
  bgSide: string;
  bgInput: string;
  bgHover: string;
  bgActive: string;
  bgPanel: string;
  border: string;
  borderHi: string;
  text: string;
  textDim: string;
  textHi: string;
  statusbarBg: string;
}

export const THEMES: ThemePreset[] = [
  {
    id: 'vscode-dark',
    label: 'VS Code Dark Modern',
    accent: '#0078d4',
    ok: '#3fb950',
    danger: '#f14c4c',
    warn: '#cca700',
    bg: '#1f1f1f',
    bgSide: '#181818',
    bgInput: '#313131',
    bgHover: '#2a2d2e',
    bgActive: '#37373d',
    bgPanel: '#1f1f1f',
    border: '#2b2b2b',
    borderHi: '#454545',
    text: '#cccccc',
    textDim: '#9d9d9d',
    textHi: '#ffffff',
    statusbarBg: '#181818',
  },
  {
    id: 'ember',
    label: 'Ember',
    accent: '#e06c2b',
    ok: '#3fb950',
    danger: '#f14c4c',
    warn: '#e0a030',
    bg: '#1a1a1a',
    bgSide: '#141414',
    bgInput: '#2a2a2a',
    bgHover: '#2c2420',
    bgActive: '#3a2e26',
    bgPanel: '#1a1a1a',
    border: '#2a2420',
    borderHi: '#4a3a30',
    text: '#cccccc',
    textDim: '#9d9d9d',
    textHi: '#ffffff',
    statusbarBg: '#141414',
  },
  {
    id: 'emerald',
    label: 'Emerald',
    accent: '#2ea87a',
    ok: '#3fb950',
    danger: '#f14c4c',
    warn: '#cca700',
    bg: '#1a1c1a',
    bgSide: '#141614',
    bgInput: '#262e28',
    bgHover: '#222e26',
    bgActive: '#2a3a2e',
    bgPanel: '#1a1c1a',
    border: '#222e24',
    borderHi: '#3a4a3e',
    text: '#cccccc',
    textDim: '#9d9d9d',
    textHi: '#ffffff',
    statusbarBg: '#141614',
  },
  {
    id: 'nightfall',
    label: 'Nightfall',
    accent: '#8b5cf6',
    ok: '#3fb950',
    danger: '#f14c4c',
    warn: '#cca700',
    bg: '#1a1a22',
    bgSide: '#14141c',
    bgInput: '#282838',
    bgHover: '#252535',
    bgActive: '#30304a',
    bgPanel: '#1a1a22',
    border: '#252530',
    borderHi: '#3a3a50',
    text: '#cccccc',
    textDim: '#9d9d9d',
    textHi: '#ffffff',
    statusbarBg: '#14141c',
  },
  {
    id: 'crimson',
    label: 'Crimson',
    accent: '#dc3545',
    ok: '#3fb950',
    danger: '#f14c4c',
    warn: '#cca700',
    bg: '#1c1a1a',
    bgSide: '#161414',
    bgInput: '#302828',
    bgHover: '#2e2424',
    bgActive: '#3e2e2e',
    bgPanel: '#1c1a1a',
    border: '#2c2222',
    borderHi: '#4a3535',
    text: '#cccccc',
    textDim: '#9d9d9d',
    textHi: '#ffffff',
    statusbarBg: '#161414',
  },
  {
    id: 'ocean',
    label: 'Ocean',
    accent: '#22a2c8',
    ok: '#3fb950',
    danger: '#f14c4c',
    warn: '#cca700',
    bg: '#1a1c1e',
    bgSide: '#141618',
    bgInput: '#262e34',
    bgHover: '#222e34',
    bgActive: '#2a3842',
    bgPanel: '#1a1c1e',
    border: '#222830',
    borderHi: '#364450',
    text: '#cccccc',
    textDim: '#9d9d9d',
    textHi: '#ffffff',
    statusbarBg: '#141618',
  },
];

const STORAGE_KEY = 'ax-theme';

export function loadThemeId(): string {
  try {
    return localStorage.getItem(STORAGE_KEY) ?? 'vscode-dark';
  } catch {
    return 'vscode-dark';
  }
}

export function saveThemeId(id: string): void {
  try {
    localStorage.setItem(STORAGE_KEY, id);
  } catch {}
}

export function themeById(id: string): ThemePreset {
  return THEMES.find((t) => t.id === id) ?? THEMES[0];
}

export function applyTheme(theme: ThemePreset): void {
  const root = document.documentElement;
  root.style.setProperty('--accent', theme.accent);
  root.style.setProperty('--ok', theme.ok);
  root.style.setProperty('--danger', theme.danger);
  root.style.setProperty('--warn', theme.warn);
  root.style.setProperty('--bg', theme.bg);
  root.style.setProperty('--bg-side', theme.bgSide);
  root.style.setProperty('--bg-input', theme.bgInput);
  root.style.setProperty('--bg-hover', theme.bgHover);
  root.style.setProperty('--bg-active', theme.bgActive);
  root.style.setProperty('--bg-panel', theme.bgPanel);
  root.style.setProperty('--border', theme.border);
  root.style.setProperty('--border-hi', theme.borderHi);
  root.style.setProperty('--text', theme.text);
  root.style.setProperty('--text-dim', theme.textDim);
  root.style.setProperty('--text-hi', theme.textHi);
  root.style.setProperty('--statusbar-bg', theme.statusbarBg);
}

export function initTheme(): string {
  const id = loadThemeId();
  applyTheme(themeById(id));
  return id;
}
