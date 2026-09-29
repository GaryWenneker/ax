import { expect, test, type Page } from '@playwright/test';

async function openClusterRow(page: Page) {
  await page.goto('/logging');
  const row = page.locator('.mcp-trace-row.mcp-trace-row--enrich').first();
  await row.locator('.page-item-title').click();
  return page.locator('aside.memory-blade.mcp-blade');
}

test.describe('Logging blade', () => {
  test('a row opens a Memory-style blade, not a popup', async ({ page }) => {
    const blade = await openClusterRow(page);
    await expect(blade).toBeVisible();
    await expect(page.locator('.mcp-inspect-overlay')).toHaveCount(0);
    await expect(page.locator('.mcp-trace-scroller')).toBeVisible();
    const logWidth = await blade.evaluate((el) => el.getBoundingClientRect().width);

    await page.goto('/memory');
    await page.locator('.memory-vault-list .page-item--graph .page-item-title').first().click();
    const memWidth = await page.locator('aside.memory-blade').first().evaluate((el) => el.getBoundingClientRect().width);
    expect(Math.round(logWidth)).toBe(Math.round(memWidth));
  });

  test('details show time without milliseconds and the direction', async ({ page }) => {
    const blade = await openClusterRow(page);
    const kv = await blade.locator('.detail-kv').evaluateAll((rows) =>
      Object.fromEntries(
        rows.map((r) => [r.querySelector('.detail-key')?.textContent?.trim() ?? '', r.querySelector('.detail-val')?.textContent?.trim() ?? '']),
      ),
    );
    expect(kv.Time).toMatch(/^\d{4}-\d{2}-\d{2} \d{2}:\d{2}:\d{2}$/);
    expect(kv.Direction).toMatch(/^(Prompt in|Returned to agent|Internal)$/);
    expect(kv.Kind).toBeTruthy();
  });

  test('steps are calm-list rows with non-overlapping, kind-colored badges', async ({ page }) => {
    await page.goto('/logging');
    const listEnr = await page
      .locator('.mcp-trace-calm .mcp-kind-badge--enrich')
      .first()
      .evaluate((el) => getComputedStyle(el).borderTopColor);
    const blade = await openClusterRow(page);
    const steps = blade.locator('.calm-list .page-item--graph');
    expect(await steps.count()).toBeGreaterThan(1);
    const geo = await steps.evaluateAll((rows) =>
      rows.map((r) => {
        const badge = r.querySelector('.mcp-kind-badge') as Element;
        const title = r.querySelector('.page-item-title') as Element;
        void title;
        return {
          enr: badge.classList.contains('mcp-kind-badge--enrich') ? getComputedStyle(badge).borderTopColor : '',
          overflow: badge.scrollWidth > badge.clientWidth + 1,
        };
      }),
    );
    for (const g of geo) expect(g.overflow).toBe(false);
    const titles = await steps.evaluateAll((rows) =>
      rows.map((r) => {
        const t = (r.querySelector('.page-item-title') as Element).getBoundingClientRect();
        return [...r.querySelectorAll('.page-item-badge')].every((b) => {
          const bb = b.getBoundingClientRect();
          return bb.left >= t.right - 1 || bb.right <= t.left + 1 || bb.top >= t.bottom - 1 || bb.bottom <= t.top + 1;
        });
      }),
    );
    expect(titles.every(Boolean)).toBe(true);
    const enr = geo.find((g) => g.enr);
    expect(enr?.enr).toBe(listEnr);
    await expect(blade.locator('.calm-list .page-item--selected')).toHaveCount(1);
  });

  test('no blur on the list while the blade is open, even offline', async ({ page }) => {
    const look = () =>
      page.locator('.mcp-trace-scroller').first().evaluate((el) => {
        const cs = getComputedStyle(el);
        return { filter: cs.filter, opacity: cs.opacity };
      });
    await page.goto('/logging');
    await page.locator('.mcp-trace-row').first().waitFor();
    await page.locator('.mcp-trace-shell').first().evaluate((el) => el.classList.add('mcp-trace-shell--offline'));
    await expect.poll(async () => (await look()).filter).toContain('blur');
    const blade = await openClusterRow(page);
    await expect(blade).toBeVisible();
    await page.locator('.mcp-trace-shell').first().evaluate((el) => el.classList.add('mcp-trace-shell--offline'));
    await expect.poll(async () => (await look()).filter).toBe('none');
    expect((await look()).opacity).toBe('1');
  });

  test('step click selects, list click switches, Esc closes', async ({ page }) => {
    const blade = await openClusterRow(page);
    const steps = blade.locator('.calm-list .page-item--graph');
    await steps.nth(0).locator('.page-item-title').click();
    await expect(steps.nth(0)).toHaveClass(/page-item--selected/);
    const before = await blade.locator('.detail-title').textContent();
    await page.locator('.mcp-trace-row.mcp-trace-row--inbound:not(.page-item--selected)').first().locator('.page-item-title').click();
    await expect(blade).toBeVisible();
    await expect.poll(() => blade.locator('.detail-kv').first().textContent()).not.toBe('');
    expect(before).toBeTruthy();
    await page.keyboard.press('Escape');
    await expect(page.locator('aside.mcp-blade')).toHaveCount(0);
  });
});
