# SPEC — Specs hierarchy UI (specs first, then plans)

Tier 2. Command Center Specs page: when `hierarchy_v2` is on, the list shows **specs only**. Plans open **under** an opened spec. Owner clarification: “first hierarchy specs, from there the plans.” Approved by “do it, you must.”

## Goal

A person opens a hierarchy spec from the list, lands on the Agreement tab, then opens Plans for that spec to navigate and edit plans. Authority stays under the same blade. Legacy mode (flag off) keeps the flat list and editor unchanged.

## Behaviors

### List (hierarchy mode)

1. Given a hierarchy payload with one spec and two plans, the list renders **one** row: that spec. No plan rows appear in the list.
2. `hierarchySpecRows(nodes)` returns only `kind: 'spec'` rows. `plansForSpec(nodes, specId)` returns that spec’s plans.
3. Existing `hierarchyRows` may remain for tests that assert the full tree; the Specs page must use `hierarchySpecRows` for the list.

### Blade

4. Opening a list row loads that spec’s Agreement (`GET /api/specs/{id}/revisions`) and sets tab **Agreement** by default.
5. Tabs: **Agreement** | **Plans** | **Authority**.
6. Breadcrumb when a plan is open: `Spec › {planId}` with a control to return to the Plans list without closing the blade.
7. Deep link `?spec={id}` opens Agreement. `?spec={id}&plan={planId}` opens Plans and that plan.
8. Escape / outside click still closes the blade.

### Plans tab

9. Plans tab lists the opened spec’s plans (id + latest version). Empty state when none.
10. Choosing a plan loads `GET /api/specs/plan/{id}/revisions` and shows SpecsDetail for that plan.
11. Saving a plan posts `POST /api/specs/plan-revision` with `planId`, `basedOn`, `body`, `sessionId: "web-ui"`, `turnId: "local"`. It does not rewrite an approved body in place.
12. Creating an alternative posts `POST /api/specs/alternative-plan` with projectId, specId, planId, planTitle, planBody, sessionId/turnId as above; then reloads list and opens the new plan.

### Authority tab

13. Authority tab shows `ManifestAuthorityPanel`. In hierarchy mode the panel is not duplicated above the list.

### Legacy mode

14. When `mode !== 'hierarchy'`, keep the flat list, kind filters, and page-level ManifestAuthorityPanel. No blade tabs required.

## Must not

- Migrate the live project database or force-enable `hierarchy_v2` as part of this UI change.
- Claim 95% replay fidelity.
- Rewrite approved revision bodies in place.
- Remove or weaken existing `hierarchyList` / SpecsDetail tests.
- Add a new npm dependency.
- Commit or deploy unless asked.

## Setup plan

- Isolation: edit in the working tree (UI + small API surface).
- Files: `docs/specs/specs-hierarchy-ui/SPEC.md`, `hierarchyList.ts` (+test), `Specs.tsx`, optional thin Plans panel component, `spec_hierarchy.rs` `list_plan_revisions`, `specs.rs` GET route, `lib.rs` export, CSS for blade tabs, EVIDENCE.md.
- Gauntlet: `node --test` on hierarchyList tests; vitest/npm test for web-ui unit tests touched; `npm run build` + reinstall CLI so embed matches dist.
- No new packages.

## Tests (RED first)

1. `hierarchySpecRows` for a node with two plans → one spec row only.
2. `plansForSpec` returns those two plans; unknown spec → `[]`.
3. Existing hierarchy parse test still passes (payload without bodies).
4. Backend: `list_plan_revisions` returns ordered plan revision rows for a seeded plan (Rust test in spec_hierarchy).
