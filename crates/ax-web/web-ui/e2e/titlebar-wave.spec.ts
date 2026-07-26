import { expect, test, type Page } from '@playwright/test';
import path from 'node:path';
import { fileURLToPath } from 'node:url';
import fs from 'node:fs';

const __dirname = path.dirname(fileURLToPath(import.meta.url));
const SHOT_DIR = path.join(__dirname, '..', 'test-results', 'wave-shots');

async function waitForApiOk(page: Page) {
  const res = await page.request.get('/api/stats');
  expect(res.ok(), `GET /api/stats → ${res.status()}`).toBeTruthy();
}

type Boxes = {
  titlebar: { top: number; bottom: number; height: number };
  waves: { top: number; bottom: number; height: number; visible: boolean };
  content: { top: number; bottom: number };
  pluginsText: { top: number; bottom: number } | null;
  waveHVar: string;
  titlebarOverflow: string;
  titlebarBg: string;
  titlebarInnerBg: string;
  titlebarZ: string;
  workspaceZ: string;
  viewportWidth: number;
};

async function measure(page: Page): Promise<Boxes> {
  return page.evaluate(() => {
    const titlebar = document.querySelector('.titlebar') as HTMLElement | null;
    const titlebarInner = document.querySelector('.titlebar-inner') as HTMLElement | null;
    const waves = document.querySelector('.cc-waves') as HTMLElement | null;
    const content = document.querySelector('#main-content') as HTMLElement | null;
    const workspace = document.querySelector('.workspace') as HTMLElement | null;
    const plugins =
      Array.from(document.querySelectorAll('p')).find((p) =>
        (p.textContent || '').includes('Process / WASM extractors'),
      ) ?? null;

    const box = (el: Element | null) => {
      if (!el) return { top: 0, bottom: 0, height: 0 };
      const r = el.getBoundingClientRect();
      return { top: r.top, bottom: r.bottom, height: r.height };
    };

    const tb = box(titlebar);
    const wv = box(waves);
    const ct = box(content);
    const pl = plugins ? box(plugins) : null;
    const cs = getComputedStyle(document.documentElement);
    const tbStyle = titlebar ? getComputedStyle(titlebar) : null;
    const tiStyle = titlebarInner ? getComputedStyle(titlebarInner) : null;
    return {
      titlebar: tb,
      waves: {
        ...wv,
        visible: !!waves && getComputedStyle(waves).display !== 'none',
      },
      content: { top: ct.top, bottom: ct.bottom },
      pluginsText: pl ? { top: pl.top, bottom: pl.bottom } : null,
      waveHVar: cs.getPropertyValue('--titlebar-wave-h').trim(),
      titlebarOverflow: tbStyle?.overflow ?? '',
      titlebarBg: tbStyle?.backgroundColor ?? '',
      titlebarInnerBg: tiStyle?.backgroundColor ?? '',
      titlebarZ: tbStyle?.zIndex ?? '',
      workspaceZ: workspace ? getComputedStyle(workspace).zIndex : '',
      viewportWidth: window.innerWidth,
    };
  });
}

function countMintStreaks(
  data: Uint8ClampedArray,
  w: number,
  h: number,
): { streaks: number; hotRows: number[] } {
  const isMint = (r: number, g: number, b: number, a: number) =>
    a > 80 && g > 140 && g > r + 30 && g > b - 20 && r > 20 && r < 140 && b > 80 && b < 220;

  const hotRows: number[] = [];
  for (let y = 0; y < h; y++) {
    let mint = 0;
    for (let x = 0; x < w; x++) {
      const i = (y * w + x) * 4;
      if (isMint(data[i], data[i + 1], data[i + 2], data[i + 3])) mint++;
    }
    // Wave hang signature: mint across a large fraction of the text width
    if (mint > w * 0.35) hotRows.push(y);
  }

  // Collapse contiguous rows into streak count
  let streaks = 0;
  for (let i = 0; i < hotRows.length; i++) {
    if (i === 0 || hotRows[i] !== hotRows[i - 1] + 1) streaks++;
  }
  return { streaks, hotRows };
}

async function assertWaveClearOfText(page: Page, tag: string) {
  await page.goto('/settings');
  await expect(page.getByText('Extractor plugins').first()).toBeVisible({ timeout: 15_000 });
  const plugins = page.getByText(/Process \/ WASM extractors/).first();
  await expect(plugins).toBeVisible();

  // Match the user bug: plugins blurb scrolled flush under the titlebar
  await plugins.evaluate((el) => {
    const scroller = el.closest('.workspace > .container') as HTMLElement | null;
    if (!scroller) {
      el.scrollIntoView({ block: 'start' });
      return;
    }
    const top =
      el.getBoundingClientRect().top - scroller.getBoundingClientRect().top + scroller.scrollTop;
    scroller.scrollTop = Math.max(0, top - 6);
  });
  await page.waitForTimeout(80);

  await page.locator('.titlebar').screenshot({
    path: path.join(SHOT_DIR, `${tag}-titlebar.png`),
  });

  // Mint must appear in the brand row (not only a thin line under an opaque bar)
  const tbShot = fs.readFileSync(path.join(SHOT_DIR, `${tag}-titlebar.png`));
  const tbPix = await page.evaluate(async (b64) => {
    const img = new Image();
    img.src = `data:image/png;base64,${b64}`;
    await img.decode();
    const c = document.createElement('canvas');
    c.width = img.naturalWidth;
    c.height = img.naturalHeight;
    const ctx = c.getContext('2d')!;
    ctx.drawImage(img, 0, 0);
    const { data, width, height } = ctx.getImageData(0, 0, c.width, c.height);
    const isMint = (r: number, g: number, b: number, a: number) =>
      a > 60 && g > 120 && g > r + 25 && r > 15 && r < 150 && b > 70 && b < 230;
    let upper = 0;
    let lower = 0;
    const mid = Math.floor(height * 0.45);
    for (let y = 0; y < height; y++) {
      for (let x = 0; x < width; x++) {
        const i = (y * width + x) * 4;
        if (!isMint(data[i], data[i + 1], data[i + 2], data[i + 3])) continue;
        if (y < mid) upper++;
        else lower++;
      }
    }
    return { upper, lower, width, height };
  }, tbShot.toString('base64'));
  fs.writeFileSync(path.join(SHOT_DIR, `${tag}-titlebar-mint.json`), JSON.stringify(tbPix, null, 2));
  expect(tbPix.upper + tbPix.lower, `${tag} titlebar should show mint wave pixels`).toBeGreaterThan(80);
  expect(tbPix.upper, `${tag} mint should reach into brand row`).toBeGreaterThan(20);
  const vw = page.viewportSize()?.width ?? 400;
  await page.screenshot({
    path: path.join(SHOT_DIR, `${tag}-top.png`),
    clip: { x: 0, y: 0, width: Math.min(vw, 900), height: 140 },
  });

  const plugBox = await plugins.boundingBox();
  expect(plugBox).toBeTruthy();
  // Crop tightly to the text line only — do not include the titlebar wave band above it
  await page.screenshot({
    path: path.join(SHOT_DIR, `${tag}-plugins-line.png`),
    clip: {
      x: Math.max(0, plugBox!.x),
      y: Math.max(0, plugBox!.y),
      width: Math.min(vw - 4, Math.ceil(plugBox!.width)),
      height: Math.max(16, Math.ceil(plugBox!.height)),
    },
  });

  // Pixel-sample the plugins line: hang bug = bright mint streaks through glyphs
  const shotPath = path.join(SHOT_DIR, `${tag}-plugins-line.png`);
  const pngBuf = fs.readFileSync(shotPath);
  const streakInfo = await page.evaluate(async (b64) => {
    const img = new Image();
    img.src = `data:image/png;base64,${b64}`;
    await img.decode();
    const c = document.createElement('canvas');
    c.width = img.naturalWidth;
    c.height = img.naturalHeight;
    const ctx = c.getContext('2d')!;
    ctx.drawImage(img, 0, 0);
    const { data, width, height } = ctx.getImageData(0, 0, c.width, c.height);
    return { data: Array.from(data), width, height };
  }, pngBuf.toString('base64'));

  const { streaks, hotRows } = countMintStreaks(
    Uint8ClampedArray.from(streakInfo.data),
    streakInfo.width,
    streakInfo.height,
  );
  fs.writeFileSync(
    path.join(SHOT_DIR, `${tag}-streaks.json`),
    JSON.stringify({ streaks, hotRows, w: streakInfo.width, h: streakInfo.height }, null, 2),
  );
  expect(streaks, `mint hang streaks through plugins text (${tag})`).toBe(0);

  const m = await measure(page);
  fs.writeFileSync(path.join(SHOT_DIR, `${tag}-measure.json`), JSON.stringify(m, null, 2));

  expect(m.waves.visible, 'cc-waves should render').toBeTruthy();
  expect(m.titlebarOverflow).toMatch(/hidden/);
  expect(Number(m.titlebarZ) || 0).toBeLessThanOrEqual(Number(m.workspaceZ) || 0);
  // Header chrome must not be solid opaque black over the waves
  const parseAlpha = (bg: string) => {
    const slash = bg.match(/\/\s*([\d.]+)\s*\)/);
    if (slash) return Number(slash[1]);
    const mrgba = bg.match(/rgba?\(([^)]+)\)/);
    if (!mrgba) return 1;
    const parts = mrgba[1].split(',').map((p) => p.trim());
    if (parts.length < 4) return 1; // rgb() → opaque
    return Number(parts[3]);
  };
  expect(parseAlpha(m.titlebarBg), `titlebar bg alpha (${m.titlebarBg})`).toBeLessThan(0.25);
  expect(parseAlpha(m.titlebarInnerBg), `titlebar-inner bg alpha (${m.titlebarInnerBg})`).toBeLessThan(
    0.15,
  );
  expect(m.waves.top).toBeGreaterThanOrEqual(m.titlebar.top - 0.5);
  expect(m.waves.bottom).toBeLessThanOrEqual(m.titlebar.bottom + 0.5);
  // Wave fills most of the titlebar (shifted slightly down from the top edge)
  expect(m.waves.height).toBeGreaterThanOrEqual(m.titlebar.height * 0.7);
  expect(m.content.top).toBeGreaterThanOrEqual(m.titlebar.bottom - 0.5);
  expect(m.pluginsText, 'plugins blurb should be on Settings').not.toBeNull();
  // Hang bug = mint through glyphs (pixel streaks). Geometry: text must sit in/below chrome.
  expect(m.pluginsText!.top).toBeGreaterThanOrEqual(m.titlebar.bottom - 2);
  return m;
}

test.describe('Titlebar mint wave must not cut page text', () => {
  test.beforeEach(async ({ page }) => {
    await waitForApiOk(page);
    fs.mkdirSync(SHOT_DIR, { recursive: true });
  });

  test('settings mobile viewport', async ({ page }) => {
    await page.setViewportSize({ width: 393, height: 851 });
    const m = await assertWaveClearOfText(page, 'mobile');
    expect(m.viewportWidth).toBeLessThanOrEqual(480);
    expect(m.waveHVar).toBe('18px');
  });

  test('mobile wave soft-pans on a motion wrapper', async ({ page }) => {
    await page.setViewportSize({ width: 393, height: 851 });
    await page.goto('/nodes');
    await expect(page.locator('.cc-waves__motion')).toBeVisible({ timeout: 15_000 });

    // Mobile uses JS pan (CSS transform animations flake on phone WebViews)
    await expect(page.locator('.cc-waves__motion--js')).toBeVisible({ timeout: 5_000 });

    const t0 = await page.locator('.cc-waves__motion').evaluate((el) => getComputedStyle(el).transform);
    await page.waitForTimeout(700);
    const t1 = await page.locator('.cc-waves__motion').evaluate((el) => getComputedStyle(el).transform);
    expect(t1).not.toBe(t0);
    expect(t1).toMatch(/matrix|translate/);

    // SVG soft-blur filter present (fuzzy edges without CSS filter)
    await expect(page.locator('.cc-waves__svg filter feGaussianBlur')).toHaveCount(1);
  });

  test('settings tablet viewport', async ({ page }) => {
    await page.setViewportSize({ width: 768, height: 1024 });
    const m = await assertWaveClearOfText(page, 'tablet');
    expect(m.viewportWidth).toBeGreaterThan(480);
    expect(m.viewportWidth).toBeLessThanOrEqual(899);
    expect(m.waveHVar).toBe('16px');
  });

  test('settings desktop viewport', async ({ page }) => {
    await page.setViewportSize({ width: 1280, height: 800 });
    const m = await assertWaveClearOfText(page, 'desktop');
    expect(m.viewportWidth).toBeGreaterThan(899);
    expect(m.waveHVar).toBe('12px');
  });
});
