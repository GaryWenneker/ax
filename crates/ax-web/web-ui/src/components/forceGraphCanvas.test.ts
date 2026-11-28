import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import { test } from 'node:test';
import { DEFAULT_GRAPH_SETTINGS, type GraphSettings } from '../lib/graphSettings.ts';
import { filterKey, simulationKey } from '../lib/graphForces.ts';
import { PULSE_MS, SELECTION_PULSE_CYCLES, selectionPulse, selectionRing } from '../lib/graphHover.ts';
import { LabelWidths } from '../lib/graphLabels.ts';

const source = readFileSync(new URL('./ForceGraphCanvas.tsx', import.meta.url), 'utf8');

function block(start: string, end: string): string {
  const from = source.indexOf(start);
  const to = source.indexOf(end, from + start.length);
  assert.ok(from >= 0 && to > from, `${start} … ${end}`);
  return source.slice(from, to);
}

const base: GraphSettings = { ...DEFAULT_GRAPH_SETTINGS };

test('display-only settings do not change the simulation or the filter', () => {
  const display: Partial<GraphSettings>[] = [
    { linkThickness: 3 },
    { arrows: true },
    { textFadeThreshold: 0.8 },
    { labels: true },
    { groups: [{ query: 'x', color: '#f00' }] },
  ];
  for (const change of display) {
    const next = { ...base, ...change };
    assert.equal(simulationKey(next), simulationKey(base), JSON.stringify(change));
    assert.equal(filterKey(next), filterKey(base), JSON.stringify(change));
  }
});

test('force settings change the simulation and filter settings change the filter', () => {
  for (const change of [{ repelForce: 3 }, { centerForce: 0.1 }, { linkForce: 0.2 }, { linkDistance: 40 }, { nodeSize: 2 }]) {
    assert.notEqual(simulationKey({ ...base, ...change }), simulationKey(base), JSON.stringify(change));
  }
  for (const change of [{ search: 'abc' }, { attachments: false }, { existingOnly: true }, { orphans: false }]) {
    assert.notEqual(filterKey({ ...base, ...change }), filterKey(base), JSON.stringify(change));
  }
});

test('the simulation restarts only for force inputs and the filter result', () => {
  const deps = (code: string) => code.slice(code.lastIndexOf('['), code.lastIndexOf(']') + 1);
  const restart = deps(block('const restart = useCallback(', 'useEffect(() => {'));
  assert.doesNotMatch(restart, /\bsettings\b/);
  assert.match(restart, /simKey/);
  const visible = deps(block('const visible = useMemo(', 'const shownEdges'));
  assert.doesNotMatch(visible, /\bsettings\b/);
  assert.match(visible, /fKey/);
});

test('drawing reads cached theme colours and label widths', () => {
  const draw = block('const draw = useCallback(', 'drawRef.current = draw;');
  assert.doesNotMatch(draw, /getComputedStyle/);
  assert.doesNotMatch(draw, /measureText/);
});

test('the selection animation loop stops once the pulse has finished', () => {
  const loop = block('if (selectedKey == null) return;', '}, [selectedKey]);');
  assert.match(loop, /selectionPulseDone\(/);
});

test('the selection ring pulses for a few cycles, then holds still', () => {
  assert.deepEqual(selectionRing(PULSE_MS / 2), selectionPulse(PULSE_MS / 2));
  const settled = selectionRing(PULSE_MS * SELECTION_PULSE_CYCLES + 1);
  assert.deepEqual(settled, selectionRing(PULSE_MS * SELECTION_PULSE_CYCLES * 10));
  assert.ok(settled.alpha > 0);
});

test('label widths are measured once per label and scaled to the font size', () => {
  let measured = 0;
  const widths = new LabelWidths((text, px) => {
    measured++;
    return text.length * px * 0.5;
  });
  assert.equal(widths.width('abcd', 10), 20);
  assert.equal(widths.width('abcd', 20), 40);
  assert.equal(widths.width('abcd', 13), 26);
  assert.equal(measured, 1);
  widths.width('ef', 10);
  assert.equal(measured, 2);
});
