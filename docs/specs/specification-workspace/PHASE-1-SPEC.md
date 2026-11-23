# Phase 1 — links, gated schema, and the checklist store

Derived from [SPEC.md](SPEC.md), which the owner accepted on 2026-11-22 ("treat as accepted"). The agent wrote this executable breakdown; it has not been approved separately. Facts come from [PHASE-0-INVENTORY.md](PHASE-0-INVENTORY.md).

Revision 2 folds in the owner's answers of 2026-11-22. It adds the human `completed` state and test B16, the project-and-role access rule, and the live database go-ahead.

Tier 3: persistence, permissions, and concurrency.

Phase 1 is three slices, built in order. This file specifies 1A and 1B. Slice 1C (evidence links, cursor and time-scoped reads, historical snapshots) gets its own revision of this file before it starts.

## Slice 1A — workspace flag, gated schema, spec–plan links, read-only dry run

New module `crates/ax-usage/src/spec_workspace.rs`. New tables `spec_plan_links` and `spec_plan_link_events` exist only after the flag is on.

| # | Behavior | Test |
| --- | --- | --- |
| A1 | With the flag off, reading the flag and listing documents create no workspace table. | `the_workspace_flag_defaults_off_and_creates_nothing` |
| A2 | Turning `spec_workspace` on creates the tables and their indexes (link pair; spec with order; item by document revision). | `turning_the_flag_on_creates_the_workspace_tables` |
| A3 | A plan linked to two specs lists under both with one plan id and one revision history (criterion 2). | `a_plan_can_link_to_two_specs_with_one_identity` |
| A4 | A relation type outside `implements`, `depends_on`, `alternative_to`, `phase_of` is refused with that list in the error. | `relation_types_are_validated` |
| A5 | A spec with two `phase_of` plans (order 1 and 2) and one `alternative_to` plan lists them in order with their relation types (criterion 1). | `phase_order_is_kept_and_alternatives_are_marked` |
| A6 | Adding and removing a link each write an audit event (actor, actor kind, reason, time). A removed link is no longer listed; its history stays. | `link_and_unlink_are_audited` |
| A7 | A link added by an agent is `proposed` until a human confirms it. Proposed links are labelled as such and never counted as confirmed. | `an_agent_link_stays_proposed_until_a_human_confirms` |
| A8 | Each existing hierarchy plan becomes a confirmed, primary `implements` link to its `owner_spec_id`, with origin `migrated`. | `existing_hierarchy_plans_migrate_as_primary_implements_links` |
| A9 | A legacy document with `plan_mode` revisions and no hierarchy plan becomes a Needs review item with its path, revision count, and detected type, and no invented parent (criterion 10). | `legacy_plan_documents_without_a_parent_go_to_needs_review` |
| A10 | The dry run on a database opened read-only returns its report; the database file's hash and `sqlite_master` are identical before and after. | `the_dry_run_changes_nothing` |
| A11 | The dry run reports specs, spec revisions, plans, plan revisions, legacy plan documents, links to create, and review items, and every count matches the source tables. | `the_dry_run_report_reconciles_counts` |
| A12 | Applying the migration twice creates no duplicate link or review item. | `apply_is_idempotent` |
| A13 | Turning the flag off brings back the current list payload; workspace tables stay. | `turning_the_flag_off_restores_the_current_list` |

## Slice 1B — checklist store

New module `crates/ax-usage/src/checklist.rs`. Tables `checklist_items` and `checklist_events` exist only after the flag is on.

- An item has a stable id, a document kind and id, the document revision, an optional parent, an item type (requirement, acceptance, phase, step, task), a critical flag, the required evidence, a title, and a position.
- An event is append-only: kind (`checked`, `unchecked`, `started`, `blocked`, `submitted_for_review`, `accepted`, `rejected`, `reopened`, `waived`), actor id and kind, session and turn, revision, reason, evidence links, idempotency key, per-item sequence number, and time.
- The two states the spec names are derived from the events, never stored. `implementation_state` is one of open, in_progress, agent_completed, completed (reported by a human, owner decision), reopened, blocked. `validation_state` is one of not_reviewed, pending_review, accepted, rejected, waived.

| # | Behavior | Test |
| --- | --- | --- |
| B1 | Importing Markdown turns `- [ ] CHK-001 …` lines into items whose id is the key (`CHK-001`), or an `<!-- chk:ID -->` marker when present. Position and nesting by indentation are kept. A line without a key gets a new id and the marker on export. | `items_are_imported_from_markdown_with_stable_ids` |
| B2 | Export writes `- [x]` for completed items, keeps every id, and a second import creates no duplicate (AC-20). | `export_round_trips_ids_and_states` |
| B3 | An agent check with the required evidence gives `agent_completed` and `pending_review`, never `accepted` (AC-16). | `an_agent_check_with_evidence_is_agent_completed_and_not_accepted` |
| B4 | An agent check without the required evidence is refused and writes nothing. | `an_agent_check_without_required_evidence_is_refused` |
| B5 | A human uncheck gives `reopened`; the history lists the check and the uncheck in order, with actors (AC-17). | `a_human_uncheck_reopens_and_keeps_history` |
| B6 | Two writers with the same expected sequence: the first is stored, the second gets `conflict` with the current sequence, and no event is lost (AC-18). | `a_stale_expected_sequence_is_a_conflict` |
| B7 | Repeating a request with the same idempotency key returns the first result and writes nothing. | `the_same_idempotency_key_returns_the_first_result` |
| B8 | An item accepted on revision 1 whose text changes in revision 2 is `pending_review` in revision 2; the acceptance on revision 1 stays queryable (AC-19). | `a_changed_requirement_needs_revalidation_in_the_new_revision` |
| B9 | "As of time T" returns only events stored at or before T, including check and uncheck history (AC-21). | `as_of_shows_only_events_known_then` |
| B10 | An agent cannot accept a critical item. An owner or approver who did not report its completion can. The release gate stays blocked while any critical item lacks acceptance, or has a failed or missing evidence link (AC-22). | `an_agent_cannot_accept_a_critical_item` |
| B11 | An actor kind `agent` cannot record any human action, whatever member id it sends. MCP forces the actor kind to `agent`. | `an_agent_cannot_act_as_a_human` |
| B12 | Invalid transitions are refused: accept on an item that is not completed, reject or reopen or waive without a reason. | `invalid_transitions_are_rejected` |
| B13 | A plan linked to two specs counts its items once in each spec's roll-up and once in the project total. | `a_shared_plan_is_counted_once` |
| B14 | A parent is not verified because all its children are; roll-up progress counts the children separately. | `a_parent_is_not_verified_because_its_children_are` |
| B15 | `ax checklist` updates items with no MCP server running and applies the same permission and audit rules as HTTP (AC-23). | `the_cli_updates_items_without_mcp` (in `crates/ax-cli`) |
| B16 | A human check gives `completed` and `pending_review`. Acceptance is still a separate action, and for a critical item it follows B10. | `a_human_check_is_completed_and_still_needs_acceptance` |

Interfaces, one JSON contract on three surfaces, as the hierarchy already does:

- HTTP: `GET /api/spec-workspace/checklist?document=…&revision=…&state=…&asOf=…`, `POST /api/spec-workspace/checklist/event`, `GET /api/spec-workspace/checklist/{item}/events`, `POST /api/spec-workspace/checklist/import`, `GET /api/spec-workspace/checklist/export`.
- CLI: `ax checklist list|event|history|import|export`.
- MCP: `ax_checklist` with an `action` field.

## Must not

Each line is mapped in EVIDENCE to a test, a gauntlet layer, or a stated reason.

- Request and response fields of every existing `/api/specs` route stay the same. Existing tests stay as they are.
- With the flag off, no code path creates a table. The live project is not touched unless the owner turns the flag on there.
- No existing row is deleted or rewritten. `specs` and `spec_revisions` get no new column in this phase.
- An agent never accepts, rejects, waives, or approves anything.
- `hierarchy_v2` keeps its current behavior.
- Every new route sits behind the existing request gate and read-only rules. Lists, counts, and filters only cover the caller's project (criterion 11 as project and role access, owner decision).
- No new dependency. Item and event ids are derived with `sha2` and `hex`, which `ax-usage` already uses alongside `serde_json` and `sqlx`.

## Failure model

| Failure | What catches it |
| --- | --- |
| Lost update when two people or agents change one item | B6, and a stress test: 16 concurrent writers on one item, where stored events equal accepted writes and every other write is a conflict |
| Double write on retry | B7, and a unique index on the idempotency key |
| An agent gaining human powers | B10, B11, and a test that the MCP handler ignores a supplied actor kind |
| Old validation counted for a changed requirement | B8 |
| Time travel showing later events | B9, and a randomized test with fixed seeds (no new dependency): for random event sequences, "as of T" equals replaying only the events up to T |
| Migration losing or inventing data | A9 to A12 |
| Tables created in the live database by a page view | A1, and a route test that `GET /api/specs` with the flag off leaves `sqlite_master` unchanged |

Known limit, as with hierarchy approval today: HTTP and CLI take the actor member id from the request until login binds it to a session.

## Gauntlet

One script, `scripts/spec-workspace-gauntlet.sh`, reruns every layer and stops at the first failure.

- **Tests:** `cargo test -p ax-usage` (all, including the new modules), all ax-web suites, the CLI test, and `npm test` for any UI helper added.
- **Lint and format:** `cargo clippy -D warnings` and `rustfmt --check` on changed files.
- **Coverage:** `cargo llvm-cov` on the new modules, failing below 100% of changed lines.
- **Mutation:** `cargo mutants` on the new modules. Survivors are killed, or classified as equivalent with a reason.
- **Stress:** the concurrency stress test, 20 runs, 0 failures.
- **Real run:** in an isolated temporary project:
  - turn the flag on and run the dry run;
  - import the CHK list from [SPEC.md](SPEC.md) and check CHK-001 as an agent with this phase's evidence;
  - try an agent acceptance and see it refused;
  - export, and diff against the source.
- **Latency:** the baseline script, extended with the checklist routes, against the budgets in [PHASE-0-INVENTORY.md](PHASE-0-INVENTORY.md).
- **Docs:** CLI, MCP, and site references in the same change (`docs-with-features`).
- **Review:** the review loop (`review-loop` skill) with the Rust review and the API review (`old-coder-api`), until a round has zero findings.

## Setup

- **Isolation:** none; the work happens in the main tree, `/Users/gary/io/ax`.
  - Why: the work builds on the uncommitted overhaul (`spec_hierarchy.rs`, the specs routes, the CLI and MCP commands). A worktree from `HEAD` would not contain that base.
  - Containment: new code lives in new files, and edits to shared files are additive registrations.
- **Commits:** none unless you ask.
- **Release binary:** builds use `cargo build --release -p ax-cli`, not `scripts/reinstall-cli.sh`, which would stop every ax process. Restarting the live server needs your go-ahead.
- **Files added or changed:**
  - `crates/ax-usage/src/spec_workspace.rs` and `crates/ax-usage/src/checklist.rs`, plus their registrations in `crates/ax-usage/src/lib.rs`
  - `crates/ax-web/src/spec_workspace_api.rs` and its mount
  - `crates/ax-cli/src/commands/checklist.rs` and its registration
  - the `ax_checklist` entry in `crates/ax-mcp/src/tools.rs`
  - `scripts/spec-workspace-gauntlet.sh`
  - docs under `site/src/content/docs/`, and `README.md`
- **Dependencies:** none new.

## Owner decisions folded in

- A human check is the new implementation state `completed` (test B16). It does not accept the item.
- Criterion 11 is project and role access (the last line under "Must not").
- When the owner turns the flag on for the live project, the tables may be created there and CHK-001 to CHK-012 imported. Until then the real run uses temporary projects only.
- Still proposals, not yet answered: the center-pane editor with modals for create and metadata (Phase 3), and the critical acceptance rule in B10.
