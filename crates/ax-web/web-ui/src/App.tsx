import { useState } from 'react';
import StatsPage from './pages/Stats';
import NodesPage from './pages/Nodes';
import FilesPage from './pages/Files';
import SearchPage from './pages/Search';

type Page = 'stats' | 'nodes' | 'files' | 'search';

const NAV: Array<{ id: Page; label: string; icon: string }> = [
  { id: 'stats',  label: 'Stats',  icon: '◈' },
  { id: 'nodes',  label: 'Nodes',  icon: '⬡' },
  { id: 'files',  label: 'Files',  icon: '◻' },
  { id: 'search', label: 'Search', icon: '⌕' },
];

export default function App() {
  const [page, setPage] = useState<Page>('stats');
  const [sidebarOpen, setSidebarOpen] = useState(false);

  function navigate(p: Page) {
    setPage(p);
    setSidebarOpen(false);
  }

  return (
    <div className="layout">
      <header className="topbar">
        <div style={{ display: 'flex', alignItems: 'center', gap: '0.75rem' }}>
          <button
            className="hamburger"
            onClick={() => setSidebarOpen(!sidebarOpen)}
            aria-label="Toggle menu"
            aria-expanded={sidebarOpen}
          >
            ☰
          </button>
          <span className="topbar-brand">
            ax <span>/ local graph</span>
          </span>
        </div>
      </header>

      <div className="body">
        {sidebarOpen && (
          <div
            className="sidebar-overlay"
            style={{ display: 'block' }}
            onClick={() => setSidebarOpen(false)}
          />
        )}
        <nav className={`sidebar${sidebarOpen ? ' open' : ''}`} aria-label="Main navigation">
          {NAV.map((n) => (
            <button
              key={n.id}
              className={`nav-item${page === n.id ? ' active' : ''}`}
              onClick={() => navigate(n.id)}
            >
              <span className="nav-icon">{n.icon}</span>
              {n.label}
            </button>
          ))}
        </nav>

        <main className="main" id="main-content">
          {page === 'stats'  && <StatsPage />}
          {page === 'nodes'  && <NodesPage />}
          {page === 'files'  && <FilesPage />}
          {page === 'search' && <SearchPage />}
        </main>
      </div>
    </div>
  );
}
