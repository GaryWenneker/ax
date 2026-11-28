import assert from 'node:assert/strict';
import { describe, it } from 'node:test';
import {
  THEMES,
  ACCENT_COLORS,
  accentForTheme,
  contrastRatioHex,
  ensureTextContrast,
  ensureWhiteOnFill,
  initTheme,
  loadThemeId,
  statusbarInk,
  themeAccentFill,
  themeById,
} from './themes.ts';

function fakeBrowser(initial: Record<string, string>) {
  const store = new Map(Object.entries(initial));
  const writes: Array<[string, string]> = [];
  const props = new Map<string, string>();
  const g = globalThis as Record<string, unknown>;
  const saved = { localStorage: g.localStorage, document: g.document, window: g.window, CustomEvent: g.CustomEvent };
  g.localStorage = {
    getItem: (key: string) => store.get(key) ?? null,
    setItem: (key: string, value: string) => { writes.push([key, value]); store.set(key, value); },
  };
  g.document = { documentElement: { dataset: {}, style: { setProperty: (name: string, value: string) => props.set(name, value) } } };
  g.window = { dispatchEvent: () => true };
  g.CustomEvent = class { type: string; constructor(type: string) { this.type = type; } };
  const restore = () => { for (const [key, value] of Object.entries(saved)) { if (value === undefined) delete g[key]; else g[key] = value; } };
  return { store, writes, props, restore };
}

describe('stored theme id', () => {
  it('reading the theme id does not write storage', () => {
    const env = fakeBrowser({ 'ax-theme': 'light' });
    try {
      assert.equal(loadThemeId(), 'azure-light');
      assert.equal(loadThemeId(), 'azure-light');
      assert.deepEqual(env.writes, []);
    } finally { env.restore(); }
  });
  it('startup stores the migrated id once', () => {
    const env = fakeBrowser({ 'ax-theme': 'macos' });
    try {
      assert.equal(initTheme(), 'azure-dark');
      assert.deepEqual(env.writes, [['ax-theme', 'azure-dark']]);
      env.writes.length = 0;
      initTheme();
      assert.deepEqual(env.writes, []);
    } finally { env.restore(); }
  });
});

describe('available themes', () => {
  it('keeps only Light and Dark', () => { assert.deepEqual(THEMES.map(t => [t.id,t.label]), [['azure-light','Light'],['azure-dark','Dark']]); });
  it('migrates removed themes to Dark and preserves Light', () => { assert.equal(themeById('macos').id,'azure-dark'); assert.equal(themeById('unknown').id,'azure-dark'); assert.equal(themeById('azure-light').id,'azure-light'); });
});

describe('WCAG AA contrast', () => {
  it('W1 system blue on macOS bg fails as body text (the bug)', () => {
    assert.ok(contrastRatioHex('#0a84ff', '#1c1c1e') < 7, 'expected saturated blue to be a weak text color');
  });

  it('W2 ensureTextContrast lifts blue on dark to AA 4.5:1', () => {
    const fg = ensureTextContrast('#0a84ff', '#1c1c1e');
    assert.ok(contrastRatioHex(fg, '#1c1c1e') >= 4.5, `${fg} vs #1c1c1e`);
  });

  it('W3 white labels on darkened fill meet AA', () => {
    const fill = ensureWhiteOnFill('#0a84ff');
    assert.ok(contrastRatioHex('#ffffff', fill) >= 4.5, `white vs ${fill}`);
  });

  it('W4 every theme accentText vs bg is AA', () => {
    for (const t of THEMES) {
      const fg = ensureTextContrast(t.accent, t.bg);
      const r = contrastRatioHex(fg, t.bg);
      assert.ok(r >= 4.5, `${t.id} ${fg} on ${t.bg} ratio ${r.toFixed(2)}`);
    }
  });

  it('dim text in both themes meets AA', () => { for (const theme of THEMES) assert.ok(contrastRatioHex(theme.textDim,theme.bg)>=4.5); });
  it('W7 ensureTextContrast default is AAA', () => {
    const fg = ensureTextContrast('#0a84ff', '#1c1c1e');
    const r = contrastRatioHex(fg, '#1c1c1e');
    assert.ok(r >= 7, `${fg} vs #1c1c1e ratio ${r.toFixed(2)}`);
  });

  it('W8 statusbar ink vs painted accent meets AA 4.5:1 (macOS footer bug)', () => {
    for (const t of THEMES) {
      const painted = themeAccentFill(t);
      const ink = statusbarInk(painted);
      const r = contrastRatioHex(ink.fg, painted);
      assert.ok(r >= 4.5, `${t.id} statusbar ${ink.fg} on painted ${painted} ratio ${r.toFixed(2)}`);
    }
  });

  it('uses dark status ink on a light fill', () => { const ink=statusbarInk('#64d2ff');assert.equal(ink.onLight,true);assert.ok(contrastRatioHex(ink.fg,'#64d2ff')>=4.5); });
  it('keeps the status bar surface neutral when accent changes', () => {
    const env = fakeBrowser({ 'ax-theme': 'azure-light' });
    try {
      for (const accent of ACCENT_COLORS) {
        env.store.set('ax-accent', accent.id);
        initTheme();
        assert.equal(env.props.get('--statusbar-bg'), THEMES[0].bgSide);
        assert.equal(env.props.get('--statusbar-fg'), statusbarInk(THEMES[0].bgSide).fg);
      }
    } finally { env.restore(); }
  });
  it('offers distinct readable Light and Dark shades for every accent', () => {
    assert.equal(accentForTheme(THEMES[1],'lime'),'#b9ff6a');
    for (const theme of THEMES) for (const color of ACCENT_COLORS) {
      assert.notEqual(color.light,color.dark);
      const accent = accentForTheme(theme,color.id);
      const text = [theme.bg,theme.bgPanel,theme.bgInput,theme.bgHover].reduce((fg,bg)=>ensureTextContrast(fg,bg),accent);
      for (const bg of [theme.bg,theme.bgPanel,theme.bgInput,theme.bgHover]) assert.ok(contrastRatioHex(text,bg)>=7,`${theme.id}/${color.id} text`);
      const fill = themeAccentFill({...theme,accent});
      assert.ok(contrastRatioHex(statusbarInk(fill).fg,fill)>=4.5,`${theme.id}/${color.id} button`);
    }
  });
});
