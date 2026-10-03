import { test } from 'node:test';
import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import { liveNewRuleIsTextChase } from './liveChase.ts';

const oldGlow = `
/* live-new-chase */
@keyframes live-new-glow {
  0% { background-color: red; box-shadow: 0 0 14px red; }
}
.live-new { animation: live-new-glow 2.6s ease-out 1 both; }
/* live-new-chase-end */
`;

test('the checker rejects a background glow', () => {
  const result = liveNewRuleIsTextChase(oldGlow);
  assert.equal(result.ok, false);
});

test('removing the text clip makes a chase block fail', () => {
  const css = readFileSync(new URL('./index.css', import.meta.url), 'utf8');
  const result = liveNewRuleIsTextChase(css);
  assert.equal(result.ok, true, result.reason);
  const start = css.indexOf('/* live-new-chase */');
  const end = css.indexOf('/* live-new-chase-end */');
  const broken =
    css.slice(0, start) +
    css
      .slice(start, end)
      .split('\n')
      .map((line) =>
        line.includes('-webkit-background-clip')
          ? line
          : line.replace('background-clip: text', 'background-clip: padding-box'),
      )
      .join('\n') +
    css.slice(end);
  assert.equal(liveNewRuleIsTextChase(broken).ok, false);
});
