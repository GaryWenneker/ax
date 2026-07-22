/** Color-coded o200k token chips (tokviz-inspired). */

export interface TokenizeResult {
  tokens: string[];
  count: number;
  chars: number;
  truncated: boolean;
}

function hashToken(s: string): number {
  let h = 2166136261;
  for (let i = 0; i < s.length; i++) {
    h ^= s.charCodeAt(i);
    h = Math.imul(h, 16777619);
  }
  return h >>> 0;
}

/** Stable HSL background from token text (golden-ratio hue spacing). */
export function tokenColor(token: string): string {
  const golden = 0.618033988749895;
  const n = hashToken(token);
  const hue = ((n * golden) % 1) * 360;
  const s = 48 + (n % 18);
  const l = 28 + (n % 12);
  return `hsl(${hue.toFixed(1)} ${s}% ${l}%)`;
}

function displayToken(token: string): string {
  return token
    .replace(/ /g, '·')
    .replace(/\t/g, '⇥')
    .replace(/\n/g, '↵')
    .replace(/\r/g, '');
}

export function TokenChips({
  tokens,
  count,
  chars,
  truncated,
  emptyHint,
}: {
  tokens: string[];
  count?: number;
  chars?: number;
  truncated?: boolean;
  emptyHint?: string;
}) {
  const n = count ?? tokens.length;
  const c = chars ?? tokens.join('').length;

  if (tokens.length === 0) {
    return (
      <div className="sv-token-empty">
        {emptyHint ?? 'No token preview for this call.'}
      </div>
    );
  }

  return (
    <div className="sv-token-block">
      <div className="sv-token-meta">
        <span>
          {n.toLocaleString()} tokens · {c.toLocaleString()} chars
        </span>
        {truncated && <span className="sv-token-truncated">preview truncated</span>}
      </div>
      <div className="sv-token-chips" role="list" aria-label="Token stream">
        {tokens.map((t, i) => (
          <span
            key={`${i}-${t.slice(0, 8)}`}
            className="sv-token-chip"
            role="listitem"
            style={{ background: tokenColor(t) }}
            title={`#${i}: ${JSON.stringify(t)}`}
          >
            {displayToken(t)}
          </span>
        ))}
      </div>
    </div>
  );
}

/** Abstract ribbon when only token counts exist (no stored preview text). */
export function AbstractTokenRibbon({
  label,
  tokens,
  tone,
}: {
  label: string;
  tokens: number;
  tone: 'without' | 'with';
}) {
  const maxChips = 64;
  const chipCount = tokens <= 0 ? 0 : Math.min(maxChips, Math.max(4, Math.round(Math.log10(tokens + 1) * 12)));
  return (
    <div className="sv-token-block">
      <div className="sv-token-meta">
        <span>
          {label}: {tokens.toLocaleString()} tokens
        </span>
        <span className="sv-token-truncated">count only — no text preview</span>
      </div>
      <div className={`sv-token-ribbon sv-token-ribbon--${tone}`} aria-hidden>
        {Array.from({ length: chipCount }, (_, i) => (
          <span key={i} className="sv-token-ribbon-seg" />
        ))}
      </div>
    </div>
  );
}
