import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import { describe, it } from 'node:test';

const portal = readFileSync(new URL('./portal.css', import.meta.url), 'utf8');
const savings = readFileSync(new URL('./pages/Savings.tsx', import.meta.url), 'utf8');
const block = portal.slice(portal.indexOf('/* Savings portal'));

describe('Savings portal colors', () => {
  it('S1 timeline and matched path use the accent', () => {
    assert.match(block, /\.sv-tl-bar[\s\S]*?background:\s*var\(--accent\)/);
    assert.match(block, /\.tv-graph-matched\s*\{[^}]*stroke:\s*var\(--accent\)/);
    assert.equal(block.includes('#238636'), false);
    assert.equal(block.includes('#39ff8c'), false);
  });

  it('S2 without-ax bars are neutral', () => {
    assert.match(block, /\.sv-comparison-bar--without[\s\S]*?background:\s*var\(--border-hi\)/);
    assert.equal(block.includes('#d18616'), false);
  });

  it('S3 heatmap levels use the accent', () => {
    assert.match(block, /\.sv-hm-cell--4\s*\{[^}]*background:\s*var\(--accent\)/);
    assert.match(block, /\.sv-hm-cell--0\s*\{[^}]*var\(--text\)/);
  });

  it('S4 token path has no neon glow and uses the panel surface', () => {
    assert.match(block, /\.tv-graph-matched-glow\s*\{[^}]*stroke:\s*transparent/);
    assert.match(block, /\.tv-graph-canvas-wrap\s*\{[^}]*background:\s*var\(--bg-input\)/);
    assert.match(block, /#tvPlotFade stop\s*\{[^}]*stop-color:\s*var\(--bg-input\)/);
  });

  it('S5 hero figures and the active window match other portal pages', () => {
    assert.match(block, /\.sv-hero-stat-value--ok\s*\{[^}]*color:\s*var\(--text-hi\)/);
    assert.match(block, /\.sv-tl-win--active\s*\{[^}]*background:\s*var\(--bg-active\)/);
  });

  it('S6 the daily trend Tokens saved series uses the accent', () => {
    const saved = savings.slice(
      savings.indexOf("dailyView === 'saved'"),
      savings.indexOf("dailyView === 'reduction'"),
    );
    assert.match(saved, /<AreaChart/);
    assert.equal(saved.includes('--ok'), false);
    assert.equal(saved.includes('var(--accent)') || !saved.includes('color='), true);
  });
});
