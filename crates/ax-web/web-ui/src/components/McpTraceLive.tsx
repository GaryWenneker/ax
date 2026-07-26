import { useEffect, useLayoutEffect, useMemo, useRef, useState } from 'react';
import { createPortal } from 'react-dom';

import Codicon from './Codicon';
import LoggingProjectSwitch from './LoggingProjectSwitch';
import {
  classifyFieldValue,
  colorizeTraceMessage,
  entryHasQueryPayload,
  entryHeadline,
  entryMeta,
  extractFields,
  extractPayloadJson,
  findCallCluster,
  flattenPayload,
  fetchMcpTraceChunk,
  isTraceLogLine,
  MCP_TRACE_EVENTS_URL,
  MCP_TRACE_PATH_URL,
  parseTraceEntry,
  prettyPayload,
  reformatTraceEntry,
  setTraceTimeZone,
  traceEntriesFromLines,
  type TraceEntry,
  type TraceKind,
} from '../lib/mcpTrace';
import { fetchShipConfig } from '../shipApi';
import {
  computeMcpTraceStats,
  filterTraceEntries,
  MCP_TRACE_ACTION,
  MCP_TRACE_FILTER,
  publishMcpTraceStats,
  TRACE_KIND_ORDER,
  type McpTraceFilterDetail,
} from '../lib/mcpTraceEvents';
import {
  emptyQualitySnapshot,
  fetchMcpQuality,
  MCP_QUALITY_FINDING,
  openMcpQualitySlideout,
  type QualitySnapshot,
} from '../lib/mcpQuality';
import { McpQualityStrip } from './McpQualitySlideout';
import { detectStructuredLang, highlightStructured } from '../lib/mcpSyntax';
import { navigateRoute } from '../lib/routes';

const KIND_LABELS: Record<TraceKind, string> = {
  inbound: 'Inbound',
  outbound: 'Outbound',
  preview: 'Preview',
  error: 'Error',
  internal: 'Internal',
  enrich: 'Enrich',
  plugin: 'Plugin',
  lsp: 'LSP',
  ship: 'Ship',
  share: 'Share',
  workspace: 'Workspace',
  embed: 'Embed',
  action: 'Action',
  other: 'Other',
};

type Props = {
  verboseEnabled: boolean;
  variant?: 'page' | 'embedded';
};

function SummaryCell({ text }: { text: string }) {
  if (!text || text === '—') {
    return <span className="mcp-trace-msg">—</span>;
  }
  // "key=value · key=value" summaries from JSON flatten
  const chunks = text.split(/( · )/);
  return (
    <span className="mcp-trace-msg mcp-trace-msg--rich">
      {chunks.map((chunk, i) => {
        if (chunk === ' · ') {
          return (
            <span key={i} className="mcp-sum-sep">
              {chunk}
            </span>
          );
        }
        const eq = chunk.indexOf('=');
        if (eq > 0 && eq < chunk.length - 1) {
          return (
            <span key={i}>
              <span className="mcp-sum-key">{chunk.slice(0, eq)}</span>
              <span className="mcp-sum-eq">=</span>
              <span className="mcp-sum-val">{chunk.slice(eq + 1)}</span>
            </span>
          );
        }
        return (
          <span key={i} className="mcp-sum-val">
            {chunk}
          </span>
        );
      })}
    </span>
  );
}

function FieldValue({ value }: { value: string }) {
  const kind = useMemo(() => classifyFieldValue(value), [value]);
  const structured = useMemo(() => {
    if (kind !== 'json' && kind !== 'xml') return null;
    return highlightStructured(value);
  }, [kind, value]);

  if (structured && (kind === 'json' || kind === 'xml')) {
    return (
      <pre
        className="mcp-kv-block mcp-syn mcp-syn--vscode"
        dangerouslySetInnerHTML={{ __html: structured.html }}
      />
    );
  }

  const multiline = value.includes('\n') || value.length > 120;
  if (multiline) {
    return <pre className={`mcp-kv-block mcp-kv-val mcp-kv-val--${kind}`}>{value}</pre>;
  }

  return <span className={`mcp-kv-val mcp-kv-val--${kind}`}>{value || '—'}</span>;
}

function FieldsTable({
  fields,
  title = 'Fields',
}: {
  fields: { key: string; value: string }[];
  title?: string;
}) {
  if (fields.length === 0) {
    return (
      <section className="mcp-inspect-section mcp-inspect-section--tight" aria-label={title}>
        <h3 className="mcp-inspect-section-title">{title}</h3>
        <p className="mcp-inspect-empty">No structured fields on this line.</p>
      </section>
    );
  }

  return (
    <section
      className="mcp-inspect-section mcp-inspect-section--tight mcp-inspect-section--fields"
      aria-label={title}
    >
      <h3 className="mcp-inspect-section-title">
        {title}
        <span className="mcp-inspect-section-count">{fields.length}</span>
      </h3>
      <div className="mcp-kv-grid" role="table" aria-label={title}>
        {fields.map((f) => (
          <div key={`${f.key}-${f.value.slice(0, 48)}`} className="mcp-kv-row" role="row">
            <div className="mcp-kv-key" role="rowheader">
              {f.key}
            </div>
            <div className="mcp-kv-eq" aria-hidden="true">
              =
            </div>
            <div className="mcp-kv-value" role="cell">
              <FieldValue value={f.value} />
            </div>
          </div>
        ))}
      </div>
    </section>
  );
}

function MessageBlock({ entry }: { entry: TraceEntry }) {
  // Prefer full structured highlight when the message body is JSON/XML-ish.
  const structured = useMemo(() => {
    const m = entry.message.match(/\b(?:args|text|message)=(.+)$/);
    if (!m) return null;
    const body = m[1].replace(/⏎/g, '\n').replace(/↵/g, '\n');
    const lang = detectStructuredLang(body);
    if (!lang) return null;
    return highlightStructured(body);
  }, [entry.message]);

  if (structured) {
    return (
      <pre
        className="mcp-inspect-message mcp-syn mcp-syn--vscode"
        dangerouslySetInnerHTML={{ __html: structured.html }}
      />
    );
  }

  return (
    <pre className={`mcp-inspect-message mcp-trace-msg--${entry.kind}`}>
      {colorizeTraceMessage(entry.message, entry.tool).map((part, i) => (
        <span key={i} className={`mcp-tok mcp-tok--${part.type}`}>
          {part.text}
        </span>
      ))}
    </pre>
  );
}

function TraceFields({ message }: { message: string }) {
  const fields = useMemo(() => extractFields(message), [message]);
  return <FieldsTable fields={fields} />;
}

function PayloadView({ entry }: { entry: TraceEntry }) {
  const payload = useMemo(() => extractPayloadJson(entry), [entry]);
  const flat = useMemo(() => (payload !== null ? flattenPayload(payload) : []), [payload]);
  const pretty = useMemo(() => {
    if (payload !== null) return prettyPayload(payload);
    const msgPayload = entry.message.match(/\b(?:args|text|message)=(.+)$/)?.[1];
    if (!msgPayload) return '';
    return msgPayload.replace(/⏎/g, '\n').replace(/↵/g, '\n');
  }, [payload, entry.message]);
  const syn = useMemo(() => (pretty ? highlightStructured(pretty) : null), [pretty]);

  if (!syn || !pretty) return null;

  return (
    <section
      className="mcp-inspect-section mcp-inspect-section--tight mcp-inspect-section--payload"
      aria-label="Payload"
    >
      <h3 className="mcp-inspect-section-title">
        Payload{syn.lang ? ` · ${syn.lang.toUpperCase()}` : ''}
      </h3>
      <pre
        className="mcp-inspect-payload-json mcp-syn mcp-syn--vscode"
        dangerouslySetInnerHTML={{ __html: syn.html }}
      />
      {flat.length > 0 && (
        <details className="mcp-inspect-fold" open={flat.length <= 8}>
          <summary>Fields ({flat.length})</summary>
          <div className="mcp-kv-grid mcp-kv-grid--nested" role="table" aria-label="Payload fields">
            {flat.map((f) => (
              <div key={`${f.key}-${f.value.slice(0, 48)}`} className="mcp-kv-row" role="row">
                <div className="mcp-kv-key" role="rowheader">
                  {f.key}
                </div>
                <div className="mcp-kv-eq" aria-hidden="true">
                  =
                </div>
                <div className="mcp-kv-value" role="cell">
                  <FieldValue value={f.value} />
                </div>
              </div>
            ))}
          </div>
        </details>
      )}
    </section>
  );
}

function entryHasStructuredPayload(entry: TraceEntry): boolean {
  if (extractPayloadJson(entry) !== null) return true;
  const m = entry.message.match(/\b(?:args|text|message)=(.+)$/)?.[1];
  if (!m) return false;
  return detectStructuredLang(m.replace(/⏎/g, '\n').replace(/↵/g, '\n')) !== null;
}


async function enterBrowserFullscreen(el: HTMLElement | null) {
  if (!el) return;
  try {
    if (!document.fullscreenElement) {
      await el.requestFullscreen();
    }
  } catch {
    // Browser may block without a direct gesture; CSS maximize still applies.
  }
}

async function exitBrowserFullscreen() {
  try {
    if (document.fullscreenElement) {
      await document.exitFullscreen();
    }
  } catch {
    // ignore
  }
}

/**
 * Live SSE view of daily `<project>/.ax/mcp-verbose-YYYY-MM-DD.log`.
 * Newest events render at the top; scroll down for older days.
 * Table layout; tap a row for the compact Call Inspector.
 */
export default function McpTraceLive({ verboseEnabled, variant = 'embedded' }: Props) {
  const [entries, setEntries] = useState<TraceEntry[]>([]);
  const [live, setLive] = useState(false);
  /** When true, keep the viewport pinned to the newest rows (top). */
  const [follow, setFollow] = useState(true);
  const isPage = variant === 'page';
  const [maximized, setMaximized] = useState(isPage);
  const [browserFs, setBrowserFs] = useState(false);
  const [selectedId, setSelectedId] = useState<string | null>(null);
  const [cursorId, setCursorId] = useState<string | null>(null);
  const [path, setPath] = useState('<project>/.ax/mcp-verbose-YYYY-MM-DD.log');
  const [logDay, setLogDay] = useState('');
  const [oldestLoadedDay, setOldestLoadedDay] = useState('');
  const [historyExhausted, setHistoryExhausted] = useState(false);
  const [loadingHistory, setLoadingHistory] = useState(false);
  const [projectLabel, setProjectLabel] = useState('…');
  const [projectRoot, setProjectRoot] = useState('');
  const [err, setErr] = useState<string | null>(null);
  const [copied, setCopied] = useState(false);
  const [quality, setQuality] = useState<QualitySnapshot>(emptyQualitySnapshot);
  const [kindFilter, setKindFilter] = useState<Set<TraceKind>>(() => {
    const fromUrl = new URLSearchParams(window.location.search).get('kind');
    const valid: TraceKind[] = [
      'inbound',
      'enrich',
      'internal',
      'outbound',
      'preview',
      'error',
      'plugin',
      'lsp',
      'ship',
      'share',
      'workspace',
      'embed',
      'action',
      'other',
    ];
    if (fromUrl && valid.includes(fromUrl as TraceKind)) {
      return new Set([fromUrl as TraceKind]);
    }
    return new Set();
  });
  const [dateFilter, setDateFilter] = useState('');
  const [toolFilter, setToolFilter] = useState('');
  const [query, setQuery] = useState('');
  /** Keep only rows whose JSON payload has a top-level `query` property. */
  const [hasQueryFilter, setHasQueryFilter] = useState(false);
  /** On phone, filters start collapsed so the log table owns the viewport. */
  const [filtersOpen, setFiltersOpen] = useState(() =>
    typeof window !== 'undefined' ? !window.matchMedia('(max-width: 899px)').matches : true,
  );
  const scrollerRef = useRef<HTMLDivElement>(null);
  /** Intersection target at the bottom — load older history when reached. */
  const historySentinelRef = useRef<HTMLDivElement>(null);
  const shellRef = useRef<HTMLDivElement>(null);
  const maxHostRef = useRef<HTMLDivElement>(null);
  const followRef = useRef(true);
  followRef.current = follow;

  function applyProjectMeta(d: {
    path?: string;
    projectRoot?: string;
    projectLabel?: string;
    logDay?: string;
  }) {
    if (d.path) setPath(d.path);
    if (d.logDay) {
      setLogDay(d.logDay);
      setOldestLoadedDay(d.logDay);
      setHistoryExhausted(false);
    }
    if (d.projectRoot) setProjectRoot(d.projectRoot);
    if (d.projectLabel) setProjectLabel(d.projectLabel);
    else if (d.projectRoot) {
      const parts = d.projectRoot.replace(/[\\/]+$/, '').split(/[\\/]/);
      setProjectLabel(parts[parts.length - 1] || d.projectRoot);
    }
  }

  const visibleEntries = useMemo(
    () =>
      filterTraceEntries(entries, {
        kinds: kindFilter,
        tool: toolFilter,
        q: query,
        date: dateFilter,
        hasQuery: hasQueryFilter,
      }),
    [entries, kindFilter, toolFilter, query, dateFilter, hasQueryFilter],
  );
  /** Newest first for the table / keyboard navigation. `entries` stays chronological. */
  const displayEntries = useMemo(
    () => [...visibleEntries].reverse(),
    [visibleEntries],
  );
  const filtersActive =
    kindFilter.size > 0 ||
    dateFilter.trim().length > 0 ||
    toolFilter.trim().length > 0 ||
    query.trim().length > 0 ||
    hasQueryFilter;

  useEffect(() => {
    const mq = window.matchMedia('(max-width: 899px)');
    function onChange() {
      // Desktop/tablet wide: always show filters. Narrow: leave user toggle as-is
      // when entering mobile; expand when leaving mobile.
      if (!mq.matches) setFiltersOpen(true);
    }
    mq.addEventListener('change', onChange);
    return () => mq.removeEventListener('change', onChange);
  }, []);

  const queryPayloadCount = useMemo(
    () => entries.reduce((n, e) => n + (entryHasQueryPayload(e) ? 1 : 0), 0),
    [entries],
  );

  const dateOptions = useMemo(() => {
    const set = new Set<string>();
    for (const e of entries) {
      if (e.day) set.add(e.day);
    }
    return [...set].sort((a, b) => b.localeCompare(a));
  }, [entries]);

  const toolOptions = useMemo(() => {
    const set = new Set<string>();
    for (const e of entries) {
      if (e.tool) set.add(e.tool);
    }
    return [...set].sort((a, b) => a.localeCompare(b));
  }, [entries]);

  const kindCounts = useMemo(() => {
    const counts: Record<TraceKind, number> = {
      inbound: 0,
      outbound: 0,
      preview: 0,
      error: 0,
      internal: 0,
      enrich: 0,
      plugin: 0,
      lsp: 0,
      ship: 0,
      share: 0,
      workspace: 0,
      embed: 0,
      action: 0,
      other: 0,
    };
    for (const e of entries) counts[e.kind] += 1;
    return counts;
  }, [entries]);

  const selectedIndex = useMemo(
    () => (selectedId ? entries.findIndex((e) => e.id === selectedId) : -1),
    [entries, selectedId],
  );
  const cursorIndex = useMemo(
    () => (cursorId ? displayEntries.findIndex((e) => e.id === cursorId) : -1),
    [displayEntries, cursorId],
  );
  const selected = selectedIndex >= 0 ? entries[selectedIndex] : null;
  const cluster = useMemo(
    () => (selectedIndex >= 0 ? findCallCluster(entries, selectedIndex) : null),
    [entries, selectedIndex],
  );
  const stats = useMemo(
    () =>
      computeMcpTraceStats(entries, {
        live,
        projectLabel,
        projectRoot,
        path,
      }),
    [entries, live, projectLabel, projectRoot, path],
  );

  function toggleKind(kind: TraceKind) {
    setKindFilter((prev) => {
      const next = new Set(prev);
      if (next.has(kind)) next.delete(kind);
      else next.add(kind);
      return next;
    });
  }

  function clearFilters() {
    setKindFilter(new Set());
    setDateFilter('');
    setToolFilter('');
    setQuery('');
    setHasQueryFilter(false);
  }

  useEffect(() => {
    if (!isPage) return;
    publishMcpTraceStats(stats);
  }, [isPage, stats]);

  useEffect(() => {
    function onFilter(ev: Event) {
      const detail = (ev as CustomEvent<McpTraceFilterDetail>).detail;
      if (!detail) return;
      if (detail.clear) {
        setKindFilter(new Set());
        setDateFilter('');
        setToolFilter('');
        setQuery('');
        setHasQueryFilter(false);
        return;
      }
      if (detail.kinds) {
        setKindFilter(new Set(detail.kinds));
      }
      if (detail.toggleKind) {
        const kind = detail.toggleKind;
        setKindFilter((prev) => {
          const next = new Set(prev);
          if (next.has(kind)) next.delete(kind);
          else next.add(kind);
          return next;
        });
      }
      if (typeof detail.date === 'string') setDateFilter(detail.date);
      if (typeof detail.tool === 'string') setToolFilter(detail.tool);
      if (typeof detail.q === 'string') setQuery(detail.q);
      if (typeof detail.hasQuery === 'boolean') setHasQueryFilter(detail.hasQuery);
    }
    window.addEventListener(MCP_TRACE_FILTER, onFilter);
    return () => window.removeEventListener(MCP_TRACE_FILTER, onFilter);
  }, []);

  useEffect(() => {
    function onAction(ev: Event) {
      const detail = (ev as CustomEvent<{ jumpToNew?: boolean }>).detail;
      if (detail?.jumpToNew) jumpToNew();
    }
    window.addEventListener(MCP_TRACE_ACTION, onAction);
    return () => window.removeEventListener(MCP_TRACE_ACTION, onAction);
  }, []);

  useEffect(() => {
    if (visibleEntries.length === 0) {
      setCursorId(null);
      return;
    }
    if (cursorId && visibleEntries.some((e) => e.id === cursorId)) return;
    setCursorId(visibleEntries[visibleEntries.length - 1].id);
  }, [visibleEntries, cursorId]);

  useLayoutEffect(() => {
    if (!cursorId || selectedId) return;
    const row = scrollerRef.current?.querySelector<HTMLElement>(
      `[data-entry-id="${CSS.escape(cursorId)}"]`,
    );
    row?.scrollIntoView({ block: 'nearest' });
  }, [cursorId, selectedId]);

  useEffect(() => {
    if (!isPage) return;
    // Desktop: focus the listbox for keyboard nav. Mobile: skip — focusing
    // on open steals taps and can surface the soft keyboard on some browsers.
    if (window.matchMedia('(max-width: 899px)').matches) return;
    scrollerRef.current?.focus({ preventScroll: true });
  }, [isPage, maximized]);

  useEffect(() => {
    let cancelled = false;
    fetch(MCP_TRACE_PATH_URL)
      .then((r) => r.json())
      .then((d: {
        path?: string;
        projectRoot?: string;
        projectLabel?: string;
        logDay?: string;
      }) => {
        if (!cancelled) applyProjectMeta(d);
      })
      .catch(() => {});
    return () => {
      cancelled = true;
    };
  }, []);

  // Logging Date/time follows Settings → Interface → Timezone (default: browser local).
  useEffect(() => {
    let cancelled = false;

    function applyTimezone(configured?: string | null) {
      setTraceTimeZone(configured);
      setEntries((prev) => prev.map((e) => reformatTraceEntry(e)));
    }

    fetchShipConfig()
      .then((d) => {
        if (!cancelled) applyTimezone(d.config.ui?.timezone);
      })
      .catch(() => {
        if (!cancelled) applyTimezone('');
      });

    function onConfig(ev: Event) {
      const detail = (ev as CustomEvent<{ timezone?: string }>).detail;
      if (detail && typeof detail.timezone === 'string') {
        applyTimezone(detail.timezone);
        return;
      }
      fetchShipConfig()
        .then((d) => {
          if (!cancelled) applyTimezone(d.config.ui?.timezone);
        })
        .catch(() => {});
    }

    window.addEventListener('ax-ship-config-updated', onConfig);
    return () => {
      cancelled = true;
      window.removeEventListener('ax-ship-config-updated', onConfig);
    };
  }, []);

  useEffect(() => {
    function onFsChange() {
      setBrowserFs(Boolean(document.fullscreenElement));
      if (!document.fullscreenElement && isPage) {
        setMaximized(true);
      }
    }
    document.addEventListener('fullscreenchange', onFsChange);
    return () => document.removeEventListener('fullscreenchange', onFsChange);
  }, [isPage]);

  useEffect(() => {
    if (!isPage) return;
    setMaximized(true);
    let cancelled = false;
    const tryFs = () => {
      if (cancelled) return;
      // Fullscreen the whole app so the status bar (log stats) stays visible.
      void enterBrowserFullscreen(document.documentElement);
    };
    const t = window.setTimeout(tryFs, 0);
    return () => {
      cancelled = true;
      window.clearTimeout(t);
      void exitBrowserFullscreen();
    };
  }, [isPage]);

  useEffect(() => {
    let es: EventSource | null = null;
    let disposed = false;
    let reconnectTimer: ReturnType<typeof setTimeout> | null = null;

    function ingestLines(lines: string[]) {
      const batch = traceEntriesFromLines(lines);
      if (batch.length === 0) return;
      setEntries((prev) => [...prev, ...batch]);
    }

    function connect() {
      if (disposed) return;
      es = new EventSource(MCP_TRACE_EVENTS_URL);
      setLive(true);
      setErr(null);

      es.addEventListener('line', (ev) => {
        const data = ((ev as MessageEvent).data as string) ?? '';
        ingestLines([data]);
      });

      es.addEventListener('batch', (ev) => {
        const raw = ((ev as MessageEvent).data as string) ?? '[]';
        try {
          const lines = JSON.parse(raw) as string[];
          if (Array.isArray(lines)) ingestLines(lines);
        } catch {
          // ignore malformed batch
        }
      });

      es.addEventListener('path', (ev) => {
        const p = ((ev as MessageEvent).data as string) ?? '';
        if (p) setPath(p);
      });

      es.addEventListener('project', (ev) => {
        const raw = ((ev as MessageEvent).data as string) ?? '';
        if (!raw) return;
        try {
          applyProjectMeta(
            JSON.parse(raw) as {
              path?: string;
              projectRoot?: string;
              projectLabel?: string;
              logDay?: string;
            },
          );
        } catch {
          // ignore malformed project events
        }
      });

      es.addEventListener('reset', () => {
        setEntries([]);
        setSelectedId(null);
        setCursorId(null);
        setHistoryExhausted(false);
        setOldestLoadedDay('');
      });

      es.addEventListener('rotate', () => {
        // Day rolled over: keep buffer; tail follows the new file via server.
      });

      es.addEventListener('ready', () => {
        setLive(true);
      });

      es.onerror = () => {
        setLive(false);
        es?.close();
        es = null;
        if (!disposed) {
          setErr('Reconnecting…');
          reconnectTimer = setTimeout(connect, 1500);
        }
      };
    }

    connect();
    return () => {
      disposed = true;
      if (reconnectTimer) clearTimeout(reconnectTimer);
      es?.close();
      setLive(false);
    };
  }, []);

  useEffect(() => {
    let cancelled = false;
    function load() {
      fetchMcpQuality()
        .then((s) => {
          if (!cancelled) setQuality(s);
        })
        .catch(() => {});
    }
    load();
    const id = window.setInterval(load, 8_000);
    return () => {
      cancelled = true;
      clearInterval(id);
    };
  }, []);

  useEffect(() => {
    function onFinding(ev: Event) {
      const detail = (ev as CustomEvent<{ tool?: string | null }>).detail;
      const tool = detail?.tool;
      if (!tool) return;
      setEntries((prev) => {
        const hit = [...prev].reverse().find((e) => e.tool === tool);
        if (hit) {
          setSelectedId(hit.id);
          setCursorId(hit.id);
        }
        return prev;
      });
    }
    window.addEventListener(MCP_QUALITY_FINDING, onFinding);
    return () => window.removeEventListener(MCP_QUALITY_FINDING, onFinding);
  }, []);

  useEffect(() => {
    if (logDay && !oldestLoadedDay) setOldestLoadedDay(logDay);
  }, [logDay, oldestLoadedDay]);

  const loadOlderDayRef = useRef<() => void>(() => {});
  loadOlderDayRef.current = () => {
    if (loadingHistory || historyExhausted || !oldestLoadedDay) return;
    setLoadingHistory(true);
    // Server walks back past gaps (a day with no verbose activity, or a
    // history-mangling bug) to find the nearest real dated file before
    // `oldestLoadedDay` — it may jump back several calendar days at once.
    void fetchMcpTraceChunk(oldestLoadedDay)
      .then((res) => {
        if (!res.ok) return;
        if (!res.day) {
          setHistoryExhausted(true);
          return;
        }
        const older = res.lines?.length ? traceEntriesFromLines(res.lines) : [];
        if (older.length > 0) {
          // Older rows append at the visual bottom (newest-first). Viewport
          // stays put; keep follow so first paint / live top remain pinned.
          setEntries((e) => [...older, ...e]);
        }
        setOldestLoadedDay(res.day);
        if (!res.hasOlder) setHistoryExhausted(true);
      })
      .catch(() => {})
      .finally(() => setLoadingHistory(false));
  };

  // Today's dated log file is often empty (nothing run yet today, or the
  // daemon just rotated). Don't leave the page looking broken — pull in
  // whatever history exists automatically, same as scrolling down would.
  useEffect(() => {
    if (!live) return;
    if (entries.length > 0) return;
    if (loadingHistory || historyExhausted || !oldestLoadedDay) return;
    loadOlderDayRef.current();
  }, [live, entries.length, loadingHistory, historyExhausted, oldestLoadedDay]);

  useEffect(() => {
    const root = scrollerRef.current;
    const sentinel = historySentinelRef.current;
    if (!root || !sentinel) return;
    const obs = new IntersectionObserver(
      (entries) => {
        if (entries.some((e) => e.isIntersecting)) loadOlderDayRef.current();
      },
      { root, rootMargin: '120px', threshold: 0 },
    );
    obs.observe(sentinel);
    return () => obs.disconnect();
  }, [maximized, isPage, entries.length]);

  useLayoutEffect(() => {
    if (!followRef.current) return;
    const el = scrollerRef.current;
    if (!el) return;
    el.scrollTop = 0;
  }, [entries, maximized]);

  useEffect(() => {
    function onKey(e: KeyboardEvent) {
      const target = e.target as HTMLElement | null;
      const tag = target?.tagName;
      if (
        tag === 'INPUT' ||
        tag === 'TEXTAREA' ||
        tag === 'SELECT' ||
        target?.isContentEditable
      ) {
        return;
      }

      // Inspector open: navigate events / close
      if (selectedId) {
        if (e.key === 'Escape') {
          e.preventDefault();
          setSelectedId(null);
          return;
        }
        if (
          e.key === 'ArrowLeft' ||
          e.key === 'ArrowUp' ||
          e.key === 'k' ||
          e.key === 'K'
        ) {
          e.preventDefault();
          selectNeighbor(-1);
          return;
        }
        if (
          e.key === 'ArrowRight' ||
          e.key === 'ArrowDown' ||
          e.key === 'j' ||
          e.key === 'J'
        ) {
          e.preventDefault();
          selectNeighbor(1);
          return;
        }
        if (e.key === 'Home') {
          e.preventDefault();
          if (displayEntries.length) {
            const id = displayEntries[0].id;
            setSelectedId(id);
            setCursorId(id);
          }
          return;
        }
        if (e.key === 'End') {
          e.preventDefault();
          if (displayEntries.length) {
            const id = displayEntries[displayEntries.length - 1].id;
            setSelectedId(id);
            setCursorId(id);
          }
          return;
        }
        return;
      }

      // Table browsing (display order: newest at top)
      if (e.key === 'ArrowDown' || e.key === 'j' || e.key === 'J') {
        e.preventDefault();
        moveCursor(1);
        return;
      }
      if (e.key === 'ArrowUp' || e.key === 'k' || e.key === 'K') {
        e.preventDefault();
        moveCursor(-1);
        return;
      }
      if (e.key === 'PageDown') {
        e.preventDefault();
        moveCursor(10);
        return;
      }
      if (e.key === 'PageUp') {
        e.preventDefault();
        moveCursor(-10);
        return;
      }
      if (e.key === 'Home') {
        e.preventDefault();
        if (displayEntries.length) setCursorId(displayEntries[0].id);
        return;
      }
      if (e.key === 'End') {
        e.preventDefault();
        if (displayEntries.length) setCursorId(displayEntries[displayEntries.length - 1].id);
        return;
      }
      if (e.key === 'Enter' || e.key === ' ') {
        e.preventDefault();
        const id = cursorId ?? (displayEntries.length ? displayEntries[0].id : null);
        if (id) {
          setCursorId(id);
          setSelectedId(id);
        }
        return;
      }
      if (e.key === 'Escape') {
        if (document.fullscreenElement) {
          e.preventDefault();
          void exitBrowserFullscreen();
          return;
        }
        if (maximized && !isPage) {
          e.preventDefault();
          setMaximized(false);
        }
        return;
      }
      if (isPage && (e.key === 'b' || e.key === 'B') && !e.metaKey && !e.ctrlKey && !e.altKey) {
        e.preventDefault();
        void backToApp();
      }
    }

    window.addEventListener('keydown', onKey);
    const lockScroll = maximized || Boolean(selectedId);
    if (lockScroll) document.body.style.overflow = 'hidden';
    return () => {
      document.body.style.overflow = '';
      window.removeEventListener('keydown', onKey);
    };
  }, [
    maximized,
    selectedId,
    cursorId,
    isPage,
    entries,
    visibleEntries,
    displayEntries,
  ]);

  function onScroll() {
    const el = scrollerRef.current;
    if (!el) return;
    const atTop = el.scrollTop < 48;
    setFollow(atTop);
  }

  function jumpToNew() {
    setFollow(true);
    const el = scrollerRef.current;
    if (el) el.scrollTop = 0;
  }

  async function copySelected() {
    if (!selected) return;
    try {
      await navigator.clipboard.writeText(selected.raw);
      setCopied(true);
      window.setTimeout(() => setCopied(false), 1200);
    } catch {
      setErr('Copy failed');
    }
  }

  function moveCursor(delta: number) {
    if (displayEntries.length === 0) return;
    const idx = cursorIndex >= 0 ? cursorIndex : 0;
    const next = Math.max(0, Math.min(displayEntries.length - 1, idx + delta));
    setCursorId(displayEntries[next].id);
    setFollow(false);
  }

  function selectNeighbor(delta: number) {
    if (selectedIndex < 0) return;
    // Prefer navigating within the displayed (filtered, newest-first) list.
    const visIdx = displayEntries.findIndex((e) => e.id === selectedId);
    if (visIdx >= 0) {
      const next = visIdx + delta;
      if (next < 0 || next >= displayEntries.length) return;
      const id = displayEntries[next].id;
      setSelectedId(id);
      setCursorId(id);
      return;
    }
    // Fallback: chronological neighbors in the full buffer.
    const next = selectedIndex + delta;
    if (next < 0 || next >= entries.length) return;
    const id = entries[next].id;
    setSelectedId(id);
    setCursorId(id);
  }

  function openEntry(id: string) {
    setCursorId(id);
    setSelectedId(id);
  }

  async function toggleMaximize() {
    if (maximized && (browserFs || document.fullscreenElement)) {
      await exitBrowserFullscreen();
      if (!isPage) setMaximized(false);
      return;
    }
    setMaximized(true);
    // Page mode: keep status bar in the fullscreen tree. Embedded: host overlay only.
    const host = isPage
      ? document.documentElement
      : maxHostRef.current ?? shellRef.current ?? document.documentElement;
    await enterBrowserFullscreen(host);
  }

  async function backToApp() {
    setSelectedId(null);
    await exitBrowserFullscreen();
    setMaximized(false);
    navigateRoute({ page: 'stats' });
  }

  const panel = (
    <div
      ref={shellRef}
      className={`mcp-trace-shell${maximized ? ' mcp-trace-shell--max' : ''}${
        isPage ? ' mcp-trace-shell--page' : ''
      }${!live ? ' mcp-trace-shell--offline' : ''}${
        filtersOpen ? '' : ' mcp-trace-shell--filters-collapsed'
      }`}
      aria-busy={!live || undefined}
    >
      <div className="mcp-trace-chrome">
        <div className="mcp-trace-toolbar">
          <div className="mcp-trace-toolbar-left">
            <span className="mcp-trace-title">Trace</span>
            <span className="mcp-trace-project-chip" title={projectRoot || path}>
              <span className="mcp-trace-project-chip-label">Project</span>
              <strong className="mcp-trace-project-chip-name">{projectLabel}</strong>
            </span>
            {live ? (
              <span className="settings-log-live">live</span>
            ) : (
              <span className="settings-log-live settings-log-live--err">offline</span>
            )}
            <span className="mcp-trace-count" title="Visible / total events in buffer">
              {filtersActive
                ? `${visibleEntries.length.toLocaleString()} / ${entries.length.toLocaleString()}`
                : entries.length.toLocaleString()}
            </span>
            <span className="mcp-trace-keys" title="Keyboard: ↑↓ move · Enter open · Esc close · j/k · b back">
              ↑↓ Enter Esc
            </span>
          </div>
          <div className="mcp-trace-header-actions">
            {isPage && (
              <button
                type="button"
                className="btn btn-compact mcp-trace-icon-btn mcp-trace-back-btn"
                title="Back to Command Center"
                aria-label="Back to Command Center"
                onClick={() => void backToApp()}
              >
                <Codicon name="arrow-left" />
                <span className="mcp-trace-btn-label">Back</span>
              </button>
            )}
            <button
              type="button"
              className={`btn btn-compact mcp-trace-icon-btn mcp-trace-filters-toggle${
                filtersOpen ? ' mcp-trace-icon-btn--active' : ''
              }${filtersActive ? ' mcp-trace-filters-toggle--lit' : ''}`}
              title={filtersOpen ? 'Hide filters' : 'Show filters'}
              aria-label={filtersOpen ? 'Hide filters' : 'Show filters'}
              aria-expanded={filtersOpen}
              onClick={() => setFiltersOpen((v) => !v)}
            >
              <Codicon name="filter" />
              <span className="mcp-trace-btn-label">Filters</span>
            </button>
            {isPage && (
              <button
                type="button"
                className="btn btn-compact mcp-trace-icon-btn"
                title="MCP quality — Copy fixpack & metrics"
                aria-label="Open MCP quality"
                onClick={() => openMcpQualitySlideout()}
              >
                <Codicon name="shield" />
                <span className="mcp-trace-btn-label">Quality</span>
              </button>
            )}
            <button
              type="button"
              className={`btn btn-compact mcp-trace-icon-btn${!follow ? ' mcp-trace-icon-btn--active' : ''}`}
              title={
                follow
                  ? 'Pinned to newest (click to pause)'
                  : 'Scroll to new'
              }
              aria-label={follow ? 'Pinned to newest' : 'Scroll to new'}
              aria-pressed={follow}
              onClick={() => {
                if (follow) setFollow(false);
                else jumpToNew();
              }}
            >
              <Codicon name="arrow-up" />
              <span className="mcp-trace-btn-label">{follow ? 'Newest' : 'To new'}</span>
            </button>
            <button
              type="button"
              className="btn btn-compact mcp-trace-icon-btn mcp-trace-full-btn"
              title={
                browserFs || document.fullscreenElement
                  ? 'Exit fullscreen (Esc)'
                  : 'Fullscreen (browser)'
              }
              aria-label={
                browserFs || document.fullscreenElement ? 'Exit fullscreen' : 'Enter fullscreen'
              }
              aria-pressed={maximized || browserFs}
              onClick={() => void toggleMaximize()}
            >
              <Codicon name={browserFs || document.fullscreenElement ? 'screen-normal' : 'screen-full'} />
              <span className="mcp-trace-btn-label">
                {browserFs || document.fullscreenElement ? 'Exit' : 'Full'}
              </span>
            </button>
          </div>
        </div>

        {!verboseEnabled && (
          <div className="settings-toast settings-toast--ok mcp-trace-hint">
            Enable <strong>Verbose MCP logging</strong> in Settings to record new tool calls. History
            below still tails the project log file.
          </div>
        )}

        {isPage && <McpQualityStrip snap={quality} />}
      </div>
      {filtersOpen && (
      <div className="mcp-trace-project-banner" title={projectRoot || undefined}>
        <div className="mcp-trace-project-banner-main">
          <span className="mcp-trace-project-banner-kicker">Viewing MCP log for</span>
          <span className="mcp-trace-project-banner-name">{projectLabel}</span>
          {isPage && (
            <LoggingProjectSwitch
              currentPath={projectRoot}
              currentLabel={projectLabel}
              variant="banner"
            />
          )}
        </div>
        <div className="mcp-trace-project-banner-meta">
          <span className="mcp-trace-project-banner-root">{projectRoot || '—'}</span>
          <span className="mcp-trace-project-banner-sep">·</span>
          <code className="mcp-trace-project-banner-file">{path}</code>
        </div>
      </div>
      )}

      {filtersOpen && (
      <div className="mcp-trace-filters" role="search" aria-label="Filter MCP log">
        <div className="mcp-trace-filter-kinds" role="group" aria-label="Filter by kind">
          {TRACE_KIND_ORDER.map((kind) => {
            const count = kindCounts[kind];
            const active = kindFilter.has(kind);
            if (count === 0 && !active) return null;
            return (
              <button
                key={kind}
                type="button"
                className={`mcp-trace-filter-chip mcp-trace-filter-chip--${kind}${
                  active ? ' mcp-trace-filter-chip--on' : ''
                }`}
                aria-pressed={active}
                title={`${active ? 'Hide' : 'Show only'} ${KIND_LABELS[kind]} (toggle)`}
                onClick={() => toggleKind(kind)}
              >
                <span className={`mcp-trace-badge mcp-trace-badge--${kind}`}>
                  {kind === 'inbound'
                    ? 'IN'
                    : kind === 'outbound'
                      ? 'OUT'
                      : kind === 'preview'
                        ? 'PREV'
                        : kind === 'error'
                          ? 'ERR'
                          : kind === 'enrich'
                            ? 'ENR'
                            : kind === 'internal'
                              ? 'INT'
                              : 'LOG'}
                </span>
                <span className="mcp-trace-filter-chip-label">{KIND_LABELS[kind]}</span>
                <span className="mcp-trace-filter-chip-count">{count}</span>
              </button>
            );
          })}
          {(queryPayloadCount > 0 || hasQueryFilter) && (
            <button
              type="button"
              className={`mcp-trace-filter-chip mcp-trace-filter-chip--query${
                hasQueryFilter ? ' mcp-trace-filter-chip--on' : ''
              }`}
              aria-pressed={hasQueryFilter}
              title={
                hasQueryFilter
                  ? 'Show all events (clear query-payload filter)'
                  : 'Show only events whose JSON payload has a top-level query property'
              }
              onClick={() => setHasQueryFilter((v) => !v)}
            >
              <span className="mcp-trace-badge mcp-trace-badge--query">QRY</span>
              <span className="mcp-trace-filter-chip-label">Has query</span>
              <span className="mcp-trace-filter-chip-count">{queryPayloadCount}</span>
            </button>
          )}
        </div>
        <div className="mcp-trace-filter-controls">
          <label className="mcp-trace-filter-field">
            <span className="mcp-trace-filter-field-label">Date</span>
            <select
              className="mcp-trace-filter-select"
              value={dateFilter}
              onChange={(e) => setDateFilter(e.target.value)}
              aria-label="Filter by date"
            >
              <option value="">All dates</option>
              {dateOptions.map((d) => (
                <option key={d} value={d}>
                  {d}
                </option>
              ))}
            </select>
          </label>
          <label className="mcp-trace-filter-field">
            <span className="mcp-trace-filter-field-label">Tool</span>
            <select
              className="mcp-trace-filter-select"
              value={toolFilter}
              onChange={(e) => setToolFilter(e.target.value)}
              aria-label="Filter by tool"
            >
              <option value="">All tools</option>
              {toolOptions.map((t) => (
                <option key={t} value={t}>
                  {t}
                </option>
              ))}
            </select>
          </label>
          <label className="mcp-trace-filter-field mcp-trace-filter-field--grow">
            <span className="mcp-trace-filter-field-label">Search</span>
            <input
              type="search"
              className="mcp-trace-filter-input"
              value={query}
              placeholder="tool, args, message…"
              onChange={(e) => setQuery(e.target.value)}
              aria-label="Search log lines"
            />
          </label>
          {filtersActive && (
            <button
              type="button"
              className="btn btn-compact mcp-trace-filter-clear"
              onClick={clearFilters}
              title="Clear all filters"
            >
              Clear filters
            </button>
          )}
        </div>
      </div>
      )}

      <div className="mcp-trace-scroller-wrap">
        {!follow && entries.length > 0 ? (
          <button
            type="button"
            className="mcp-trace-scroll-new"
            onClick={jumpToNew}
          >
            <Codicon name="arrow-up" />
            Scroll to new
          </button>
        ) : null}
      <div
        ref={scrollerRef}
        className="mcp-trace-scroller"
        onScroll={onScroll}
        role="listbox"
        tabIndex={0}
        aria-label="MCP trace log. Newest at top. Use arrow keys to move, Enter to open, Escape to close."
        aria-activedescendant={cursorId ? `mcp-row-${cursorId}` : undefined}
      >
        {entries.length === 0 ? (
          <div className="mcp-trace-empty">
            {loadingHistory ? (
              'Loading earlier history…'
            ) : historyExhausted ? (
              'Waiting for MCP tool calls… (enable verbose + reconnect ax MCP)'
            ) : (
              <>
                Nothing logged for {logDay || 'today'} yet.{' '}
                <button
                  type="button"
                  className="linkish"
                  onClick={() => loadOlderDayRef.current()}
                >
                  Show previous day
                </button>
              </>
            )}
          </div>
        ) : visibleEntries.length === 0 ? (
          <div className="mcp-trace-empty">
            No events match the current filters.{' '}
            <button type="button" className="linkish" onClick={clearFilters}>
              Clear filters
            </button>
          </div>
        ) : (
          <table className="mcp-trace-table">
            <thead>
              <tr>
                <th className="mcp-col-time">Date / time</th>
                <th className="mcp-col-kind">Kind</th>
                <th className="mcp-col-tool">Tool</th>
                <th className="mcp-col-summary">Summary</th>
                <th className="mcp-col-meta">Meta</th>
              </tr>
            </thead>
            <tbody>
              {displayEntries.map((e) => {
                const headline = entryHeadline(e);
                const meta = entryMeta(e);
                const hasQuery = entryHasQueryPayload(e);
                const isCursor = cursorId === e.id;
                const isOpen = selectedId === e.id;
                return (
                  <tr
                    key={e.id}
                    id={`mcp-row-${e.id}`}
                    data-entry-id={e.id}
                    role="option"
                    aria-selected={isCursor || isOpen}
                    className={`mcp-trace-row mcp-trace-row--${e.kind}${
                      hasQuery ? ' mcp-trace-row--has-query' : ''
                    }${isOpen ? ' mcp-trace-row--selected' : ''}${
                      isCursor && !isOpen ? ' mcp-trace-row--cursor' : ''
                    }`}
                    title={e.raw}
                    onClick={() => openEntry(e.id)}
                  >
                    <td className="mcp-col-time" title={e.raw.match(/^\S+/)?.[0] ?? e.time}>
                      {e.time}
                    </td>
                    <td className="mcp-col-kind">
                      <button
                        type="button"
                        className={`mcp-trace-badge mcp-trace-badge--${e.kind} mcp-trace-badge--btn`}
                        title={`Filter ${KIND_LABELS[e.kind]}`}
                        onClick={(ev) => {
                          ev.stopPropagation();
                          toggleKind(e.kind);
                        }}
                      >
                        {e.badge}
                      </button>
                    </td>
                    <td className="mcp-col-tool">
                      {e.tool ? (
                        <button
                          type="button"
                          className="mcp-trace-tool-btn"
                          title={`Filter tool ${e.tool}`}
                          onClick={(ev) => {
                            ev.stopPropagation();
                            setToolFilter((prev) => (prev === e.tool ? '' : e.tool ?? ''));
                          }}
                        >
                          {e.tool}
                        </button>
                      ) : (
                        '—'
                      )}
                    </td>
                    <td className="mcp-col-summary">
                      <span className="mcp-trace-summary-wrap">
                        {hasQuery && (
                          <button
                            type="button"
                            className="mcp-trace-badge mcp-trace-badge--query mcp-trace-badge--btn"
                            title="JSON payload has query — click to filter"
                            onClick={(ev) => {
                              ev.stopPropagation();
                              setHasQueryFilter(true);
                            }}
                          >
                            query
                          </button>
                        )}
                        <SummaryCell text={headline || '—'} />
                      </span>
                    </td>
                    <td className="mcp-col-meta">{meta || '—'}</td>
                  </tr>
                );
              })}
            </tbody>
          </table>
        )}
        <div ref={historySentinelRef} className="mcp-trace-history-sentinel" aria-hidden="true" />
        {loadingHistory && entries.length > 0 ? (
          <div className="mcp-trace-history-status">Loading earlier history…</div>
        ) : null}
        {historyExhausted && entries.length > 0 ? (
          <div className="mcp-trace-history-status mcp-trace-history-status--end">
            End of available history
          </div>
        ) : null}
      </div>
      </div>
      {err && <div className="settings-row-desc mcp-trace-err">{err}</div>}
    </div>
  );

  const hasStructured = selected ? entryHasStructuredPayload(selected) : false;
  const selectedFields = useMemo(
    () => (selected && !hasStructured ? extractFields(selected.message) : []),
    [selected, hasStructured],
  );
  const hasTimeline = Boolean(cluster && cluster.end > cluster.start);

  const inspector =
    selected && cluster
      ? createPortal(
          <div
            className="mcp-inspect-overlay"
            role="presentation"
            onMouseDown={() => setSelectedId(null)}
          >
            <div
              className={`mcp-inspect-sheet mcp-inspect-sheet--compact mcp-inspect-sheet--${selected.kind}`}
              role="dialog"
              aria-modal="true"
              aria-label="Call inspector"
              onMouseDown={(e) => e.stopPropagation()}
            >
              <header className="mcp-inspect-header">
                <div className="mcp-inspect-heading">
                  <span className={`mcp-trace-badge mcp-trace-badge--${selected.kind}`}>
                    {selected.badge}
                  </span>
                  {entryHasQueryPayload(selected) && (
                    <span
                      className="mcp-trace-badge mcp-trace-badge--query"
                      title="JSON payload includes a top-level query property"
                    >
                      query
                    </span>
                  )}
                  <div className="mcp-inspect-title-block">
                    <h2 className="mcp-inspect-title">
                      {cluster.tool ?? selected.tool ?? 'Log event'}
                    </h2>
                    <p className="mcp-inspect-sub">
                      {selected.time}
                      {hasTimeline ? ` · ${cluster.end - cluster.start + 1} steps` : ''}
                      {entryMeta(selected) ? ` · ${entryMeta(selected)}` : ''}
                    </p>
                  </div>
                </div>
                <div className="mcp-inspect-nav">
                  <button
                    type="button"
                    className="btn btn-compact mcp-trace-icon-btn"
                    aria-label="Previous event"
                    disabled={selectedIndex <= 0}
                    onClick={() => selectNeighbor(-1)}
                  >
                    <Codicon name="chevron-left" />
                  </button>
                  <button
                    type="button"
                    className="btn btn-compact mcp-trace-icon-btn"
                    aria-label="Next event"
                    disabled={selectedIndex >= entries.length - 1}
                    onClick={() => selectNeighbor(1)}
                  >
                    <Codicon name="chevron-right" />
                  </button>
                  <button
                    type="button"
                    className="btn btn-compact mcp-trace-icon-btn"
                    aria-label="Close inspector"
                    onClick={() => setSelectedId(null)}
                  >
                    <Codicon name="close" />
                  </button>
                </div>
              </header>

              <div
                className={`mcp-inspect-body${hasTimeline ? ' mcp-inspect-body--split' : ''}`}
              >
                {hasTimeline && (
                  <aside className="mcp-inspect-rail" aria-label="Call timeline">
                    <h3 className="mcp-inspect-section-title">Steps</h3>
                    <ol className="mcp-inspect-timeline">
                      {entries.slice(cluster.start, cluster.end + 1).map((step) => (
                        <li key={step.id}>
                          <button
                            type="button"
                            className={`mcp-inspect-step mcp-inspect-step--${step.kind}${
                              step.id === selected.id ? ' mcp-inspect-step--active' : ''
                            }`}
                            onClick={() => setSelectedId(step.id)}
                          >
                            <span className={`mcp-trace-badge mcp-trace-badge--${step.kind}`}>
                              {step.badge}
                            </span>
                            <span className="mcp-inspect-step-body">
                              <span className="mcp-inspect-step-time">{step.time}</span>
                              <span className="mcp-inspect-step-msg">{entryHeadline(step) || '—'}</span>
                            </span>
                          </button>
                        </li>
                      ))}
                    </ol>
                  </aside>
                )}

                <div className="mcp-inspect-main">
                  <PayloadView entry={selected} />
                  {!hasStructured && <TraceFields message={selected.message} />}
                  {!hasStructured && selectedFields.length === 0 && (
                    <section className="mcp-inspect-section mcp-inspect-section--tight" aria-label="Message">
                      <h3 className="mcp-inspect-section-title">Message</h3>
                      <MessageBlock entry={selected} />
                    </section>
                  )}
                  {!hasStructured && selectedFields.length > 0 && (
                    <details className="mcp-inspect-fold">
                      <summary>Message line</summary>
                      <MessageBlock entry={selected} />
                    </details>
                  )}
                  <details className="mcp-inspect-fold mcp-inspect-fold--raw">
                    <summary>
                      <span>Raw line</span>
                      <button
                        type="button"
                        className="btn btn-compact mcp-trace-icon-btn"
                        onClick={(e) => {
                          e.preventDefault();
                          e.stopPropagation();
                          void copySelected();
                        }}
                      >
                        <Codicon name="copy" />
                        <span className="mcp-trace-btn-label">{copied ? 'Copied' : 'Copy'}</span>
                      </button>
                    </summary>
                    <pre className="mcp-inspect-raw">{selected.raw}</pre>
                  </details>
                </div>
              </div>
            </div>
          </div>,
          document.body,
        )
      : null;

  if (maximized) {
    // Portal to body so fixed overlay is not trapped under
    // `.workspace { isolation: isolate; z-index: 1 }` beneath the titlebar.
    return (
      <>
        {createPortal(
          <div
            ref={maxHostRef}
            className="mcp-trace-max-overlay"
            role="dialog"
            aria-label="MCP logging maximized"
          >
            {panel}
          </div>,
          document.body,
        )}
        {inspector}
      </>
    );
  }

  return (
    <>
      <div className={`mcp-trace-card${isPage ? ' mcp-trace-card--page' : ' settings-card'}`}>
        {!isPage && (
          <div className="settings-card-header">
            <h2>MCP verbose trace</h2>
            <p>
              Newest events at the top from <code>{path}</code>. Or open the full{' '}
              <button
                type="button"
                className="linkish"
                onClick={() => navigateRoute({ page: 'logging' })}
              >
                Logging
              </button>{' '}
              page from the status bar.
            </p>
          </div>
        )}
        <div className={isPage ? 'mcp-trace-page-body' : 'settings-card-body'}>{panel}</div>
      </div>
      {inspector}
    </>
  );
}
