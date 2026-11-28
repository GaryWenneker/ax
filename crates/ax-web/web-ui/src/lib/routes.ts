/** Fixed path URLs for every Command Center page (no hash routing). */

/** The one page table: page id → fixed path. Every other page list derives from it. */
const PAGE_PATH = {
  stats: '/stats',
  nodes: '/nodes',
  graph: '/graph',
  files: '/files',
  search: '/search',
  memory: '/memory',
  sonar: '/sonar',
  settings: '/settings',
  logging: '/logging',
  specs: '/specs',
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
} as const satisfies Record<string, `/${string}`>;

export type Page = keyof typeof PAGE_PATH;

/** Every Command Center page, in table order. */
export const PAGES: readonly Page[] = Object.keys(PAGE_PATH) as Page[];

const VALID_PAGES: ReadonlySet<string> = new Set(PAGES);

export type SonarTab = 'dashboard' | 'setup';

export interface RouteState {
  page: Page;
  ruleId: string | null;
  skillName: string | null;
  kind: string | null;
  /** Memory vault row id (`/memory?memory=…`), e.g. from Logging CLI turn links. */
  memoryId?: string | null;
  sonarTab: SonarTab;
  /** Full-page rule editor (policy-rule-edit with an existing id). */
  ruleEditMode?: boolean;
  /** Full-page skill editor (policy-skill-edit with an existing name). */
  skillEditMode?: boolean;
  origin?: string | null;
  projectId?: number | null;
  /** Logging filter (`/logging?session=…`) and the same session on Memory. */
  session?: string | null;
  /** Memory search (`/memory?q=…`). */
  recallQuery?: string | null;
  /** Open one spec (`/specs?spec=…`). */
  specId?: string | null;
  /** Settings section (`/settings?section=…`); Settings checks the id. */
  section?: string | null;
}

/** Pages that use the full workspace width (split panes, graph canvas, terminal). */
export const FULL_BLEED_PAGES: ReadonlySet<Page> = new Set([
  'graph',
  'logging',
  'specs',
]);

const PATH_PAGE: Record<string, Page> = Object.fromEntries(
  Object.entries(PAGE_PATH).map(([page, path]) => [path, page as Page]),
) as Record<string, Page>;

PATH_PAGE['/'] = 'stats';

/** Legacy hash segment → page: every page id, plus retired pages that now land on Stats. */
const LEGACY_HASH_PAGE: Readonly<Record<string, Page>> = {
  ...Object.fromEntries(PAGES.map((page) => [page, page])),
  ship: 'stats',
  agent: 'stats',
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
  const edit = params.get('mode') === 'edit';
  const origin = params.get('origin');
  const pidRaw = params.get('projectId');
  const projectId = pidRaw != null && pidRaw !== '' ? Number(pidRaw) : null;
  return {
    page,
    valid,
    ruleId: params.get('id'),
    skillName: params.get('name'),
    kind: params.get('kind'),
    memoryId: params.get('memory'),
    sonarTab,
    ruleEditMode: edit && page === 'policy-rule-edit',
    skillEditMode: edit && page === 'policy-skill-edit',
    origin,
    projectId: Number.isFinite(projectId) ? projectId : null,
    session: params.get('session'),
    recallQuery: params.get('q'),
    specId: params.get('spec'),
    section: params.get('section'),
  };
}

export function buildPath(state: Partial<RouteState> & Pick<RouteState, 'page'>): string {
  const base = PAGE_PATH[state.page];
  const params = new URLSearchParams();
  if ((state.page === 'policy-rules' || state.page === 'policy-rule-edit') && state.ruleId) {
    params.set('id', state.ruleId);
  }
  if (state.page === 'policy-rule-edit' && state.ruleId && state.ruleEditMode) {
    params.set('mode', 'edit');
  }
  if ((state.page === 'policy-skills' || state.page === 'policy-skill-edit') && state.skillName) {
    params.set('name', state.skillName);
  }
  if (state.page === 'policy-skill-edit' && state.skillName && state.skillEditMode) {
    params.set('mode', 'edit');
  }
  if (state.origin === 'global') {
    params.set('origin', 'global');
    if (state.projectId != null) params.set('projectId', String(state.projectId));
  }
  if (state.page === 'unresolved' && state.kind) params.set('kind', state.kind);
  if (state.page === 'logging' && state.kind) params.set('kind', state.kind);
  if (state.page === 'logging' && state.session) params.set('session', state.session);
  if (state.page === 'memory' && state.session) params.set('session', state.session);
  if (state.page === 'memory' && state.memoryId) params.set('memory', state.memoryId);
  if (state.page === 'memory' && state.recallQuery) params.set('q', state.recallQuery);
  if (state.page === 'specs' && state.specId) params.set('spec', state.specId);
  if (state.page === 'sonar' && state.sonarTab === 'setup') params.set('tab', 'setup');
  if (state.page === 'settings' && state.section) params.set('section', state.section);
  const qs = params.toString();
  return qs ? `${base}?${qs}` : base;
}

/**
 * Old bookmarks such as `/policy/rules/edit?id=x` (no `mode=edit`) now open the slideout
 * view on the list page. Returns that list route, or null when the route is not one.
 */
export function legacyEditRedirect(route: RouteState): RouteState | null {
  let page: Page | null = null;
  if (route.page === 'policy-rule-edit' && route.ruleId && !route.ruleEditMode) page = 'policy-rules';
  if (route.page === 'policy-skill-edit' && route.skillName && !route.skillEditMode) page = 'policy-skills';
  if (!page) return null;
  // Keep every other field (origin, projectId among them) so a global copy stays selected.
  return { ...route, page, ruleEditMode: false, skillEditMode: false };
}

export function navigateRoute(state: Partial<RouteState> & Pick<RouteState, 'page'>, replace = false) {
  const path = buildPath({
    page: state.page,
    ruleId: state.ruleId ?? null,
    skillName: state.skillName ?? null,
    memoryId: state.memoryId ?? null,
    kind: state.kind ?? null,
    sonarTab: state.sonarTab ?? 'dashboard',
    ruleEditMode: state.ruleEditMode ?? false,
    skillEditMode: state.skillEditMode ?? false,
    origin: state.origin ?? null,
    projectId: state.projectId ?? null,
    session: state.session ?? null,
    recallQuery: state.recallQuery ?? null,
    specId: state.specId ?? null,
    section: state.section ?? null,
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
  if (!page || !VALID_PAGES.has(page)) return null;

  const params = new URLSearchParams(qs);
  const path = buildPath({
    page,
    ruleId: params.get('id'),
    skillName: params.get('name'),
    kind: params.get('kind'),
    memoryId: params.get('memory'),
    sonarTab: params.get('tab') === 'setup' ? 'setup' : 'dashboard',
    ruleEditMode: params.get('mode') === 'edit' && page === 'policy-rule-edit',
    skillEditMode: params.get('mode') === 'edit' && page === 'policy-skill-edit',
    section: params.get('section'),
  });
  window.history.replaceState(null, '', path);
  return path;
}

export function pageFromNavId(id: string): Page | null {
  if (VALID_PAGES.has(id) && id !== 'policy-rule-edit' && id !== 'policy-skill-edit' && id !== 'policy-match') {
    return id as Page;
  }
  return null;
}
