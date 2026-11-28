---
title: Pi integration
description: Observe a Pi agent run from Ax without taking over the agent loop.
---

Pi owns the agent loop, model calls, tool execution, and the conversation transcript. Ax observes that run and can advise the next turn with graph context, memories, rules, and skills.

Ax does not copy the transcript, does not start a second agent loop, and does not modify Pi. If Ax cannot record an event, Pi keeps running. The log line is `AX_PI_INTEGRATION_ERROR`.

## Modes

`optimization.mode` defaults to `observe`.

| Mode | Behavior |
|---|---|
| `observe` | Record session, turn, model, and tool metrics |
| `advise` | Also return alternatives such as `rg` to `ax_explore` |
| `optimize` | Approve a high-confidence deterministic alternative. Pi's `beforeToolCall` hook cannot rename a tool, so approval is a decision for the host, not a patched Pi runtime |

In `optimize` mode an exact repeat (same tool, arguments and repository state) is replayed from the earlier output only when the tool is read-only (`read`, `cat`, `rg`, `grep`, `search`, `ax_search`, `ax_explore`, `ax_node`), the earlier call succeeded, and the host has set a known repository state. Shell commands, edits and other tools always run again, and any such call drops the cached reads, because it may have changed what they saw. Other repeats are only suggested. At most 16 MiB of tool output is kept for replay; the oldest outputs are dropped first.

A search is matched to a known graph entity only when the search text names the whole entity on identifier boundaries, so `renewal` does not match `new`. Names shorter than four characters never match. A search that names only the last segment of a qualified entity (`advise_observation` for `ax_pi::economics::advise_observation`) is a medium-confidence suggestion and is never approved in `optimize` mode. A read counts as an indexed-source read when an indexed path is the trailing path components of the target, so `lib.rs` matches `src/lib.rs` but not `src/mylib.rs`.

```json
{
  "pi": {
    "integration": true,
    "optimization": { "mode": "observe" }
  },
  "budget": { "monthly": 60, "currency": "EUR" }
}
```

Context selection uses the existing context planner. Always-apply rules stay even when they exceed the budget. The default cap is `AX_CONTEXT_MAX_TOKENS` (12000). The 256 most recent selections are cached; older ones are selected again when asked for.

## Commands

`ax bootstrap` writes the architecture seed: Pi owns execution, Ax owns intelligence, and an Ax failure does not stop Pi. `ax init` applies that seed. `ax bootstrap --verify` checks it.

```bash
ax bootstrap
ax agent economics
ax agent economics --json
ax agent optimize
```

Figures are estimates. A missing catalog rate stays unknown. Tool runtime is not billed as model tokens. Only tokens delivered into a later model context are priced, and that amount is not added on top of the model usage quote.

`ax agent economics` reads `budget.currency` and `budget.usdPerEur` from the same settings as `ax budget`. Euro amounts are USD divided by `usdPerEur`. Without that rate the report stays in USD, and so does any other currency.

`ax agent economics` and the MCP `ax_tool_economics` tool report spend only; they do not compare it with a monthly budget. Use `ax budget` for that. The live Pi integration's own economics report does compare spend in USD with the monthly budget. Only a USD budget, or a euro budget with `usdPerEur`, can be compared. A monthly budget in any other currency (GBP, CHF and so on), or a euro budget without `usdPerEur`, is left out of that report rather than compared at an assumed rate; its `budget_omitted` field and its text Budget line say so.

`ax_context` adds a `piContext` object. That object is the selected rules, skills, memories, and graph hits for the task, capped by `AX_CONTEXT_MAX_TOKENS`. It is not written into the Pi transcript.

A whole-file read or symbol search that the read guard already redirects is also recorded as an optimization row when `.ax/ax.db` exists. The guard's allow or deny decision is unchanged.

## MCP

The existing server exposes `ax_tool_economics`, `ax_optimization_advice`, and `ax_cost`. Repository context still goes through `ax_explore`, `ax_node`, and `ax_context`.
