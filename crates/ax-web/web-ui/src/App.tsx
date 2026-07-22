import { useEffect, useState } from 'react';

import StatsPage from './pages/Stats';
import NodesPage from './pages/Nodes';
import GraphPage from './pages/Graph';
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
import LoggingPage from './pages/Logging';
import SonarQubePage from './pages/SonarQube';
import SavingsPage from './pages/Savings';
import MemoryPage from './pages/Memory';
import StatusBar from './components/StatusBar';
import { McpQualityHost } from './components/McpQualitySlideout';
import SidebarResizeHandle, { initSidebarWidth } from './components/SidebarResize';
import { initBladeWidth } from './components/BladeResize';
import { NavIcon, adjustUiScale, initUiScale, loadUiScale, type NavId } from './components/NavIcons';
import { UiProvider } from './context/UiContext';
import { initTheme } from './lib/themes';
import {
  FULL_BLEED_PAGES,
  migrateLegacyHash,
  navigateRoute,
  parseLocation,
  type Page,
  type RouteState,
} from './lib/routes';
import { fetchShipConfig } from './shipApi';
import { WORKSPACE_SWITCHED } from './workspaceEvents';

const NAV_MAIN_BASE: Array<{ id: NavId; label: string }> = [
  { id: 'stats', label: 'Stats' },
  { id: 'nodes', label: 'Nodes' },
  { id: 'graph', label: 'Graph' },
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

function stripValid(parsed: RouteState & { valid?: boolean }): RouteState {
  const { valid: _valid, ...route } = parsed;
  return route;
}

function AppShell() {
  const [route, setRoute] = useState<RouteState>(() => stripValid(parseLocation()));
  const [sidebarOpen, setSidebarOpen] = useState(false);
  const [fontScale, setFontScale] = useState(loadUiScale);
  const [showSavings, setShowSavings] = useState(false);
  const [showAgent, setShowAgent] = useState(true);
  const [workspaceKey, setWorkspaceKey] = useState(0);

  const { page, ruleId: editRuleId, skillName: editSkillName, sonarTab } = route;

  function refreshNavConfig() {
    fetchShipConfig()
      .then((d) => {
        setShowSavings(d.config.ui?.show_savings ?? d.config.ui?.show_tokens ?? true);
        setShowAgent(d.config.ui?.show_agent_terminal ?? true);
      })
      .catch(() => {});
  }

  function applyRoute(next: RouteState, replace = false) {
    setRoute(next);
    navigateRoute(next, replace);
  }

  function syncRouteFromBrowser() {
    const parsed = parseLocation();
    setRoute(stripValid(parsed));
    if (!parsed.valid) {
      navigateRoute(stripValid(parsed), true);
    }
    refreshNavConfig();
  }

  useEffect(() => {
    initUiScale();
    initSidebarWidth();
    initBladeWidth();
    initTheme();
    setFontScale(loadUiScale());

    migrateLegacyHash();
    const initial = parseLocation();
    setRoute(stripValid(initial));
    if (!initial.valid || window.location.pathname === '/' || window.location.pathname === '') {
      navigateRoute(stripValid(initial), true);
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
      const next = { ...route, page: 'stats' as const };
      setRoute(next);
      navigateRoute(next, true);
    }
  }, [page, showSavings]);

  useEffect(() => {
    if (page === 'agent' && !showAgent) {
      const next = { ...route, page: 'stats' as const };
      setRoute(next);
      navigateRoute(next, true);
    }
  }, [page, showAgent]);

  useEffect(() => {
    function onPopState() {
      syncRouteFromBrowser();
    }
    window.addEventListener('popstate', onPopState);
    function onShipConfigUpdated(ev: Event) {
      const detail = (ev as CustomEvent<{ show_savings?: boolean }>).detail;
      if (typeof detail?.show_savings === 'boolean') {
        setShowSavings(detail.show_savings);
      }
      refreshNavConfig();
    }
    window.addEventListener('ax-ship-config-updated', onShipConfigUpdated);
    return () => {
      window.removeEventListener('popstate', onPopState);
      window.removeEventListener('ax-ship-config-updated', onShipConfigUpdated);
    };
  }, []);

  function navigate(
    p: Page,
    extras?: Partial<Pick<RouteState, 'ruleId' | 'skillName' | 'kind' | 'sonarTab'>>,
  ) {
    const next: RouteState = {
      page: p,
      ruleId: extras?.ruleId !== undefined ? extras.ruleId : editRuleId,
      skillName: extras?.skillName !== undefined ? extras.skillName : editSkillName,
      kind: extras?.kind !== undefined ? extras.kind : route.kind,
      sonarTab: extras?.sonarTab ?? route.sonarTab,
    };
    setRoute(next);
    setSidebarOpen(false);
    navigateRoute(next);
  }

  function adjFont(delta: number) {
    setFontScale(adjustUiScale(delta));
  }

  const navMain = NAV_MAIN_BASE.filter((n) => {
    if (n.id === 'savings' && !showSavings) return false;
    if (n.id === 'agent' && !showAgent) return false;
    return true;
  });

  const containerClass = FULL_BLEED_PAGES.has(page) ? 'container container--full' : 'container';

  return (
    <>
      {sidebarOpen && (
        <div
          className="sidebar-overlay open"
          onClick={() => setSidebarOpen(false)}
          aria-hidden="true"
        />
      )}

      <div className="app">
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
              className={`nav-item${page === n.id || (n.id === 'policy-rules' && page === 'policy-rule-edit') || (n.id === 'policy-skills' && page === 'policy-skill-edit') ? ' active' : ''}`}
              onClick={() => navigate(n.id as Page)}
            >
              <NavIcon id={n.id} />
              {n.label}
            </button>
          ))}
        </nav>

        <SidebarResizeHandle />

        <div className="workspace">
          <main className={containerClass} id="main-content">
            {page === 'stats' && <StatsPage key={workspaceKey} />}
            {page === 'nodes' && <NodesPage key={workspaceKey} />}
            {page === 'graph' && <GraphPage key={workspaceKey} />}
            {page === 'files' && <FilesPage key={workspaceKey} />}
            {page === 'search' && <SearchPage key={workspaceKey} />}
            {page === 'memory' && <MemoryPage key={workspaceKey} />}
            {page === 'unresolved' && <UnresolvedPage key={workspaceKey} route={route} onRouteChange={applyRoute} />}
            {page === 'savings' && showSavings && <SavingsPage key={workspaceKey} />}
            {page === 'ship' && <ShipPage key={workspaceKey} onOpenSonar={() => navigate('sonar')} />}
            {page === 'sonar' && (
              <SonarQubePage key={workspaceKey} tab={sonarTab} onTabChange={(tab) => navigate('sonar', { sonarTab: tab })} />
            )}
            {page === 'agent' && showAgent && <AgentPage key={workspaceKey} />}
            {page === 'settings' && <SettingsPage key={workspaceKey} />}
            {page === 'logging' && <LoggingPage key={workspaceKey} />}
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
        <McpQualityHost />
      </div>
    </>
  );
}

export default function App() {
  return (
    <UiProvider>
      <AppShell />
    </UiProvider>
  );
}
