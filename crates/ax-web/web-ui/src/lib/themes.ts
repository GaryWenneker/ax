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
  layout?: 'portal';
  colorScheme?: 'light' | 'dark';
}

export const THEMES: ThemePreset[] = [
  {
    id: 'azure-light', label: 'Light', layout: 'portal', colorScheme: 'light',
    accent: '#386900', ok: '#107c10', danger: '#a4262c', warn: '#865d00',
    bg: '#f5f5f5', bgSide: '#fafafa', bgInput: '#ffffff', bgHover: '#eeeeee',
    bgActive: '#deecf9', bgPanel: '#ffffff', border: '#d6d6d6', borderHi: '#8a8886',
    text: '#323130', textDim: '#605e5c', textHi: '#201f1e', statusbarBg: '#0078d4',
  },
  {
    id: 'azure-dark', label: 'Dark', layout: 'portal', colorScheme: 'dark',
    accent: '#b9ff6a', ok: '#6ccb5f', danger: '#ff8585', warn: '#f2c661',
    bg: '#1b1b1b', bgSide: '#202020', bgInput: '#252525', bgHover: '#333333',
    bgActive: '#163b59', bgPanel: '#252525', border: '#444444', borderHi: '#797775',
    text: '#f0f0f0', textDim: '#bdbdbd', textHi: '#ffffff', statusbarBg: '#2899f5',
  },
];

export const ACCENT_COLORS = [
  {id:'lime',label:'Lime',light:'#386900',dark:'#b9ff6a'},
  {id:'blue',label:'Blue',light:'#0060a8',dark:'#71b7ff'},
  {id:'teal',label:'Teal',light:'#00665f',dark:'#63ddd1'},
  {id:'violet',label:'Violet',light:'#6f329c',dark:'#cba8ff'},
  {id:'orange',label:'Orange',light:'#924300',dark:'#ffc178'},
  {id:'rose',label:'Rose',light:'#a42658',dark:'#ff9dc3'},
];

export function loadAccentId(): string {
  try { const id = localStorage.getItem('ax-accent'); return ACCENT_COLORS.some(color => color.id === id) ? id! : 'lime'; }
  catch { return 'lime'; }
}
export function accentForTheme(theme: ThemePreset, id = loadAccentId()): string {
  const color = ACCENT_COLORS.find(color => color.id === id) ?? ACCENT_COLORS[0];
  return theme.colorScheme === 'light' ? color.light : color.dark;
}
export function selectAccent(id: string, theme = themeById(loadThemeId())): void {
  if (!ACCENT_COLORS.some(color => color.id === id)) return;
  try { localStorage.setItem('ax-accent',id); } catch {}
  applyTheme(theme, id);
}

const STORAGE_KEY = 'ax-theme';
function normalizeThemeId(id: string | null): string {
  return id === 'azure-light' || id === 'light' ? 'azure-light' : 'azure-dark';
}
export function loadThemeId(): string {
  try { const id = normalizeThemeId(localStorage.getItem(STORAGE_KEY)); localStorage.setItem(STORAGE_KEY,id); return id; }
  catch { return 'azure-dark'; }
}
export function saveThemeId(id: string): void {
  try { localStorage.setItem(STORAGE_KEY,normalizeThemeId(id)); } catch {}
}
export function themeById(id: string): ThemePreset {
  return THEMES.find((theme) => theme.id === normalizeThemeId(id)) ?? THEMES[1];
}

/** sRGB relative luminance (0 = black, 1 = white). */
function relativeLuminance(hex: string): number {
  const raw = hex.replace('#', '').trim();
  if (raw.length !== 6) return 0.5;
  const toLin = (n: number) => {
    const c = n / 255;
    return c <= 0.03928 ? c / 12.92 : ((c + 0.055) / 1.055) ** 2.4;
  };
  const r = toLin(parseInt(raw.slice(0, 2), 16));
  const g = toLin(parseInt(raw.slice(2, 4), 16));
  const b = toLin(parseInt(raw.slice(4, 6), 16));
  return 0.2126 * r + 0.7152 * g + 0.0722 * b;
}

function contrastRatio(l1: number, l2: number): number {
  const a = Math.max(l1, l2);
  const b = Math.min(l1, l2);
  return (a + 0.05) / (b + 0.05);
}

function parseHexRgb(hex: string): [number, number, number] {
  const raw = hex.replace('#', '').trim();
  return [parseInt(raw.slice(0, 2), 16), parseInt(raw.slice(2, 4), 16), parseInt(raw.slice(4, 6), 16)];
}

function toHex(n: number): string {
  return Math.max(0, Math.min(255, Math.round(n)))
    .toString(16)
    .padStart(2, '0');
}

function mixHex(a: string, b: string, t: number): string {
  const [ar, ag, ab] = parseHexRgb(a);
  const [br, bg, bb] = parseHexRgb(b);
  return `#${toHex(ar + (br - ar) * t)}${toHex(ag + (bg - ag) * t)}${toHex(ab + (bb - ab) * t)}`;
}

/** WCAG 2 contrast ratio of two sRGB hex colors. */
export function contrastRatioHex(fg: string, bg: string): number {
  return contrastRatio(relativeLuminance(fg), relativeLuminance(bg));
}

const WCAG_AA = 4.5;
const WCAG_AAA = 7;

/** Mix `fg` toward white or black until contrast vs `bg` meets WCAG AAA (7:1) by default. */
export function ensureTextContrast(fg: string, bg: string, min = WCAG_AAA): string {
  if (contrastRatioHex(fg, bg) >= min) return fg.toLowerCase();
  const toward = relativeLuminance(bg) < 0.5 ? '#ffffff' : '#000000';
  let t = 0;
  for (let i = 0; i < 32; i++) {
    t = Math.min(1, t + 0.06);
    const c = mixHex(fg, toward, t);
    if (contrastRatioHex(c, bg) >= min) return c;
  }
  return toward;
}

/** Darken a fill until white label text meets WCAG AA. */
export function ensureWhiteOnFill(fill: string, min = WCAG_AA): string {
  if (contrastRatioHex('#ffffff', fill) >= min) return fill.toLowerCase();
  let t = 0;
  for (let i = 0; i < 32; i++) {
    t = Math.min(1, t + 0.06);
    const c = mixHex(fill, '#000000', t);
    if (contrastRatioHex('#ffffff', c) >= min) return c;
  }
  return '#000000';
}

export function themeAccentFill(theme: ThemePreset): string {
  let fill = theme.accent;
  if (relativeLuminance(theme.bg) < 0.5 && contrastRatioHex(fill, theme.bg) < WCAG_AAA) {
    fill = ensureTextContrast(fill, theme.bg);
  }
  return fill;
}

/**
 * Pick status-bar foreground for the painted accent fill (the footer uses `--accent`).
 * Chooses dark vs light ink by measured contrast, then lifts to WCAG AA 4.5:1.
 */
export function statusbarInk(accentHex: string): {
  fg: string;
  muted: string;
  sep: string;
  hoverBg: string;
  onLight: boolean;
} {
  const dark = '#0d1412';
  const light = '#f3f3f3';
  const preferDark = contrastRatioHex(dark, accentHex) >= contrastRatioHex(light, accentHex);
  let fg = preferDark ? dark : light;
  if (contrastRatioHex(fg, accentHex) < WCAG_AA) {
    fg = ensureTextContrast(fg, accentHex, WCAG_AA);
  }
  if (preferDark) {
    return {
      fg,
      muted: 'color-mix(in srgb, #0d1412 88%, transparent)',
      sep: 'color-mix(in srgb, #0d1412 35%, transparent)',
      hoverBg: 'rgba(0, 0, 0, 0.1)',
      onLight: true,
    };
  }
  return {
    fg,
    muted: 'color-mix(in srgb, #ffffff 88%, transparent)',
    sep: 'color-mix(in srgb, #ffffff 35%, transparent)',
    hoverBg: 'rgba(255, 255, 255, 0.12)',
    onLight: false,
  };
}

export function applyTheme(theme: ThemePreset, accentId = loadAccentId()): void {
  theme = {...theme,accent:accentForTheme(theme, accentId)};
  const root = document.documentElement;
  root.dataset.axLayout = theme.layout ?? 'workbench';
  root.style.colorScheme = theme.colorScheme ?? 'dark';
  const accentText = [theme.bg, theme.bgPanel, theme.bgInput, theme.bgHover].reduce((color, bg) => ensureTextContrast(color, bg), theme.accent);
  const fill = themeAccentFill(theme);
  const onFill = statusbarInk(fill).fg;
  root.style.setProperty('--accent', fill);
  root.style.setProperty('--accent-text', accentText);
  root.style.setProperty('--accent-on-fill', onFill);
  root.style.setProperty('--ok', theme.ok);
  root.style.setProperty('--danger', theme.danger);
  root.style.setProperty('--warn', theme.warn);
  root.style.setProperty('--bg', theme.bg);
  root.style.setProperty('--bg-side', theme.bgSide);
  root.style.setProperty('--bg-input', theme.bgInput);
  root.style.setProperty('--bg-hover', theme.bgHover);
  root.style.setProperty('--bg-active', `color-mix(in srgb, ${theme.accent} 14%, ${theme.bgPanel})`);
  root.style.setProperty('--bg-panel', theme.bgPanel);
  root.style.setProperty('--border', theme.border);
  root.style.setProperty('--border-hi', theme.borderHi);
  root.style.setProperty('--text', theme.text);
  root.style.setProperty('--text-dim', theme.textDim);
  root.style.setProperty('--control-chevron', `url("data:image/svg+xml,${encodeURIComponent(`<svg xmlns="http://www.w3.org/2000/svg" width="16" height="16" viewBox="0 0 16 16" fill="none"><path d="m4 6 4 4 4-4" stroke="${accentText}" stroke-width="1.5" stroke-linecap="round" stroke-linejoin="round"/></svg>`)}")`);
  root.style.setProperty('--text-hi', theme.textHi);
  root.style.setProperty('--statusbar-bg', theme.bgSide);
  const ink = statusbarInk(theme.bgSide);
  root.style.setProperty('--statusbar-fg', ink.fg);
  root.style.setProperty('--statusbar-fg-muted', ink.muted);
  root.style.setProperty('--statusbar-fg-sep', ink.sep);
  root.style.setProperty('--statusbar-hover-bg', ink.hoverBg);
  root.dataset.statusbarInk = ink.onLight ? 'dark' : 'light';
  root.style.setProperty('--ax-project-accent', theme.accent);
  root.style.setProperty(
    '--ax-project-bg',
    `color-mix(in srgb, ${theme.accent} 10%, transparent)`,
  );
  root.style.setProperty(
    '--ax-project-border',
    `color-mix(in srgb, ${theme.accent} 45%, transparent)`,
  );
  root.style.setProperty(
    '--ax-project-glow',
    `color-mix(in srgb, ${theme.accent} 20%, transparent)`,
  );
  root.dataset.axTheme = theme.id;
  window.dispatchEvent(new CustomEvent(THEME_CHANGED, { detail: { id: theme.id, accent: theme.accent } }));
}

export const THEME_CHANGED = 'ax-theme-changed';

export function initTheme(): string {
  const id = loadThemeId();
  applyTheme(themeById(id));
  return id;
}

export function currentThemeAccent(): string {
  return accentForTheme(themeById(loadThemeId()));
}
