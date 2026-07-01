import type { ReactNode } from 'react';

type NavId = 'stats' | 'nodes' | 'files' | 'search' | 'policy-rules' | 'policy-skills';

export type { NavId };

export function NavIcon({ id }: { id: NavId }) {
  const icons: Record<NavId, ReactNode> = {
    stats: (
      <svg width="14" height="14" viewBox="0 0 16 16" fill="currentColor" aria-hidden="true">
        <path d="M2 13V8h3v5H2zm4 0V3h3v10H6zm4 0V6h3v7h-3z" />
      </svg>
    ),
    nodes: (
      <svg width="14" height="14" viewBox="0 0 16 16" fill="currentColor" aria-hidden="true">
        <circle cx="3" cy="8" r="2" />
        <circle cx="13" cy="3" r="2" />
        <circle cx="13" cy="13" r="2" />
        <path d="M5 8h6M8.5 4.5L11 11.5" stroke="currentColor" strokeWidth="1" fill="none" />
      </svg>
    ),
    files: (
      <svg width="14" height="14" viewBox="0 0 16 16" fill="currentColor" aria-hidden="true">
        <path d="M2 2h5v5H2V2zm7 0h5v5H9V2zM2 9h5v5H2V9zm7 3.5a1.5 1.5 0 110-3 1.5 1.5 0 010 3z" />
      </svg>
    ),
    search: (
      <svg width="14" height="14" viewBox="0 0 16 16" fill="currentColor" aria-hidden="true">
        <path d="M11.742 10.344a6.5 6.5 0 10-1.397 1.398l3.85 3.85a1 1 0 001.415-1.414l-3.868-3.834zM12 6.5a5.5 5.5 0 11-11 0 5.5 5.5 0 0111 0z" />
      </svg>
    ),
    'policy-rules': (
      <svg width="14" height="14" viewBox="0 0 16 16" fill="currentColor" aria-hidden="true">
        <path d="M1 2.5A1.5 1.5 0 012.5 1h11A1.5 1.5 0 0115 2.5v11a1.5 1.5 0 01-1.5 1.5h-11A1.5 1.5 0 011 13.5v-11zM4 5.5a.5.5 0 01.5-.5h7a.5.5 0 010 1h-7a.5.5 0 01-.5-.5zm0 3a.5.5 0 01.5-.5h5a.5.5 0 010 1h-5a.5.5 0 01-.5-.5z" />
      </svg>
    ),
    'policy-skills': (
      <svg width="14" height="14" viewBox="0 0 16 16" fill="currentColor" aria-hidden="true">
        <path d="M8 4.754a3.246 3.246 0 100 6.492 3.246 3.246 0 000-6.492zM5.754 8a2.246 2.246 0 114.492 0 2.246 2.246 0 01-4.492 0z" />
        <path d="M9.796 1.343c-.527-1.79-3.065-1.79-3.592 0l-.094.319a.873.873 0 01-1.255.52l-.292-.16c-1.64-.892-3.433.902-2.54 2.541l.159.292a.873.873 0 01-.52 1.255l-.319.094c-1.79.527-1.79 3.065 0 3.592l.319.094a.873.873 0 01.52 1.255l-.16.292c-.892 1.64.901 3.434 2.541 2.54l.292-.159a.873.873 0 011.255.52l.094.319c.527 1.79 3.065 1.79 3.592 0l.094-.319a.873.873 0 011.255-.52l.292.16c1.64.893 3.434-.902 2.54-2.541l-.159-.292a.873.873 0 01.52-1.255l.319-.094c1.79-.527 1.79-3.065 0-3.592l-.319-.094a.873.873 0 01-.52-1.255l.16-.292c.893-1.64-.902-3.433-2.541-2.54l-.292.159a.873.873 0 01-1.255-.52l-.094-.319z" />
      </svg>
    ),
  };
  return <span className="nav-icon">{icons[id]}</span>;
}

export const PAGE_LABELS: Record<string, string> = {
  stats: 'Stats',
  nodes: 'Nodes',
  files: 'Files',
  search: 'Search',
  'policy-rules': 'Rules',
  'policy-rule-edit': 'Rule editor',
  'policy-skills': 'Skills',
  'policy-skill-edit': 'Skill editor',
  'policy-match': 'Match test',
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
