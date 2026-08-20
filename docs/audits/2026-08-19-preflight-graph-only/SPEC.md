# SPEC — Preflight / graph-only audit

- Tier: 3 (core agent contract: policy inject + graph vs filesystem scans)
- Kind: **audit of existing behavior** — no implementation in this task
- Spec approval: requested with this document
- Isolation: none (read-only investigation via ax graph + targeted reads of graph-pointed files)
- Tools to install: none
- Git: no commits unless requested
- Files this audit adds:
  - `c:\gary\ax\docs\audits\2026-08-19-preflight-graph-only\SPEC.md`
  - `c:\gary\ax\docs\audits\2026-08-19-preflight-graph-only\EVIDENCE.md`
- New dependencies: none

## Scope (what “scan” means)

A **source-tree scan** is a WalkDir / ignore-walker / glob over the project that discovers and/or reads code files that were not named by the caller.

A **graph query** is SQLite against `nodes` / `edges` / `files` / `nodes_fts` / policy tables, plus in-memory watcher state.

A **targeted source read** is `fs::read_to_string` of a path the graph already named (line-range snippet). Not a tree walk. Still not “graph-only.”

**Index-time extraction** (`ax index` / `ax_sync` / `scan_files`) is out of query-time scope: it *must* walk the tree to *build* the graph. The claim under audit is query-time MCP (`ax_preflight`, `ax_explore`, policy match, graph tools).

## Claims (must be true for “10000% graph, never scan code”)

### C1 — Preflight does not walk the source tree

Given `ax_preflight` is called with a prompt,
When policy, index snapshot, and memories are assembled,
Then no `WalkBuilder` / `scan_files` / recursive `read_dir` of project source runs.
Filesystem use is limited to: `.ax/ax.db` existence, parent-dir walk for nearest ax root, and canonicalize of **caller-supplied** open-file paths.

### C2 — Rules and skills come from SQLite, not `.ax/policy/` on disk

Given policy is indexed into `ax.db`,
When preflight matches rules/skills,
Then bodies are loaded via `list_rules` / `list_skills` on the pool (with generation cache), not by reading `.mdc` / `SKILL.md` files during the match.

### C3 — `<ax_index>` is graph stats, not a live file inventory

Given the index exists,
When preflight injects `<ax_index>`,
Then counts come from `SELECT COUNT(*)` on `nodes` / `edges` / `files` (and grouped metadata), plus in-memory pending-sync entries — not from walking the repo.

### C4 — Graph tools resolve symbols and edges from SQLite

Given `ax_explore` / `ax_search` / callers / callees,
When a query is answered,
Then entry points and neighbors come from `search_nodes` + `GraphTraverser` SQL — not from grepping the tree.

### C5 — Source snippets at query time come from the graph store, not live files

Given `includeCode` is true (explore default),
When numbered snippets / context code blocks are produced,
Then source text is read from the graph database (or equivalent indexed store),
And `std::fs::read_to_string` of project source is not used.

### C6 — Agents can discover graph tools

Given MCP `tools/list` with default env (`AX_MCP_TOOLS` unset),
When an agent catalogs `user-ax`,
Then graph query tools needed by the agent-workflow rule (`ax_search`, `ax_node`, `ax_callers`, `ax_callees`, `ax_impact`, `ax_status`, `ax_sync`, …) are advertised — not lean-hidden.

## Must NOT

- Treat index-time `scan_files` as a preflight bug (building the graph requires a walk).
- Treat parent-directory lookup for `.ax/` as a source scan.
- Claim PASS on C5 if snippets are sliced from live files using graph line ranges.
- Weaken this spec after findings; append a revision instead.

## Revisions

- 2026-08-19: initial audit contract from user request (full audit of preflight rules/skills + graph; no query-time code scans).
