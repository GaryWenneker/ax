# EVIDENCE — cc-command-center-pages

Spec: `/Users/gary/io/ax/docs/specs/cc-command-center-pages.md`
Spec approval: not obtained (autonomous run)

Tier: 2

## Mapping

| Behavior | Check |
|---|---|
| S1 selection | `policy row selection` in `src/gitShare.test.ts` — 5 pass |
| S2 click outside | PolicySkills/PolicyRules pointerdown listener (browser) |
| S3 zip global | Compose `includeGlobal`, restore `copyToGlobal` |
| S4 sonar/agent | Removed from `NAV_MAIN_BASE`; `/sonar` `/agent` redirect to stats |
| S5 prices | Grouped by provider |
| S6 unresolved | Kind pills + scroller root |
| S7 savings | Optional `pricing` |

## Gauntlet

```
cd crates/ax-web/web-ui
node --experimental-strip-types --test src/gitShare.test.ts
# 19 pass, 0 fail
npx tsc --noEmit
# exit 0
npm run build
```

Mutation: skipped this turn (UI wiring). Negative control of selection helper is the RED-style assertions in S1.

Known limits: zip still packs `.agents` files only; global.db-only rows are listed then skipped on download.
