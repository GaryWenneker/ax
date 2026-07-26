<!-- AX_START -->
## ax

Call `ax_preflight` exactly once per turn **before all other work** whenever the `user-ax` MCP server is available. Team policy arrives via MCP inject — do not Read `.ax/policy/` files when ax MCP tools are available.

**Inject fallback:** If preflight lacks `<ax_policy>` (empty inject/rules), call `ax_skill("startup")` once.

**Explore before Grep/Read (CRITICAL):** For structural code questions (how X works, call paths, blast radius, architecture), call `ax_explore` (or `ax_search` / `ax_node` / graph tools) **before** broad Grep/Read. Treat explore source as already read — then open only the files the graph pointed to. `GetMcpTools` is not explore. Skipping this burns tokens and lowers MCP quality (`ExploreBeforeGrep`).

**Directive capture:** When the user states a durable rule — `je moet`, `altijd`, `nooit`, `voortaan`, `always`, `never`, `you must`, `@rule` — persist it. `ax_preflight` returns `directiveDetected` + a ready `captureProposal`; ask the questions it lists, then call `ax_policy_capture(action="save", rule)` after the user confirms. Works even if the project has no policy yet (the first save bootstraps it). Never silently ignore such a directive.

**Capability discovery:** ax is actively developed. Do not rely on cached knowledge of ax features — `ax_preflight` returns the latest capabilities, rules, and skills each call. Use any new tools or rules it returns.

**Version freshness:** Call `ax_status` at session start. If the index is stale or a newer version exists, warn the user and suggest `ax upgrade` or re-index.

**Tool reference:** `ax_explore`/`ax_search`/`ax_node` for code structure, `ax_impact`/`ax_callers`/`ax_callees` for change impact, `ax_affected` for test coverage, `ax_insights`/`ax_report` for whole-graph architecture (communities, god nodes, surprising links), `ax_guard` (`path` + `operation`; also `paths[]` / `action`) before writes when CRITICAL rules exist, `ax_policy_capture` for durable rules, `ax_context` for task context.

Run preflight exactly once per turn. MCP unreachable → report `ax MCP unreachable: [error]`, state `Mode: DEGRADED`; do not proceed silently.
<!-- AX_END -->
