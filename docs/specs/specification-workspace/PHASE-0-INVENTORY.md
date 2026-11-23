# Phase 0 — inventory and UX contract

Spec: [SPEC.md](SPEC.md), accepted on 2026-11-22. This file is the evidence for CHK-001.

The checklist store from section 8A does not exist yet, so CHK-001 cannot be checked inside AX. It is recorded here instead: implementation state `agent_completed` (this file), validation state `not_reviewed`, waiting for a human. When the checklist store exists, all twelve CHK items are imported with that state and this file as their evidence.

Everything below was read from the code at `HEAD` `43496761` plus the uncommitted overhaul in the working tree, through the ax graph. Live numbers come from read-only queries on `.ax/ax.db`. Nothing in the live database was changed.

## Summary

- The data model already has stable spec and plan ids, separate revision tables with session, turn, and agent provenance, an approval audit log, and links from documents to sessions, turns, and rule or skill revisions.
- A plan has exactly one parent (`plans.owner_spec_id`). There is no many-to-many link, no relation type, and no phase versus alternative designation.
- There is no checklist store, no general evidence-link table, no historical (as-of) query, and no per-document permission.
- The UI is a two-column page: a list and a blade that overlays it. The hierarchy list shows ids instead of titles, and its response carries only ids and latest versions.
- Logging and Memory links filter by session id only. Nothing carries a return link.
- CLI and MCP already cover the hierarchy writes and reads. Fourteen `ax_spec_*` tools are registered in the source.
- At 300 specs with 3 plans each, every existing read route answers in under 7 ms at the 95th percentile.

## Data model and API

| Spec requirement | State | What exists (evidence) |
| --- | --- | --- |
| Specification: stable id, project id, title, metadata, lifecycle status, approved revision id, timestamps | Partial | `specs` (id, project_id, title, markdown_path, created_at, needs_plan, execution_ready), shared by `spec_registry.rs` and `spec_hierarchy.rs`. Status and approved version are derived from `spec_revisions` (`pointer_pair`). No owner, tags, external PBI reference, or technology. |
| Plan: stable id, title, type, execution status, approved revision id, metadata | Partial | `plans` (id, project_id, owner_spec_id, title, created_at). No type, no execution status. `specs.execution_ready` is per spec. |
| SpecPlanLink, many-to-many with relation type, ordering, primary flag, provenance, approval state | Missing | One parent per plan (`owner_spec_id`). Each `plan_revisions` row pins `spec_id`, `spec_version`, and `spec_content_hash`. `workstreams` and `plan_selections` record which alternative was selected. |
| DocumentRevision, immutable, with parent, author or agent, session, turn, review state | Partial | `spec_revisions` and `plan_revisions`. Body and hash are never rewritten. `based_on` is the parent. Plans carry `session_id`, `turn_id`, `agent_id`; specs carry `session_id`, `turn_id`. The `status` column is updated in place when a revision is approved or superseded. Approvals are logged in `approval_events` (actor member, actor kind, reason, hash). |
| Lifecycle Draft → In review → Approved → Change requested → Superseded/Archived | Partial | States used in code: `draft`, `approved`, `change_requested` (set by file drift), `superseded` (set on older approved revisions). No `in_review`, `archived`, or `rejected`. |
| ContextEvidenceLink with origin, confidence, valid time interval | Missing | Separate sources exist: `spec_session_links`, `spec_bindings`, `spec_deliveries` (which spec version reached which turn), revision provenance, `artifact_policy_links` (document to rule or skill revision), `context_manifests` and `replay_runs`, memories with session ids. None records origin, confidence, or a validity interval. |
| AuthorityState | Partial | `pointer_pair` (approved, else latest), drift detection, `context_authority.rs` (manifest seal and approval, `classify_replica_manifest_sync`, `blocks_authority`). The classifier is not called from sync or replay. |
| List hierarchy with pagination | Partial | `GET /api/specs` with `hierarchy_v2` on returns every spec with `{planId, latestVersion}` per plan. No titles, statuses, counts, or paging. |
| Fetch spec or plan with revisions and links | Partial | `GET /api/specs/{id}/revisions`, `GET /api/specs/plan/{id}/revisions`, `POST /tree`, `POST /provenance`. |
| Create draft | Exists | `POST /api/specs/{id}/revisions` (registry), `POST /spec-revision`, `POST /plan-revision` with `basedOn`; a stale `basedOn` returns `conflict`. |
| Diff revisions | Partial | `POST /plan-compare` returns both bodies and hashes. The UI diffs on the client (`unifiedDiff.ts`). |
| Submit, approve, reject | Partial | `POST /{id}/approve` (registry) and `POST /member-approval` (owner or approver membership; agent actors refused). No submit-for-review, reject, or request-changes action. |
| Add or remove a relationship with audit | Missing | — |
| Filtered evidence with cursor and time scope | Missing | — |
| Resolve a back-reference | Partial | `specs_for_session`, `spec_pointer_for(session)`. |
| Historical snapshot | Missing | The raw material exists: `created_at` on revisions, approval events, deliveries, and `policy_revisions` / `global_policy_revisions`. Code-graph history has `node_as_of`; specs do not. |
| Indexes for project+status, link pairs, revision lookup, evidence targets | Partial | Primary keys only, for example `(spec_id, version)` and `(plan_id, version)`. |
| Every mutation permission-checked and audited | Partial | Approval is checked and audited. Other writes are only gated by the web login (opt-in) and the read-only hub. |

## Integration

| Requirement | State | Evidence |
| --- | --- | --- |
| Context Resolver reads approved versions by stable id | Exists | `spec_context_block` and `spec_pointer_for` give preflight the approved (else latest) version and hash for the bound session. The hierarchy has `gated_preflight_block`. |
| Authority Validator surfaces missing, conflicting, superseded, unapproved sources | Partial | The Authority tab lists sealed manifests with approval and replay state. It is not per spec or plan revision. |
| Rules and skills with scope, priority, effective version | Partial | `artifact_policy_links` and `POST /policy-link` store the rule or skill revision a document was written against. No UI. |
| Replay results tied to exact revisions and manifest | Partial | `replay_runs` belong to a manifest; manifests list their sources. Reports do not link back to the spec or plan. |
| Logging and Memory accept scoped deep links and a return link | Partial | `routes.ts` `buildPath`: Logging takes `kind` and `session`, Memory takes `session`, `memory`, `q`. No spec, plan, revision, or turn filter. No return context. |
| CLI and MCP parity | Exists for current operations | `ax spec-create`, `spec-plan`, `spec-revise`, `spec-link`, `spec-select`, `spec-compare`, `spec-tree`, `spec-provenance`, `spec-approve`, `spec-flag`, `spec-review`, and fourteen MCP tools in `crates/ax-mcp/src/tools.rs`. The MCP server running in this session predates them. |
| Stable, shareable deep links that do not leak | Partial | `/specs?spec=…&plan=…&tab=…`. There is no per-document permission, so "protected document" has no meaning yet. |

## Verification checklists (section 8A)

Missing. A `- [ ]` line is rendered as a checkbox in the preview and nothing more. There is no item identity, event log, state machine, permission check, concurrency check, API, or CLI. Because every phase must expose its items in AX, this store is built first in Phase 1.

## UI

| Requirement | State | Evidence |
| --- | --- | --- |
| Navigation entry | Exists as "Specs" | Under Policy, between Skills and Memory. |
| Workspace view: expandable specs with nested plans, status, progress | Partial | With `hierarchy_v2` on, the list shows one row per spec, but by id ([phase0-desktop-list.png](phase0-desktop-list.png)). Plans appear only in the opened spec's Plans tab, again by id ([phase0-desktop-plans-tab.png](phase0-desktop-plans-tab.png)). No inline expansion, status, or progress. |
| All documents view | Exists as the legacy list | With the flag off: shared column header (Title, Path, Version, Logging, Memory, Updated, Kind), kind and status chips, search, windows of 20 rows ([phase0-live-legacy-list.png](phase0-live-legacy-list.png)). |
| Needs review section | Partial | `GET /api/specs/review`, `ax spec-review`, `ax_spec_review` list queued finished plans. No UI. |
| Three-pane layout | Missing | Two columns; the blade overlays the list from `left: 8rem` ([phase0-desktop-spec-open.png](phase0-desktop-spec-open.png)). |
| Detail tabs Overview, Content, Plans, History, Validation, Dependencies | Partial | Agreement, Plans, Authority. |
| Evidence panel | Missing | Logging and Memory buttons per bound session in the list. |
| Editor: Markdown, WYSIWYG, Split, Diff, version comparison | Exists | Blade with version chips, compare chips, Save (appends a draft), Discard, Approve (clean draft only). Unsaved-change protection was not verified. |
| Historical mode | Missing | — |
| Narrow screens | Partial | The app has a phone layout with a menu button and a bottom bar. The Specs list centres the ids; an open blade fills the screen ([phase0-mobile-list.png](phase0-mobile-list.png), [phase0-mobile-spec-open.png](phase0-mobile-spec-open.png)). |
| Create flows in a modal | Violated today | "New alternative plan" is an inline form in the Plans tab, which the `modal-forms` rule forbids. |

## Permissions

- Roles are owner, approver, admin, member, and readonly (`MemberRole::default_scopes` in `crates/ax-global-db/src/access.rs`). Approver, member, and admin all have `memory:write`; readonly has read scopes only.
- No scope is specific to specs. Approval reads `member_roles` directly: owner or approver may approve; an agent actor never may, and MCP forces the actor kind to `agent`.
- Login is opt-in. Until an owner turns it on, `web_auth::gate` lets every request through. The read-only hub returns `readonly` for writes.
- There is no per-document or per-link permission.

## Tests today

- Rust: `spec_hierarchy.rs` 34, `spec_registry.rs` 15, `context_authority.rs` 30, `crates/ax-web/tests/context_authority_api.rs`. `crates/ax-web/src/specs.rs` has no tests of its own.
- UI unit tests: `hierarchyList`, `manifestAuthority`, `specSession`, `specSort`, `unifiedDiff`, `specsPage`.
- Playwright: 36 files under `crates/ax-web/web-ui/e2e`, none for the Specs page.

## Baselines and proposed budgets

Live project, read-only: 30 specs, 41 spec revisions (at most 12 for one spec), 0 hierarchy plans, `hierarchy_v2` off. `GET /api/specs` on the live server: median 29 ms over 10 runs, 14 KB.

Seeded baseline, `bash scripts/spec-workspace-baseline.sh` (defaults `SPECS=300 PLANS=3 RUNS=20`, release build `ax 10.0.2`, isolated temporary project, local loopback):

| Route | p50 | p95 | Size |
| --- | --- | --- | --- |
| `GET /api/specs` (hierarchy, all 300 specs and 900 plans) | 5.6 ms | 6.4 ms | 47 KB |
| `GET /api/specs/spec-001/revisions` | 2.7 ms | 3.5 ms | 225 B |
| `GET /api/specs/plan/plan-001-1/revisions` | 2.8 ms | 3.2 ms | 201 B |

Seeding took 900 CLI calls in 14 s, about 16 ms per call.

Proposed budgets for acceptance criterion 13, measured with the same script and dataset once the routes exist:

| Operation | Server p95 |
| --- | --- |
| Explorer page of 50 specs with titles, statuses, plan summaries, counts | 50 ms |
| Expand one spec (plans with type, status, approved version, progress) | 30 ms |
| Open a document with its revision list | 30 ms |
| Evidence tab, first page of 25 with a cursor | 100 ms |
| Checklist for one document (up to 200 items) | 30 ms |
| One checklist state change | 50 ms |

In the browser, expanding a spec should paint within 100 ms and an evidence tab within 300 ms, measured with Playwright performance marks. The budgets leave room for richer payloads; the current routes use about a tenth of them.

## Unsupported assumptions in the spec

1. **Protected documents (criterion 11).** AX has no per-document permission. Access is per Command Center (login opt-in) and per role scope.
2. **Plan type.** Every plan after the first is created as an "alternative". No plan is marked as a phase, and the first plan has no type.
3. **Multiple parents.** A plan revision pins one spec revision. A plan linked to two specs needs a rule for which spec revision each plan revision was written against.
4. **Review states.** `in_review`, `archived`, and `rejected` do not exist. `change_requested` is set only by file drift, never by a reviewer.
5. **Turn ordering for "as of turn N".** Turns have ids and timestamps but no project-wide sequence number, so "turn N" has to be resolved to a timestamp.
6. **A migration dry run that changes nothing.** `dry_run_legacy` calls `ensure_hierarchy_schema` first, so it creates tables in whatever database it opens.

## Policy constraints on the design

- **Lists:** `calm-lists` forbids `<table>` and `DataTable` on list pages. The All documents view uses `ItemList` with the shared column model (`ListColumns`, `columnModel`), which already supports resize, sort, and filter.
- **Forms:** `modal-forms` requires create and multi-field edit flows in `ModalShell`. The center-pane document editor is a routed primary view, which that rule allows; creating specs and plans and editing their metadata use `ModalShell`. The inline "New alternative plan" form moves into a modal.
- **Badges and contrast:** `outline-badges` (square outline chips with codicons, theme tokens) and `wcag-contrast` (AA) apply to every new badge.
- **Docs:** `docs-with-features` requires the site guide, the CLI and MCP references, and the README in the same change as each user-facing feature.
- **Build:** `web-ui-rebuild` requires a release rebuild before UI handoff. On this Mac `scripts/reinstall-cli.sh` stops every ax process, including MCP and the live server, so each handoff builds the release binary in place and asks before restarting anything live.

## Migration constraints

- No live database change without the owner. New tables are created only when the new `spec_workspace` flag is turned on for a project, or by an explicit migrate command. A page view with the flag off creates nothing.
- The migration dry run opens the database read-only, or works on a copy. It never calls a schema helper on the source.
- `specs` and `spec_revisions` are shared by the registry and the hierarchy, so changes to them are additive columns only.
- On the live project all 30 documents are legacy plans without a parent spec. Under criterion 10 they start in Needs review with their content, paths, and revisions intact.
- Rollback is turning the flag off. The old list and editor come back, new tables stay, and nothing is deleted.

## Phase plan

| Phase | Delivers | CHK |
| --- | --- | --- |
| 1A | `spec_workspace` flag, gated schema, spec–plan links with relation type, order, primary flag, and audit; read-only migration dry run with a reconciliation report | CHK-002, CHK-003 (dry run) |
| 1B | Checklist store: items, append-only events, implementation and validation states, agent versus human rules, critical acceptance, optimistic concurrency, idempotency keys, Markdown import and export, as-of queries; HTTP, CLI, and MCP | CHK-008, part of CHK-009 |
| 1C | Evidence links with origin and confidence; cursor and time-scoped reads; historical snapshot query | part of CHK-006, CHK-007, CHK-009 |
| 2 | Explorer, search, filters, deep links, Needs review queue, All documents view | CHK-004 |
| 3 | Detail pages, plan creation and linking in modals, lifecycle, diff, review, conflict handling | CHK-005 |
| 4 | Evidence panel, scoped Logging and Memory links with return context, historical mode | CHK-006, CHK-007 |
| 5 | Accessibility, performance budgets, permissions, shadow comparison, rollback rehearsal, docs | CHK-010, CHK-011 |
| — | Human review and release sign-off | CHK-012 |

## Decisions

The owner answered the first four on 2026-11-22; they are also recorded at the end of [SPEC.md](SPEC.md).

1. **Live database:** yes. When the owner turns the `spec_workspace` flag on for the live project, the tables may be created there and the twelve CHK items imported. Until then everything is tested in temporary projects.
2. **Protected documents:** criterion 11 means project and role access, with nothing leaking through counts or filters. No per-document permission model.
3. **Human completion:** a new implementation state, `completed`, for completion reported by a human.
4. **Name:** "Specs & Plans".
5. **Editing (proposal, not yet answered):** a center-pane editor as a routed view, with `ModalShell` for creating documents and editing metadata.
6. **Critical items (proposal, not yet answered):** only an owner or approver who did not report the completion may accept a critical item, and its required evidence must be present first.
