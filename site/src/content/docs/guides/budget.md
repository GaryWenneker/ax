---
title: Agent budget
description: Quote local agent usage, plan a monthly budget, and simulate cycles without blocking provider calls.
---

Ax reads Claude Code and Cursor session logs already on disk, quotes each turn from the price catalog, and compares the known total with a budget in `~/.ax/config.json`. A missing price stays **unknown**. Catalog quotes are **estimated**. **Actual** is a provider invoice, which Ax does not receive.

`ax savings` is a separate counterfactual: tokens avoided versus opening whole files, priced on the reference input rate.

## Set a budget

```bash
ax budget set --monthly 60 --currency eur --usd-per-eur 1.08 --working-days 22 --hours 8 --mode balanced --context-tokens 12000
ax budget plan
```

EUR converts only when `usdPerEur` is set (how many USD equal 1 EUR). Leave it unset and Ax will not invent an exchange rate.

Warning, critical, and hard thresholds default to 75, 90, and 100 percent. Crossing the hard limit returns **deny** from `ax_budget`. That tool sets `enforced: false`. Ax does not block the provider HTTP call. An integration can refuse to start a turn when it sees deny.

## See spend

```bash
ax costs
ax costs today
ax costs month
ax pricing sync
```

Known events sum into the month. Unknown events are counted and omitted from the dollar total. `ax costs` also prints cost, input, and output per known cycle, and a cache hit ratio. A missing token class stays unknown. That block is not a model ranking. Claude logs can include input, output, cache read, and cache write. Cursor often has input only. OpenCode, Gemini, and Codex do not currently write token usage Ax can quote.

## Simulate

```bash
ax budget simulate --cycles 500
ax budget simulate --cycles 500 --cost-per-cycle 0.02
```

Cycles are fully priced turns. The monthly figure is average cycle cost times cycles per day times working days. Recorded cycles also print p50, p90, p95, and p99. A `--cost-per-cycle` fixture has no distribution, so those percentiles stay unknown. With no history and no `--cost-per-cycle`, the result is unknown.

## Stay under the number

- Sync prices (`ax pricing sync`) so known models stop showing as unknown.
- Import sessions (`ax savings import --all`) before reading `ax costs`.
- Prefer a cheaper input-price band with `agent.budgetMode` (`cheap`, `balanced`, `quality`). Bands use configured dollars per million input tokens, not a model name list.
- Set `context.budgetTokens` to drop optional context blocks whole. Always-apply rules stay. Once that budget is spent, preflight omits the context catalog and the memory-title list.
- Ask `ax_budget` before a long run. Treat warn as a reason to shorten the turn, and deny as a reason for the integration to stop. Ax will not stop the provider for you.


## Preflight size

`ax budget context` sets how large `ax_preflight` may be. The band is level 0–100, which is 400–8,000 tokens. The default is level 50 (4,000 tokens). This is not a percentage of the model window.

```text
ax budget context --level 50
ax budget context --tokens 4000
ax budget context --level 0 --project .
```

When both flags are set, `--tokens` wins. Values outside 400–8,000 are rejected. The same slider is on the Command Center Settings page ("Preflight size"). Every MCP tool reply ends with `tokens=<N> budget=<cap>`.
