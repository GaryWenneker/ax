import { expect, test, type Page } from '@playwright/test';

const EDITOR_URL = '/policy/rules/edit?id=agent-workflow&mode=edit';

const LONG_QUOTE =
  '> **ABSOLUTE**: Every agent in this workspace — including Cursor Task subagents — must call ax MCP the same way as the parent. Subagents do not inherit parent MCP calls when tools are available.';

const GHOST_BODY = [
  'Explore before Grep: read only the files the graph already pointed to.',
  '',
  'Skipping that step does **not** satisfy ExploreBeforeGrep.',
  '',
  'Full guide: https://getax.wenneker.io/guides/policy-engine/)',
].join('\n');

async function openEditor(page: Page) {
  const list = await page.request.get('/api/policy/rules');
  expect(list.ok(), `GET /api/policy/rules → ${list.status()}`).toBeTruthy();
  const payload = (await list.json()) as { rules?: Array<{ id: string }> };
  const ids = (payload.rules ?? []).map((r) => r.id);
  const id = ids.includes('agent-workflow') ? 'agent-workflow' : ids[0];
  expect(id, 'need at least one policy rule').toBeTruthy();
  await page.goto(`/policy/rules/edit?id=${encodeURIComponent(id)}&mode=edit`);
  const textarea = page.locator('textarea.w-md-editor-text-input');
  await expect(textarea).toBeVisible({ timeout: 20_000 });
  return textarea;
}

async function setSource(page: Page, value: string) {
  const textarea = page.locator('textarea.w-md-editor-text-input');
  await textarea.fill(value);
  await expect(textarea).toHaveValue(value);
}

test.describe('md editor caret sync', () => {
  test.use({ viewport: { width: 1280, height: 800 } });

  test('overlay code and textarea share font metrics', async ({ page }) => {
    const textarea = await openEditor(page);
    await setSource(
      page,
      `${LONG_QUOTE}\n\nFull guide: https://getax.wenneker.io/guides/policy-engine/)`,
    );
    const metrics = await page.evaluate(() => {
      const code = document.querySelector('.w-md-editor-text-pre > code');
      const ta = document.querySelector('textarea.w-md-editor-text-input');
      if (!code || !ta) return null;
      const cs = getComputedStyle(code);
      const ts = getComputedStyle(ta);
      return {
        code: { fontSize: cs.fontSize, lineHeight: cs.lineHeight, fontFamily: cs.fontFamily },
        ta: { fontSize: ts.fontSize, lineHeight: ts.lineHeight, fontFamily: ts.fontFamily },
      };
    });
    expect(metrics).toBeTruthy();
    expect(metrics!.code.fontSize).toBe(metrics!.ta.fontSize);
    expect(metrics!.code.lineHeight).toBe(metrics!.ta.lineHeight);
    expect(metrics!.code.fontFamily).toBe(metrics!.ta.fontFamily);
    await expect(textarea).toHaveValue(/tools are available/);
  });

  test('click the last source line places the caret at the document end', async ({ page }) => {
    const textarea = await openEditor(page);
    const body = `${LONG_QUOTE}\n\nFull guide: https://getax.wenneker.io/guides/policy-engine/)`;
    await setSource(page, body);
    await textarea.evaluate((el) => {
      el.scrollTop = el.scrollHeight;
    });
    const box = await textarea.boundingBox();
    expect(box).toBeTruthy();
    await page.mouse.click(box!.x + Math.min(box!.width - 16, 240), box!.y + box!.height - 10);
    const pos = await textarea.evaluate((el: HTMLTextAreaElement) => ({
      start: el.selectionStart,
      len: el.value.length,
      before: el.value.slice(Math.max(0, el.selectionStart - 40), el.selectionStart),
    }));
    expect(Math.abs(pos.start - pos.len)).toBeLessThanOrEqual(2);
    expect(pos.before).toContain('policy-engine');
  });

  test('click after a long wrapping bold line does not land mid-word', async ({ page }) => {
    const textarea = await openEditor(page);
    const body = `${LONG_QUOTE}\nnext-line`;
    await setSource(page, body);
    const click = await page.evaluate(() => {
      const pre = document.querySelector('.w-md-editor-text-pre') as HTMLElement | null;
      if (!pre) return null;
      const walker = document.createTreeWalker(pre, NodeFilter.SHOW_TEXT);
      let acc = '';
      const chunks: Array<{ node: Text; start: number }> = [];
      let n: Node | null;
      while ((n = walker.nextNode())) {
        const t = n as Text;
        chunks.push({ node: t, start: acc.length });
        acc += t.textContent ?? '';
      }
      const needle = 'tools are available.';
      const i = acc.indexOf(needle);
      if (i < 0) return null;
      const end = i + needle.length;
      const r = document.createRange();
      let started = false;
      for (const c of chunks) {
        const cEnd = c.start + (c.node.textContent?.length ?? 0);
        if (!started && i >= c.start && i < cEnd) {
          r.setStart(c.node, i - c.start);
          started = true;
        }
        if (started && end > c.start && end <= cEnd) {
          r.setEnd(c.node, end - c.start);
          break;
        }
      }
      const rects = [...r.getClientRects()];
      const last = rects[rects.length - 1];
      if (!last) return null;
      return { x: last.right - 2, y: last.top + last.height / 2 };
    });
    expect(click).toBeTruthy();
    await page.mouse.click(click!.x, click!.y);
    const pos = await textarea.evaluate((el: HTMLTextAreaElement) => {
      const start = el.selectionStart;
      const nl = el.value.indexOf('\n');
      const around = el.value.slice(Math.max(0, start - 12), start + 12);
      return { start, nl, len: el.value.length, around };
    });
    const expected = pos.nl === -1 ? pos.len : pos.nl;
    expect(pos.start).toBe(expected);
    expect(pos.around.includes('available') && pos.start !== expected).toBe(false);
    const availableAt = body.indexOf('available');
    expect(pos.start < availableAt || pos.start >= availableAt + 'available'.length).toBe(true);
  });

  test('drag-select a wrapping paragraph does not ghost a second copy', async ({ page }) => {
    const textarea = await openEditor(page);
    await setSource(page, GHOST_BODY);
    const report = await page.evaluate(() => {
      const ta = document.querySelector('textarea.w-md-editor-text-input') as HTMLTextAreaElement | null;
      const pre = document.querySelector('.w-md-editor-text-pre') as HTMLElement | null;
      if (!ta || !pre) return null;
      const a = ta.value.indexOf('read only the files the graph already pointed to');
      const bNeedle = 'does **not** satisfy';
      const b = ta.value.indexOf(bNeedle);
      ta.setSelectionRange(a, b + bNeedle.length);
      const sel = getComputedStyle(ta, '::selection');
      const fill = sel.getPropertyValue('-webkit-text-fill-color') || sel.color;
      const walker = document.createTreeWalker(pre, NodeFilter.SHOW_TEXT);
      let acc = '';
      const chunks: Array<{ node: Text; start: number }> = [];
      let n: Node | null;
      while ((n = walker.nextNode())) {
        const t = n as Text;
        chunks.push({ node: t, start: acc.length });
        acc += t.textContent ?? '';
      }
            let overlayTop: number | null = null;
            const needles = [bNeedle, bNeedle.replace(/\*\*/g, '')];
            let i = -1;
            let used = '';
            for (const n of needles) {
              i = acc.indexOf(n);
              if (i >= 0) {
                used = n;
                break;
              }
            }
            if (i >= 0) {
              for (const c of chunks) {
                const end = c.start + (c.node.textContent?.length ?? 0);
                if (i >= c.start && i < end) {
                  const r = document.createRange();
                  const local = i - c.start;
                  r.setStart(c.node, local);
                  r.setEnd(c.node, Math.min(local + Math.min(used.length, 8), c.node.textContent?.length ?? 0));
                  overlayTop = r.getBoundingClientRect().top;
                  break;
                }
              }
            }
      const mirror = document.createElement('div');
      const ts = getComputedStyle(ta);
      const padTop = parseFloat(ts.paddingTop) || 0;
      mirror.style.cssText = [
        'position:absolute',
        'visibility:hidden',
        'white-space:pre-wrap',
        'overflow-wrap:break-word',
        'word-break:break-word',
        `font:${ts.font}`,
        `font-size:${ts.fontSize}`,
        `line-height:${ts.lineHeight}`,
        `font-family:${ts.fontFamily}`,
        `width:${ta.clientWidth}px`,
        'padding:0',
        'box-sizing:content-box',
        `letter-spacing:${ts.letterSpacing}`,
        `tab-size:${ts.tabSize}`,
      ].join(';');
      const marker = document.createElement('span');
      marker.textContent = '\u200b';
      mirror.appendChild(document.createTextNode(ta.value.slice(0, b)));
      mirror.appendChild(marker);
      document.body.appendChild(mirror);
      const taTop =
        ta.getBoundingClientRect().top + padTop + marker.offsetTop - ta.scrollTop;
      mirror.remove();
      return {
        heightDelta: Math.abs(ta.scrollHeight - pre.getBoundingClientRect().height),
        fill,
        overlayTop,
        taTop,
        fillIsTransparent:
          fill === 'transparent' ||
          fill === 'rgba(0, 0, 0, 0)' ||
          fill.includes('0, 0, 0, 0'),
      };
    });
    expect(report).toBeTruthy();
    expect(report!.heightDelta).toBeLessThanOrEqual(24);
    expect(report!.fillIsTransparent).toBe(true);
    expect(report!.overlayTop).not.toBeNull();
    expect(Math.abs((report!.overlayTop as number) - report!.taTop)).toBeLessThanOrEqual(3);
  });
});
