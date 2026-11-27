import { useEffect, useMemo, useState } from 'react';
import {
  isLinkTarget,
  isUrlQuery,
  parseLinkQuery,
  searchLinkTargets,
  type LinkKind,
  type LinkTarget,
} from '../lib/linkPicker';
import { createRefreshingCache } from '../lib/refreshingCache';
import { WORKSPACE_SWITCHED } from '../workspaceEvents';

export type LinkTargets = LinkTarget[] | 'error' | null;

/** How long a loaded target list is reused before opening the picker loads it again. */
const TARGETS_MAX_AGE_MS = 5000;

const targetCache = createRefreshingCache<LinkTarget[]>(
  () =>
    fetch('/api/links/graph')
      .then((res) => {
        if (!res.ok) throw new Error(`HTTP ${res.status}`);
        return res.json() as Promise<unknown>;
      })
      .then((g) => {
        const nodes = (g as { nodes?: unknown } | null)?.nodes; // only read as unknown, then checked
        if (!Array.isArray(nodes)) throw new Error('bad /api/links/graph payload');
        return nodes.filter(isLinkTarget);
      }),
  TARGETS_MAX_AGE_MS,
);

/**
 * Link targets for the `[[` picker. They load while `mounted`, load again when the picker
 * opens (`open`) and the list is older than a few seconds, after an error, and after a
 * workspace switch, so new, renamed or deleted items show up without a reload.
 */
export function useLinkTargets(mounted: boolean, open = false): LinkTargets {
  const [targets, setTargets] = useState<LinkTargets>(null);
  const [epoch, setEpoch] = useState(0);
  useEffect(() => {
    const switched = () => {
      targetCache.clear();
      setTargets(null);
      setEpoch((e) => e + 1);
    };
    window.addEventListener(WORKSPACE_SWITCHED, switched);
    return () => window.removeEventListener(WORKSPACE_SWITCHED, switched);
  }, []);
  useEffect(() => {
    if (!mounted && !open) return;
    let live = true;
    targetCache.get().then(
      (t) => live && setTargets(t),
      () => live && setTargets('error'),
    );
    return () => {
      live = false;
    };
  }, [mounted, open, epoch]);
  return targets;
}

export type PickerRow = { type: 'url'; url: string } | { type: 'target'; item: LinkTarget };

export interface LinkSearch {
  targets: LinkTargets;
  rows: PickerRow[];
  more: number;
  counts: Record<LinkKind, number>;
  kind: LinkKind | null;
  setKind: (k: LinkKind | null) => void;
  /** Kind set by `s:` / `r:` / `m:` in the query; it wins over the chip. */
  queryKind: LinkKind | null;
  tagTerms: string[];
  grouped: boolean;
}

const NO_COUNTS: Record<LinkKind, number> = { rule: 0, skill: 0, memory: 0 };

/**
 * Everything a picker shows for `query`; `allowUrl` adds a "Link to URL" row for http(s) queries.
 * Targets load when the editor mounts, so a quick search + Enter does not land before they arrive.
 */
export function useLinkSearch(active: boolean, query: string, selfKey?: string, allowUrl = false): LinkSearch {
  const targets = useLinkTargets(true, active);
  const [kind, setKind] = useState<LinkKind | null>(null);
  if (!active && kind !== null) setKind(null);
  const parsed = useMemo(() => parseLinkQuery(query), [query]);
  const found = useMemo(
    () => (active && Array.isArray(targets) ? searchLinkTargets(targets, query, { kind, selfKey }) : null),
    [active, targets, query, kind, selfKey],
  );
  const rows = useMemo<PickerRow[]>(() => {
    const url: PickerRow[] = allowUrl && isUrlQuery(query) ? [{ type: 'url', url: query.trim() }] : [];
    return [...url, ...(found?.items ?? []).map((item): PickerRow => ({ type: 'target', item }))];
  }, [allowUrl, query, found]);
  return {
    targets,
    rows,
    more: found?.more ?? 0,
    counts: found?.counts ?? NO_COUNTS,
    kind,
    setKind,
    queryKind: parsed.kind,
    tagTerms: parsed.tags,
    grouped: parsed.words.length === 0 && parsed.tags.length === 0,
  };
}

/** Moves the highlighted row for an arrow key; returns null for other keys. */
export function stepIndex(key: string, index: number, count: number): number | null {
  if (count === 0) return null;
  if (key === 'ArrowDown') return (index + 1) % count;
  if (key === 'ArrowUp') return (index - 1 + count) % count;
  return null;
}

export const optionId = (listId: string, i: number) => `${listId}-opt-${i}`;

const CHIPS: { kind: LinkKind | null; label: string }[] = [
  { kind: null, label: 'All' },
  { kind: 'rule', label: 'Rules' },
  { kind: 'skill', label: 'Skills' },
  { kind: 'memory', label: 'Memory' },
];

const GROUP_LABEL: Record<LinkKind, string> = { rule: 'Rules', skill: 'Skills', memory: 'Memory' };

function TargetRow({ item, tagTerms }: { item: LinkTarget; tagTerms: string[] }) {
  const tags = item.tags ?? [];
  const hit = (t: string) => tagTerms.some((q) => t.toLowerCase().startsWith(q));
  const shown = [...tags.filter(hit), ...tags.filter((t) => !hit(t))].slice(0, 3);
  return (
    <>
      <span className={`badge link-picker-kind link-picker-kind--${item.kind}`}>{item.kind}</span>
      <span className="link-picker-label">{item.label}</span>
      {shown.map((t) => (
        <span key={t} className={`badge link-picker-tag${hit(t) ? ' link-picker-tag--hit' : ''}`}>{t}</span>
      ))}
      {item.origin === 'global' && <span className="badge link-picker-origin">global</span>}
    </>
  );
}

export default function LinkPickerList({
  id,
  search,
  index,
  onPick,
  searchBox,
  onFilter,
  style,
  className,
  dialogLabel,
  panelRef,
}: {
  id: string;
  search: LinkSearch;
  index: number;
  onPick: (row: PickerRow) => void;
  /** The button picker's own search input; the `[[` picker types into the editor instead. */
  searchBox?: React.ReactNode;
  onFilter?: () => void;
  style?: React.CSSProperties;
  className?: string;
  /** Set for the button picker: the panel is then a labelled dialog. */
  dialogLabel?: string;
  panelRef?: React.Ref<HTMLDivElement>;
}) {
  const { targets, rows, more, counts, kind, setKind, queryKind, tagTerms, grouped } = search;
  const activeKind = queryKind ?? kind;
  const empty =
    targets === 'error' ? 'Links unavailable' : !targets ? 'Loading…' : rows.length === 0 ? 'No matches' : null;
  let lastKind: LinkKind | null = null;
  return (
    <div
      ref={panelRef}
      className={`link-picker${className ? ` ${className}` : ''}`}
      style={style}
      role={dialogLabel ? 'dialog' : undefined}
      aria-label={dialogLabel}
      onMouseDown={(e) => {
        if (!(e.target instanceof HTMLInputElement)) e.preventDefault();
      }}
    >
      {searchBox}
      <div className="link-picker-filters" role="group" aria-label="Filter links">
        {CHIPS.map((c) => (
          <button
            key={c.label}
            type="button"
            className={`link-picker-chip${activeKind === c.kind ? ' link-picker-chip--on' : ''}`}
            aria-pressed={activeKind === c.kind}
            disabled={queryKind !== null}
            onClick={() => {
              setKind(c.kind);
              onFilter?.();
            }}
          >
            {c.label}
          </button>
        ))}
      </div>
      {empty ? (
        <div id={id} className="link-picker-empty" role="status">{empty}</div>
      ) : (
        <ul id={id} className="link-picker-list" role="listbox" aria-label="Link to">
          {rows.map((row, i) => {
            const header =
              grouped && row.type === 'target' && row.item.kind !== lastKind ? row.item.kind : null;
            if (row.type === 'target') lastKind = row.item.kind;
            return [
              header && (
                <li key={`h-${header}`} role="presentation" className="link-picker-group">
                  {`${GROUP_LABEL[header]} (${counts[header]})`}
                </li>
              ),
              <li
                key={row.type === 'url' ? 'url' : row.item.key}
                id={optionId(id, i)}
                role="option"
                aria-selected={i === index}
                className={`link-picker-row${i === index ? ' link-picker-row--active' : ''}`}
                onMouseDown={(e) => {
                  e.preventDefault();
                  onPick(row);
                }}
              >
                {row.type === 'url' ? (
                  <>
                    <span className="badge link-picker-kind">url</span>
                    <span className="link-picker-label">Link to URL {row.url}</span>
                  </>
                ) : (
                  <TargetRow item={row.item} tagTerms={tagTerms} />
                )}
              </li>,
            ];
          })}
        </ul>
      )}
      {!empty && more > 0 && <div className="link-picker-more">{more} more — refine the search</div>}
    </div>
  );
}
