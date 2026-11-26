import { useCallback, useEffect, useMemo, useState } from 'react';
import Codicon from '@cc/components/Codicon';
import { ErrorBoundary } from '@cc/components/ErrorBoundary';
import { PageCard, PageLoading } from '@cc/components/ui/PageLayout';
import { ApiError, listConnections, remoteInfo, type Connection, type RemoteInfo } from './api';
import { SECTIONS, hasAdminAccess, pageFromHash, visiblePages, type PageId } from './pages';
import ConnectionForm from './components/ConnectionForm';
import ConnectionsPanel from './components/ConnectionsPanel';
import { PageView } from './pages/Pages';

type Remote =
  | { state: 'loading' }
  | { state: 'ready'; info: RemoteInfo }
  | { state: 'error'; message: string };

function describe(e: unknown): string {
  return e instanceof ApiError || e instanceof Error ? e.message : 'Something went wrong';
}

export default function App() {
  const [connections, setConnections] = useState<Connection[] | null>(null);
  const [startupError, setStartupError] = useState<string | null>(null);
  const [remote, setRemote] = useState<Remote>({ state: 'loading' });
  const [panel, setPanel] = useState(false);
  const [hash, setHash] = useState(window.location.hash);

  const active = connections?.find((c) => c.active) ?? null;

  const reload = useCallback(async () => {
    try {
      const list = await listConnections();
      setConnections(list.connections);
      setStartupError(null);
      if (!list.activeId) return;
      setRemote({ state: 'loading' });
      try {
        setRemote({ state: 'ready', info: await remoteInfo() });
      } catch (e) {
        setRemote({ state: 'error', message: describe(e) });
      }
    } catch (e) {
      setStartupError(describe(e));
    }
  }, []);

  useEffect(() => {
    void reload();
  }, [reload]);

  useEffect(() => {
    const onHash = () => setHash(window.location.hash);
    window.addEventListener('hashchange', onHash);
    return () => window.removeEventListener('hashchange', onHash);
  }, []);

  const tools = remote.state === 'ready' ? remote.info.tools : [];
  const names = useMemo(() => tools.map((tool) => tool.name), [tools]);
  const pages = useMemo(() => visiblePages(names), [names]);
  const page: PageId = pageFromHash(hash, pages);

  useEffect(() => {
    document.title = active ? `${active.name} · Ax Admin` : 'Ax Admin';
  }, [active]);

  if (startupError) {
    return (
      <div className="admin-connect">
        <div className="admin-connect-panel">
          <div className="settings-toast settings-toast--err" role="alert">{startupError}</div>
          <div className="admin-actions"><button type="button" className="btn" onClick={() => void reload()}>Try again</button></div>
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
          <PageCard title="Connect to an Ax environment" description="Enter the address of the environment and credentials for it. You can add more environments afterwards.">
            <ConnectionForm submitLabel="Connect" onSaved={() => void reload()} />
          </PageCard>
        </div>
      </main>
    );
  }

  const admin = hasAdminAccess(names);
  return (
    <div className="app">
      <header className="titlebar">
        <div className="titlebar-inner">
          <span className="titlebar-brand">
            <span className="ax-app-wordmark">Ax<span aria-hidden="true">_</span></span>
            <span>/ admin</span>
          </span>
          <div className="titlebar-tools">
            <span className="admin-titlebar-current" title={active?.url}>{active?.name}</span>
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
                <a
                  key={item.id}
                  href={`#/${item.id}`}
                  className={`nav-item${item.id === page ? ' active' : ''}`}
                  aria-current={item.id === page ? 'page' : undefined}
                >
                  <Codicon name={item.icon} className="nav-icon" />
                  {item.label}
                </a>
              ))}
            </div>
          );
        })}
      </nav>

      <div className="workspace">
        <main className="container" id="main-content">
          {remote.state === 'loading' && <PageLoading label={`Connecting to ${active?.name ?? 'the environment'}…`} />}
          {remote.state === 'error' && (
            <div className="page settings-page">
              <div className="settings-toast settings-toast--err" role="alert">{remote.message}</div>
              <div className="admin-actions">
                <button type="button" className="btn" onClick={() => void reload()}>Try again</button>
                <button type="button" className="btn btn-secondary" onClick={() => setPanel(true)}>Open connections</button>
              </div>
            </div>
          )}
          {remote.state === 'ready' && (
            // A new connection or page starts from a clean page state.
            <ErrorBoundary key={`${active?.id}:${page}`} fallbackTitle="This view failed to render">
              <PageView page={page} tools={tools} />
            </ErrorBoundary>
          )}
        </main>
      </div>

      <footer className="statusbar status-bar" aria-label="Connection status">
        <div className="statusbar-inner">
          <div className="statusbar-left">
            <span className="status-item"><Codicon name="server-environment" /> <span className="status-lbl">{active?.name}</span></span>
            {remote.state === 'ready' && (
              <>
                <span className="status-item"><span className="status-lbl">{remote.info.serverName || 'Ax'} {remote.info.serverVersion}</span></span>
                <span className="status-item"><span className="status-lbl">{tools.length} tools</span></span>
                <span className="status-item" data-testid="access-level"><span className="status-lbl">{admin ? 'admin access' : names.includes('ax_remember') ? 'write access' : 'read access'}</span></span>
              </>
            )}
            {remote.state === 'error' && <span className="status-item"><span className="status-lbl">not connected</span></span>}
          </div>
          <div className="statusbar-right">
            {remote.state === 'ready' && <span className="status-item"><span className="status-lbl">MCP {remote.info.protocolVersion}</span></span>}
          </div>
        </div>
      </footer>

      {panel && <ConnectionsPanel connections={connections} onClose={() => setPanel(false)} onChanged={reload} />}
    </div>
  );
}
