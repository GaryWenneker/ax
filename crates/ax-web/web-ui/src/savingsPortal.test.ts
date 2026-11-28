import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import { describe, it } from 'node:test';

// Stylesheet lint: the Savings block of portal.css draws only with theme tokens.
const portal = readFileSync(new URL('./portal.css', import.meta.url), 'utf8');
const block = portal.slice(portal.indexOf('/* Savings portal'));

describe('portal.css Savings block (stylesheet lint)', () => {
  it('uses theme tokens instead of fixed colors', () => {
    for (const fixed of ['#238636', '#39ff8c', '#d18616']) assert.equal(block.includes(fixed), false, fixed);
    const expected: Array<[string, RegExp]> = [
      ['timeline bar', /\.sv-tl-bar[\s\S]*?background:\s*var\(--accent\)/],
      ['matched path', /\.tv-graph-matched\s*\{[^}]*stroke:\s*var\(--accent\)/],
      ['without-ax bar', /\.sv-comparison-bar--without[\s\S]*?background:\s*var\(--border-hi\)/],
      ['heatmap top level', /\.sv-hm-cell--4\s*\{[^}]*background:\s*var\(--accent\)/],
      ['heatmap empty level', /\.sv-hm-cell--0\s*\{[^}]*var\(--text\)/],
      ['token path glow', /\.tv-graph-matched-glow\s*\{[^}]*stroke:\s*transparent/],
      ['graph surface', /\.tv-graph-canvas-wrap\s*\{[^}]*background:\s*var\(--bg-input\)/],
      ['plot fade', /#tvPlotFade stop\s*\{[^}]*stop-color:\s*var\(--bg-input\)/],
      ['hero figure', /\.sv-hero-stat-value--ok\s*\{[^}]*color:\s*var\(--text-hi\)/],
      ['active window', /\.sv-tl-win--active\s*\{[^}]*background:\s*var\(--bg-active\)/],
    ];
    for (const [name, rule] of expected) assert.match(block, rule, name);
  });
});
