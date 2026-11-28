# Plan B phase B0 contract

Date: 2026-11-22
Plan: `03-PLAN-RESOLVER-AND-REPLAY.md` (`AX-OVERHAUL-PLAN-RESOLVER`), accepted for implementation on 2026-11-22 when the owner said "all approved".
Parent: `01-MASTER-SPEC.md`, accepted.
Code: `crates/ax-usage/src/context_authority.rs`.

`04-VALIDATION-SPEC-AND-PLAN.md` (`AX-REPLAY-VALIDATION-CONTRACT`) is accepted in the repo. Benchmark scoring and the 95% / 100% gates follow that independent protocol. Plan B does not grade its own replay accuracy. No measured 95% result is claimed until Plan C V4 evidence exists. The live database was not migrated.

## Identity and time

- Project identity is the project-database `project_id`. It is not taken from a caller-supplied tenant that disagrees with that database.
- Agent identity is the stable AX agent id. Provider chat and turn ids are stored only when the provider issued them. They are never invented. `SessionIdentity` in `continuity.rs` already keeps that split for provider ids and does not yet store a separate AX agent id. Plan A `plan_revisions.agent_id` is the AX id to reuse.
- `created_at` is unix milliseconds, the same clock Plan A uses.
- `continue` / `current` reads the versions that are authoritative now. A cutoff is not required.
- `reconstruct` / `as_of` reads only rows that were valid at `cutoffMs`. A missing cutoff is an error. A later row is not a substitute for a missing historical row.
- `benchmark` / `reproduce` reads a frozen `snapshotId` in an isolated environment. It does not read the completed worktree as the answer. A missing snapshot is an error. The benchmark protocol itself waits for document 04.

## What already exists

| Surface | Reuse | Not this yet |
| --- | --- | --- |
| `gated_preflight_block` | Compact spec/plan pointer when `hierarchy_v2` is on; old spec block when it is off | No authority state and no manifest |
| `ax_context` | Task context from the graph | No cutoff, no mandatory-versus-advisory split |
| Policy match | Rules and skills stay in their own revisions | A retrieved instruction is not yet a fail-closed authority decision |
| Code graph | Explore and search | Not filtered by a historical cutoff |
| Memory vault | Export and recall | Not sealed into a manifest |
| `context_plan` snapshots | Project and session change the snapshot; dropping a part changes it | Not a sealed manifest and not a replay |
| Plan A tables | Specs, plans, selections, policy links, approval events | No manifest, authority evaluation, or replay run |

There is no `ax context resolve` and no `ax replay` command. The names in the plan are examples, not commands that exist.

## Provenance chain

Work item, when present, points at a spec revision. That revision owns selected plan revisions. Decisions, rule revisions, and skill revisions stay outside the spec body and are linked by id and hash. Session, turn, and AX agent id sit on the revision that was written. Source commit and the manifest are the last two links. Plan B does not copy rule bodies into the spec.

## Threats and the test that has to kill them

| Threat | Outcome | Test |
| --- | --- | --- |
| Prompt injection in a retrieved file | Stays advisory. Never becomes binding policy. | `authority_checks_fail_closed_on_the_known_bad_cases` (skill text) |
| Cross-project leakage | `unauthorized`, fail closed | `authority_checks_fail_closed_on_the_known_bad_cases` (other project) |
| Tampered manifest | `integrity_failed`. Seal mismatch. | `a_reordered_manifest_seals_the_same_and_a_changed_byte_does_not` |
| Stale replica | `stale`. No replay success. | Recheck 2026-11-22: `a_stale_replica_is_stale_after_integrity_and_before_conflict` (validator) and `b6_replica_manifest_sync_classifies_conflict_and_stale` (seal comparison). Neither result is wired into replica sync or replay start yet, so "no replay success" is not enforced end to end. |
| Wrong or ambiguous agent | Explicit failure. No invented provider id. | `a_wrong_agent_or_invented_provider_id_fails_replay` and `a_replay_reference_requires_session_turn_and_agent_and_invents_no_provider_id` |
| Hidden-test or future-file leakage | `as_of` cannot see rows after the cutoff. | Recheck 2026-11-22: `as_of_excludes_a_candidate_created_after_the_cutoff`. This covers resolver input only; there is no historical database read path yet. |
| Dropped mandatory rule | Receipt is `blocking`. | `a_rejected_mandatory_candidate_blocks_the_receipt` |
| Time travel via a newer body | Missing history is `blocked` or `partial`, not recreated. | B3. Not written yet. Historical snapshot reads do not exist (still true at the 2026-11-22 recheck). |
| Conflict treated as permission | `conflict` is fail closed and not authoritative. | `a_conflict_is_fail_closed_and_not_authoritative` and `authority_checks_fail_closed_on_the_known_bad_cases` |
| Resolver input without a project, cutoff, or snapshot | Error naming `project`, `cutoff`, or `snapshot`. | `resolver_input_requires_a_project_and_historic_bounds` |

## Schema locked by those tests

Authority states: `authoritative`, `advisory`, `superseded`, `unapproved`, `conflict`, `stale`, `unauthorized`, `missing`, `integrity_failed`.

Fail closed: `conflict`, `unauthorized`, `integrity_failed`, `missing`. Advisory is not authoritative and does not fail closed. Only `authoritative` is authoritative.

Resolver JSON uses `projectId`, `mode`, `cutoffMs`, and `snapshotId`.

A selection receipt is `blocking` when any mandatory candidate is rejected. The reason is kept. The candidate is not silently dropped.

A manifest seal is SHA-256 over the domain prefix `ax-manifest-v1`, a NUL, then key/value pairs sorted by key. Key order does not change the seal. A changed value does.

Command that ran: `cargo test -p ax-usage --lib context_authority`. Result: 4 passed, 0 failed.

## Phase B1 validator

`evaluate_authority` checks one subject against the project it is being used in. Order: other project, missing hash, hash mismatch, stale replica, conflict with a binding rule, plan bound to a different spec hash, skill, revoked or unapproved, otherwise authoritative.

A skill stays `advisory` even when its text says to ignore policy and even when it is marked approved. An old plan whose spec hash is not the current spec hash is `conflict`. A revoked approval is `unapproved`. An empty hash is `missing`. A different hash is `integrity_failed`. Another project's subject is `unauthorized`.

`resolve_authority` rejects an agent and rejects a human request to grant `authoritative`. A human may record a non-authoritative outcome, such as `advisory`, with a reason. That decision is not stored as a revision row yet.

The known-negative control was the stub that returned `authoritative` for every subject and `Ok` for an agent override. Both new tests failed on that stub, then passed after the checks existed.

Command: `cargo test -p ax-usage --lib context_authority`. Result after B1: 6 passed, 0 failed.

## Phase B2 resolver

`resolve_context` keeps a mandatory rule, selects a skill only as advisory when its tags match the task, and rejects a skill for another stack as `not relevant`. A skill whose text says to ignore policy stays advisory. `as_of` rejects a candidate created after `cutoffMs` and does not block the receipt for that. A mandatory rule that does not fit the token budget stays in the receipt with reason `budget` and the receipt is `blocking`. A repeated content hash is `duplicate`.

Command: `cargo test -p ax-usage --lib context_authority`. Result after B2: 11 passed, 0 failed.

## Stored receipt

`store_receipt` writes `selection_receipts` and `selection_receipt_items` in the project database it is given. The receipt id is the manifest seal. Approval starts as `unapproved`. An agent cannot approve it. A human approval requires an active membership role of `owner`, `approver`, or `admin` in the membership database passed in. The verified member id is stored on the receipt. A mismatched or missing member is rejected. `verify_receipt` recomputes the seal. A changed item reason makes verification fail. These tests use a temporary database. The live project database was not opened.

Command: `cargo test -p ax-usage --lib context_authority`. Result: 15 passed, 0 failed.

## Stored manifest

`store_manifest` writes `context_manifests` with project, session, turn, AX agent id, mode, cutoff, snapshot, receipt id, model, tool versions, and commit hash. A provider chat id is stored only when one was supplied. `as_of` without a cutoff is rejected. Approval starts as `unapproved`. An agent cannot approve it. A human approval requires the same membership check as receipts and stores `actor_member_id`. Changing the turn id makes `verify_manifest` fail. `parse_replay_reference` requires session, turn, and agent, and does not invent a provider id. Tests use a temporary database. The live project database was not opened.

Command: `cargo test -p ax-usage --lib context_authority`. Result: 15 passed, 0 failed.

## Membership-checked approval (this turn)

Live `~/.ax/global.db` was read read-only. Member `a4c2d177-d0ca-4d7d-8789-cba7ebcae5c7` (`Gary Wenneker`) is `active` with role `owner`. No `admin` role row exists for that member. Approval accepts `owner` / `approver` / `admin`, so the verified identity used for store-and-approve is that owner id. A claim of an unknown member id is rejected. The live project database was not migrated.

## Replay identity (B4)

`store_replay_run` loads the sealed manifest and rejects a reference whose session, turn, or AX agent id does not match. A provider chat id that was not stored on the manifest is rejected as invented. Matching ids are allowed. Automated scoring stays `not_implemented` / `not_tested`.

Command: `cargo test -p ax-usage --lib context_authority`. Result: 16 passed, 0 failed.

## Manifest summaries (B5 start)

`list_manifest_summaries` returns each sealed manifest with approval, integrity, latest replay score, and an explicit `uncertainties` list (`unapproved`, `integrity_failed`, `legacy_seal`, `not_run`, `not_tested`). `legacy_seal` marks an unchanged row sealed with the earlier `ax-manifest-v1` format; it verifies and is not `integrity_failed`. It never invents a confident green state. Web surface: `GET /api/context-authority/manifests`.

Command: `cargo test -p ax-usage --lib context_authority`. Result after summaries: see latest suite.

## Plan C V1 fixture registry

Independent module `replay_validation` freezes task criteria into a definition seal, runs fixtures into a sealed evidence record, and fails verification after tamper. VAL-14 (broken assertion → FAIL) and VAL-16/17 remain covered. No write path into the resolver.

Command: `cargo test -p ax-usage --lib replay_validation`.

## Specs authority panel (B5)

The Specs page shows a Context authority calm list fed by `GET /api/context-authority/manifests`. Rows expose approval, seal integrity, replay status, and uncertainty labels. Empty uncertainty is labeled explicitly; the UI does not invent a confident pass.

## Plan C V2 isolation start

`create_arm_workspaces` builds separate directories per experimental arm and rejects golden-tree overlap and duplicate arm names. Pilot cases and provider launches are not claimed yet.

## Still after B5 panel

Full Replay action and B6 hardening remain. No provider adapter and no measured 95% claim. Live spec revisions were not mass-approved.

## Status after B6 (recheck, 2026-11-22)

The Replay action (`docs/specs/specs-hierarchy-ui/REPLAY-ACTION.md`) and the first B6 slices (`B6-HARDENING.md`, `B6-REPLICA-AND-FIXTURE-PILOT.md`) are implemented. Evidence: `EVIDENCE-B6.md` and `EVIDENCE-RECHECK.md`.

Still open: replica sync and replay start do not call the stale or conflict classification yet. The `cursor` pilot adapter is listed but has no runner. Historical snapshot reads (B3) do not exist. No measured 95% claim. Live spec revisions were not mass-approved.
