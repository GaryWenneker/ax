/** The marked CSS block is a text chase, not a row background glow. */
export function liveNewRuleIsTextChase(css: string): { ok: boolean; reason: string } {
  const start = css.indexOf('/* live-new-chase */');
  const end = css.indexOf('/* live-new-chase-end */');
  if (start < 0 || end <= start) return { ok: false, reason: 'markers missing' };
  const block = css.slice(start, end);
  if (!block.includes('@keyframes live-new-chase')) return { ok: false, reason: 'no chase keyframes' };
  const hasTextClip = block.split('\n').some(
    (line) => line.includes('background-clip: text') && !line.includes('-webkit-background-clip'),
  );
  if (!hasTextClip) return { ok: false, reason: 'text is not clipped' };
  if (block.includes('live-new-glow')) return { ok: false, reason: 'old glow name' };
  if (block.includes('background-color:') || block.includes('box-shadow:')) {
    return { ok: false, reason: 'background glow' };
  }
  return { ok: true, reason: '' };
}
