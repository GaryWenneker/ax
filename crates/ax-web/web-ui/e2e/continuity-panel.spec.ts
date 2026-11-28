import { expect, test, type Page } from '@playwright/test';

import { emptyQualitySnapshot } from '../src/lib/mcpQuality';
import { baseReply, routeTraceStream } from './fixtures';

const line = '2026-10-12T20:00:00Z [ax-mcp] inbound tool=ax_search session=axs_live args={"query":"First event"}';

const fields = [
  'External chat id',
  'External turn id',
  'Ax session id',
  'Ax turn id',
  'Agent provider',
  'Project id',
  'Created at',
  'Parent session',
  'Checkpoint',
  'Mode',
  'Target agent',
  'Task',
];

/** The control inside the continuity label whose caption is exactly `caption`. */
function field(page: Page, caption: string) {
  return page
    .locator('.continuity-grid label')
    .filter({ has: page.locator(':scope > span', { hasText: new RegExp(`^${caption}$`) }) })
    .locator('input, select, textarea');
}

async function openLogging(page: Page, width = 1600): Promise<void> {
  await page.route('**/api/**', (route) => {
    const path = new URL(route.request().url()).pathname;
    const json =
      path === '/api/usage/mcp-quality'
        ? { ...emptyQualitySnapshot(), projectLabel: 'Continuity', verbosePresent: true, verboseEnabled: true }
        : path === '/api/usage/mcp-trace/path'
          ? { projectLabel: 'Continuity', projectRoot: '/fixtures/continuity', logDay: '2026-10-12', logExists: true }
          : path === '/api/ship/config'
            ? { config: { ship: {}, ui: {} }, sonar: {} }
            : path === '/api/specs'
              ? { specs: [] }
              : (baseReply(path) ?? {});
    return route.fulfill({ json });
  });
  await routeTraceStream(page, [line]);
  await page.setViewportSize({ width, height: 900 });
  await page.goto('/logging');
  await expect(page.locator('.mcp-trace-row')).toHaveCount(1);
}

test('the continuity toggle shows and hides an editable field for every id', async ({ page, isMobile }) => {
  test.skip(isMobile, 'desktop layout');
  await openLogging(page);
  const toggle = page.getByRole('button', { name: 'Context continuity' });
  const body = page.locator('#continuity-fields');
  await expect(toggle).toHaveAttribute('aria-expanded', 'false');
  await expect(body).toBeHidden();

  await toggle.click();
  await expect(toggle).toHaveAttribute('aria-expanded', 'true');
  await expect(body).toBeVisible();
  for (const caption of fields) {
    const control = field(page, caption);
    await expect(control, caption).toHaveCount(1);
    await expect(control, caption).toBeVisible();
    await expect(control, caption).toBeEnabled();
    if (caption !== 'Mode') await expect(control, caption).toBeEditable();
  }
  await expect(page.locator('.continuity-panel')).not.toContainText('Test ');

  await toggle.click();
  await expect(body).toBeHidden();
});

test('the new-chat prompt can be prepared and copied', async ({ page, context, isMobile }) => {
  test.skip(isMobile, 'desktop layout');
  await context.grantPermissions(['clipboard-read', 'clipboard-write']);
  await openLogging(page);
  await page.getByRole('button', { name: 'Context continuity' }).click();
  await field(page, 'Ax session id').fill('axs_live');
  await field(page, 'Checkpoint').fill('cp-1');
  await field(page, 'Task').fill('Continue the review');
  await page.getByRole('button', { name: 'Prepare new-chat prompt' }).click();
  const prompt = page.locator('#continuity-prompt-text');
  await expect(prompt).toBeEditable();
  await expect(prompt).not.toHaveValue('');
  await page.getByRole('button', { name: 'Copy prompt' }).click();
  await expect(page.locator('.continuity-notice')).toHaveText('Copied prompt.');
  expect(await page.evaluate(() => navigator.clipboard.readText())).toBe(await prompt.inputValue());
});

test('the open continuity panel scrolls with the page and shows every field unclipped', async ({ page, isMobile }) => {
  test.skip(isMobile, 'desktop layout');
  await openLogging(page);
  await page.getByRole('button', { name: 'Context continuity' }).click();
  const panel = page.locator('.continuity-panel');
  const styles = await panel.evaluate((el) => {
    const s = getComputedStyle(el);
    return { position: s.position, overflow: s.overflow, maxHeight: s.maxHeight };
  });
  expect(styles).toEqual({ position: 'static', overflow: 'visible', maxHeight: 'none' });

  // Six fields per row at desktop width.
  const columns = await page.locator('.continuity-grid').evaluate((el) => getComputedStyle(el).gridTemplateColumns.split(' ').length);
  expect(columns).toBe(6);

  // Every field and the prompt lie fully inside the panel: nothing is cut off.
  const box = await panel.boundingBox();
  expect(box).not.toBeNull();
  for (const target of [...fields.map((caption) => field(page, caption)), page.locator('#continuity-prompt-text')]) {
    const b = await target.boundingBox();
    expect(b).not.toBeNull();
    expect(b!.y).toBeGreaterThanOrEqual(box!.y - 1);
    expect(b!.y + b!.height).toBeLessThanOrEqual(box!.y + box!.height + 1);
  }

  // The logging page scrolls; it is not pinned to the viewport height.
  const pageStyles = await page.locator('.logging-page').evaluate((el) => {
    const s = getComputedStyle(el);
    return { overflowY: s.overflowY, minHeight: s.minHeight };
  });
  expect(pageStyles).toEqual({ overflowY: 'auto', minHeight: '0px' });
});

test('the toggle uses the quiet header button look and Prepare uses the readable accent text', async ({ page, isMobile }) => {
  test.skip(isMobile, 'desktop layout');
  await openLogging(page);
  const toggle = page.getByRole('button', { name: 'Context continuity' });
  const look = await toggle.evaluate((el) => {
    const s = getComputedStyle(el);
    return { background: s.backgroundColor, weight: s.fontWeight };
  });
  expect(look).toEqual({ background: 'rgba(0, 0, 0, 0)', weight: '400' });

  await toggle.click();
  const prepare = page.getByRole('button', { name: 'Prepare new-chat prompt' });
  const [color, expected] = await prepare.evaluate((el) => {
    const probe = document.createElement('span');
    probe.style.color = 'var(--accent-on-fill)';
    el.appendChild(probe);
    const want = getComputedStyle(probe).color;
    probe.remove();
    return [getComputedStyle(el).color, want];
  });
  expect(color).toBe(expected);
});

test('the continuity fields keep more than one column on a narrow desktop window', async ({ page, isMobile }) => {
  test.skip(isMobile, 'desktop layout');
  await openLogging(page, 880);
  await page.getByRole('button', { name: 'Context continuity' }).click();
  const columns = await page.locator('.continuity-grid').evaluate((el) => getComputedStyle(el).gridTemplateColumns.split(' ').length);
  expect(columns).toBeGreaterThan(1);
});
