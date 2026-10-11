/** Public, synthetic data for reproducible UI screenshots. Never production metrics. */
const now = Date.UTC(2026, 9, 11, 12);
const project = '/projects/ax-demo';
const names = ['AppShell', 'SettingsPage', 'applyTheme', 'fetchMemories', 'PolicyCalmList', 'GraphPage'];
const nodes = names.map((name, i) => ({ id: `demo-${i}`, name, qualified_name: name, kind: 'function',
  file_path: `src/${name}.tsx`, language: 'TypeScript', start_line: 20 + i * 8, end_line: 45 + i * 8,
  signature: `function ${name}()`, is_exported: 1 }));
const files = nodes.map(n => ({ path: n.file_path, language: n.language, size: 2400, node_count: 8, indexed_at: now }));
const rules = ['typescript-safety', 'review-before-merge', 'document-public-api'].map((id, i) => ({
  id, level: ['CRITICAL', 'WARNING', 'INFO'][i], alwaysApply: true, globs: [], triggers: [], tags: ['quality'],
  priority: 100 - i * 10, body: `Keep ${id.replaceAll('-', ' ')} consistent.`, sourcePath: `.agents/rules/${id}.md`,
  enabled: true, scope: 'project', origin: 'project', effectiveStorage: 'files', group: 'Project standards',
}));
const skills = ['review-changes', 'debug-workflow', 'write-documentation'].map((name, i) => ({
  name, description: ['Review changes and affected tests', 'Investigate errors with code context', 'Write clear product documentation'][i],
  tags: ['workflow'], priority: 90, body: 'Use focused code context and verify the result.',
  sourcePath: `.agents/skills/${name}/SKILL.md`, enabled: true, scope: 'project', origin: 'project', effectiveStorage: 'files',
}));
const memories = ['Azure design system', 'Preserve workspace preferences', 'Improve code navigation'].map((title, i) => ({
  id: `memory-${i}`, kind: ['decision', 'convention', 'note'][i], title, body: 'Example project knowledge for the public design walkthrough.',
  tags: ['design'], files: [], confidence: 0.95, source: 'agent', enabled: true, created_at: now, updated_at: now,
}));
const graphNodes = Array.from({ length: 48 }, (_, i) => ({ id: `graph-${i}`, name: `${names[i % names.length]}${i}`,
  kind: 'function', file_path: `src/module${i % 4}.ts`, community_id: i % 4, community_label: ['Interface', 'Policy', 'Knowledge', 'Index'][i % 4], degree: 3 }));
const graph = { nodes: graphNodes, edges: graphNodes.slice(1).map((n, i) => ({ source: `graph-${i}`, target: n.id, kind: 'calls', confidence: 'extracted' })), total_nodes: 48, truncated: false };
const config = {
  ship: { target_branch: 'main', web_port: 7070, git_root: project, git_roots: [project] },
  quality_gate: { steps: ['index', 'tia', 'tests', 'policy'], tests: { runner: 'npm test' }, index_mode: 'incremental' },
  remote: { provider: 'github', github: { owner: 'example', repo: 'ax-demo', token_env: 'GITHUB_TOKEN' } },
  sonar: { enabled: false, host: 'http://localhost:9000', project_key: 'ax-demo', token_env: 'SONAR_TOKEN', scanner_path: 'sonar-scanner', container_runtime: 'docker' },
  ui: { show_savings: true, verbose_mcp: true, timezone: 'Europe/Amsterdam' }, reviewers: {},
};
const quality = { projectRoot: project, projectLabel: 'Ax demo', logPath: `${project}/.ax/mcp.log`, mode: 'correlated',
  windowMinutes: 30, updatedAtMs: now, score: 94, grade: 'A', correlationPct: 98, matchedCalls: 126, unmatchedAxCalls: 2,
  verboseClusters: 12, enrichment: { injectCharsP50: 1800, injectCharsP95: 4200, enrichDoneRate: 0.98, emptyEnrichCount: 0, matchedRulesRate: 0.96, preflightCount: 30, inboundCount: 128 },
  toolMix: { preflight: 30, explore: 48, guard: 12, graph: 25, otherAx: 13, read: 4, grep: 2 }, findings: [],
  tokensAtRisk: 0, criticalCount: 0, verboseEnabled: true, verbosePresent: true, sessionId: 'demo-session', sessionPath: null };
const tools = ['ax_explore', 'ax_node', 'ax_context'];
const savings = {
  period: 'week', from: '2026-10-05', to: '2026-10-11', mcp_calls: 128, graph_calls: 86, failed_calls: 0,
  tokens_saved_est: 148000, net_tokens_saved_est: 136000, counterfactual_files: 64, counterfactual_tokens_est: 180000,
  graph_response_tokens_est: 32000, response_tokens_est: 44000, counterfactual_exact_files: 64, cost_saved_usd_est: 0.74,
  graph_response_cost_usd_est: 0.16, counterfactual_cost_usd_est: 0.90, policy_calls: 42, success_rate_pct: 100,
  avg_duration_ms: 38, projects_active: 1, clamp_tokens_absorbed: 0, graph_calls_with_savings: 86,
  pricing: { reference_model: 'Demo reference model', input_per_mtok: 5, output_per_mtok: 15, source: 'demo', config_path: `${project}/.ax/pricing.json` },
  assumptions: { exact_tokenizer: false, chars_per_token: 4, tokens_per_line: 12, avg_file_tokens: 2800, counterfactual_mode: 'whole-file' },
  by_tool: tools.map((tool, i) => ({ tool, calls: 42 - i * 10, graph_calls: 42 - i * 10, failed_calls: 0, tokens_saved_est: 64000 - i * 12000, counterfactual_files: 22, counterfactual_tokens_est: 76000, graph_response_tokens_est: 12000, avg_duration_ms: 38 })),
  by_project: [{ project: 'Ax demo', calls: 128, graph_calls: 86, tokens_saved_est: 148000, counterfactual_files: 64 }], by_model: [],
  by_weekday: [], by_hour: [], timeline: Array.from({ length: 7 }, (_, i) => ({ bucket: `2026-10-${String(i + 5).padStart(2, '0')}`, tokens_saved_est: 14000 + i * 2000, calls: 18, graph_calls: 12 })), agent_sessions: [], db_path: `${project}/.ax/usage.db`,
  daily: Array.from({ length: 7 }, (_, i) => ({ date: `2026-10-${String(i + 5).padStart(2, '0')}`, tokens_saved_est: 14000 + i * 2000, calls: 18, graph_calls: 12, failed_calls: 0, counterfactual_files: 8, counterfactual_tokens_est: 26000, graph_response_tokens_est: 4000, cost_saved_usd_est: 0.10 })),
  recent_calls: tools.map((tool, i) => ({ id: i + 1, tool, project: 'Ax demo', tokens_saved_est: 2400, counterfactual_tokens_est: 2800, response_tokens_est: 400, counterfactual_files: 1, ok: true, savings_eligible: true, duration_ms: 38, created_at: now - i * 60000, has_preview: false })),
};
const recent = [{ path: project, label: 'Ax demo', initialized: true, last_opened: now }, { path: '/projects/sample-app', label: 'Sample app', initialized: true, last_opened: now }];
const data = {
  '/api/version': { version: '8.0.0' },
  '/api/stats': { node_count: 2450, edge_count: 7800, file_count: 312, languages: [{ language: 'Rust', count: 1500 }, { language: 'TypeScript', count: 950 }], last_indexed_at: now, unresolved_ref_count: 2, db_size_bytes: 2400000, policy_rules_count: 3, policy_skills_count: 3, readonly: false, project_name: 'Ax demo' },
  '/api/nodes': { nodes, total: nodes.length }, '/api/files': { files, total: files.length },
  '/api/files/roots': { roots: [{ name: 'src', path: 'src', count: files.length }], files: [] },
  '/api/search': { results: nodes.map(n => ({ ...n, snippet: n.signature })), total: nodes.length },
  '/api/memory': { memories, total: memories.length }, '/api/graph': graph, '/api/graph/recent': { nodes: [], edges: [] },
  '/api/insights': { nodeCount: 48, edgeCount: 47, numCommunities: 4, modularity: 0.7, godNodes: [], communities: [], surprisingConnections: [], suggestedQuestions: [] },
  '/api/domain-graph': { version: 1, nodes: [], edges: [] },
  '/api/unresolved': { refs: [{ id: 1, from_node_id: nodes[0].id, reference_name: 'loadWorkspace', reference_kind: 'calls', line: 28, col: 4, file_path: nodes[0].file_path, language: 'TypeScript' }], total: 1 },
  '/api/unresolved/summary': { total: 1, by_kind: [{ kind: 'calls', count: 1 }], top_names: [{ name: 'loadWorkspace', count: 1 }] },
  '/api/policy/rules': { rules }, '/api/policy/skills': { skills },
  '/api/policy/settings': { policySync: true, requireReview: true, storage: 'files', roots: [], agentsDir: '.agents' },
  '/api/policy/review': { items: [{ kind: 'rule', id: 'review-before-merge', path: '.agents/rules/review-before-merge.md', status: 'pending', preview: 'Review the diff and run affected checks before merging.', levelOrDescription: 'WARNING' }] },
  '/api/policy/pack/status': { packPath: '.agents/shared', hasManifest: true, rulesInPack: 3, skillsInPack: 3, exportedAt: now, localSharedRules: 3, localSharedSkills: 3, requireReview: true, policySync: true },
  '/api/ship/config': { config, sonar: config.sonar },
  '/api/ship/status': { branch: 'feature/azure-portal-design', config, report: { git: { head_branch: 'feature/azure-portal-design', base_ref: 'main' }, changed_files: ['src/App.tsx', 'src/portal.css'], tia: { tests: [{ name: 'Portal navigation', runner_hint: 'playwright' }], test_files: ['e2e/azure-portal.spec.ts'] }, quality_gate: { passed: true, steps: ['index', 'tia', 'tests', 'sonar', 'policy'].map(step => ({ step, status: step === 'sonar' ? 'skipped' : 'passed', detail: step === 'sonar' ? 'Disabled in demo' : 'Example result' })) }, breaking_warnings: [], business_rule_warnings: [], affected_routes: ['/settings', '/memory'] }, last_run: { ok: true, lines: ['Example quality gate complete.'] } },
  '/api/ship/review-language': { ok: true, code: 'en', name: 'English', languages: [] },
  '/api/ship/sonar/discover': { runtimes: [], preferred: null, container: null, reachable: false, host: 'http://localhost:9000' },
  '/api/ship/sonar/setup': { login_user: '', login_password_hint: '', project_exists: false, project_lookup: 'missing', token_configured: false, scanner_available: false, token_path: '', cli_connected: false },
  '/api/workspace/current': { ok: true, workspace: { path: project, label: 'Ax demo', initialized: true }, recent },
  '/api/workspace/recent': { ok: true, recent },
  '/api/workspace/browse': { ok: true, path: '/projects', parent: '/', initialized: false, entries: recent.map(r => ({ name: r.label, path: r.path, is_dir: true, initialized: true })) },
  '/api/agent/status': { ok: true, targets: ['cursor', 'vscode'].map((id, i) => ({ id, display_name: i ? 'Visual Studio Code' : 'Cursor', detected: true, cli_on_path: true, cli_installable: false, configured: !i, config_paths: [`${project}/.${id}/mcp.json`], panel: !i })), config: { profiles: {}, active_profile: {}, enabled_targets: ['cursor'], terminal_mode: 'builtin' }, all_targets: ['cursor', 'vscode'] },
  '/api/usage/savings': savings, '/api/usage/mcp-quality': quality,
  '/api/usage/mcp-trace/path': { path: `${project}/.ax/mcp.log`, projectLabel: 'Ax demo', projectRoot: project, verboseMcp: true, logExists: true, logDay: '2026-10-11' },
  '/api/usage/mcp-trace/chunk': { lines: [], has_more: false },
  '/api/usage/pricing': { status: { today: '2026-10-11', synced_today: true, sources: [], price_rows: 2, benchmark_rows: 0, agent_rows: 0, aa_key_configured: false }, models: ['Example standard model', 'Example fast model'].map((display_name, i) => ({ date: '2026-10-11', source: 'builtin', model_id: `demo/model-${i}`, display_name, provider: 'Demo provider', input_per_mtok: 1 + i, output_per_mtok: 4 + i, cache_read_per_mtok: null, blended_3_to_1: 2, context_length: 128000, intelligence: null, coding: null, agentic: null })) },
  '/api/usage/pricing/history': [], '/api/usage/pricing/agents': [],
  '/api/remote-mcp/status': { csrf: 'demo-only', connections: [] },
  '/api/lsp/status': { servers: [], available: false },
};

export function screenshotResponse(url) {
  const path = new URL(url).pathname;
  if (path.endsWith('/events')) return { status: 200, contentType: 'text/event-stream', body: path.includes('mcp-trace')
    ? `retry: 600000\nevent: batch\ndata: ${JSON.stringify(tools.map((tool, i) => `[2026-10-11T12:0${i}:00Z] [ax-mcp] inbound tool=${tool} args={"query":"Explore project navigation","projectPath":"${project}"}`))}\n\n`
    : ': demo stream\n\n' };
  if (path.startsWith('/api/node/')) return { json: { node: { ...nodes[0], docstring: 'Demo symbol', visibility: 'public', is_async: 0 }, callers: [], callees: [] } };
  if (path.startsWith('/api/links/')) return { json: { outgoing: [], incoming: [] } };
  if (data[path] !== undefined) return { json: data[path] };
  return { status: 404, json: { error: `Screenshot fixture missing: ${path}` } };
}

/** Serve the actual production UI with synthetic API responses; no Rust build needed. */
export async function startScreenshotServer(uiDist) {
  const { createServer } = await import('node:http');
  const { readFile } = await import('node:fs/promises');
  const path = await import('node:path');
  const root = path.resolve(uiDist);
  const mime = { '.html': 'text/html', '.js': 'text/javascript', '.css': 'text/css', '.svg': 'image/svg+xml', '.png': 'image/png', '.ttf': 'font/ttf', '.woff2': 'font/woff2' };
  const server = createServer(async (req, res) => {
    const url = new URL(req.url, 'http://localhost');
    if (url.pathname.startsWith('/api/')) {
      const response = screenshotResponse(url.href);
      res.writeHead(response.status ?? 200, { 'Content-Type': response.contentType ?? 'application/json' });
      if (response.contentType === 'text/event-stream') {
        res.write(response.body);
        const timer = setInterval(() => res.write(': keepalive\n\n'), 20000);
        req.on('close', () => clearInterval(timer));
      } else res.end(JSON.stringify(response.json));
      return;
    }
    const file = path.resolve(root, '.' + decodeURIComponent(url.pathname));
    if (!file.startsWith(root + path.sep) && file !== root) { res.writeHead(403); res.end(); return; }
    try {
      const asset = path.extname(file) ? file : path.join(root, 'index.html');
      res.writeHead(200, { 'Content-Type': mime[path.extname(asset)] ?? 'application/octet-stream' });
      res.end(await readFile(asset));
    } catch { res.writeHead(404); res.end(); }
  });
  await new Promise(resolve => server.listen(0, '127.0.0.1', resolve));
  return { server, url: `http://127.0.0.1:${server.address().port}` };
}
