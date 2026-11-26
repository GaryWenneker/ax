// Which pages exist, and which tools each one needs. A page is shown only when
// the connected server lists at least one of its tools, so an older server
// simply shows fewer pages.

export type PageId =
  | 'status' | 'search' | 'nodes' | 'explore' | 'insights'
  | 'rules' | 'skills' | 'memory' | 'sessions'
  | 'operations' | 'files' | 'usage' | 'logging'
  | 'tools';

export interface PageDef {
  id: PageId;
  label: string;
  icon: string;
  section: 'Graph' | 'Knowledge' | 'Management' | 'System';
  tools: readonly string[];
}

export const PAGES: readonly PageDef[] = [
  { id: 'status', label: 'Status', icon: 'pulse', section: 'Graph', tools: ['ax_status', 'ax_cache_status'] },
  { id: 'search', label: 'Search', icon: 'search', section: 'Graph', tools: ['ax_search'] },
  { id: 'nodes', label: 'Nodes', icon: 'symbol-class', section: 'Graph', tools: ['ax_node', 'ax_callers', 'ax_callees', 'ax_impact'] },
  { id: 'explore', label: 'Explore', icon: 'compass', section: 'Graph', tools: ['ax_explore'] },
  { id: 'insights', label: 'Insights', icon: 'graph', section: 'Graph', tools: ['ax_insights', 'ax_report'] },
  { id: 'rules', label: 'Rules', icon: 'law', section: 'Knowledge', tools: ['ax_rules'] },
  { id: 'skills', label: 'Skills', icon: 'checklist', section: 'Knowledge', tools: ['ax_skill'] },
  { id: 'memory', label: 'Memory', icon: 'note', section: 'Knowledge', tools: ['ax_recall', 'ax_history', 'ax_remember'] },
  { id: 'sessions', label: 'Sessions', icon: 'comment-discussion', section: 'Knowledge', tools: ['ax_session', 'ax_durable'] },
  { id: 'operations', label: 'Operations', icon: 'server-process', section: 'Management', tools: ['ax_sync', 'ax_index', 'ax_policy_index'] },
  { id: 'files', label: 'Files', icon: 'files', section: 'Management', tools: ['ax_files'] },
  { id: 'usage', label: 'Usage', icon: 'dashboard', section: 'Management', tools: ['ax_cost', 'ax_tool_economics', 'ax_budget', 'ax_optimization_advice'] },
  { id: 'logging', label: 'Logging', icon: 'output', section: 'Management', tools: ['ax_logs'] },
  // Every listed tool, also the ones this version has no page for.
  { id: 'tools', label: 'Tools', icon: 'tools', section: 'System', tools: [] },
];

export const SECTIONS = ['Graph', 'Knowledge', 'Management', 'System'] as const;

/** Tools that exist only for an admin grant on the remote server. */
export const ADMIN_TOOLS: readonly string[] = [
  'ax_sync', 'ax_index', 'ax_policy_index', 'ax_files',
  'ax_cost', 'ax_tool_economics', 'ax_budget', 'ax_optimization_advice', 'ax_logs',
];

export function visiblePages(toolNames: readonly string[]): PageDef[] {
  const offered = new Set(toolNames);
  return PAGES.filter((page) => page.id === 'tools' || page.tools.some((tool) => offered.has(tool)));
}

export function hasAdminAccess(toolNames: readonly string[]): boolean {
  return toolNames.some((name) => ADMIN_TOOLS.includes(name));
}

/** The page to show for a location hash such as "#/memory". */
export function pageFromHash(hash: string, pages: readonly PageDef[]): PageId {
  const id = hash.replace(/^#\/?/, '');
  return pages.find((page) => page.id === id)?.id ?? pages[0]?.id ?? 'tools';
}
