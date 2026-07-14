---
name: ax MCP bootstrap
alwaysApply: true
description: ax MCP bootstrap — call ax_preflight every turn before other work.
---
# ax

Call **`ax_preflight`** exactly once per turn **before all other work**. This is mandatory whenever the `user-ax` MCP server is available — regardless of whether `.ax/policy/` exists locally. Team rules and skills arrive via MCP inject — do not Read `.ax/policy/` files on disk when MCP policy tools are available.

**Inject fallback:** If the response lacks team policy (no `<ax_policy>` in `inject`, or empty `rules`), call **`ax_skill("startup")`** once — then continue.

**Directive capture:** When the user gives durable rules (`je moet`, `always`, `never`, `@rule`), call **`ax_policy_capture`** with `action: "propose"`, ask each question from `questions[]`, and save only after explicit confirmation (stored in ax.db).

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
| Pre-write policy guard (when CRITICAL rules exist) | `ax_guard` |
| Capture durable rules | `ax_policy_capture` |
| Re-index after large changes | `ax_index` |
| File context for a symbol | `ax_node` |
| Build task context | `ax_context` |

## Version freshness

If `ax_status` reports a stale index or outdated version, warn immediately and suggest `ax upgrade` or re-index.

## Degraded mode

MCP unreachable → report `ax MCP unreachable: [error]`, state `Mode: DEGRADED`. Do not silently proceed without policy checks.

Full guide: [Policy Engine](https://getax.wenneker.io/guides/policy-engine/).
