/** The marked CSS block is a text chase, not a row background glow. */
export function liveNewRuleIsTextChase(css: string): { ok: boolean; reason: string } {
  const start = css.indexOf('/* live-new-chase */');
  const end = css.indexOf('/* live-new-chase-end */');
  if (start < 0 || end <= start) return { ok: false, reason: 'markers missing' };
  const block = css.slice(start, end);
  if (!block.includes('@keyframes live-new-chase')) return { ok: false, reason: 'no chase keyframes' };
  if (!block.includes('::after')) return { ok: false, reason: 'chase is not a text copy' };
  if (!block.includes('content: attr(data-chase)')) return { ok: false, reason: 'no text copy' };
  if (!block.includes('-webkit-mask-image:')) return { ok: false, reason: 'no running light' };
  if (block.includes('live-new-glow')) return { ok: false, reason: 'old glow name' };
  if (block.includes('background-color:') || block.includes('box-shadow:')) {
    return { ok: false, reason: 'background glow' };
  }
  if (block.includes('-webkit-text-fill-color: transparent')) {
    return { ok: false, reason: 'text fill hides the line' };
  }
  if (block.includes('tr.live-new > td')) return { ok: false, reason: 'cell clip' };
  return { ok: true, reason: '' };
}
