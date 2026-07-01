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

import { fetchVersion } from './api';



type Page =

  | 'stats' | 'nodes' | 'files' | 'search'

  | 'policy-rules' | 'policy-rule-edit' | 'policy-skills' | 'policy-skill-edit' | 'policy-match';



const NAV: Array<{ id: Page; label: string; icon: string }> = [

  { id: 'stats',  label: 'Stats',  icon: '◈' },

  { id: 'nodes',  label: 'Nodes',  icon: '⬡' },

  { id: 'files',  label: 'Files',  icon: '◻' },

  { id: 'search', label: 'Search', icon: '⌕' },

  { id: 'policy-rules', label: 'Rules', icon: '◆' },

  { id: 'policy-skills', label: 'Skills', icon: '◇' },

];



export default function App() {

  const [page, setPage] = useState<Page>('stats');

  const [sidebarOpen, setSidebarOpen] = useState(false);

  const [editRuleId, setEditRuleId] = useState<string | null>(null);

  const [editSkillName, setEditSkillName] = useState<string | null>(null);

  const [version, setVersion] = useState<string>('');

  useEffect(() => {
    fetchVersion()
      .then((v) => setVersion(v.version))
      .catch(() => setVersion(''));
  }, []);



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

            ax <span>/ graph + policy</span>

          </span>

        </div>

        {version && <span className="topbar-version">v{version}</span>}

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

    </div>

  );

}


