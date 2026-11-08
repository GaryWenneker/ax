# MCP ↔ CLI parity matrix

Baseline: local HEAD `6a3996f` / crate version `8.3.8` (gap pass on `feat/cli-mcp-parity`).  
Inventory source: `crates/ax-mcp/src/tool_filter.rs` (`CORE_TOOLS` + `GATED_BY_DESIGN`) and `ToolHandler::call_tool` in `tools.rs`.

Parity legend: **FULL** = same engine + CLI entry; **PARTIAL** = CLI exists but missing session/activity/JSON contract; **MISSING** = no first-class CLI; **N/A** = MCP-only ceremony.

**Gap-pass status (2026-11-08):** session/durable/preflight/cache/turn/doctor/cli-only install are **FULL** for the shared in-process engine. Graph CLIs gain central activity via `record_cli_command`. Deep History UI and `McpEngine` rename remain deferred (see `GAP-PLAN.md`).

| MCP tool | CLI equivalent | Engine | DB / side effects | Logging | Parity (now) | Notes |
|---|---|---|---|---|---|---|
| `ax_preflight` | `ax preflight` | in-process `call_tool` | policy match; memory recall; working context; reuse cache; budget | verbose enrich | **FULL** | Scenario F green |
| `ax_context` | `ax context` | `ax_core::build_context` / `ax_context` | none required | `log_cli` + activity | **FULL** | central activity |
| `ax_session` | `ax session *` | `ax_usage::working_context_apply` | `mcp_working_context` | CLI path | **FULL** | Scenario D green |
| `ax_durable` | `ax durable *` | `ax_usage::durable_project_apply` | `ax_durable_*` | CLI path | **FULL** | fork/handoff/doc-put/doc-get wired |
| `ax_stash` | `ax cache stash` | context cache store | `mcp_context_cache` | CLI | **FULL** | |
| `ax_expand` | `ax cache expand` | context cache load | `mcp_context_cache` | CLI | **FULL** | |
| `ax_cache_status` | `ax cache status` | `context_cache_counts` | read | CLI | **FULL** | |
| `ax_explore` | `ax explore` | graph explore | optional savings log | `log_cli` + activity | **FULL** | central activity |
| `ax_search` | `ax query` | FTS | optional | `log_cli` + activity | **FULL** | |
| `ax_node` | `ax node` | node details | cache possible | `log_cli` + activity | **FULL** | |
| `ax_status` | `ax status` | index stats | none | `log_cli` + activity | **FULL** | |
| `ax_files` | `ax files` | file list | none | `log_cli` + activity | **FULL** | |
| `ax_callers` | `ax callers` | callers | none | `log_cli` + activity | **FULL** | |
| `ax_callees` | `ax callees` | callees | none | `log_cli` + activity | **FULL** | |
| `ax_impact` | `ax impact` | impact | none | `log_cli` + activity | **FULL** | |
| `ax_cycles` | `ax cycles` | cycles | none | `log_cli` + activity | **FULL** | |
| `ax_path` | `ax path` | path | none | `log_cli` + activity | **FULL** | |
| `ax_api` | `ax api` | public API | none | `log_cli` + activity | **FULL** | |
| `ax_affected` | `ax affected` | TIA | none | `log_cli` + activity | **FULL** | |
| `ax_insights` | `ax insights` | insights | none | `log_cli` + activity | **FULL** | |
| `ax_report` | `ax report` | report | file write | `log_cli` + activity | **FULL** | |
| `ax_rules` | `ax policy` (rules) | policy | none | policy log | **FULL** | |
| `ax_skill` | `ax skill` / `ax policy` | policy/skills | none | policy log | **FULL** | |
| `ax_policy_capture` | `ax policy capture` | policy capture | policy DB | policy log | **FULL** | |
| `ax_guard` | `ax read-guard` + `ax doctor --mode cli` | guard / diagnostics | none | policy | **FULL** (mapped) | documented in site CLI “Guard mapping”; no dedicated `ax guard` subcommand |
| `ax_policy_index` | `ax policy index` | reindex policy | policy tables | policy log | **FULL** | |
| `ax_remember` | `ax remember` / `ax turn finish` | memory store | `memories` | memory log | **FULL** | auto `cli-turn` on finish |
| `ax_recall` | `ax recall` | memory search | read | memory log | **FULL** | |
| `ax_history` | `ax history` | turns + git | turns | memory log | **FULL** | research turns without files |
| `ax_diagnostics` | `ax doctor` | diagnostics | none | CLI | **FULL** | `--mode cli\|all` |
| `ax_index` | `ax index` | index | ax.db | workspace log | **FULL** | |
| `ax_sync` | `ax sync` | sync | ax.db | workspace log | **FULL** | |
| `ax_lsp` | `ax lsp` | LSP enrich | edges | lsp log | **FULL** | |
| `ax_ship` | `ax ship` | ship gate | ship state | ship log | **FULL** | |
| `ax_budget` | `ax budget` | budget | settings | — | **FULL** | |
| `ax_cost` | `ax costs` / `ax agent` | pi/cost | usage | — | **PARTIAL** | naming docs |
| `ax_tool_economics` | `ax agent` | pi economics | agent_* | — | **PARTIAL** | document mapping |
| `ax_optimization_advice` | `ax agent` | pi advice | agent_* | — | **PARTIAL** | document mapping |

## New CLI capabilities (not MCP tools, but required for CLI-only parity)

| Capability | CLI | Engine | Parity target |
|---|---|---|---|
| Explicit turn lifecycle | `ax turn begin/event/checkpoint/finish/fail/status` | `turn_lifecycle` + memory turns | FULL |
| Transport-agnostic activity | automatic + `ax turn event` | `activity` store + verbose log | FULL |
| CLI-only install | `ax install --mode cli-only` | installer | FULL |
| CLI doctor | `ax doctor --mode cli` | diagnostics | FULL |
| IDE hooks (optional) | existing `turn-hook` etc. | hooks | N/A (optional) |

## Observability parity (separate from functional)

| Surface | MCP today | CLI before | Target |
|---|---|---|---|
| Verbose log | yes | partial `log_cli` | all CLI ops + activity schema |
| Logging UI filter | MCP-centric + `cli` kind | limited | source filters: all/mcp/cli/hooks/agent |
| History | turn-hook / MCP turns | same DB if hooks used | CLI turns without hooks |
| Memory | remember + turn-hook | manual remember | auto turn memory on finish |
| Savings | MCP graph counterfactuals | CLI explore may log | measured vs estimated labels; no invention |

## Conditional / gated tools

`GATED_BY_DESIGN`: `ax_index`, `ax_lsp`, `ax_ship`, `ax_diagnostics`, `ax_policy_index` — already have CLI commands except diagnostics (→ doctor).
