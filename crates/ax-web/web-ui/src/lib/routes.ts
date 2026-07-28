/** Fixed path URLs for every Command Center page (no hash routing). */

export type Page =
  | 'stats'
  | 'nodes'
  | 'graph'
  | 'files'
  | 'search'
  | 'memory'
  | 'ship'
  | 'sonar'
  | 'agent'
  | 'settings'
  | 'logging'
  | 'savings'
  | 'prices'
  | 'unresolved'
  | 'policy-rules'
  | 'policy-rule-edit'
  | 'policy-skills'
  | 'policy-skill-edit'
  | 'policy-match'
  | 'policy-review'
  | 'policy-sync';

export type SonarTab = 'dashboard' | 'setup';

export interface RouteState {
  page: Page;
  ruleId: string | null;
  skillName: string | null;
  kind: string | null;
  sonarTab: SonarTab;
}

const VALID_PAGES: Page[] = [
  'stats',
  'nodes',
  'graph',
  'files',
  'search',
  'memory',
  'ship',
  'sonar',
  'agent',
  'settings',
  'logging',
  'savings',
  'prices',
  'unresolved',
  'policy-rules',
  'policy-rule-edit',
  'policy-skills',
  'policy-skill-edit',
  'policy-match',
  'policy-review',
  'policy-sync',
];

/** Pages that use the full workspace width (split panes, graph canvas, terminal). */
export const FULL_BLEED_PAGES: ReadonlySet<Page> = new Set([
  'graph',
  'files',
  'search',
  'nodes',
  'unresolved',
  'agent',
  'sonar',
  'ship',
  'logging',
]);

const PAGE_PATH: Record<Page, string> = {
  stats: '/stats',
  nodes: '/nodes',
  graph: '/graph',
  files: '/files',
  search: '/search',
  memory: '/memory',
  ship: '/ship',
  sonar: '/sonar',
  agent: '/agent',
  settings: '/settings',
  logging: '/logging',
  savings: '/savings',
  prices: '/prices',
  unresolved: '/unresolved',
  'policy-rules': '/policy/rules',
  'policy-rule-edit': '/policy/rules/edit',
  'policy-skills': '/policy/skills',
  'policy-skill-edit': '/policy/skills/edit',
  'policy-match': '/policy/match',
  'policy-review': '/policy/review',
  'policy-sync': '/policy/sync',
};

const PATH_PAGE: Record<string, Page> = Object.fromEntries(
  Object.entries(PAGE_PATH).map(([page, path]) => [path, page as Page]),
) as Record<string, Page>;

PATH_PAGE['/'] = 'stats';

/** Legacy hash segment → fixed path (preserves query string). */
const LEGACY_HASH_PAGE: Record<string, Page> = {
  stats: 'stats',
  nodes: 'nodes',
  graph: 'graph',
  files: 'files',
  search: 'search',
  memory: 'memory',
  ship: 'ship',
  sonar: 'sonar',
  agent: 'agent',
  settings: 'settings',
  logging: 'logging',
  savings: 'savings',
  prices: 'prices',
  unresolved: 'unresolved',
  'policy-rules': 'policy-rules',
  'policy-rule-edit': 'policy-rule-edit',
  'policy-skills': 'policy-skills',
  'policy-skill-edit': 'policy-skill-edit',
  'policy-match': 'policy-match',
  'policy-review': 'policy-review',
  'policy-sync': 'policy-sync',
};

function normalizePathname(pathname: string): string {
  const trimmed = pathname.replace(/\/+$/, '') || '/';
  return trimmed.startsWith('/') ? trimmed : `/${trimmed}`;
}

export function parsePathname(pathname: string): { page: Page; valid: boolean } {
  const path = normalizePathname(pathname);
  const page = PATH_PAGE[path];
  if (page) return { page, valid: true };
  return { page: 'stats', valid: false };
}

export function parseLocation(loc: Location = window.location): RouteState & { valid: boolean } {
  const { page, valid } = parsePathname(loc.pathname);
  const params = new URLSearchParams(loc.search);
  const sonarTab = params.get('tab') === 'setup' ? 'setup' : 'dashboard';
  return {
    page,
    valid,
    ruleId: params.get('id'),
    skillName: params.get('name'),
    kind: params.get('kind'),
    sonarTab,
  };
}

export function buildPath(state: Partial<RouteState> & Pick<RouteState, 'page'>): string {
  const base = PAGE_PATH[state.page];
  const params = new URLSearchParams();
  if (state.page === 'policy-rule-edit' && state.ruleId) params.set('id', state.ruleId);
  if (state.page === 'policy-skill-edit' && state.skillName) params.set('name', state.skillName);
  if (state.page === 'unresolved' && state.kind) params.set('kind', state.kind);
  if (state.page === 'logging' && state.kind) params.set('kind', state.kind);
  if (state.page === 'sonar' && state.sonarTab === 'setup') params.set('tab', 'setup');
  const qs = params.toString();
  return qs ? `${base}?${qs}` : base;
}

export function navigateRoute(state: Partial<RouteState> & Pick<RouteState, 'page'>, replace = false) {
  const path = buildPath({
    page: state.page,
    ruleId: state.ruleId ?? null,
    skillName: state.skillName ?? null,
    kind: state.kind ?? null,
    sonarTab: state.sonarTab ?? 'dashboard',
  });
  const current = `${window.location.pathname}${window.location.search}`;
  if (current === path) return;
  if (replace) {
    window.history.replaceState(null, '', path);
  } else {
    window.history.pushState(null, '', path);
  }
  window.dispatchEvent(new PopStateEvent('popstate'));
}

/** Redirect legacy `#page?…` bookmarks to fixed paths once on load. */
export function migrateLegacyHash(): string | null {
  const hash = window.location.hash.replace(/^#/, '').trim();
  if (!hash) return null;

  const [segment, qs = ''] = hash.split('?');
  const page = LEGACY_HASH_PAGE[segment];
  if (!page || !VALID_PAGES.includes(page)) return null;

  const params = new URLSearchParams(qs);
  const path = buildPath({
    page,
    ruleId: params.get('id'),
    skillName: params.get('name'),
    kind: params.get('kind'),
    sonarTab: params.get('tab') === 'setup' ? 'setup' : 'dashboard',
  });
  window.history.replaceState(null, '', path);
  return path;
}

export function pageFromNavId(id: string): Page | null {
  if (VALID_PAGES.includes(id as Page) && id !== 'policy-rule-edit' && id !== 'policy-skill-edit' && id !== 'policy-match') {
    return id as Page;
  }
  return null;
}
