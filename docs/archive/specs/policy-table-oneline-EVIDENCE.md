# EVIDENCE: Rules and Skills tables are one-liners

**SPEC:** `/Users/gary/io/ax/docs/specs/policy-table-oneline.md`
**spec approval:** not obtained (autonomous run)
**tier:** 2
**Revision 2026-09-01 (tag truncation bug):** B1 now requires all tags to render. `compactTagItems` / `+N` removed.

Command: `bash /Users/gary/io/ax/tools/gauntlet-policy-table-oneline.sh`

```
T1 compactTagItems is not part of the list utils — pass
killed compactTagItems-export
gauntlet-policy-table-oneline: ok
npx tsc --noEmit — exit 0
```

## Mapping

| Behavior | Result |
|----------|--------|
| B1 all tags render | T1: `compactTagItems` not exported |
| B2 nowrap | `flex-wrap: nowrap` on `.policy-table-tags .policy-view-tags` |
| B3 dense padding | `padding: 2px 8px` |
| B5 no crush | `table.policy-table` `width: max-content` + `flex-flow: row nowrap` in CSS |

## Gauntlet (fresh after last CSS/TS edit)

See command output at top of this file. Do not use the older compactTagItems 4/4 numbers.

Negative control: `flex-wrap: wrap-never-valid` absent.

## Layers skipped

- Full Rust suite: CSS/TS UI only
- Browser E2E: no Command Center session in this turn; restart `ax web` after embed
- Coverage fail-under: no instrumented coverage tool in web-ui package.json
