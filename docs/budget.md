# Agent cost and budget

Ax quotes local agent usage against a price catalog and compares that quote to a budget you set. It does not call a provider, and it does not block one.

## What is measured

Each imported turn is one event in `~/.ax/usage.db` (`agent_usage_event`). Claude Code JSONL can include input, output, cache read, and cache write. Those four counts stay separate. Cursor transcripts often have a model name and sometimes input tokens. Output and cache are unknown when the file does not contain them. OpenCode, Gemini, and Codex are installed as MCP clients only. Ax does not invent token counts for them.

## Actual, estimated, unknown

| Label | Meaning |
|---|---|
| Estimated | Tokens were present and a catalog rate exists for that token class. |
| Unknown | The tokens or the rate is missing, or the rate is not a finite non-negative number. |
| Actual | Reserved for a provider invoice. Ax does not receive one, so catalog math is never labeled actual. |

A missing model price stays unknown. Ax does not fill it with the savings reference model. `ax savings` is unchanged: it still estimates tokens avoided on the reference input rate and says so.

## Pricing source

`ax pricing sync` stores daily OpenRouter rates, including cache read and cache write when the feed has them. `~/.ax/pricing.toml` overrides the cache. Built-in fallback rates are only for the savings counterfactual.

## Budget

`~/.ax/config.json`:

```json
{
  "budget": {
    "enabled": true,
    "monthly": 60,
    "currency": "eur",
    "usdPerEur": 1.08,
    "workingDays": 22,
    "hoursPerDay": 8,
    "warningPercent": 75,
    "criticalPercent": 90,
    "hardLimitPercent": 100
  }
}
```

EUR is compared only when `usdPerEur` is set (USD for 1 EUR). Without it, Ax prints the EUR budget and does not convert. Catalog math stays in USD.

The month plan uses the first N weekdays. Projection blends the month run-rate with the last five elapsed working days. At 75% the decision is warn, at 90% warn, at 100% deny. Deny is advice. `ax_budget` returns `enforced: false`.

`agent.budgetMode` is `cheap`, `balanced`, or `quality`. Bands use `cheapMaxInputPerMtok` and `standardMaxInputPerMtok` (defaults 1 and 5 USD per million input tokens). Model names are not the band.

## Context

`context.budgetTokens` is the optional token budget for context selection. Hard-required blocks stay even when they exceed it. Optional blocks are dropped whole. When that budget is set, preflight still sends always-apply rules, matched memories, and the working-context snapshot, then omits the context catalog and the memory-title list once the budget is spent. A snapshot hash is FNV-1a over the delivered parts so a repeat can be compared as empty, same, partial, or changed. That hash is not a provider prompt cache.

## Cycle efficiency

`ax costs` prints cost, input, and output per known cycle, plus a cache hit ratio of cache-read tokens over input plus cache-read. A known cycle is one usage event with a complete catalog quote. That is not a completed task, and it is not a model ranking. A missing token class stays unknown. `ax budget simulate` prints p50, p90, p95, and p99 from those recorded cycles. A `--cost-per-cycle` fixture has no distribution, so those percentiles stay unknown.

## Commands

```bash
ax costs
ax costs today
ax costs month
ax budget plan
ax budget simulate --cycles 500
ax budget set --monthly 60 --currency eur --usd-per-eur 1.08 --working-days 22 --hours 8 --mode balanced --context-tokens 12000
```
