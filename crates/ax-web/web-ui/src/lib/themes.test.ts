import assert from 'node:assert/strict';
import { describe, it } from 'node:test';
import {
  THEMES,
  ACCENT_COLORS,
  accentForTheme,
  contrastRatioHex,
  ensureTextContrast,
  ensureWhiteOnFill,
  statusbarInk,
  themeAccentFill,
  themeById,
} from './themes.ts';

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
  it('keeps the status bar surface neutral when accent changes', async () => {
    const { readFile } = await import('node:fs/promises');
    const src = await readFile(new URL('./themes.ts', import.meta.url), 'utf8');
    assert.match(src, /statusbarInk\(theme.bgSide\)/);
    assert.equal(src.includes('statusbarInk(theme.statusbarBg'), false);
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
