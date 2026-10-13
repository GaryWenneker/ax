import {test} from 'node:test';
import assert from 'node:assert/strict';
import {fitWidths, titleTrack, combineDemands, computeFit, type ColumnDemand} from './columnFit.ts';

const sum = (widths: number[]) => widths.reduce((total, width) => total + width, 0);
const flexible = (natural: number): ColumnDemand => ({natural, rigid: false});
const rigid = (natural: number): ColumnDemand => ({natural, rigid: true});

test('spare space goes to flexible columns in proportion to their content, rigid columns keep their natural width', () => {
  const widths = fitWidths([rigid(28), flexible(600), rigid(100), rigid(100), flexible(60), rigid(114)], [28, 64, 64, 64, 64, 64], 1300);
  assert.equal(sum(widths), 1300);
  assert.deepEqual([widths[0], widths[2], widths[3], widths[5]], [28, 100, 100, 114]);
  // Spare space follows content beyond the minimum: a column that already shows all of its 60px of text gains nothing.
  assert.deepEqual([widths[1], widths[4]], [894, 64]);
});

test('spare space is shared by flexible columns in proportion to the content they hold beyond their minimum', () => {
  const widths = fitWidths([flexible(364), flexible(164), rigid(100)], [64, 64, 64], 1000);
  assert.equal(sum(widths), 1000);
  // 372 spare split 300:100 → 279:93.
  assert.deepEqual(widths, [643, 257, 100]);
});

test('overflowing content shrinks flexible columns first, never below their minimum, and leaves rigid columns alone', () => {
  const widths = fitWidths([flexible(2000), flexible(300), rigid(100), rigid(28)], [64, 64, 64, 28], 900);
  assert.equal(sum(widths), 900);
  assert.deepEqual([widths[2], widths[3]], [100, 28]);
  assert.ok(widths[0] > widths[1] && widths[1] >= 64, `shrunk ${widths}`);
  // Each flexible column loses the same share of what it asked for.
  assert.ok(Math.abs(widths[0] / widths[1] - 2000 / 300) < 0.1, `${widths[0]}:${widths[1]}`);
});

test('when minimums already exceed the container the list keeps its minimums and overflows horizontally', () => {
  const widths = fitWidths([flexible(400), rigid(300), rigid(300)], [120, 64, 64], 500);
  assert.deepEqual(widths, [120, 300, 300]);
});

test('a list where every column is rigid gives the spare space to its widest column', () => {
  assert.deepEqual(fitWidths([rigid(28), rigid(400), rigid(100)], [28, 64, 64], 1000), [28, 872, 100]);
});

test('widths are whole pixels and never fall below a minimum even for empty columns', () => {
  const widths = fitWidths([flexible(0), flexible(0)], [64, 80], 701);
  assert.equal(sum(widths), 701);
  assert.ok(widths.every(width => Number.isInteger(width) && width >= 64), `${widths}`);
});

test('title track keeps at least a 22% share when both fit, and the exact title when that is wider', () => {
  assert.equal(titleTrack({title: 170, description: 300, width: 800, gap: 12}), 176);
  assert.equal(titleTrack({title: 300, description: 100, width: 800, gap: 12}), 304);
});

test('title track keeps the description readable when both cannot fit, and never collapses below a usable width', () => {
  // Logging: huge prompt title, short description → description gets what it needs, title takes the rest.
  assert.equal(titleTrack({title: 2000, description: 330, width: 670, gap: 12}), 328);
  // Skills: short title, long description → the title column keeps its 22% share, description gets the remainder.
  assert.equal(titleTrack({title: 170, description: 900, width: 830, gap: 12}), 183);
  // Both long → title is capped at 40% so the description keeps the majority.
  assert.equal(titleTrack({title: 900, description: 900, width: 800, gap: 12}), 320);
  // Narrow: a title never collapses under 96px.
  assert.equal(titleTrack({title: 900, description: 900, width: 200, gap: 12}), 96);
});

test('a column never shrinks below its heading, even when the content overflows', () => {
  const fit = computeFit([{demands: [flexible(2000), flexible(73), rigid(135)], headings: [40, 73, 135], available: 900, nameColumn: -1, title: 0, description: 0, gap: 12}], [64, 64, 64], 900);
  assert.ok(fit);
  assert.deepEqual([fit.widths[1], fit.widths[2], sum(fit.widths), fit.titleTrack], [73, 135, 900, null]);
});

test('demands from several lists on one page combine to the widest content and stay rigid only when every list agrees', () => {
  const combined = combineDemands([
    [rigid(100), flexible(300), rigid(114)],
    [rigid(100), flexible(500), rigid(118)],
  ]);
  assert.deepEqual(combined, [rigid(100), flexible(500), {natural: 118, rigid: false}]);
  assert.deepEqual(combineDemands([]), []);
});
