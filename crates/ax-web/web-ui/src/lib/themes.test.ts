import assert from 'node:assert/strict';
import { describe, it } from 'node:test';
import {
  THEMES,
  contrastRatioHex,
  ensureTextContrast,
  ensureWhiteOnFill,
  themeById,
} from './themes.ts';

describe('macOS theme preset', () => {
  it('M1 THEMES includes macos with light system blue', () => {
    const macos = THEMES.find((t) => t.id === 'macos');
    assert.ok(macos, 'missing macos preset');
    assert.equal(macos.label, 'macOS');
    assert.equal(macos.accent, '#64d2ff');
  });

  it('M2 themeById resolves macos and unknown falls back to ax Mint', () => {
    assert.equal(themeById('macos').id, 'macos');
    assert.equal(themeById('not-a-theme').id, 'ax');
  });

  it('M3 existing presets stay registered', () => {
    const ids = THEMES.map((t) => t.id);
    for (const id of ['ax', 'vscode-dark', 'ember', 'emerald', 'nightfall', 'crimson', 'ocean', 'macos']) {
      assert.ok(ids.includes(id), `missing ${id}`);
    }
  });
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

  it('W5 macos dim text vs bg is AA', () => {
    const macos = THEMES.find((t) => t.id === 'macos')!;
    assert.ok(contrastRatioHex(macos.textDim, macos.bg) >= 4.5);
  });

  it('W6 macos accent vs charcoal is AAA 7:1', () => {
    const macos = THEMES.find((t) => t.id === 'macos')!;
    const r = contrastRatioHex(macos.accent, macos.bg);
    assert.ok(r >= 7, `${macos.accent} on ${macos.bg} ratio ${r.toFixed(2)}`);
  });

  it('W7 ensureTextContrast default is AAA', () => {
    const fg = ensureTextContrast('#0a84ff', '#1c1c1e');
    const r = contrastRatioHex(fg, '#1c1c1e');
    assert.ok(r >= 7, `${fg} vs #1c1c1e ratio ${r.toFixed(2)}`);
  });
});
