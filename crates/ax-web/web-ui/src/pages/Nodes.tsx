import { useEffect, useRef, useState } from 'react';
import { fetchNodes, fetchStats } from '../api';
import NodeDetailPanel from '../components/NodeDetail';
import Codicon from '../components/Codicon';
import {
  FilterBar,
  ItemList,
  ItemRow,
  PageCard,
  PageCardBody,
  PageEmpty,
  PageHero,
  PageLoading,
  PagePagination,
  PageShell,
  PageStack,
  PageToasts,
} from '../components/ui/PageLayout';
import { usePersistedNumber, usePersistedString } from '../hooks/usePersistedState';
import { usePageContext } from '../context/UiContext';
import type { NodeRow } from '../types';

const LIMIT = 50;

const KIND_OPTIONS = [
  '', 'function', 'method', 'class', 'struct', 'enum', 'trait', 'interface',
  'type', 'const', 'variable', 'module', 'file', 'doc',
];

const KIND_ICONS: Record<string, string> = {
  function: 'symbol-method',
  method: 'symbol-method',
  class: 'symbol-class',
  struct: 'symbol-structure',
  enum: 'symbol-enum',
  trait: 'symbol-interface',
  interface: 'symbol-interface',
  type: 'symbol-type-parameter',
  const: 'symbol-constant',
  variable: 'symbol-variable',
  module: 'symbol-namespace',
  file: 'file',
  doc: 'book',
};

function kindIcon(kind: string): string {
  return KIND_ICONS[kind] ?? 'symbol-misc';
}

export default function NodesPage() {
  const [nodes, setNodes] = useState<NodeRow[]>([]);
  const [total, setTotal] = useState(0);
  const [languages, setLanguages] = useState<string[]>([]);
  const [q, setQ] = usePersistedString('nodes-q', '');
  const [kind, setKind] = usePersistedString('nodes-kind', '');
  const [lang, setLang] = usePersistedString('nodes-lang', '');
  const [offset, setOffset] = usePersistedNumber('nodes-offset', 0, 0, 1_000_000);
  const [selectedId, setSelectedId] = usePersistedString('nodes-selected', '');
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const debounce = useRef<ReturnType<typeof setTimeout> | null>(null);
  const mounted = useRef(false);

  useEffect(() => {
    fetchStats()
      .then((s) => setLanguages(s.languages.map((l) => l.language).sort()))
      .catch(() => {});
  }, []);

  function load(newOffset: number, newQ: string, newKind: string, newLang: string) {
    setLoading(true);
    setError(null);
    fetchNodes({ q: newQ, kind: newKind || undefined, lang: newLang || undefined, limit: LIMIT, offset: newOffset })
      .then((page) => {
        setNodes(page.nodes);
        setTotal(page.total);
        setLoading(false);
      })
      .catch((e: Error) => { setError(e.message); setLoading(false); });
  }

  useEffect(() => {
    if (!mounted.current) {
      mounted.current = true;
      load(offset, q, kind, lang);
      return;
    }
    if (debounce.current) clearTimeout(debounce.current);
    debounce.current = setTimeout(() => {
      setOffset(0);
      load(0, q, kind, lang);
    }, q ? 300 : 0);
    return () => { if (debounce.current) clearTimeout(debounce.current); };
  }, [q, kind, lang]);

  function goPage(dir: 1 | -1) {
    const next = offset + dir * LIMIT;
    setOffset(next);
    load(next, q, kind, lang);
  }

  const page = Math.floor(offset / LIMIT) + 1;
  const pages = Math.ceil(total / LIMIT) || 1;

  const filterParts: string[] = [];
  if (q) filterParts.push(`"${q}"`);
  if (kind) filterParts.push(kind);
  if (lang) filterParts.push(lang);
  const detail = `${nodes.length.toLocaleString()} shown · ${total.toLocaleString()} total · p${page}/${pages}${filterParts.length ? ` · ${filterParts.join(' · ')}` : ''}`;
  usePageContext('Nodes', detail);

  return (
    <PageShell>
      <PageHero
        title="Nodes"
        subtitle="Browse indexed symbols. Click a row to inspect callers and callees."
      />

      <PageToasts err={error} />

      <PageStack>
        <PageCard
          title="Symbol browser"
          description={`${total.toLocaleString()} nodes in the graph.`}
          footer={
            total > LIMIT ? (
              <PagePagination
                page={page}
                pages={pages}
                onPrev={() => goPage(-1)}
                onNext={() => goPage(1)}
                prevDisabled={offset === 0}
                nextDisabled={offset + LIMIT >= total}
              />
            ) : undefined
          }
        >
          <FilterBar>
            <input
              className="settings-input settings-input--grow"
              type="search"
              placeholder="Search symbols…"
              value={q}
              onChange={(e) => setQ(e.target.value)}
            />
            <select
              className="settings-select"
              value={kind}
              onChange={(e) => setKind(e.target.value)}
              aria-label="Filter by kind"
            >
              <option value="">All kinds</option>
              {KIND_OPTIONS.filter(Boolean).map((k) => (
                <option key={k} value={k}>{k}</option>
              ))}
            </select>
            <select
              className="settings-select"
              value={lang}
              onChange={(e) => setLang(e.target.value)}
              aria-label="Filter by language"
            >
              <option value="">All languages</option>
              {languages.map((l) => (
                <option key={l} value={l}>{l}</option>
              ))}
            </select>
          </FilterBar>

          <PageCardBody>
            {loading ? (
              <PageLoading />
            ) : nodes.length === 0 ? (
              <PageEmpty title="No nodes found">Try a different search or filter.</PageEmpty>
            ) : (
              <div className={`page-split${selectedId ? ' page-split--with-detail' : ''}`}>
                <div className="page-split-main">
                  <ItemList>
                    {nodes.map((n) => (
                      <ItemRow
                        key={n.id}
                        icon={<Codicon name={kindIcon(n.kind)} className="page-item-codicon" />}
                        title={n.name}
                        subtitle={`${n.file_path}:${n.start_line}`}
                        selected={selectedId === n.id}
                        onClick={() => setSelectedId(n.id)}
                        badges={
                          <>
                            <span className="page-item-badge">{n.language}</span>
                            {n.is_exported ? <span className="page-item-badge">pub</span> : null}
                          </>
                        }
                      />
                    ))}
                  </ItemList>
                </div>
                {selectedId && (
                  <NodeDetailPanel
                    nodeId={selectedId}
                    onClose={() => setSelectedId('')}
                    onNavigate={(id) => setSelectedId(id)}
                    variant="blade"
                  />
                )}
              </div>
            )}
          </PageCardBody>
        </PageCard>
      </PageStack>
    </PageShell>
  );
}
