# Phase A0 inventory — spec / plan hierarchy

Date: 2026-11-22
Branch: `feat/spec-plan-context-overhaul`
Commit the branch points at: `43496761db03ba3256bd390f6f06d2a941abd2d0`
Working tree: dirty. This inventory describes that tree, not a clean commit.
Database: read-only `file:.ax/ax.db?mode=ro`. The `.ax` directory was not moved, copied over, or deleted.

Parent contract: `01-MASTER-SPEC.md` (accepted for implementation, plus Amendment 1).
Plan: `02-PLAN-FOUNDATION.md`.

A0 inventory is recorded. A1 is in `crates/ax-usage/src/spec_hierarchy.rs` and is not finished. On 2026-11-22, `cargo test -p ax-usage --lib spec_hierarchy` passed 16 tests and `cargo test -p ax-usage --lib spec_registry` passed 15. The live database was not migrated.

Landed in the library: atomic spec-plus-plan create, empty-plan rejection, duplicate-plan rollback, capture-key retry, `hierarchy_v2` default off, alternative plans, session links to two specs, workstream selection of two plans, agent approval block, owner approval that keeps the body, stale `based_on` conflict, a legacy dry-run queue, `needs_plan` with an execution-ready gate, and approval that reads `member_roles` instead of a claimed role list. A member of another project cannot approve.

`MemberRole::Approver` is now a real role beside owner. It can be stored, parsed, and shown in access settings. It does not receive `admin:team`, and it does not count as the last owner. An existing global database whose role check predates this role is rebuilt on the next schema init; the project database is not migrated. `cargo test -p ax-global-db --lib` on 2026-11-22: 32 passed.

Create, an alternative plan, a plan revision, and a session link each have one JSON contract. HTTP routes are `POST /api/specs/with-plan`, `/alternative-plan`, `/plan-revision`, and `/session-link`. CLI commands are `ax spec-create`, `ax spec-plan`, `ax spec-revise`, and `ax spec-link`. MCP tools are `ax_spec_create`, `ax_spec_plan`, `ax_spec_revise`, and `ax_spec_link`. A stale `basedOn` returns a conflict. Repeating a session link returns the same spec ids. `ax spec <id>` is unchanged.

Selection, compare, the project tree, and plan provenance use the same JSON contract. HTTP routes are `POST /api/specs/plan-selection`, `/plan-compare`, `/tree`, and `/provenance`. CLI commands are `ax spec-select`, `ax spec-compare`, `ax spec-tree`, and `ax spec-provenance`. MCP tools are `ax_spec_select`, `ax_spec_compare`, `ax_spec_tree`, and `ax_spec_provenance`. Compare returns both bodies and hashes. The tree lists each plan's latest version. Provenance returns the session, turn, and spec revision the plan revision was written against.

Human approval is `POST /api/specs/member-approval` and `ax spec-approve`. The JSON may include `roles`; that field is ignored. The role is read from global `member_roles`. An agent actor is refused. MCP `ax_spec_approve` forces the actor kind to `agent`, so an agent call cannot approve. `actorMemberId` is bound to the signed-in member: a different id is a member mismatch, a missing id is filled in, and the actor kind is forced to `human`. When web login is on, the member comes from the session cookie. When it is off, the local browser or CLI uses the active owner. The older `POST /api/specs/{id}/approve` route is unchanged.

`hierarchy_v2` still defaults off. While it is off, preflight uses the existing spec block and `GET /api/specs` keeps the flat list. Turning it on makes preflight a compact pointer (spec id and plan id, no plan body), and the specs list returns the hierarchy tree without importing legacy plan files. `capture_plan_draft` refuses while the flag is off and appends a plan revision while it is on. `capture_finished_plan`, including turn capture and file import, now checks the same flag. While the flag is on it does not create a spec or a plan. It records the id in `import_review` so a parent spec is not invented. While the flag is off, the old spec insert remains.

The Specs page keeps the flat list when `GET /api/specs` has no hierarchy mode. When `mode` is `hierarchy`, it lists each spec and then its plans in the same calm list. A plan row opens the parent spec. The plan body is not copied into the row. The editor is unchanged. The embedded Command Center bundle was not rebuilt in this step.

Rollback is `ax spec-flag off`, `POST /api/specs/flag` with `{"enabled":false}`, or MCP `ax_spec_flag` with the same body. That restores spec capture, the flat list, and the old preflight block. Rows already written under the flag stay. Nothing is dropped. Finished plans that arrived while the flag was on are listed by `GET /api/specs/review`, `ax spec-review`, and MCP `ax_spec_review`.

Owner decision on 2026-11-22: Plan B is preferred. SPEC-12 through SPEC-18 are out of Plan A's release gate. Authority, Manifest, and Replay stay reserved for Plan B. Plan A's gate is SPEC-01..07 and SPEC-20.

Cutover evidence is `A6-MIGRATION-REPORT.md`. A copy of the live database was dry-run outside `.ax`. The payload hashes were unchanged, 30 specs were queued, and no plan rows were inserted. The live database was not migrated.

A spec or plan can link one policy revision. The link stores the revision id and content hash. The policy body stays in `policy_revisions`.

The release binary embeds `/assets/index-CeRVtvNU.js`, and the Command Center on port 7070 serves that same file.

Still open: owner sign-off of that report. The live database stays unmigrated and the flag stays off. Plan B is not part of this implementation.

## Phase report

| Field | Record |
| --- | --- |
| Scope | Inventory only. No schema migration, no product behavior change. |
| Edited paths | `docs/specs/spec-plan-context-overhaul/01-MASTER-SPEC.md`, `02-PLAN-FOUNDATION.md`, `A0-INVENTORY.md`, `crates/ax-usage/src/lib.rs`, `crates/ax-usage/src/spec_hierarchy.rs` |
| Migration operations | None. Read-only counts and hashes. |
| New / reused entities | Proposal only. Live tables reused as the source of history: `specs`, `spec_revisions`, `spec_bindings`, `spec_deliveries`, `spec_file_seals`. |
| Spec acceptance IDs | Evidence for the inventory obligations in SPEC-17. No SPEC-01..06 behavior is implemented. |
| Test commands | RED, seen failing: `cargo test -p ax-usage --lib spec_hierarchy::tests::an_empty_plan_is_rejected_and_is_not_stored_as_a_spec -- --exact`. Result: `0 passed; 1 failed`. Panic: `called Result::unwrap_err() on an Ok value: "spec-only"`. Legacy `spec_registry` suite was not run. |
| Negative controls | None yet. |
| Unresolved risks | Listed at the end. |
| Human approval | Owner, 2026-11-22: the pasted spec and Plan A are the final version and implementation starts. Approver model is Amendment 1 ("A en B"). |
| Branch | `feat/spec-plan-context-overhaul` at `43496761db03ba3256bd390f6f06d2a941abd2d0`, plus the pre-existing dirty tree. No commit was created. |
| Next phase | A1. First change the RED test so it uses a temporary database and asserts zero `specs` rows, re-run it, and watch it fail. Then implement the transaction. Do not make the current test pass by returning an error string from the stub. Baseline before that edit: `cargo test -p ax-usage --lib spec_registry`. |

## 1. Live schema

`ensure_spec_schema` in `crates/ax-usage/src/spec_registry.rs` runs one SQL string of `CREATE TABLE IF NOT EXISTS`. It does not use the numbered `schema_versions` migrations in `ax-memory`. Adding a column later will not happen by itself: `CREATE TABLE IF NOT EXISTS` leaves an existing table unchanged.

There are no `FOREIGN KEY` clauses. A read-only connection reported `PRAGMA foreign_keys = 0`. That is the SQLite default for that connection. It does not prove what the application pool does. New checks must be explicit in the spec transaction. Do not turn foreign keys on for the shared project pool until an orphan audit of the whole database says that is safe.

Content hash is SHA-256 hex from `hash_text` (`sha2::Sha256` is imported at the top of `spec_registry.rs`).

```text
specs (id PK, project_id, title, markdown_path, created_at)
spec_revisions (PK spec_id+version, status, body, content_hash, source,
                session_id, turn_id, based_on, created_at)
spec_bindings (session_id PK, spec_id)
spec_deliveries (PK session_id+turn_id, spec_id, version, content_hash)
spec_file_seals (PK spec_id+path, content_hash, sealed_version)
```

Consequences for the new invariants:

- SPEC-01. `spec_bindings.session_id` is the primary key, so one session can point at one spec. Two sessions can point at the same spec. A session cannot bind two specs.
- SPEC-02. `approve_revision` updates status in place (`approved` rows become `superseded`, the target row becomes `approved`). The body is not rewritten. Status is not append-only today. There is no actor, reason, or approval time column.
- SPEC-05. `insert_revision` allocates `MAX(version)+1` without a surrounding immediate transaction in the function itself. Two overlapping captures can race. Idempotency for turn capture is `revision_for_turn(spec_id, session_id, turn_id)` inside `capture_finished_plan`, which returns the existing row. One turn can store one revision of one spec. A turn that writes both a spec and a plan needs a wider key.
- SPEC-12. Revision rows have no AX agent id and no provider chat id. `continuity.rs` `SessionIdentity` already separates `ax_session_id`, optional `ax_turn_id`, optional `external_chat_id`, optional `external_turn_id`, and `agent_provider`. It does not store a provider turn id that was invented. Reuse that split. Do not copy provider ids that were not issued.
- List side effect. `list_specs` inserts a binding `plan-{spec_id}` when a spec has no bindings. A read creates rows. Migration dry-runs must not call `list_specs`.

Kind is computed, not stored:

```text
kind = "plan" if any spec_revisions row has source = 'plan_mode', else "spec"
```

The approved spec-driven contract (`docs/specs/spec-driven/SPEC.md`, approved 2026-11-20) says kind is plan only when a `plan_mode` revision exists and nothing is approved. The code and the test `an_approved_plan_file_stays_a_plan` keep kind as plan after approval. The code is the behavior A1 must migrate. The sentence in the older contract is the drift.

Capture still stores a plan as a spec. `capture_finished_plan` inserts into `specs` with `markdown_path = docs/specs/{spec_id}/SPEC.md`. Turn capture runs only when `is_plan_request` matches a fixed phrase list (`make a plan`, `maak een plan`, `write a plan`, `create a plan`, `draft a plan`, `nieuwe spec`, `write a spec`, `maak een spec`). A file counts as finished at two or more non-empty lines (`plan_file_is_finished`).

## 2. Read-only database snapshot

Taken 2026-11-22 from the project `ax.db`. Hashes are SHA-256 over the listed columns in row order, with NUL between fields. Empty `spec_deliveries` hashes as SHA-256 of no bytes.

| Table | Rows | Payload bytes | SHA-256 |
| --- | ---: | ---: | --- |
| specs | 30 | 3638 | `5e82c182be798b7d8dadc8cec1f5a6d9a98907a15f9fbfb14897a1d61427c8b0` |
| spec_revisions | 34 | 254891 | `e45bba14dcbba6df8ec55290dc56c930be164c439e20b1b7b64a0569a2f09e3a` |
| spec_bindings | 32 | 2194 | `d2de034ff8019ddd2144dbaa75c94e9714b9eb43efce2d1c55388a0c5e5cc6d5` |
| spec_deliveries | 0 | 0 | `e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855` |
| spec_file_seals | 7 | 950 | `f10e4b8032fab84ff573caf8ee1a6c1ce6afd1807fb11e0e80eb515a890685df` |

Revision status: 31 draft, 2 change_requested, 1 approved. No duplicate content hashes. No revision whose spec row is missing. No binding whose spec row is missing. Versions run from 1 to 5.

Revision source: 31 `plan_mode`, 2 `drift`, 1 `edit`.

Every spec's `markdown_path` is under `docs/specs/`. None point at `docs/plans/` or `.cursor/plans/`.

Every spec has at least one `plan_mode` revision. The current list classifier would label all 30 rows `plan`.

Bindings: 30 rows match the synthetic key `plan-{spec_id}`. Two rows are real session ids, both bound to the same spec:

- `2057ea92-20ba-4bce-a732-3cbf46e61b9e` → `link-plans-to-sessions-b38a9318-plan`
- `e08eb680-799f-4e45-ae20-45ed81ada655` → `link-plans-to-sessions-b38a9318-plan`

The only approved revision is that same spec, version 3, source `edit`, based on version 2, body length 5406, content hash prefix `683758226f94c998` (full hash matches the preflight pointer). Versions 4 and 5 are `change_requested` drift rows, each based on version 3, body length 256. Preflight for this chat reports `drift: yes` for this spec. That approved body stays the historical record. Migration must not rewrite it.

### Classifier result on this database

SPEC-17 allows an automatic mapping only when it is unambiguous, and it forbids inventing a parent spec.

On this database the automatic result is:

| Class | Rows | Why |
| --- | ---: | --- |
| unambiguous first-class plan with an existing parent spec | 0 | No row has a separate owning spec. |
| unambiguous specification that already owns a plan | 0 | Plan and spec are the same row. |
| review queue, label `legacy_plan_as_spec` | 30 specs, 34 revisions | Every identity was captured as a spec with `plan_mode` history. |

`needs_plan` is the wrong label for these rows. They have plan text and no parent specification. The queue label is `legacy_plan_as_spec`. They stay readable. They are not attached to a newly invented spec. The already-approved revision stays approved history. It is not reclassified as execution-ready under the new hierarchy.

The two-line "finished file" rule and the phrase list are not unambiguous classification signals.

Synthetic `plan-{id}` bindings are not session history. Copy the two UUID bindings. Keep the 30 synthetic rows in `spec_bindings` so the old reader does not change, and exclude them from the new session-link table.

## 3. Integrations

| Surface | What exists | Overhaul note |
| --- | --- | --- |
| Library | `spec_registry.rs` capture, approve, list, save, markdown sync, file seals, preflight block | Authority for the current product. |
| HTTP | `specs::router`: `GET /`, `GET/POST /{id}/revisions`, `POST /{id}/approve` | Graph shows no caller of this router, so the mount prefix is not proven here. The 2026-11-20 contract names `GET /api/specs`. A3 confirms the mount before adding routes. |
| CLI | `crates/ax-cli/src/commands/spec.rs` indexes `show` and `project_root` only | Small surface. Parity work is mostly new. |
| MCP / preflight | Uncommitted `crates/ax-mcp/src/tools.rs` calls `spec_context_block` and wraps it in an `ax_section` named `spec` | This wiring is dirty working-tree code. The graph's callers of `spec_context_block` are tests. Preserve the compact one-spec block until the hierarchy flag is on. |
| UI | Dirty `Specs.tsx` (large), new untracked `SpecsDetail.tsx`, `specSession.ts`, `specSort.ts`, `unifiedDiff.ts` | In-flight editor, not the hierarchy. Do not replace it blindly. |
| Continuity | `SessionIdentity` in `continuity.rs` | Identity split to reuse. The harness does not prove live-model replay. SPEC-19 stays out of Plan A. |
| Access | `team_members.role` and `member_roles` in the global database. `bootstrap_owner` inserts role `owner` | Amendment 1. Project `ax.db` does not store members. Approval checks cross from the project action to the global database. |
| Replica | `ax-cli` `server_replica.rs` syncs snapshots and memories | It is not a spec-revision authority protocol. SPEC-15 is not satisfied by calling the current sync. |
| Policy | `policy_revisions` is a different table. The 2026-11-20 contract forbids reusing it for specs | Leave it alone. Link applicable policy versions later. Do not copy rule bodies into spec rows. |
| Markdown | Approved body is written to the path stored on the spec. Drafts do not create the file. Hash mismatch becomes `delivery: conflict` via `sync_markdown` | Projection stays a copy of the approved body. No `.ax/plans/`. |

`spec_context_block` also calls `import_workspace_plans` on every preflight. That is a write path on the read path. A1 must not add hierarchy imports to that call.

## 4. Uncommitted work already on this branch

The branch was created from `main` with a dirty tree, so the following changes came along and are not part of this overhaul yet:

- `crates/ax-usage/src/spec_registry.rs` — about +183 lines. Includes `live_bound_spec`, list ordering, and tests `an_approved_plan_file_stays_a_plan`, `a_live_session_plan_reaches_a_different_chat`, `the_list_uses_the_latest_revision_time_and_puts_the_newest_first`. The `SCHEMA` constant is outside those hunks.
- `crates/ax-mcp/src/tools.rs` and `crates/ax-mcp/tests/preflight_standard_contract.rs` — preflight spec section.
- `crates/ax-web/src/specs.rs` — small list-handler change.
- `crates/ax-web/web-ui` Specs page, detail page, session hop, diff helpers, and their tests.
- Untracked docs: `docs/specs/link-plans-to-sessions-b38a9318-plan/SPEC.md`, `docs/specs/specs-editor/`.
- Other dirty files (`ax-memory`, Memory page, site walkthrough) are outside this overhaul. Do not fold them into overhaul commits.

A1 edits `spec_registry.rs` only after a fresh diff of that file, so the in-flight session-link behavior is kept or explicitly migrated.

## 5. Schema proposal for A1

Additive tables in the project database. Old tables stay. Feature flag `hierarchy_v2` defaults off and lives in a new `spec_flags(key PRIMARY KEY, value TEXT)` row, not in an environment variable, so a manifest can record it.

```text
plans
  id PK
  project_id
  owner_spec_id -> specs.id
  title
  created_at

plan_revisions
  PK (plan_id, version)
  status, body, content_hash   -- SHA-256 hex, same hash_text
  source, session_id, turn_id
  agent_id                      -- AX id, null when unknown
  provider_chat_id              -- only when the provider issued one
  model
  based_on
  spec_id, spec_version, spec_content_hash
  created_at

workstreams
  id PK
  project_id, spec_id, name
  UNIQUE (project_id, spec_id, name)

plan_selections
  id PK
  workstream_id, plan_id, plan_version
  session_id, turn_id, created_at
  UNIQUE (workstream_id, plan_id, plan_version, session_id, turn_id)

approval_events
  id PK
  artifact_kind   -- spec | plan | decision
  artifact_id, version, action
  actor_member_id, actor_kind   -- actor_kind is human
  reason, content_hash, created_at

spec_session_links
  PK (session_id, spec_id)
  created_at

capture_keys
  PK (artifact_kind, artifact_id, session_id, turn_id, source)
  version
```

`spec_plan_links` is not added. A plan's owner is `plans.owner_spec_id`. A cross-reference that does not transfer ownership can wait until a caller needs it.

Workstream is a real table because the accepted spec requires selection per workstream and does not name an entity. This is the A0 refinement the spec asked for after inventory.

### Behavior the proposal locks

- Create spec and first plan in one transaction. The plan title and the plan body must both be non-empty after trim. An empty body is the empty placeholder the spec rejects. A short incomplete draft is allowed once the body is non-empty. If either insert fails, neither row remains.
- New revision rows are immutable in body and content hash. Approval appends `approval_events` and, in the same transaction, updates the status cache so current readers of `status = 'approved'` keep working. A test must prove the body bytes do not change.
- An agent principal cannot insert `action = 'approved'`. The actor must be a global-database member with role `owner` or `approver`.
- Version allocation takes an immediate transaction. A client whose `based_on` is not the parent it claims gets a conflict and both bodies. There is no merge.
- Capture idempotency is the `capture_keys` primary key, not "one row per turn".
- `needs_plan` is a column on `specs` for future imports that are specifications without a plan. It is not the label for today's 30 rows.
- Flag off: old list, old capture, old preflight block. Flag on: tree reads, transactional create, new links. Rollback is the flag returning to off. New tables are kept. `specs` and `spec_revisions` are never dropped.

### API compatibility window

Keep the current relative routes and `ax spec show` working while the flag is off. New hierarchy routes are added beside them. Breaking removals wait for an explicit versioned cutover in A6. This chooses the compatibility side of SPEC-17's "compatibility or explicit breaking change" for the whole of Plan A.

### What is intentionally not in Plan A tables

Context manifests, replay runs, and authority evaluation tables belong to Plan B. A5 reserves UI slots for them. On 2026-11-22 the owner took SPEC-12..18 out of Plan A's release gate. See section 7.

## 6. Acceptance mapping (inventory scope)

| ID | A0 finding | Test that will have to exist |
| --- | --- | --- |
| SPEC-01 | One session key cannot hold two specs. Plans are not a table. | Create two plans under one spec; bind one session to two specs; both links read back. |
| SPEC-02 | Status is updated in place. Bodies are already kept. | Approve a revision; body and hash of that version stay equal to the inserted bytes. |
| SPEC-03 | No actor on approval. Amendment 1 names owner and approver. | Agent approval is rejected. Owner approval appends an event. |
| SPEC-04 | No spec hash on a plan, because there is no plan row. | Plan revision stores the spec version and hash it was written against. |
| SPEC-05 | Phrase list and two-line file check. One revision per spec per turn. | The word "plan" in a sentence writes nothing. Retry with the same capture key writes one row. |
| SPEC-06 | `MAX(version)+1` is racy and there is no conflict response. | Stale `based_on` returns conflict and leaves the previous version as the only new head. |
| SPEC-14 | Members live in the global database. Project DB has the spec rows. | A member of another project cannot approve. |
| SPEC-17 | Snapshot hashes above. Classifier queues all 30 rows. | Dry-run report matches these counts and hashes and inserts zero parent specs. |
| SPEC-20 | Older contract remains `docs/specs/spec-driven/SPEC.md`. | Supersede link is written at cutover, not during A0. |

SPEC-08, SPEC-09, SPEC-10, SPEC-11, and SPEC-19 are Plan B. They are not given A1 tests.

## 7. Owner decision, 2026-11-22

Plan A phase A5 reserves Authority, Manifest, and Replay for Plan B. The original A6 sentence also required SPEC-12 through SPEC-18. The owner chose Plan B. Those SPEC items are out of Plan A's release gate.

Plan A's gate is SPEC-01..07 and SPEC-20. SPEC-12 through SPEC-18 remain in the master spec for Plan B. The code already written for project approval, hierarchy navigation, and the migration flag stays. It is not a reason to pull replay, manifests, or replica authority back into this plan.

Companion documents named by the spec and not in the repo: `03-PLAN-RESOLVER-AND-REPLAY.md`, `04-VALIDATION-SPEC-AND-PLAN.md`. SPEC-19 has no protocol to implement until 04 exists.

## 8. Destructive-migration check

No proposed step deletes or rewrites `specs`, `spec_revisions`, `spec_bindings`, `spec_deliveries`, or `spec_file_seals`. No proposed step creates `.ax/plans/`. No proposed step runs against the live database until a copy in a temp project has matched the hashes in section 2. `list_specs` must not run during that dry-run, because it inserts synthetic bindings.

## 9. Open risks

- Baseline `cargo test -p ax-usage --lib spec_registry` on 2026-11-22: 15 passed, 0 failed. The registry file has not been edited by this overhaul.
- The empty-plan test now uses a temporary database and asserts zero rows. It passed after the transaction check was added.
- `specs::router` mount path is not proven by the graph.
- Enabling SQLite foreign keys on the shared pool is unsafe until audited.
- Replica sync cannot carry approval heads today. SPEC-15 stays blocked on a later plan.
- Preflight reports a database incident and suggests `ax db repair`. Repair was not run. It would write the database, and this phase was read-only.
- The dirty Memory, web-ui dist, and site files on this branch are unrelated. They must stay out of overhaul commits.
