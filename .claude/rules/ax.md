# ax

> **ABSOLUTE**: Every turn starts with `ax_preflight` — mandatory whenever the ax MCP server is available. Team policy lives in `.ax/policy/` and is delivered via MCP — do not Read policy files on disk when MCP policy tools are available.

## Turn order

```text
1. ax_preflight          [mandatory — prompt + open/changed files; full bodies in inject]
2. ax_explore / ax_context / ax_search   [code graph — NOT policy]
3. ax_guard              [before Write/Delete on project files when CRITICAL rules exist]
```

> **Run preflight exactly once per turn.** If you already called `ax_preflight` this turn, skip it and continue work.

**Inject fallback:** If step 1 returns no `<ax_policy>` inject (empty `rules`), call `ax_skill("startup")` once before other work.

## Directive capture

When the user states a durable rule — `je moet`, `altijd`, `nooit`, `voortaan`, `always`, `never`, `you must`, `@rule` — persist it. `ax_preflight` sets `directiveDetected` and returns a ready `captureProposal` (rule + `questions`). Ask each question, then call `ax_policy_capture(action="save", rule)` after the user confirms. This works even if the project has no policy yet — the first save bootstraps it. Never silently ignore such a directive.

## Capability discovery

ax is actively developed. **Do not rely on cached knowledge of ax features.** `ax_preflight` returns the latest matched rules, skills, and capabilities every call. When preflight returns tools or rules you haven't seen before, use them.

## Tool reference

| When | Call |
|---|---|
| Start of turn (always) | `ax_preflight` |
| Session start / version check | `ax_status` |
| Code architecture, how something works | `ax_explore`, `ax_search`, `ax_node` |
| Impact analysis before changes | `ax_impact`, `ax_callers`, `ax_callees` |
| Which tests are affected by changes | `ax_affected` |
| Architecture overview: communities, god nodes, surprising links | `ax_insights` |
| Full Markdown architecture report | `ax_report` |
| Pre-write policy guard (CRITICAL rules) | `ax_guard` (`path` + `operation`; also `paths[]` / `action`) |
| Correlate editor/linter diagnostics with the graph | `ax_diagnostics` (pass `diagnostics[]` gathered from the IDE) |
| Capture durable rules | `ax_policy_capture` |
| Incremental re-index after edits | `ax_sync` |
| Full index rebuild | `ax_index({ "force": true })` |
| LSP status / Exact-edge enrich | `ax_lsp` (`action`: `status` \| `enrich`) |
| Quality gate / CI evaluate | `ax_ship` (`mode`: `evaluate` \| `ci`) |
| Refresh policy from `.ax/policy/` | `ax_policy_index` |
| Store / search memories | `ax_remember` / `ax_recall` |
| Build task context | `ax_context` |

**Prefer MCP over shell:** When ax MCP is connected, call these tools directly — do **not** run `ax sync` / `ax lsp` / `ax ship --ci` / `ax policy index` / `ax remember` via the terminal. Shell CLI is only for DEGRADED mode or ops with no MCP tool (install, upgrade, web, share, ship --watch).

## Hard rules

- Never skip step 1 on a new user message.
- **Run preflight exactly once per turn** — do not re-call after the startup skill.
- MCP unreachable → report `ax MCP unreachable: [error]`, state `Mode: DEGRADED`, do not proceed silently.
- For structural code questions (how X works, call paths, blast radius) call `ax_explore` **before** broad Grep/Read — not policy tools and not a Grep-first sweep.
- If `ax_status` reports a stale index or outdated version, warn immediately and suggest `ax upgrade` or re-index.

Full guide: [Policy Engine](https://getax.wenneker.io/guides/policy-engine/).
