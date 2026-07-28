import { expect, test } from '@playwright/test';
import fs from 'node:fs';
import path from 'node:path';
import { fileURLToPath } from 'node:url';

import {
  classifyTraceBody,
  entryHasTextPayload,
  entryTextPayloads,
  parseTraceEntry,
  primaryTextPayload,
} from '../src/lib/mcpTrace';

const __dirname = path.dirname(fileURLToPath(import.meta.url));
const MARKER = `logging-text-e2e-${Date.now()}`;

test.describe('logging text + domain kinds', () => {
  test('TOOL column labels domain and cli cmd lines', () => {
    expect(parseTraceEntry('[ax] memory remember ok id=abc').tool).toBe('memory');
    expect(parseTraceEntry('[ax] cli cmd=explore ok').tool).toBe('cli:explore');
    expect(parseTraceEntry('[ax] workspace index ok files=10 tool=workspace').tool).toBe(
      'workspace',
    );
    expect(
      parseTraceEntry(
        '2026-07-26T12:00:00.000Z [ax-mcp] inbound tool=ax_preflight args={}',
      ).tool,
    ).toBe('ax_preflight');
  });

  test('Logging UI shows TOOL=memory for domain line (not dash)', async ({ page }) => {
    const pathRes = await page.request.get('/api/usage/mcp-trace/path');
    expect(pathRes.ok(), `mcp-trace/path → ${pathRes.status()}`).toBeTruthy();
    const meta = (await pathRes.json()) as { path?: string };
    expect(meta.path).toBeTruthy();
    const logPath = meta.path as string;
    fs.mkdirSync(path.dirname(logPath), { recursive: true });
    const marker = `mem-tool-${Date.now()}`;
    fs.appendFileSync(
      logPath,
      `2026-07-26T12:00:00.000Z [ax] memory remember ok id=${marker} tool=memory\n`,
      'utf8',
    );

    await page.goto('/logging');
    const row = page.locator('.mcp-trace-row', { hasText: marker }).first();
    await expect(row).toBeVisible({ timeout: 20_000 });
    await expect(row.locator('.mcp-col-tool')).toContainText('memory');
  });

  test('classify domain prefixes memory / policy / cli', () => {
    expect(classifyTraceBody('[ax] memory remember ok id=abc').kind).toBe('memory');
    expect(classifyTraceBody('[ax] policy index ok rules=1 skills=2').kind).toBe('policy');
    expect(classifyTraceBody('[ax] cli cmd=explore ok').kind).toBe('cli');
    expect(classifyTraceBody('[ax] ship start mode=evaluate').kind).toBe('ship');
    expect(classifyTraceBody('[ax] workspace index ok files=10 duration_ms=1').kind).toBe(
      'workspace',
    );
  });

  test('entryTextPayloads finds prompt and prefers it as primary', () => {
    const entry = parseTraceEntry(
      `2026-07-26T12:00:00.000Z [ax-mcp] inbound tool=ax_preflight args={"prompt":"Ship the logging text plan now","files":["a.rs"]}`,
    );
    expect(entryHasTextPayload(entry)).toBe(true);
    const payloads = entryTextPayloads(entry);
    expect(payloads.some((p) => p.leaf === 'prompt')).toBe(true);
    const primary = primaryTextPayload(entry);
    expect(primary?.leaf).toBe('prompt');
    expect(primary?.value).toContain('logging text plan');
  });

  test('Logging UI shows text badge and Prompt / text hero', async ({ page }) => {
    const pathRes = await page.request.get('/api/usage/mcp-trace/path');
    expect(pathRes.ok(), `mcp-trace/path → ${pathRes.status()}`).toBeTruthy();
    const meta = (await pathRes.json()) as { path?: string; ok?: boolean };
    expect(meta.path, 'verbose log path from API').toBeTruthy();
    const logPath = meta.path as string;

    const dir = path.dirname(logPath);
    fs.mkdirSync(dir, { recursive: true });
    const line =
      `2026-07-26T12:00:00.000Z [ax-mcp] inbound tool=ax_preflight args={"prompt":"${MARKER} verify inspector"}` +
      '\n';
    fs.appendFileSync(logPath, line, 'utf8');

    await page.goto('/logging?hasText=1');
    await expect(page.locator('.mcp-trace-filter-chip-label', { hasText: 'Has text' })).toBeVisible({
      timeout: 15_000,
    });

    const row = page.locator('.mcp-trace-row', { hasText: MARKER }).first();
    await expect(row).toBeVisible({ timeout: 20_000 });
    await expect(row.locator('.mcp-trace-badge--text-prompt, .mcp-trace-badge--text')).toContainText(
      /prompt/i,
    );

    await row.click();
    await expect(page.locator('.mcp-inspect-section--text-hero')).toBeVisible();
    await expect(page.locator('.mcp-inspect-text-hero')).toContainText(MARKER);
  });
});
