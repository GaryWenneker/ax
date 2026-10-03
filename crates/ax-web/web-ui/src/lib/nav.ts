import type { NavId } from '../components/NavIcons';
import type { Page } from './routes';

export type NavItem = { id: NavId; label: string };

export type NavSection = {
  id: string;
  label: string;
  items: NavItem[];
};

/**
 * Sidebar order follows MCP usage in ~/.ax/usage.db (2026-09-29):
 * policy tools (preflight, skill, guard) dwarf everything else, then
 * graph tools (node, explore, search). Reference and settings sit last.
 * The ship page stays reachable at /ship; it is not a nav item.
 */
export const NAV_SECTIONS: NavSection[] = [
  {
    id: 'policy',
    label: 'Policy',
    items: [
      { id: 'policy-rules', label: 'Rules' },
      { id: 'policy-skills', label: 'Skills' },
      { id: 'memory', label: 'Memory' },
      { id: 'policy-sync', label: 'Sync' },
      { id: 'policy-review', label: 'Review' },
    ],
  },
  {
    id: 'code',
    label: 'Code',
    items: [
      { id: 'graph', label: 'Graph' },
      { id: 'search', label: 'Search' },
      { id: 'nodes', label: 'Nodes' },
      { id: 'files', label: 'Files' },
    ],
  },
  {
    id: 'activity',
    label: 'Activity',
    items: [
      { id: 'agent', label: 'Agent' },
      { id: 'logging', label: 'Logging' },
      { id: 'stats', label: 'Stats' },
      { id: 'savings', label: 'Savings' },
      { id: 'unresolved', label: 'Unresolved' },
    ],
  },
  {
    id: 'system',
    label: 'System',
    items: [
      { id: 'settings', label: 'Settings' },
      { id: 'prices', label: 'Prices' },
    ],
  },
];

export function visibleNav(showSavings: boolean): NavSection[] {
  return NAV_SECTIONS.map((section) => ({
    ...section,
    items: section.items.filter((item) => showSavings || item.id !== 'savings'),
  })).filter((section) => section.items.length > 0);
}

export function navItemActive(page: Page, id: NavId): boolean {
  if (page === id) return true;
  if (id === 'policy-rules' && page === 'policy-rule-edit') return true;
  if (id === 'policy-skills' && page === 'policy-skill-edit') return true;
  return false;
}
