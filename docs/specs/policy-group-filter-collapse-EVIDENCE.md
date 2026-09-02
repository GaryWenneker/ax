# EVIDENCE — Group filter + collapse/expand all

**SPEC:** `/Users/gary/io/ax/docs/specs/policy-group-filter-collapse.md`
**spec approval:** not obtained (autonomous run)
**Tier:** 2
**Entry point:** `/Users/gary/io/ax/tools/gauntlet-skill-groups.sh`
**Source state:** working tree (uncommitted)

## Behavior map

| Scenario | Test |
|---|---|
| F1 empty selection = all groups | `skillGroupFilter.test.ts` F1 |
| F2 hide unselected groups | F2 |
| F3 multiselect OR | F3 |
| F4 collapse all | F4 |
| F5 expand all | F5 |
| UI on Rules + Skills | grep `PolicyGroupListControls` in both pages |

**Mutant (F1):** temporarily `return false` for empty selection → F1 failed (`false !== true`); restored.

## Gauntlet (after last logic edit)

- `node --experimental-strip-types --test src/skillGroupFilter.test.ts` — **6 passed**
- `npx tsc --noEmit` in `crates/ax-web/web-ui` — **pass**
- Negative control: F1 mutant failed as above

### Layers skipped

- Full cargo workspace: skipped (no Rust change this slice beyond existing group tests in the same gauntlet script)
- Browser click-through: Command Center rebuild follows; no Cursor browser tools
- Mutation tool: skipped (manual F1 mutant recorded)

## Known limits

- Group filter options are groups present after search/tag/level filters, not the full catalog of empty folders
- Collapse all only folds currently listed folders
