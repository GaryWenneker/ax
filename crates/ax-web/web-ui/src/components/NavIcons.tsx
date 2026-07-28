import Codicon from './Codicon';

type NavId =
  | 'stats'
  | 'nodes'
  | 'graph'
  | 'files'
  | 'search'
  | 'memory'
  | 'unresolved'
  | 'savings'
  | 'prices'
  | 'ship'
  | 'sonar'
  | 'agent'
  | 'settings'
  | 'logging'
  | 'policy-rules'
  | 'policy-skills'
  | 'policy-review'
  | 'policy-sync';

export type { NavId };

const NAV_CODICONS: Record<NavId, string> = {
  stats: 'graph',
  nodes: 'symbol-class',
  graph: 'type-hierarchy-sub',
  files: 'files',
  search: 'search',
  memory: 'lightbulb',
  unresolved: 'warning',
  savings: 'dashboard',
  prices: 'tag',
  ship: 'rocket',
  sonar: 'shield',
  agent: 'terminal',
  settings: 'settings-gear',
  logging: 'output',
  'policy-rules': 'law',
  'policy-skills': 'mortar-board',
  'policy-review': 'checklist',
  'policy-sync': 'sync',
};

export function NavIcon({ id }: { id: NavId }) {
  return (
    <span className="nav-icon">
      <Codicon name={NAV_CODICONS[id]} />
    </span>
  );
}

export const PAGE_LABELS: Record<string, string> = {
  stats: 'Stats',
  nodes: 'Nodes',
  graph: 'Graph',
  files: 'Files',
  search: 'Search',
  memory: 'Memory',
  unresolved: 'Unresolved',
  savings: 'Savings',
  prices: 'Prices',
  ship: 'Command Center',
  sonar: 'SonarQube',
  agent: 'Agent',
  settings: 'Settings',
  logging: 'Logging',
  'policy-rules': 'Rules',
  'policy-rule-edit': 'Rule editor',
  'policy-skills': 'Skills',
  'policy-skill-edit': 'Skill editor',
  'policy-match': 'Match test',
  'policy-review': 'Review',
  'policy-sync': 'Sync',
};

const SCALE_KEY = 'ax-web-ui-scale';
const SCALE_MIN = 0.85;
const SCALE_MAX = 1.25;
const SCALE_STEP = 0.05;

export function loadUiScale(): number {
  const raw = localStorage.getItem(SCALE_KEY);
  const n = raw ? parseFloat(raw) : 1;
  if (Number.isNaN(n)) return 1;
  return Math.min(SCALE_MAX, Math.max(SCALE_MIN, n));
}

export function applyUiScale(scale: number) {
  document.documentElement.style.setProperty('--ui-scale', String(scale));
  localStorage.setItem(SCALE_KEY, String(scale));
}

export function adjustUiScale(delta: number): number {
  const next = Math.min(SCALE_MAX, Math.max(SCALE_MIN, loadUiScale() + delta));
  applyUiScale(next);
  return next;
}

export function initUiScale() {
  applyUiScale(loadUiScale());
}
