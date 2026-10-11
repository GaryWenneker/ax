# EVIDENCE — Prices legend, Cursor, charts

- Spec: `/Users/gary/io/ax/docs/specs/cc-prices-legend-providers.md`
- Spec approval: not obtained (autonomous run)

## Mapping

| Behavior | Check |
|---|---|
| P1 distinct colors | pricesUi.test P1 |
| P2 single-point x | P2 |
| P3 context | P3 `131,072` |
| P4 tight cards | CSS `gap: 0.4rem`; no `min-height: 240px` |
| P5 Cursor | P5 merge tests |
| P6 jump legend | Prices.tsx `prices-provider-legend` |
| P7 labels | P7 |
| C1 skip free default | `pickDefaultModelId` test |
| C2 no fake $0.01 axis | `chartDomain([0,0])` → `{0,0}`; RateChart free hint |

OpenRouter live check (2026-09-16): 444 models, **0 Cursor**, no Groq/Together/Fireworks/Azure. Present: openai, anthropic, google, x-ai, microsoft, cohere, perplexity, amazon, plus auto `~` slugs. Cursor is curated.

## Gauntlet

```
node --test crates/ax-web/web-ui/src/lib/pricesUi.test.ts  # 10 pass
npm --prefix crates/ax-web/web-ui run build               # tsc && vite, exit 0
```

Skipped: mutation, coverage fail-under, cargo-audit (no new deps).
