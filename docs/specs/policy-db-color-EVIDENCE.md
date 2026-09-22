# EVIDENCE: policy-db-color

spec approval: not obtained (autonomous run)

## Mapping

| Spec | Test |
|---|---|
| D1 project teal | `policyDbAccent` D1 |
| D2 global gold | `policyDbAccent` D2 |
| D3 row inset | `policyDbAccent` D3 |

## Gauntlet (fresh after last edit)

```
cd crates/ax-web/web-ui
node --test src/gitShare.test.ts
npx tsc --noEmit
```

Result: 7 pass, 0 fail; tsc exit 0.

Manual mutant: D1 fails if `PROJECT_DB_COLOR` is not `#3ee4b2` (import is the same constant the UI uses).

Skipped: coverage fail-under (no instrumented web-ui coverage gate); mutation tool (hand mapping via D1–D3); `npm run build` / `reinstall-cli.sh` recorded separately if run after this file.

## Limits

Visual WCAG of badge chrome is enforced by fixed `#141414` on `#3ee4b2` / `#e0b341`, not an automated contrast test in this change.
