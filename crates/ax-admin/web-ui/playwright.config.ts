import { defineConfig } from '@playwright/test';

// Started by tools/gauntlet-ax-admin.sh, which runs a real headless Ax with
// remote MCP and a real ax-admin, and passes their addresses in the environment.
export default defineConfig({
  testDir: './e2e',
  testMatch: '**/*.spec.ts',
  fullyParallel: false,
  workers: 1,
  retries: 0,
  forbidOnly: true,
  reporter: [['list']],
  outputDir: 'test-results',
  use: {
    baseURL: process.env.AX_ADMIN_URL,
    viewport: { width: 1280, height: 800 },
    launchOptions: process.env.AX_CHROME_EXECUTABLE ? { executablePath: process.env.AX_CHROME_EXECUTABLE } : {},
    channel: process.env.AX_CHROME_EXECUTABLE ? undefined : 'chrome',
    screenshot: 'only-on-failure',
  },
});
