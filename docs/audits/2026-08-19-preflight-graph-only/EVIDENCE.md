# Evidence Report — Preflight / graph-only audit (Tier 3)

- Spec: `c:\gary\ax\docs\audits\2026-08-19-preflight-graph-only\SPEC.md`
- Spec approval: not obtained (audit delivered in the same turn as the spec; confidence is investigation-grade, not gauntlet-grade)
- Source state: working tree at audit time; graph snapshot from preflight: **11769 nodes, 35827 edges, 493 code files**
- Method: `ax_preflight` once, then `ax_explore` (graph) + targeted Read of files the graph named
- Independent verification: not performed
- Implementation: none (audit only)

## Verdict

**The user’s bar (“graph used at all times; no code scans”) is not met.**

| Claim | Result | One-line |
|---|---|---|
| C1 Preflight does not WalkDir source | **pass** | Preflight never calls `scan_files` / WalkBuilder |
| C2 Rules/skills from SQLite | **pass** | `match_policy` → `cached_rules_and_skills` → `list_rules` SQL |
| C3 `<ax_index>` from graph stats | **pass** | `get_stats()` COUNT queries + watcher pending list |
| C4 Explore symbols/edges from graph | **pass** | FTS `nodes` + `get_callers` / `get_callees` / edge SQL |
| C5 Snippets from graph store, not disk | **fail** | `numbered_snippet` and `build_context` `read_to_string` live files |
| C6 Agents can discover graph tools | **fail** | Lean `tools/list` hides search/node/callers/status/sync |

**Cannot honestly claim “10000% graph-only.”** Preflight itself is graph/DB. Query-time **source text** is still live filesystem. The MCP catalog **hides** most graph tools, which pushes agents toward Grep/Read.

## Spec → evidence mapping

### C1 — pass

`crates/ax-mcp/src/tools.rs` `preflight` (500–597):

1. `ax.match_policy(input)` — policy match
2. `ax.policy_status()` — policy metadata
3. `ax.get_stats()` — graph counts
4. `ax.get_pending_files()` — in-memory watcher
5. `ax_memory::recall_for_prompt` — memory vault
6. `detect_directive` / `propose_rule_from_prompt` — prompt string only

Cwd resolution (`resolve_preflight_cwd`, 483–498) calls `find_nearest_ax_root` which walks **parent directories** until `.ax/ax.db` exists (`crates/ax-context/src/directory.rs` 36–48). That is not a source-tree scan.

`collect_relative_files` (`matcher.rs` 223–231) only relativizes **open/changed paths the caller passed**. Preflight sets `changed_files: vec![]`.

`scan_files` (`orchestrator.rs` 111–141, WalkBuilder) is called from `index_all` / `sync_changed` / MCP `ax_index` / `ax_sync` — **not** from `preflight`.

### C2 — pass

`crates/ax-policy/src/matcher.rs` `match_policy` (75–116):

```
cached_rules_and_skills(pool) → list_rules(pool) + list_skills(pool)
```

`list_rules` (`crates/ax-policy/src/index.rs` 578–586) is `sqlx` `fetch_all` with `RULE_SELECT`. Cache key is the SQLite filename + `policy_generation` (`matcher.rs` 46–73).

Disk `.ax/policy/` is used at **index** time (`index_policy` comment at `index.rs:331`: load into SQLite when DB empty or disk files changed). Preflight match does not re-read those files.

This session’s preflight inject contained full always-apply rule/skill bodies from the store (not from a Read of `.ax/policy/`).

### C3 — pass

`get_stats` (`crates/ax-db/src/queries.rs` 838–886): `SELECT COUNT(*) FROM nodes|edges|files` plus grouped counts.

`format_index_inject_block` (`crates/ax-core/src/stats_format.rs` 49–131) formats those stats + pending watcher paths into `<ax_index>`. Observed this turn:

```
Graph: 11769 nodes, 35827 edges, 493 code files
```

### C4 — pass

`crates/ax-context/src/explore.rs` `explore` (34–156):

- `search_nodes` → FTS `nodes JOIN nodes_fts` (`queries.rs` 477+)
- `traverser.get_callers` / `get_callees`
- `get_incoming_edges` / `get_outgoing_edges`

No WalkDir on this path.

`ax_files` / `get_all_files` (`queries.rs` 961–967): `SELECT * FROM files` — graph table, not a walk.

### C5 — fail (blocking for “never read code, only graph”)

Graph nodes store metadata, not file bodies (`NodeRow::into_node`, `queries.rs` 1095–1124): `file_path`, line/column, `signature`, `docstring` — **no source column**.

Explore then reads the working tree (`explore.rs` 107–118, 193–207):

```
std::fs::read_to_string(&root.join(&node.file_path))
```

Same pattern in `crates/ax-context/src/builder.rs` 64–65 (`build_context` / `ax_context`).

This is **not** a repo scan (path and span come from the graph). It **is** a query-time code read, so C5 fails the user’s stated bar.

### C6 — fail (blocking for “graph used at all times” in the agent loop)

`is_core_tool` (`crates/ax-mcp/src/tool_filter.rs` 19–28) keep-list:

`ax_explore`, `ax_preflight`, `ax_policy_capture`, `ax_rules`, `ax_skill`, `ax_guard`

Default `AX_MCP_TOOLS` unset → `resolve_tool_allowlist_from(None)` → `None` → non-core tools **stripped from `tools/list`**. Tests assert `ax_search` and `ax_sync` are hidden (`tools.rs` 1590–1605, `new_tools_smoke.rs` 8–21).

Comment at `list_tools` (36–37): unlisted tools remain **callable**; they are just not advertised.

**This session’s catalog (GetMcpTools pattern `ax_`):** only `ax_explore`, `ax_preflight`, `ax_policy_capture`, `ax_rules`, `ax_skill`, `ax_guard`. `ax_status` returned “not found.” Agent-workflow still tells agents to call `ax_search` / `ax_node` / `ax_callers` / `ax_sync` / `ax_status`.

## Call spine (preflight)

```
MCP tools/call ax_preflight
  → tools.rs::preflight
      → resolve_preflight_cwd (parent walk for .ax/ax.db)
      → Ax::match_policy
          → matcher::cached_rules_and_skills (SQLite + generation cache)
          → score_rule / score_skill (prompt + caller file paths)
          → format_inject_block → <ax_policy>
      → get_stats → format_index_inject_block → <ax_index>
      → recall_for_prompt → <ax_memories>
      → detect_directive → optional captureProposal
```

## Call spine (explore — hybrid)

```
MCP tools/call ax_explore
  → tools.rs::explore → Ax::explore
      → search_nodes (SQLite FTS)          GRAPH
      → get_callers / get_callees          GRAPH
      → numbered_snippet read_to_string    DISK  ← C5 fail
```

## Index-time (not preflight; allowed to walk)

`scan_files` WalkBuilder (`orchestrator.rs` 111–141) is the extractor. Callers: `index_all`, `sync_changed`, CLI `ax index`, MCP `ax_index` / `ax_sync`. This is how the graph is built. It is a source scan by design.

## Gauntlet

| Layer | Result |
|---|---|
| Graph exploration of preflight/explore/match/stats | run (this report) |
| Full test suite | skipped — audit, no code change |
| Mutation | skipped — audit |
| Real MCP catalog check | run — 6 tools listed, graph extras absent |

## Known limits

- Did not dump `AX_MCP_TOOLS` from the live MCP process env; catalog observation matches the lean-default tests.
- Did not instrument a runtime tracer to prove zero `read_dir` under `preflight()`; evidence is call-graph + source of `preflight` and its callees named by the graph.
- `maybe_synthesize_explore` may LLM-rewrite explore text after the graph query; that is not a filesystem scan.

## What would make C5/C6 pass (not implemented)

1. Store snippet text (or file blobs) in SQLite at index time; `numbered_snippet` / `build_context` read the store, not disk. Or require `includeCode=false` and never attach live source.
2. Default `AX_MCP_TOOLS=all` (or put `search`, `node`, `callers`, `callees`, `impact`, `status`, `sync` in the core list) so agents can actually use the graph.

## Revisions

- 2026-08-19: first evidence pass; C1–C4 pass, C5–C6 fail.
