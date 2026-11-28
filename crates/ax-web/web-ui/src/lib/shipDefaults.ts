import type { ShipConfig } from '../shipApi';
import { DEFAULT_SONAR_CONFIG } from './sonarGuide';

/** The ship config shown before ax.json is loaded; loaded sections are merged over it. */
export const DEFAULT_SHIP_CONFIG: ShipConfig = {
  ship: { target_branch: 'main', web_port: 7070, git_root: '', git_roots: [] },
  quality_gate: {
    steps: ['index', 'tia', 'tests', 'sonar', 'policy'],
    tests: { runner: 'cargo test' },
    index_mode: 'incremental',
  },
  remote: { provider: 'azure_devops' },
  sonar: { ...DEFAULT_SONAR_CONFIG },
  ui: {
    show_savings: true,
    show_agent_terminal: true,
    verbose_mcp: true,
    timezone: '',
  },
  reviewers: {},
};
