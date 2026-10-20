import assert from 'node:assert/strict';
import test from 'node:test';
import { spawnSync } from 'node:child_process';
import path from 'node:path';
import { fileURLToPath } from 'node:url';
const site = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '..');
const env = Object.fromEntries(Object.entries(process.env).filter(([name]) => !name.startsWith('PUPPETEER_')));
for (const platform of ['darwin', 'win32', 'linux']) {
    test(`Puppeteer discovers site download settings on ${platform}`, () => {
        // Load the real installed Puppeteer configuration in a fresh process;
        // platform selection is simulated, browser execution is not claimed.
        const script = `
      Object.defineProperty(process, 'platform', { value: ${JSON.stringify(platform)} });
      const { createRequire } = await import('node:module');
      const require = createRequire(import.meta.url);
      // Puppeteer 25 no longer exposes .configuration on its public instance.
      // Read the actual configuration entry-point and also check Puppeteer loads.
      const { default: puppeteer } = await import('puppeteer');
      if (typeof puppeteer.launch !== 'function') throw new Error('Puppeteer missing launch API');
      console.log(JSON.stringify(require('./puppeteer.config.cjs')));
    `;
        const result = spawnSync(process.execPath, ['--input-type=module', '-e', script], {
            cwd: site, env, encoding: 'utf8',
        });
        assert.equal(result.status, 0, result.stderr);
        const config = JSON.parse(result.stdout);
        assert.equal(config['chrome-headless-shell'].skipDownload, platform !== 'linux');
        assert.notEqual(config.chrome.skipDownload, true, 'Chrome for Testing must remain available');
        assert.equal(config.firefox.skipDownload, true);
    });
}
//# sourceMappingURL=puppeteer-config.test.js.map