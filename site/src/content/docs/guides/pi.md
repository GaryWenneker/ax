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

```json
{
  "pi": {
    "integration": true,
    "optimization": { "mode": "observe" }
  },
  "budget": { "monthly": 60, "currency": "EUR" }
}
```

Context selection uses the existing context planner. Always-apply rules stay even when they exceed the budget. The default cap is `AX_CONTEXT_MAX_TOKENS` (12000).

## Commands

`ax bootstrap` writes the architecture seed: Pi owns execution, Ax owns intelligence, and an Ax failure does not stop Pi. `ax init` applies that seed. `ax bootstrap --verify` checks it.

```bash
ax bootstrap
ax agent economics
ax agent economics --json
ax agent optimize --report
```

Figures are estimates. A missing catalog rate stays unknown. Tool runtime is not billed as model tokens. Only tokens delivered into a later model context are priced, and that amount is not added on top of the model usage quote.

`ax agent economics` reads `budget.currency` and `budget.usdPerEur` from the same settings as `ax budget`. Euro amounts are USD divided by `usdPerEur`. Without that rate the report stays in USD.

`ax_context` adds a `piContext` object. That object is the selected rules, skills, memories, and graph hits for the task, capped by `AX_CONTEXT_MAX_TOKENS`. It is not written into the Pi transcript.

A whole-file read or symbol search that the read guard already redirects is also recorded as an optimization row when `.ax/ax.db` exists. The guard's allow or deny decision is unchanged.

## MCP

The existing server exposes `ax_tool_economics`, `ax_optimization_advice`, and `ax_cost`. Repository context still goes through `ax_explore`, `ax_node`, and `ax_context`.
