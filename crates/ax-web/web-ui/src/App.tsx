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
import UnresolvedPage from './pages/Unresolved';
import ShipPage from './pages/Ship';
import AgentPage from './pages/Agent';
import SettingsPage from './pages/Settings';
import SonarQubePage from './pages/SonarQube';
import SavingsPage from './pages/Savings';
import MemoryPage from './pages/Memory';
import StatusBar from './components/StatusBar';
import SidebarResizeHandle, { initSidebarWidth } from './components/SidebarResize';
import { NavIcon, adjustUiScale, initUiScale, loadUiScale, type NavId } from './components/NavIcons';
import { UiProvider } from './context/UiContext';
import { initTheme } from './lib/themes';
import { fetchShipConfig } from './shipApi';
import { WORKSPACE_SWITCHED } from './workspaceEvents';

type Page =
  | 'stats' | 'nodes' | 'files' | 'search' | 'memory' | 'ship' | 'sonar' | 'agent' | 'settings' | 'savings' | 'unresolved'
  | 'policy-rules' | 'policy-rule-edit' | 'policy-skills' | 'policy-skill-edit' | 'policy-match';

const VALID_PAGES: Page[] = [
  'stats', 'nodes', 'files', 'search', 'memory', 'ship', 'sonar', 'agent', 'settings', 'savings', 'unresolved',
  'policy-rules', 'policy-rule-edit', 'policy-skills', 'policy-skill-edit', 'policy-match',
];

const NAV_MAIN_BASE: Array<{ id: NavId; label: string }> = [
  { id: 'stats', label: 'Stats' },
  { id: 'nodes', label: 'Nodes' },
  { id: 'files', label: 'Files' },
  { id: 'search', label: 'Search' },
  { id: 'memory', label: 'Memory' },
  { id: 'savings', label: 'Savings' },
  { id: 'ship', label: 'Command Center' },
  { id: 'sonar', label: 'SonarQube' },
  { id: 'agent', label: 'Agent' },
];

const NAV_CONFIG: Array<{ id: NavId; label: string }> = [
  { id: 'settings', label: 'Settings' },
];

const NAV_POLICY: Array<{ id: NavId; label: string }> = [
  { id: 'policy-rules', label: 'Rules' },
  { id: 'policy-skills', label: 'Skills' },
];

const SCALE_STEP = 0.05;

function parseHash(): { page: Page; ruleId: string | null; skillName: string | null } {
  const raw = window.location.hash.replace(/^#/, '') || 'stats';
  const [path, qs] = raw.split('?');
  const params = new URLSearchParams(qs ?? '');
  const page = VALID_PAGES.includes(path as Page) ? (path as Page) : 'stats';
  return {
    page,
    ruleId: params.get('id'),
    skillName: params.get('name'),
  };
}

function writeHash(page: Page, ruleId: string | null, skillName: string | null) {
  const params = new URLSearchParams();
  if (page === 'policy-rule-edit' && ruleId) params.set('id', ruleId);
  if (page === 'policy-skill-edit' && skillName) params.set('name', skillName);
  const qs = params.toString();
  const next = qs ? `${page}?${qs}` : page;
  if (window.location.hash.replace(/^#/, '') !== next) {
    window.location.hash = next;
  }
}

function AppShell() {
  const initial = parseHash();
  const [page, setPage] = useState<Page>(initial.page);
  const [sidebarOpen, setSidebarOpen] = useState(false);
  const [editRuleId, setEditRuleId] = useState<string | null>(initial.ruleId);
  const [editSkillName, setEditSkillName] = useState<string | null>(initial.skillName);
  const [fontScale, setFontScale] = useState(loadUiScale);
  const [showSavings, setShowSavings] = useState(false);
  const [showAgent, setShowAgent] = useState(true);
  const [workspaceKey, setWorkspaceKey] = useState(0);

  function refreshNavConfig() {
    fetchShipConfig()
      .then((d) => {
        setShowSavings(d.config.ui?.show_savings ?? d.config.ui?.show_tokens ?? true);
        setShowAgent(d.config.ui?.show_agent_terminal ?? true);
      })
      .catch(() => {});
  }

  useEffect(() => {
    initUiScale();
    initSidebarWidth();
    initTheme();
    setFontScale(loadUiScale());
    if (!window.location.hash) {
      writeHash(initial.page, initial.ruleId, initial.skillName);
    }
    refreshNavConfig();
  }, []);

  useEffect(() => {
    function onWorkspaceSwitched() {
      setWorkspaceKey((k) => k + 1);
      refreshNavConfig();
    }
    window.addEventListener(WORKSPACE_SWITCHED, onWorkspaceSwitched);
    return () => window.removeEventListener(WORKSPACE_SWITCHED, onWorkspaceSwitched);
  }, []);

  useEffect(() => {
    if (page === 'savings' && !showSavings) {
      setPage('stats');
      writeHash('stats', editRuleId, editSkillName);
    }
  }, [page, showSavings, editRuleId, editSkillName]);

  useEffect(() => {
    if (page === 'agent' && !showAgent) {
      setPage('stats');
      writeHash('stats', editRuleId, editSkillName);
    }
  }, [page, showAgent, editRuleId, editSkillName]);

  useEffect(() => {
    function onHashChange() {
      const { page: p, ruleId, skillName } = parseHash();
      setPage(p);
      setEditRuleId(ruleId);
      setEditSkillName(skillName);
      fetchShipConfig()
        .then((d) => {
          setShowSavings(d.config.ui?.show_savings ?? d.config.ui?.show_tokens ?? true);
          setShowAgent(d.config.ui?.show_agent_terminal ?? true);
        })
        .catch(() => {});
    }
    window.addEventListener('hashchange', onHashChange);
    function onShipConfigUpdated(ev: Event) {
      const detail = (ev as CustomEvent<{ show_savings?: boolean }>).detail;
      if (typeof detail?.show_savings === 'boolean') {
        setShowSavings(detail.show_savings);
      }
      fetchShipConfig()
        .then((d) => {
          setShowSavings(d.config.ui?.show_savings ?? d.config.ui?.show_tokens ?? true);
          setShowAgent(d.config.ui?.show_agent_terminal ?? true);
        })
        .catch(() => {});
    }
    window.addEventListener('ax-ship-config-updated', onShipConfigUpdated);
    return () => {
      window.removeEventListener('hashchange', onHashChange);
      window.removeEventListener('ax-ship-config-updated', onShipConfigUpdated);
    };
  }, []);

  function navigate(p: Page, extras?: { ruleId?: string | null; skillName?: string | null }) {
    const ruleId = extras?.ruleId !== undefined ? extras.ruleId : editRuleId;
    const skillName = extras?.skillName !== undefined ? extras.skillName : editSkillName;
    if (extras?.ruleId !== undefined) setEditRuleId(extras.ruleId);
    if (extras?.skillName !== undefined) setEditSkillName(extras.skillName);
    setPage(p);
    setSidebarOpen(false);
    writeHash(p, ruleId, skillName);
  }

  function adjFont(delta: number) {
    setFontScale(adjustUiScale(delta));
  }

  const navMain = NAV_MAIN_BASE.filter((n) => {
    if (n.id === 'savings' && !showSavings) return false;
    if (n.id === 'agent' && !showAgent) return false;
    return true;
  });

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
            <i className="codicon codicon-menu" aria-hidden="true" />
          </button>
          <span className="titlebar-brand">
            ax <span>/ graph + policy</span>
          </span>
        </div>
        <div className="font-ctrl">
          <button type="button" className="font-btn" onClick={() => adjFont(-SCALE_STEP)} title="Smaller text" aria-label="Smaller text">
            <i className="codicon codicon-remove" aria-hidden="true" />
          </button>
          <span className="font-size-lbl">{Math.round(fontScale * 100)}%</span>
          <button type="button" className="font-btn" onClick={() => adjFont(SCALE_STEP)} title="Larger text" aria-label="Larger text">
            <i className="codicon codicon-add" aria-hidden="true" />
          </button>
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
          {navMain.map((n) => (
            <button
              key={n.id}
              type="button"
              className={`nav-item${page === n.id ? ' active' : ''}`}
              onClick={() => navigate(n.id as Page)}
            >
              <NavIcon id={n.id} />
              {n.label}
            </button>
          ))}
          <div className="nav-section-label">Configuration</div>
          {NAV_CONFIG.map((n) => (
            <button
              key={n.id}
              type="button"
              className={`nav-item${page === n.id ? ' active' : ''}`}
              onClick={() => navigate(n.id as Page)}
            >
              <NavIcon id={n.id} />
              {n.label}
            </button>
          ))}
          <div className="nav-section-label">Policy</div>
          {NAV_POLICY.map((n) => (
            <button
              key={n.id}
              type="button"
              className={`nav-item${page === n.id ? ' active' : ''}`}
              onClick={() => navigate(n.id as Page)}
            >
              <NavIcon id={n.id} />
              {n.label}
            </button>
          ))}
        </nav>

        <SidebarResizeHandle />

        <main className="main" id="main-content">
          {page === 'stats' && <StatsPage key={workspaceKey} />}
          {page === 'nodes' && <NodesPage key={workspaceKey} />}
          {page === 'files' && <FilesPage key={workspaceKey} />}
          {page === 'search' && <SearchPage key={workspaceKey} />}
          {page === 'memory' && <MemoryPage key={workspaceKey} />}
          {page === 'unresolved' && <UnresolvedPage key={workspaceKey} />}
          {page === 'savings' && showSavings && <SavingsPage key={workspaceKey} />}
          {page === 'ship' && <ShipPage key={workspaceKey} onOpenSonar={() => navigate('sonar')} />}
          {page === 'sonar' && <SonarQubePage key={workspaceKey} />}
          {page === 'agent' && showAgent && <AgentPage key={workspaceKey} />}
          {page === 'settings' && <SettingsPage key={workspaceKey} />}
          {page === 'policy-rules' && (
            <PolicyRulesPage
              key={workspaceKey}
              onEdit={(id) => navigate('policy-rule-edit', { ruleId: id })}
              onMatch={() => navigate('policy-match')}
            />
          )}
          {page === 'policy-rule-edit' && (
            <PolicyRuleEditor key={workspaceKey} ruleId={editRuleId} onBack={() => navigate('policy-rules')} />
          )}
          {page === 'policy-skills' && (
            <PolicySkillsPage
              key={workspaceKey}
              onEdit={(name) => navigate('policy-skill-edit', { skillName: name })}
              onMatch={() => navigate('policy-match')}
            />
          )}
          {page === 'policy-skill-edit' && (
            <PolicySkillEditor key={workspaceKey} skillName={editSkillName} onBack={() => navigate('policy-skills')} />
          )}
          {page === 'policy-match' && <PolicyMatchPage key={workspaceKey} onClose={() => navigate('policy-rules')} />}
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
