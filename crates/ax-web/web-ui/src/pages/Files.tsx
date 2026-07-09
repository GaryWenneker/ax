import { useEffect, useRef, useState } from 'react';
import { fetchFiles } from '../api';
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
import { usePageContext } from '../context/UiContext';
import type { FileRow } from '../types';

const LIMIT = 50;

function formatBytes(b: number) {
  if (b < 1024) return `${b} B`;
  if (b < 1024 * 1024) return `${(b / 1024).toFixed(1)} KB`;
  return `${(b / (1024 * 1024)).toFixed(1)} MB`;
}

function formatDate(ts: number) {
  return new Date(ts).toLocaleDateString(undefined, { month: 'short', day: 'numeric', year: 'numeric' });
}

export default function FilesPage() {
  const [files, setFiles] = useState<FileRow[]>([]);
  const [total, setTotal] = useState(0);
  const [offset, setOffset] = useState(0);
  const [q, setQ] = useState('');
  const [lang, setLang] = useState('');
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const debounce = useRef<ReturnType<typeof setTimeout> | null>(null);

  function load(newOffset: number, newQ: string, newLang: string) {
    setLoading(true);
    setError(null);
    fetchFiles({ q: newQ, lang: newLang || undefined, limit: LIMIT, offset: newOffset })
      .then((page) => { setFiles(page.files); setTotal(page.total); setLoading(false); })
      .catch((e: Error) => { setError(e.message); setLoading(false); });
  }

  useEffect(() => {
    setOffset(0);
    if (debounce.current) clearTimeout(debounce.current);
    debounce.current = setTimeout(() => load(0, q, lang), q ? 300 : 0);
    return () => { if (debounce.current) clearTimeout(debounce.current); };
  }, [q, lang]);

  function goPage(dir: 1 | -1) {
    const next = offset + dir * LIMIT;
    setOffset(next);
    load(next, q, lang);
  }

  const page = Math.floor(offset / LIMIT) + 1;
  const pages = Math.ceil(total / LIMIT) || 1;

  const fileDetail = `${files.length} shown · ${total.toLocaleString()} total · p${page}/${pages}${q ? ` · "${q}"` : ''}${lang ? ` · ${lang}` : ''}`;
  usePageContext('Files', fileDetail);

  return (
    <PageShell>
      <PageHero
        title="Files"
        subtitle="All indexed source files with node counts and sizes."
      />

      <PageToasts err={error} />

      <PageStack>
        <PageCard
          title="Indexed files"
          description={`${total.toLocaleString()} files in the index.`}
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
              placeholder="Filter by path…"
              value={q}
              onChange={(e) => setQ(e.target.value)}
            />
            <input
              className="settings-input settings-input--narrow"
              type="text"
              placeholder="Language…"
              value={lang}
              onChange={(e) => setLang(e.target.value)}
            />
          </FilterBar>

          <PageCardBody>
            {loading ? (
              <PageLoading />
            ) : files.length === 0 ? (
              <PageEmpty title="No files found">Try a different filter.</PageEmpty>
            ) : (
              <ItemList>
                {files.map((f) => (
                  <ItemRow
                    key={f.path}
                    static
                    title={f.path}
                    subtitle={`${f.node_count} nodes · ${formatBytes(f.size)} · indexed ${formatDate(f.indexed_at)}`}
                    badges={<span className="page-item-badge">{f.language}</span>}
                  />
                ))}
              </ItemList>
            )}
          </PageCardBody>
        </PageCard>
      </PageStack>
    </PageShell>
  );
}
