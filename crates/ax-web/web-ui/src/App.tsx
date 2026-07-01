import { useEffect, useState } from 'react';

import StatsPage from './pages/Stats';
import NodesPage from './pages/Nodes';
import FilesPage from './pages/Files';
import SearchPage from './pages/Search';
import PolicyRulesPage from './pages/PolicyRules';
import PolicyRuleEditor from './pages/PolicyRuleEditor';
import PolicySkillsPage from './pages/PolicySkills';
import PolicySkillEditor from './pages/PolicySkillEditor';
import PolicyMatchPage from './pages/PolicyMatch';
import StatusBar from './components/StatusBar';
import { NavIcon, adjustUiScale, initUiScale, loadUiScale, type NavId } from './components/NavIcons';
import { UiProvider } from './context/UiContext';

type Page =
  | 'stats' | 'nodes' | 'files' | 'search'
  | 'policy-rules' | 'policy-rule-edit' | 'policy-skills' | 'policy-skill-edit' | 'policy-match';

const NAV: Array<{ id: NavId; label: string }> = [
  { id: 'stats', label: 'Stats' },
  { id: 'nodes', label: 'Nodes' },
  { id: 'files', label: 'Files' },
  { id: 'search', label: 'Search' },
  { id: 'policy-rules', label: 'Rules' },
  { id: 'policy-skills', label: 'Skills' },
];

const SCALE_STEP = 0.05;

function AppShell() {
  const [page, setPage] = useState<Page>('stats');
  const [sidebarOpen, setSidebarOpen] = useState(false);
  const [editRuleId, setEditRuleId] = useState<string | null>(null);
  const [editSkillName, setEditSkillName] = useState<string | null>(null);
  const [fontScale, setFontScale] = useState(loadUiScale);

  useEffect(() => {
    initUiScale();
    setFontScale(loadUiScale());
  }, []);

  function navigate(p: Page) {
    setPage(p);
    setSidebarOpen(false);
  }

  function adjFont(delta: number) {
    setFontScale(adjustUiScale(delta));
  }

  return (
    <div className="layout">
      <header className="titlebar">
        <div style={{ display: 'flex', alignItems: 'center', gap: '8px' }}>
          <button
            className="hamburger"
            type="button"
            onClick={() => setSidebarOpen(!sidebarOpen)}
            aria-label="Toggle menu"
            aria-expanded={sidebarOpen}
          >
            ☰
          </button>
          <span className="titlebar-brand">
            ax <span>/ graph + policy</span>
          </span>
        </div>
        <div className="font-ctrl">
          <button type="button" className="font-btn" onClick={() => adjFont(-SCALE_STEP)} title="Smaller text" aria-label="Smaller text">A−</button>
          <span className="font-size-lbl">{Math.round(fontScale * 100)}%</span>
          <button type="button" className="font-btn" onClick={() => adjFont(SCALE_STEP)} title="Larger text" aria-label="Larger text">A+</button>
        </div>
      </header>

      <div className="workbench">
        {sidebarOpen && (
          <div
            className={`sidebar-overlay${sidebarOpen ? ' open' : ''}`}
            onClick={() => setSidebarOpen(false)}
            aria-hidden="true"
          />
        )}
        <nav className={`sidebar${sidebarOpen ? ' open' : ''}`} aria-label="Main navigation">
          {NAV.map((n) => (
            <button
              key={n.id}
              type="button"
              className={`nav-item${page === n.id ? ' active' : ''}`}
              onClick={() => navigate(n.id)}
            >
              <NavIcon id={n.id} />
              {n.label}
            </button>
          ))}
        </nav>

        <main className="main" id="main-content">
          {page === 'stats' && <StatsPage />}
          {page === 'nodes' && <NodesPage />}
          {page === 'files' && <FilesPage />}
          {page === 'search' && <SearchPage />}
          {page === 'policy-rules' && (
            <PolicyRulesPage
              onEdit={(id) => { setEditRuleId(id); setPage('policy-rule-edit'); }}
              onMatch={() => setPage('policy-match')}
            />
          )}
          {page === 'policy-rule-edit' && (
            <PolicyRuleEditor ruleId={editRuleId} onBack={() => setPage('policy-rules')} />
          )}
          {page === 'policy-skills' && (
            <PolicySkillsPage
              onEdit={(name) => { setEditSkillName(name); setPage('policy-skill-edit'); }}
              onMatch={() => setPage('policy-match')}
            />
          )}
          {page === 'policy-skill-edit' && (
            <PolicySkillEditor skillName={editSkillName} onBack={() => setPage('policy-skills')} />
          )}
          {page === 'policy-match' && <PolicyMatchPage onClose={() => setPage('policy-rules')} />}
        </main>
      </div>

      <StatusBar />
    </div>
  );
}

export default function App() {
  return (
    <UiProvider>
      <AppShell />
    </UiProvider>
  );
}
