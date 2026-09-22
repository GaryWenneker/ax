# EVIDENCE: Policy global.db (edit parked copies)

**Spec:** `/Users/gary/io/ax/docs/specs/policy-global-db.md`  
**Tier:** 2  
**Spec approval:** not obtained (autonomous run)

## Mapping

| Behavior | Check |
|---|---|
| P3 list origin | existing list hydrate tests H1, P5 |
| P4 UI Open/Edit on global | `policyOverviewMenuItems` M3 — ids `open`, `edit`, `move-project`, `delete` |
| P6 GET/PUT query | `originQs` Q1/Q2 — project empty; global `?origin=global&projectId=7` |
| P6 backend | `GET/PUT /api/policy/{skills,rules}/{id}` with `OriginQuery` |

## Gauntlet (fresh after last edit)

```
npx tsc --noEmit   # crates/ax-web/web-ui — exit 0
node --test src/gitShare.test.ts src/policyPackage.test.ts
# 35 pass, 0 fail
CARGO_TARGET_DIR=/Users/gary/io/ax/target-dev cargo check -p ax-web --offline
# Finished `dev` profile
GET/PUT http://127.0.0.1:7070/api/policy/skills/old-coder-api?origin=global&projectId=1 → 200
Command Center opened Global old-coder-api inline editor (Save enabled, body loaded)
```

Layers skipped: mutation tool (manual originQs covered by Q1/Q2); cargo-audit (no new deps).

## Known limits

Agents still match only the current project `ax.db`. Zip pack cannot include global-only disk-less copies. Revision history is hidden for global rows (revisions live in project store).
