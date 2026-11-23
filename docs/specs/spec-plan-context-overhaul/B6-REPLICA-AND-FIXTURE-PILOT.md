# SPEC — B6 replica manifest sync + Plan C fixture pilot score

Tier 2. Continues Plan B Phase B6 (replica conflict) and Plan C V2 (fixture pilot score path). Does **not** claim measured 95%/100% release gates or live provider runs.

Owner continue order: implement and continue after pilot v1 registry.

## Goal

1. Fail closed when a replica’s sealed context manifest disagrees with the primary (VAL-13 plumbing).
2. Score a prepared pilot bundle with the **fixture** adapter across arms G/A/B/C and always leave the release gate unclaimed.

## Behaviors

1. `classify_replica_manifest_sync(primary_seal, replica_seal, primary_approved, replica_approved)`:
   - empty primary seal → `Missing`
   - seals equal and both approved → `Authoritative`
   - seals equal and replica not approved → `Unapproved`
   - seals differ and both approved → `Conflict`
   - seals differ otherwise → `Stale`
2. `AuthorityState::blocks_authority()` is true for `Conflict`, `Unauthorized`, `IntegrityFailed`, `Missing`, and `Stale` (Stale cannot be used as current truth).
3. `score_fixture_pilot(dir)` prepares (or reuses) a pilot bundle, runs fixture criteria for arms G/A/B/C, writes `evidence-<arm>.json` under each arm root, and returns a report with `adapter: "fixture"`, `liveAgentRuns: 0`, `releaseGate: "not claimed"`.
4. Fixture outcomes: G all pass; A critical fail; B one functional fail; C critical unverified → blocked. Report never claims a release gate even if an arm scorecard would be Pass in isolation.
5. CLI: `ax replay-pilot-score <dir>` prints JSON with those fields and exit 0 when scoring succeeds.

## Must not

- Migrate live project DB.
- Claim measured 95%/100% Plan C release gates.
- Invent provider chat ids or treat fixture scores as live Arm A evidence.
- Add Criterion/cargo-bench crates.

## Setup

- Isolation: temp dirs / temp pools only.
- Files: this SPEC, `context_authority.rs`, `replay_validation.rs`, `commands/replay.rs`, `main.rs`, EVIDENCE-B6.md.
- No new dependencies.

## Tests

- `b6_replica_manifest_sync_classifies_conflict_and_stale`
- `b6_stale_blocks_authority`
- `fixture_pilot_scores_all_arms_without_claiming_release`

Added in the recheck (2026-11-22):

- `b6_blocks_authority_covers_every_state` (behavior 2: `b6_stale_blocks_authority` did not check `Unauthorized`, `IntegrityFailed`, or `Superseded`)
- `fixture_pilot_scores_a_prepared_bundle_in_place` (behavior 3, the reuse path)
- `fixture_pilot_refuses_a_tampered_frozen_task_on_reuse` (behavior 3: a changed `frozen-task.json` fails the definition seal and writes no evidence)
- `fixture_pilot_refuses_a_partial_bundle` (behavior 3: a bundle missing arms is not reused)
