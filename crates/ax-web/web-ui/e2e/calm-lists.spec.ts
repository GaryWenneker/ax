import { expect, test, type Page } from '@playwright/test';

async function expectCalmList(page: Page, route: string, heading?: string) {
  await page.goto(route);
  if (heading) {
    await expect(page.getByRole('heading', { name: heading, exact: true }).first()).toBeVisible({ timeout: 15_000 });
  }
  const rows = page.locator('.calm-list .page-item--graph');
  await expect(rows.first()).toBeVisible({ timeout: 15_000 });
  await expect(page.locator('.page-split-main table, .mcp-trace-list table')).toHaveCount(0);

  const badgeIcons = await page
    .locator('.calm-list .page-item-badge')
    .evaluateAll((els) => els.map((el) => Boolean(el.querySelector('.badge-icon'))));
  expect(badgeIcons.length).toBeGreaterThan(0);
  expect(badgeIcons.every(Boolean)).toBe(true);

  const radius = await page
    .locator('.calm-list .page-item-badge')
    .first()
    .evaluate((el) => getComputedStyle(el).borderRadius);
  expect(radius).toBe('0px');

  const badge = await page.locator('.calm-list .page-item-badge').first().evaluate((el) => {
    const cs = getComputedStyle(el);
    return { h: el.getBoundingClientRect().height, font: parseFloat(cs.fontSize), border: parseFloat(cs.borderTopWidth) };
  });
  expect(badge.h).toBeLessThanOrEqual(14);
  expect(badge.font).toBeLessThanOrEqual(10);
  expect(badge.border).toBeLessThanOrEqual(1);

  const node = await rows.first().evaluate((row) => {
    const dot = row.querySelector('.page-item-node') as HTMLElement;
    const r = row.getBoundingClientRect();
    const d = dot.getBoundingClientRect();
    return {
      gap: d.left - r.left,
      size: d.width,
      policy: row.classList.contains('policy-calm-row'),
      shadow: getComputedStyle(row).boxShadow,
    };
  });
  expect(node.gap).toBeGreaterThanOrEqual(12);
  expect(node.size).toBeLessThanOrEqual(12);
  if (node.policy) expect(node.shadow.includes('inset')).toBe(false);
  return rows;
}

async function expectSelectable(rows: ReturnType<Page['locator']>) {
  const first = rows.first();
  await first.locator('.page-item-title').click();
  await expect(first).toHaveClass(/page-item--selected/);
  const border = await first.evaluate((el) => getComputedStyle(el).borderTopWidth);
  expect(border).toBe('1px');
}

test.describe('Calm list style', () => {
  test.use({ viewport: { width: 1280, height: 800 } });

  test('Rules list uses Memory rows with groups', async ({ page }) => {
    const rows = await expectCalmList(page, '/policy/rules', 'Rules');
    await expect(page.locator('.calm-group-header').first()).toBeVisible();
    await expectSelectable(rows);
  });

  test('Rules group header collapses its rows', async ({ page }) => {
    await expectCalmList(page, '/policy/rules', 'Rules');
    const header = page.locator('.calm-group-header').first();
    const group = page.locator('.calm-group').first();
    if ((await header.getAttribute('aria-expanded')) === 'false') await header.click();
    await expect(group.locator('.page-item--graph').first()).toBeVisible();
    await header.click();
    await expect(header).toHaveAttribute('aria-expanded', 'false');
    await expect(group.locator('.page-item--graph')).toHaveCount(0);
    await header.click();
  });

  test('Skills list uses Memory rows with groups', async ({ page }) => {
    const rows = await expectCalmList(page, '/policy/skills', 'Skills');
    await expect(page.locator('.calm-group-header').first()).toBeVisible();
    await expectSelectable(rows);
  });

  test('Logging list uses Memory rows', async ({ page }) => {
    const rows = await expectCalmList(page, '/logging');
    await expect(rows.first()).toHaveAttribute('data-entry-id', /.+/);
    const overlaps = await page.locator('.mcp-trace-calm .page-item-badge').evaluateAll((els) =>
      els.filter((el) => el.scrollWidth > el.clientWidth + 1).length,
    );
    expect(overlaps).toBe(0);
  });

  test('Logging is readable and shows prompt in vs returned by color', async ({ page }) => {
    const rows = await expectCalmList(page, '/logging');
    const first = await rows.first().evaluate((row) => ({
      meta: row.querySelector('.page-item-meta')?.textContent ?? '',
      font: getComputedStyle(row.querySelector('.page-item-title') as Element).fontFamily,
    }));
    expect(first.meta).toMatch(/^\d{4}-\d{2}-\d{2} \d{2}:\d{2}:\d{2}$/);
    expect(first.font).not.toMatch(/Cascadia|mono|Consolas|Menlo/i);

    const look = (sel: string) =>
      page.locator(sel).first().evaluate((row) => ({
        node: getComputedStyle(row.querySelector('.page-item-node') as Element).borderTopColor,
        badge: getComputedStyle(row.querySelector('.mcp-kind-badge') as Element).borderTopColor,
        subtitle: row.querySelector('.page-item-sub')?.textContent ?? '',
      }));
    const inRow = await look('.mcp-trace-row--dir-in');
    const outRow = await look('.mcp-trace-row--dir-out');
    const intRow = await look('.mcp-trace-row--dir-internal');
    expect(inRow.subtitle).toMatch(/^Prompt in/);
    expect(outRow.subtitle).toMatch(/^Returned to agent/);
    expect(new Set([inRow.node, outRow.node, intRow.node]).size).toBe(3);
    expect(inRow.badge).not.toBe(outRow.badge);
    expect(outRow.badge).not.toBe(intRow.badge);
  });

  test('selected row text is brighter than other rows', async ({ page }) => {
    for (const [route, rowSel] of [
      ['/memory', '.memory-vault-list .page-item--graph'],
      ['/policy/rules', '.policy-calm-row'],
      ['/logging', '.mcp-trace-row'],
    ] as const) {
      await page.goto(route);
      const rows = page.locator(rowSel);
      await rows.nth(1).locator('.page-item-title').click();
      await expect(rows.nth(1)).toHaveClass(/page-item--selected/);
      const colors = await page.locator(rowSel).evaluateAll((els) =>
        els.slice(0, 3).map((r) => {
          const c = (s: string) => {
            const e = r.querySelector(s);
            return e ? getComputedStyle(e).color : '';
          };
          return { selected: r.classList.contains('page-item--selected'), title: c('.page-item-title'), sub: c('.page-item-sub') };
        }),
      );
      const sel = colors.find((c) => c.selected);
      const other = colors.find((c) => !c.selected);
      expect(sel?.title, route).toBe('rgb(255, 255, 255)');
      const lum = (rgb = '') => (rgb.match(/\d+/g) ?? []).slice(0, 3).reduce((a, n) => a + Number(n), 0);
      expect(lum(sel?.sub), route).toBeGreaterThan(lum(other?.sub));
    }
  });

  test('Memory and Logging scroll the same way, bounded to their panel', async ({ page }) => {
    for (const [route, scroller] of [
      ['/memory', '.memory-vault-list'],
      ['/logging', '.mcp-trace-scroller'],
    ] as const) {
      await page.goto(route);
      const el = page.locator(scroller).first();
      await expect(el).toBeVisible();
      const s = await el.evaluate((node) => ({
        overscroll: getComputedStyle(node).overscrollBehaviorY,
        pageScrolls: document.documentElement.scrollHeight > document.documentElement.clientHeight,
      }));
      expect(s.overscroll, route).toBe('none');
      expect(s.pageScrolls, route).toBe(false);
    }
  });

  test('mouse wheel scrolls Logging down and back up, like Memory', async ({ page }) => {
    for (const [route, rowSel, scroller] of [
      ['/memory', '.memory-vault-list .page-item--graph', '.memory-vault-list'],
      ['/logging', '.mcp-trace-row', '.mcp-trace-scroller'],
    ] as const) {
      await page.goto(route);
      const box = await page.locator(rowSel).nth(2).boundingBox();
      if (!box) throw new Error(`${route}: no row box`);
      await page.mouse.move(box.x + 200, box.y + 10);
      const top = () => page.locator(scroller).first().evaluate((el) => el.scrollTop);
      await page.mouse.wheel(0, 600);
      await expect.poll(top, { message: `${route} down` }).toBeGreaterThan(300);
      const down = await top();
      await page.mouse.wheel(0, -300);
      await expect.poll(top, { message: `${route} up` }).toBeLessThan(down);
    }
  });

  test('every page panel uses the Logging background', async ({ page }) => {
    await page.goto('/logging');
    const logBg = await page.locator('.mcp-trace-shell').first().evaluate((el) => getComputedStyle(el).backgroundColor);
    for (const route of ['/policy/skills', '/policy/rules', '/settings']) {
      await page.goto(route);
      const card = page.locator('.settings-card').first();
      await expect(card).toBeVisible();
      const look = await card.evaluate((el) => {
        const cs = getComputedStyle(el);
        return { bg: cs.backgroundColor, image: cs.backgroundImage };
      });
      expect(look.bg, route).toBe(logBg);
      expect(look.image, route).toBe('none');
    }
  });
});
