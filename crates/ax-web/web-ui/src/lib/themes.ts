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
    id: 'ax',
    label: 'ax Mint',
    accent: '#3ee4b2',
    ok: '#3fb950',
    danger: '#f14c4c',
    warn: '#cca700',
    bg: '#1e1e1e',
    bgSide: '#181818',
    bgInput: '#313131',
    bgHover: '#252826',
    bgActive: '#2e3532',
    bgPanel: '#1e1e1e',
    border: '#2b2b2b',
    borderHi: '#454545',
    text: '#cccccc',
    textDim: '#9d9d9d',
    textHi: '#ffffff',
    statusbarBg: '#3ee4b2',
  },
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
    statusbarBg: '#0078d4',
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
    statusbarBg: '#e06c2b',
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
    statusbarBg: '#2ea87a',
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
    statusbarBg: '#8b5cf6',
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
    statusbarBg: '#dc3545',
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
    statusbarBg: '#22a2c8',
  },
  {
    id: 'macos',
    label: 'macOS',
    accent: '#64d2ff',
    ok: '#30d158',
    danger: '#ff453a',
    warn: '#ffd60a',
    bg: '#1c1c1e',
    bgSide: '#161618',
    bgInput: '#2c2c2e',
    bgHover: '#2c2c2e',
    bgActive: '#3a3a3c',
    bgPanel: '#1c1c1e',
    border: '#38383a',
    borderHi: '#48484a',
    text: '#f5f5f7',
    textDim: '#c7c7cc',
    textHi: '#ffffff',
    statusbarBg: '#2c2c2e',
  },
];

const STORAGE_KEY = 'ax-theme';
const MIGRATE_KEY = 'ax-theme-mint-v2';

/** Force Open-project mint as the product look (re-migrate legacy blues). */
function migrateLegacyDefaultTheme(): void {
  try {
    if (localStorage.getItem(MIGRATE_KEY) === '1') return;
    const current = localStorage.getItem(STORAGE_KEY);
    // Pull anyone still on the old default blue onto ax Mint.
    if (current == null || current === 'vscode-dark') {
      localStorage.setItem(STORAGE_KEY, 'ax');
    }
    localStorage.setItem(MIGRATE_KEY, '1');
  } catch {
    /* ignore */
  }
}

export function loadThemeId(): string {
  try {
    migrateLegacyDefaultTheme();
    return localStorage.getItem(STORAGE_KEY) ?? 'ax';
  } catch {
    return 'ax';
  }
}

export function saveThemeId(id: string): void {
  try {
    localStorage.setItem(STORAGE_KEY, id);
    localStorage.setItem(MIGRATE_KEY, '1');
  } catch {}
}

export function themeById(id: string): ThemePreset {
  return THEMES.find((t) => t.id === id) ?? THEMES[0];
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

export function applyTheme(theme: ThemePreset): void {
  const root = document.documentElement;
  const accentText = ensureTextContrast(theme.accent, theme.bg);
  const fill = themeAccentFill(theme);
  const onFill = ensureTextContrast('#ffffff', fill, WCAG_AA);
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
  root.style.setProperty('--bg-active', theme.bgActive);
  root.style.setProperty('--bg-panel', theme.bgPanel);
  root.style.setProperty('--border', theme.border);
  root.style.setProperty('--border-hi', theme.borderHi);
  root.style.setProperty('--text', theme.text);
  root.style.setProperty('--text-dim', theme.textDim);
  root.style.setProperty('--text-hi', theme.textHi);
  root.style.setProperty('--statusbar-bg', fill);
  const ink = statusbarInk(fill);
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
  return themeById(loadThemeId()).accent;
}
