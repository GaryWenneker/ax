import assert from 'node:assert/strict';
import { describe, it } from 'node:test';

import { screenNodes } from './graphProbe.ts';

describe('screenNodes', () => {
  const rect = { left: 100, top: 50, width: 400, height: 300 };

  it('maps world positions to client pixels through the view transform', () => {
    const out = screenNodes(
      [{ id: 'a', x: 10, y: 20 }],
      (n) => ({ key: n.id, x: n.x, y: n.y }),
      { scale: 2, offsetX: 30, offsetY: 40 },
      rect,
    );
    assert.deepEqual(out, [{ key: 'a', x: 100 + 30 + 20, y: 50 + 40 + 40 }]);
  });

  it('leaves out hidden, unplaced and off-canvas nodes', () => {
    const out = screenNodes(
      [
        { id: 'hidden', x: 1, y: 1, show: false },
        { id: 'nan', x: Number.NaN, y: 1, show: true },
        { id: 'off', x: 1000, y: 1, show: true },
        { id: 'in', x: 5, y: 5, show: true },
      ],
      (n) => (n.show ? { key: n.id, x: n.x, y: n.y } : null),
      { scale: 1, offsetX: 0, offsetY: 0 },
      rect,
    );
    assert.deepEqual(out.map((n) => n.key), ['in']);
  });
});
