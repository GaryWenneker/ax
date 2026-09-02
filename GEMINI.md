<!-- AX_START -->
## ax

Call `ax_preflight` exactly once per turn **before all other work** whenever the `user-ax` MCP server is available. Team policy arrives via MCP inject — do not Read `.agents/` or `.ax/policy/` files when ax MCP tools are available.

**Git-shared team files:** `.agents/rules/` and `.agents/skills/` (each skill is a directory with `SKILL.md`). Do not load `.ax/policy-private/` or `.ax/policy-inactive/`.

**Inject fallback:** If preflight lacks `<ax_policy>` (empty inject/rules), call `ax_skill("startup")` once.

**Explore before Grep/Read:** For structural code questions, call `ax_explore` (or graph tools) before broad Grep/Read.

**Directive capture:** When the user states a durable rule — `je moet`, `altijd`, `nooit`, `voortaan`, `always`, `never`, `you must`, `@rule` — persist it. `ax_preflight` returns `directiveDetected` + a ready `captureProposal`; ask the questions it lists, then call `ax_policy_capture(action="save", rule)` after the user confirms. Works even if the project has no policy yet (the first save bootstraps it). Never silently ignore such a directive.

**Capability discovery:** ax is actively developed. Do not rely on cached knowledge of ax features — `ax_preflight` returns the latest capabilities, rules, and skills each call. Use any new tools or rules it returns.

**Version freshness:** Call `ax_status` at session start. If the index is stale or a newer version exists, warn the user and suggest `ax upgrade` or re-index.

**Tool reference:** `ax_explore`/`ax_search`/`ax_node` for code structure, `ax_impact`/`ax_callers`/`ax_callees` for change impact, `ax_affected` for test coverage, `ax_insights`/`ax_report` for whole-graph architecture, `ax_guard` before writes when CRITICAL rules exist, `ax_diagnostics` for IDE/linter correlation, `ax_policy_capture` for durable rules, `ax_context` for task context, **`ax_sync`** / **`ax_index({force:true})`** for re-index, **`ax_lsp`** for LSP status/enrich, **`ax_ship`** for quality-gate evaluate/ci, **`ax_policy_index`** to refresh rules from disk, **`ax_remember`/`ax_recall`** for memory. Prefer these MCP tools over shelling out to the CLI when MCP is connected.

Run preflight exactly once per turn. MCP unreachable → report `ax MCP unreachable: [error]`, state `Mode: DEGRADED`; do not proceed silently.
<!-- AX_END -->
