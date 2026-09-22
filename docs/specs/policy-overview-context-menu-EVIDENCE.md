# EVIDENCE: policy-overview-context-menu

spec approval: not obtained (autonomous run)

## Mapping

| Spec | Check |
|---|---|
| M1–M3 menu items | `node --test src/gitShare.test.ts` policyOverviewMenuItems |
| P5 home global-only listed | `p5_lists_this_project_global_only_copy` |
| P6 sync keeps parked | `p6_sync_does_not_drop_parked_policy` |
| P7 flatten nested frontmatter | `p7_flattens_nested_frontmatter_for_list_ui` |
| F1 missing arrays no crash | `filterRules` F1 |

## Gauntlet (fresh)

```
cd crates/ax-web/web-ui && node --test src/gitShare.test.ts && npx tsc --noEmit
cargo test -p ax-global-db --lib
cargo check -p ax-web
```

Results: 10 JS tests pass; tsc 0; ax-global-db 9 passed; ax-web check ok.

Skipped: HTTP integration of POST /api/policy/relocate (no web-ui e2e in this run); mutation tool.

## Limits

Agents still match only project `ax.db`. Moving to global.db parks the item until moved back.
