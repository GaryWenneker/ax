#!/usr/bin/env node
/**
 * Sync docs/npm/README.md — ensures npm package readme matches repo constants.
 * Called by scripts/pack-npm.sh before publish.
 */
'use strict';

const fs = require('fs');
const path = require('path');

const root = path.join(__dirname, '..');
const readme = path.join(root, 'docs', 'npm', 'README.md');

if (!fs.existsSync(readme)) {
  console.error('[sync-npm-docs] missing docs/npm/README.md');
  process.exit(1);
}

const text = fs.readFileSync(readme, 'utf8');

const forbidden = [
  /@colbymchenry\/\w+/g,
  /colbymchenry/gi,
  /codegraph/gi,
  /getcodegraph\.com/gi,
];
for (const re of forbidden) {
  re.lastIndex = 0;
  if (re.test(text)) {
    console.error('[sync-npm-docs] forbidden pattern in README:', re);
    process.exit(1);
  }
}

const required = ['@garywenneker/ax', 'GaryWenneker/ax', 'getax.wenneker.io'];
for (const needle of required) {
  if (!text.includes(needle)) {
    console.error('[sync-npm-docs] README must mention:', needle);
    process.exit(1);
  }
}

console.log('[sync-npm-docs] docs/npm/README.md OK');
