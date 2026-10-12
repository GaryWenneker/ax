/** @type {import('puppeteer').Configuration} */
module.exports = {
  // macOS and Windows use ordinary headless Chrome for build screenshots.
  // Do not install the unused extra binary or inspect its incomplete cache.
  'chrome-headless-shell': {
    skipDownload: process.platform !== 'linux',
  },
  // Restricted Linux runners deliberately launch headless: 'shell'.
  // Keep that download there, and retain Chrome for Testing on all platforms.
};
