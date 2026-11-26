import { useCallback, useEffect, useMemo, useRef, useState } from 'react';
import Codicon from '@cc/components/Codicon';
import { ErrorBoundary } from '@cc/components/ErrorBoundary';
import { PageCard, PageLoading } from '@cc/components/ui/PageLayout';
import { listConnections, remoteInfo, type Connection, type RemoteInfo } from './api';
import { ActiveConnectionContext, type ActiveConnection } from './activeConnection';
import { SECTIONS, hasAdminAccess, pageFromHash, visiblePages, type PageId } from './pages';
import ConnectionForm from './components/ConnectionForm';
import ConnectionsPanel from './components/ConnectionsPanel';
import { PageView } from './pages/index';

type Remote =
  | { state: 'loading' }
  | { state: 'ready'; info: RemoteInfo; connection: Connection }
  | { state: 'error'; message: string };

const describe = (e: unknown): string => (e instanceof Error ? e.message : 'Something went wrong');
const NO_TOOLS: RemoteInfo['tools'] = [];

export default function App() {
  const [connections, setConnections] = useState<Connection[] | null>(null);
  const [startupError, setStartupError] = useState<string | null>(null);
  const [remote, setRemote] = useState<Remote>({ state: 'loading' });
  const [panel, setPanel] = useState(false);
  const [hash, setHash] = useState(window.location.hash);
  const [notice, setNotice] = useState<string | null>(null);
  const latest = useRef(0);
  const main = useRef<HTMLElement>(null);

  // Loads the saved connections and the active environment. Only the newest
  // call may change what is shown: an older, slower answer is dropped.
  const reload = useCallback(async () => {
    const ticket = ++latest.current;
    const current = () => ticket === latest.current;
    try {
      const list = await listConnections();
      if (!current()) return;
      setConnections(list.connections);
      setStartupError(null);
      if (list.connections.length === 0) setPanel(false);
      if (!list.activeId) return;
      setRemote({ state: 'loading' });
      try {
        const loaded = await remoteInfo();
        if (current()) setRemote({ state: 'ready', ...loaded });
      } catch (e) {
        if (current()) setRemote({ state: 'error', message: describe(e) });
      }
    } catch (e) {
      if (current()) setStartupError(describe(e));
    }
  }, []);

  useEffect(() => {
    void reload();
    return () => { latest.current += 1; };
  }, [reload]);

  useEffect(() => {
    const onHash = () => setHash(window.location.hash);
    window.addEventListener('hashchange', onHash);
    return () => window.removeEventListener('hashchange', onHash);
  }, []);

  // Pages, tools and names all come from the environment that answered, so
  // they can never belong to two different environments.
  const shown = remote.state === 'ready' ? remote.connection : connections?.find((c) => c.active) ?? null;
  const tools = remote.state === 'ready' ? remote.info.tools : NO_TOOLS;
  const names = useMemo(() => tools.map((tool) => tool.name), [tools]);
  const pages = useMemo(() => visiblePages(names), [names]);
  const page: PageId = pageFromHash(hash, pages);

  useEffect(() => {
    document.title = shown ? `${shown.name} · Ax Admin` : 'Ax Admin';
  }, [shown]);

  // A new page starts at its heading for keyboard and screen-reader users.
  const firstPage = useRef(true);
  useEffect(() => {
    if (firstPage.current) firstPage.current = false;
    else main.current?.focus();
  }, [page]);

  const reloadSoon = useCallback(() => { void reload(); }, [reload]);
  const environmentChanged = useCallback((message: string) => {
    setNotice(message);
    void reload();
  }, [reload]);
  const active = useMemo<ActiveConnection | null>(
    () => (remote.state === 'ready' ? { id: remote.connection.id, name: remote.connection.name, changed: environmentChanged } : null),
    [remote, environmentChanged],
  );

  if (startupError) {
    return (
      <div className="admin-connect">
        <div className="admin-connect-panel">
          <div className="settings-toast settings-toast--err" role="alert">{startupError}</div>
          <div className="admin-actions"><button type="button" className="btn" onClick={reloadSoon}>Try again</button></div>
        </div>
      </div>
    );
  }
  if (connections === null) {
    return <div className="admin-connect"><PageLoading label="Starting…" /></div>;
  }

  // First start: nothing else is reachable until one environment is connected.
  if (connections.length === 0) {
    return (
      <main className="admin-connect" data-testid="connect-screen">
        <div className="admin-connect-panel">
          <div className="admin-connect-brand">
            <span className="ax-app-wordmark">Ax<span aria-hidden="true">_</span></span>
            <span>Admin</span>
          </div>
          <ErrorBoundary fallbackTitle="The connect form failed to render">
            <PageCard title="Connect to an Ax environment" description="Enter the address of the environment and credentials for it. You can add more environments afterwards.">
              <ConnectionForm submitLabel="Connect" onSaved={reloadSoon} onChanged={reloadSoon} />
            </PageCard>
          </ErrorBoundary>
        </div>
      </main>
    );
  }

  const access = hasAdminAccess(names) ? 'admin access' : names.includes('ax_remember') ? 'write access' : 'read access';
  return (
    <div className="app">
      <header className="titlebar">
        <div className="titlebar-inner">
          <span className="titlebar-brand">
            <span className="ax-app-wordmark">Ax<span aria-hidden="true">_</span></span>
            <span>/ admin</span>
          </span>
          <div className="titlebar-tools">
            <span className="admin-titlebar-current" title={shown?.url} data-testid="active-name">{shown?.name}</span>
            <button type="button" className="admin-titlebar-button" onClick={() => setPanel(true)} aria-haspopup="dialog">
              <Codicon name="plug" />
              Connections
            </button>
          </div>
        </div>
      </header>

      <nav className="sidebar" aria-label="Main navigation">
        {SECTIONS.map((section) => {
          const items = pages.filter((p) => p.section === section);
          if (items.length === 0) return null;
          return (
            <div key={section} className="nav-section">
              <div className="nav-section-label">{section}</div>
              {items.map((item) => (
                <a key={item.id} href={`#/${item.id}`} className={`nav-item${item.id === page ? ' active' : ''}`} aria-current={item.id === page ? 'page' : undefined}>
                  <Codicon name={item.icon} className="nav-icon" />
                  {item.label}
                </a>
              ))}
            </div>
          );
        })}
      </nav>

      <div className="workspace">
        <main ref={main} className="container" id="main-content" tabIndex={-1}>
          {notice && (
            <div className="settings-toast settings-toast--warn admin-notice" role="alert">
              <span>{notice}</span>
              <button type="button" className="btn btn-subtle" onClick={() => setNotice(null)}>Dismiss</button>
            </div>
          )}
          {remote.state === 'loading' && <PageLoading label={`Connecting to ${shown?.name ?? 'the environment'}…`} />}
          {remote.state === 'error' && (
            <div className="page settings-page">
              <div className="settings-toast settings-toast--err" role="alert">{remote.message}</div>
              <div className="admin-actions">
                <button type="button" className="btn" onClick={reloadSoon}>Try again</button>
                <button type="button" className="btn btn-secondary" onClick={() => setPanel(true)}>Open connections</button>
              </div>
            </div>
          )}
          {active && (
            <ActiveConnectionContext.Provider value={active}>
              {/* A new environment or page starts from a clean page state. */}
              <ErrorBoundary key={`${active.id}:${page}`} fallbackTitle="This view failed to render">
                <PageView page={page} tools={tools} />
              </ErrorBoundary>
            </ActiveConnectionContext.Provider>
          )}
        </main>
      </div>

      <footer className="statusbar status-bar" aria-label="Connection status">
        <div className="statusbar-inner">
          <div className="statusbar-left">
            <span className="status-item"><Codicon name="server-environment" /> <span className="status-lbl">{shown?.name}</span></span>
            {remote.state === 'ready' && (
              <>
                <span className="status-item"><span className="status-lbl">{remote.info.serverName || 'Ax'} {remote.info.serverVersion}</span></span>
                <span className="status-item"><span className="status-lbl">{tools.length} tools</span></span>
                <span className="status-item" data-testid="access-level"><span className="status-lbl">{access}</span></span>
              </>
            )}
            {remote.state === 'error' && <span className="status-item"><span className="status-lbl">not connected</span></span>}
          </div>
          <div className="statusbar-right">
            {remote.state === 'ready' && <span className="status-item"><span className="status-lbl">MCP {remote.info.protocolVersion}</span></span>}
          </div>
        </div>
      </footer>

      {panel && (
        <ErrorBoundary fallbackTitle="The connections panel failed to render">
          <ConnectionsPanel connections={connections} onClose={() => setPanel(false)} onChanged={async () => { setNotice(null); await reload(); }} />
        </ErrorBoundary>
      )}
    </div>
  );
}
