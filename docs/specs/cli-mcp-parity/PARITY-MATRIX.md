# MCP ↔ CLI parity matrix

Baseline: local HEAD `6a3996f` / crate version `8.3.8`.  
Inventory source: `crates/ax-mcp/src/tool_filter.rs` (`CORE_TOOLS` + `GATED_BY_DESIGN`) and `ToolHandler::call_tool` in `tools.rs`.

Parity legend: **FULL** = same engine + CLI entry; **PARTIAL** = CLI exists but missing session/activity/JSON contract; **MISSING** = no first-class CLI; **N/A** = MCP-only ceremony.

| MCP tool | CLI equivalent (before) | Engine | DB / side effects | Logging | Parity (before → target) | Action |
|---|---|---|---|---|---|---|
| `ax_preflight` | none | `tools::preflight` → extract `preflight::run` | policy match; memory recall; working context; reuse cache; budget | verbose enrich | MISSING → FULL | Add `ax preflight`; share engine |
| `ax_context` | `ax context` | `ax_core::build_context` / `ax_context` | none required | `log_cli` | PARTIAL → FULL | Keep; add `--json` + activity |
| `ax_session` | none (hooks only) | `ax_usage::working_context_apply` | `mcp_working_context` | MCP path | MISSING → FULL | `ax session *` |
| `ax_durable` | none | `ax_usage::durable_project_apply` | `ax_durable_*` | MCP path | MISSING → FULL | `ax durable *` |
| `ax_stash` | none | context cache store | `mcp_context_cache` | MCP | MISSING → FULL | `ax cache stash` |
| `ax_expand` | none | context cache load | `mcp_context_cache` | MCP | MISSING → FULL | `ax cache expand` |
| `ax_cache_status` | none | `context_cache_counts` | read | MCP | MISSING → FULL | `ax cache status` |
| `ax_explore` | `ax explore` | graph explore | optional savings log | `log_cli` | PARTIAL → FULL | Activity + session bind |
| `ax_search` | `ax query` | FTS | optional | `log_cli` | PARTIAL → FULL | Alias docs + activity |
| `ax_node` | `ax node` | node details | cache possible | `log_cli` | PARTIAL → FULL | Activity |
| `ax_status` | `ax status` | index stats | none | `log_cli` | PARTIAL → FULL | Activity |
| `ax_files` | `ax files` | file list | none | `log_cli` | PARTIAL → FULL | Activity |
| `ax_callers` | `ax callers` | callers | none | `log_cli` | PARTIAL → FULL | Activity |
| `ax_callees` | `ax callees` | callees | none | `log_cli` | PARTIAL → FULL | Activity |
| `ax_impact` | `ax impact` | impact | none | `log_cli` | PARTIAL → FULL | Activity |
| `ax_cycles` | `ax cycles` | cycles | none | `log_cli` | PARTIAL → FULL | Activity |
| `ax_path` | `ax path` | path | none | `log_cli` | PARTIAL → FULL | Activity |
| `ax_api` | `ax api` | public API | none | `log_cli` | PARTIAL → FULL | Activity |
| `ax_affected` | `ax affected` | TIA | none | `log_cli` | PARTIAL → FULL | Activity |
| `ax_insights` | `ax insights` | insights | none | `log_cli` | PARTIAL → FULL | Activity |
| `ax_report` | `ax report` | report | file write | `log_cli` | PARTIAL → FULL | Activity |
| `ax_rules` | `ax policy` (rules) | policy | none | policy log | PARTIAL → FULL | Ensure JSON parity |
| `ax_skill` | `ax skill` / `ax policy` | policy/skills | none | policy log | PARTIAL → FULL | Ensure load path |
| `ax_policy_capture` | `ax policy capture` (if present) | policy capture | policy DB | policy log | PARTIAL → FULL | Verify CLI capture |
| `ax_guard` | `ax read-guard` (related) | guard | none | policy | PARTIAL → FULL | `ax policy guard` or document |
| `ax_policy_index` | `ax policy index` | reindex policy | policy tables | policy log | PARTIAL → FULL | Activity |
| `ax_remember` | `ax remember` | memory store | `memories` | memory log | PARTIAL → FULL | Auto path via turn |
| `ax_recall` | `ax recall` | memory search | read | memory log | FULL | Activity bind |
| `ax_history` | `ax history` | turns + git | turns | memory log | PARTIAL → FULL | Research turns without files |
| `ax_diagnostics` | none dedicated | diagnostics | none | MCP | MISSING → FULL | Fold into `ax doctor` |
| `ax_index` | `ax index` | index | ax.db | workspace log | FULL | Activity |
| `ax_sync` | `ax sync` | sync | ax.db | workspace log | FULL | Activity |
| `ax_lsp` | `ax lsp` | LSP enrich | edges | lsp log | FULL | Activity |
| `ax_ship` | `ax ship` | ship gate | ship state | ship log | FULL | Activity |
| `ax_budget` | `ax budget` | budget | settings | — | FULL | Activity |
| `ax_cost` | `ax costs` / `ax agent` | pi/cost | usage | — | PARTIAL → FULL | Align naming in docs |
| `ax_tool_economics` | `ax agent` | pi economics | agent_* | — | PARTIAL → FULL | Document mapping |
| `ax_optimization_advice` | `ax agent` | pi advice | agent_* | — | PARTIAL → FULL | Document mapping |

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
