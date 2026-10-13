import { useMemo, useState } from 'react';

/** Small removable tokens with prefix suggestions. */
export default function PolicyChipField({
  label,
  values,
  onChange,
  suggestions,
}: {
  label: string;
  values: string[];
  onChange: (next: string[]) => void;
  suggestions: readonly string[];
}) {
  const [draft, setDraft] = useState('');
  const query = draft.trim().toLowerCase();
  const matches = useMemo(
    () =>
      suggestions
        .filter((item) => item.toLowerCase().includes(query) && !values.includes(item))
        .slice(0, 6),
    [suggestions, query, values],
  );

  function add(raw: string) {
    const next = raw.trim();
    if (!next || values.some((item) => item.toLowerCase() === next.toLowerCase())) {
      setDraft('');
      return;
    }
    onChange([...values, next]);
    setDraft('');
  }

  return (
    <div className="policy-chips">
      <ul className="policy-chip-list">
        {values.map((item) => (
          <li key={item}>
            <span className="policy-chip">
              {item}
              <button
                type="button"
                className="policy-chip-remove"
                aria-label={`Remove ${item}`}
                onClick={() => onChange(values.filter((value) => value !== item))}
              >
                ×
              </button>
            </span>
          </li>
        ))}
      </ul>
      <input
        className="settings-input"
        value={draft}
        aria-label={label}
        placeholder={`Add ${label.toLowerCase()}`}
        onChange={(e) => setDraft(e.target.value)}
        onKeyDown={(e) => {
          if (e.key === 'Enter' || e.key === ',') {
            e.preventDefault();
            add(draft);
          } else if (e.key === 'Backspace' && !draft && values.length > 0) {
            onChange(values.slice(0, -1));
          }
        }}
      />
      {query && matches.length > 0 && (
        <ul className="policy-chip-suggest" role="listbox">
          {matches.map((item) => (
            <li key={item}>
              <button type="button" onClick={() => add(item)}>
                {item}
              </button>
            </li>
          ))}
        </ul>
      )}
    </div>
  );
}

const STOP = new Set([
  'the', 'and', 'for', 'with', 'that', 'this', 'from', 'your', 'when', 'must', 'not', 'are', 'you',
  'one', 'all', 'any', 'into', 'only', 'then', 'than', 'have', 'has', 'was', 'were', 'can', 'use',
]);

/** Frequent words from the body that are not already tags. */
export function suggestTags(body: string, title: string, existing: readonly string[]): string[] {
  const words = `${title} ${body}`.toLowerCase().match(/[a-z][a-z0-9-]{2,}/g) ?? [];
  const counts = new Map<string, number>();
  for (const word of words) {
    if (STOP.has(word)) continue;
    counts.set(word, (counts.get(word) ?? 0) + 1);
  }
  const have = new Set(existing.map((item) => item.toLowerCase()));
  return [...counts.entries()]
    .sort((a, b) => b[1] - a[1] || a[0].localeCompare(b[0]))
    .map(([word]) => word)
    .filter((word) => !have.has(word))
    .slice(0, 6);
}
