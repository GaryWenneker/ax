import { useCallback, useEffect, useRef, useState } from 'react';

import { fetchFileRoots, fetchFiles, fetchStats } from '../api';

import FileTree from '../components/FileTree';

import FilePreview from '../components/FilePreview';

import NodeDetailPanel from '../components/NodeDetail';

import Codicon from '../components/Codicon';

import {

  FilterBar,

  PageCard,

  PageCardBody,

  PageEmpty,

  PageHero,

  PageLoading,

  PageShell,

  PageStack,

  PageToasts,

} from '../components/ui/PageLayout';

import { usePersistedString } from '../hooks/usePersistedState';

import { usePageContext } from '../context/UiContext';

import type { FileRoot, FileRow } from '../types';



const TREE_LIMIT = 10_000;



export default function FilesPage() {

  const [files, setFiles] = useState<FileRow[]>([]);

  const [roots, setRoots] = useState<FileRoot[]>([]);

  const [total, setTotal] = useState(0);

  const [languages, setLanguages] = useState<string[]>([]);

  const [projectName, setProjectName] = useState<string | undefined>();

  const [q, setQ] = usePersistedString('files-q', '');

  const [lang, setLang] = usePersistedString('files-lang', '');

  const [selectedPath, setSelectedPath] = usePersistedString('files-selected', '');

  const [selectedNodeId, setSelectedNodeId] = useState('');

  const [detailBladeOpen, setDetailBladeOpen] = useState(false);

  const [loading, setLoading] = useState(false);

  const [error, setError] = useState<string | null>(null);

  const [loadedPrefixes, setLoadedPrefixes] = useState<Set<string>>(new Set());

  const [loadingPrefixes, setLoadingPrefixes] = useState<Set<string>>(new Set());

  const debounce = useRef<ReturnType<typeof setTimeout> | null>(null);

  const loadedPrefixesRef = useRef(loadedPrefixes);

  loadedPrefixesRef.current = loadedPrefixes;



  const selected = files.find((f) => f.path === selectedPath) ?? null;



  const filterActive = !!(q || lang);



  const loadRoots = useCallback(async () => {

    const page = await fetchFileRoots();

    setRoots(page.roots);

    if (!filterActive) {

      setTotal(page.roots.reduce((sum, r) => sum + r.count, 0));

    }

    return page.roots;

  }, [filterActive]);



  useEffect(() => {

    fetchStats()

      .then((s) => {

        setProjectName(s.project_name);

        setLanguages(s.languages.map((l) => l.language).sort());

        if (!filterActive) setTotal(s.file_count);

      })

      .catch(() => {});

    loadRoots().catch(() => {});

  }, [filterActive, loadRoots]);



  const loadPrefix = useCallback(async (prefix: string) => {

    if (loadedPrefixesRef.current.has(prefix)) return;

    setLoadingPrefixes((prev) => new Set(prev).add(prefix));

    try {

      const page = await fetchFiles({ prefix, limit: TREE_LIMIT, offset: 0 });

      setFiles((prev) => {

        const byPath = new Map(prev.map((f) => [f.path, f]));

        for (const f of page.files) byPath.set(f.path, f);

        return [...byPath.values()].sort((a, b) => a.path.localeCompare(b.path));

      });

      setLoadedPrefixes((prev) => new Set(prev).add(prefix));

    } catch (e) {

      setError(e instanceof Error ? e.message : 'Failed to load folder');

    } finally {

      setLoadingPrefixes((prev) => {

        const next = new Set(prev);

        next.delete(prefix);

        return next;

      });

    }

  }, []);



  function loadFiltered(newQ: string, newLang: string) {

    setLoading(true);

    setError(null);

    setLoadedPrefixes(new Set());

    fetchFiles({ q: newQ, lang: newLang || undefined, limit: TREE_LIMIT, offset: 0 })

      .then((page) => {

        setFiles(page.files);

        setTotal(page.total);

        setLoading(false);

      })

      .catch((e: Error) => {

        setError(e.message);

        setLoading(false);

      });

  }



  function loadBrowseMode() {

    setLoading(false);

    setError(null);

    setFiles([]);

    setLoadedPrefixes(new Set());

    loadRoots().catch(() => {});

  }



  useEffect(() => {

    if (debounce.current) clearTimeout(debounce.current);

    debounce.current = setTimeout(() => {

      if (filterActive) loadFiltered(q, lang);

      else loadBrowseMode();

    }, q ? 300 : 0);

    return () => {

      if (debounce.current) clearTimeout(debounce.current);

    };

  }, [q, lang, filterActive]);



  const refreshTree = useCallback(async () => {

    setError(null);

    const prefixes = [...loadedPrefixesRef.current];

    try {

      await loadRoots();

      if (filterActive) {

        loadFiltered(q, lang);

        return;

      }

      setFiles([]);

      setLoadedPrefixes(new Set());

      for (const prefix of prefixes) {

        await loadPrefix(prefix);

      }

    } catch (e) {

      setError(e instanceof Error ? e.message : 'Refresh failed');

    }

  }, [filterActive, q, lang, loadRoots, loadPrefix]);



  const truncated = total > files.length;

  const fileDetail = filterActive

    ? `${files.length}${truncated ? `/${total}` : ''} files${q ? ` · "${q}"` : ''}${lang ? ` · ${lang}` : ''}`

    : `${roots.length} repos · ${total.toLocaleString()} indexed files`;

  usePageContext('Files', fileDetail);



  function selectFile(file: FileRow) {

    setSelectedPath(file.path);

    setSelectedNodeId('');

  }



  function openNodeDetail(id: string) {

    setSelectedNodeId(id);

    setDetailBladeOpen(true);

  }



  function closeDetailBlade() {

    setDetailBladeOpen(false);

    setSelectedNodeId('');

  }



  function closePreview() {

    setSelectedPath('');

    closeDetailBlade();

  }



  const splitClass = [

    'files-split',

    selected ? 'files-split--with-preview' : '',

    detailBladeOpen ? 'files-split--with-detail' : '',

  ]

    .filter(Boolean)

    .join(' ');



  const showTree = !loading && (filterActive ? files.length > 0 : roots.length > 0);



  return (

    <PageShell>

      <div className="files-page">

        <PageHero

          title="Files"

          subtitle="Indexed source files in an explorer tree. Expand a repository to browse; select a file to preview its index."

        />



        <PageToasts err={error} />



        <PageStack>

          <PageCard

            title="Indexed files"

            description={

              filterActive

                ? truncated

                  ? `Showing ${files.length} of ${total.toLocaleString()} matches — narrow your filter to see more.`

                  : `${total.toLocaleString()} matching files.`

                : `${roots.length} repositories · ${total.toLocaleString()} files in the index. Expand a folder to load its files.`

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

              <select

                className="settings-select"

                value={lang}

                onChange={(e) => setLang(e.target.value)}

                aria-label="Filter by language"

              >

                <option value="">All languages</option>

                {languages.map((l) => (

                  <option key={l} value={l}>

                    {l}

                  </option>

                ))}

              </select>

            </FilterBar>



            <PageCardBody>

              {loading ? (

                <PageLoading />

              ) : !showTree ? (

                <PageEmpty title="No files found">Try a different filter or run an index.</PageEmpty>

              ) : (

                <div className={splitClass}>

                  <div className="files-tree-pane">

                    <FileTree

                      files={files}

                      roots={filterActive ? undefined : roots}

                      rootLabel={projectName?.toUpperCase()}

                      filterActive={filterActive}

                      selectedPath={selectedPath || null}

                      loadingPrefixes={loadingPrefixes}

                      onSelect={selectFile}

                      onLoadPrefix={filterActive ? undefined : loadPrefix}

                      onRefresh={refreshTree}

                    />

                  </div>

                  {selected && (

                    <FilePreview

                      file={selected}

                      onClose={closePreview}

                      onNodeSelect={openNodeDetail}

                      selectedNodeId={selectedNodeId || null}

                    />

                  )}

                  {detailBladeOpen &&

                    (selectedNodeId ? (

                      <NodeDetailPanel

                        nodeId={selectedNodeId}

                        onClose={closeDetailBlade}

                        onNavigate={openNodeDetail}

                        variant="blade"

                      />

                    ) : (

                      <div

                        className="detail-panel detail-panel--blade"

                        role="complementary"

                        aria-label="Symbol detail"

                      >

                        <div className="detail-header">

                          <span className="detail-title muted">Symbol detail</span>

                          <button

                            type="button"

                            className="detail-close"

                            onClick={closeDetailBlade}

                            aria-label="Close"

                          >

                            <Codicon name="close" />

                          </button>

                        </div>

                        <div className="detail-body">

                          <div className="empty-label">Select a symbol in the index preview.</div>

                        </div>

                      </div>

                    ))}

                </div>

              )}

            </PageCardBody>

          </PageCard>

        </PageStack>

      </div>

    </PageShell>

  );

}


