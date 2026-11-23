# EVIDENCE — Plan B Phase B6 hardening (+ replica/fixture slice)

Spec: `/Users/gary/io/ax/docs/specs/spec-plan-context-overhaul/B6-HARDENING.md`  
Spec (this slice): `/Users/gary/io/ax/docs/specs/spec-plan-context-overhaul/B6-REPLICA-AND-FIXTURE-PILOT.md`  
Approval: continue after pilot v1 (`implement and continue`); autonomous relative to formal B6 human sign-off — confidence limited to the listed tests.

Rechecked on 2026-11-22. The full behavior map, the gauntlet numbers, mutation testing, negative controls, and the CLI smoke on the final binary are in `/Users/gary/io/ax/docs/specs/spec-plan-context-overhaul/EVIDENCE-RECHECK.md`. This file lists the B6 tests.

## Spec → test map

| Behavior | Test |
|---|---|
| 120-candidate resolve + budget | `b6_resolve_handles_one_hundred_twenty_candidates` |
| 500 ms hard gate (50 ms target) | same test; it asserts the 500 ms gate and prints the time. The 50 ms target is not asserted |
| 100 manifest summaries under 2 s | `b6_list_summaries_handles_one_hundred_manifests` |
| Idempotent replay start | `b6_start_replay_is_idempotent_for_the_same_manifest` |
| Concurrent replay start | `b6_concurrent_replay_starts_share_one_run_row` |
| Malformed reference rejected | `b6_parse_replay_reference_rejects_malformed_payloads`, `b6_parse_replay_reference_requires_the_turn` |
| Concurrent approve: one wins | `b6_concurrent_manifest_approve_one_wins` |
| Inactive member blocked | `b6_permission_change_blocks_approval` |
| Replica seal Conflict/Stale/Missing | `b6_replica_manifest_sync_classifies_conflict_and_stale`, `a_stale_replica_is_stale_after_integrity_and_before_conflict` |
| `blocks_authority` for Conflict, Unauthorized, IntegrityFailed, Missing, Stale | `b6_blocks_authority_covers_every_state`, `b6_stale_blocks_authority` |
| Pilot bundle, no release claim | `pilot_bundle_writes_frozen_task_without_claiming_release`, `pilot_task_v1_freezes_the_stratum_criteria_and_both_adapters` |
| Fixture pilot prepares or reuses a bundle and scores all arms, gate not claimed | `fixture_pilot_scores_all_arms_without_claiming_release`, `fixture_pilot_scores_a_prepared_bundle_in_place`, `fixture_pilot_refuses_a_tampered_frozen_task_on_reuse`, `fixture_pilot_refuses_a_partial_bundle` |
| Replay HTTP routes (start, idempotent, show, broken seal, missing, read-only hub) | `crates/ax-web/tests/context_authority_api.rs` (4 tests) |

## Gauntlet

```bash
bash /Users/gary/io/ax/scripts/spec-recheck-gauntlet.sh
```

Final numbers: `EVIDENCE-RECHECK.md`, section "Gauntlet". An earlier version of this file reported 11 passing tests from the filtered run `cargo test -p ax-usage -- b6_ fixture_pilot pilot_bundle`.

## CLI surface (Plan B / Plan C)

`ax replay-reference`, `ax replay-run`, `ax replay-status`, `ax replay-pilot-prepare`, `ax replay-pilot-score` — wired; fixture score always returns `releaseGate: "not claimed"` and `liveAgentRuns: 0`. A changed `frozen-task.json` and a partial bundle are refused (exit 1, no evidence written); see the CLI smoke in `EVIDENCE-RECHECK.md`.

## Skipped (remaining B6 / Plan C)

- Criterion micro-benches / 100+ turn history token cost report
- Full replica HTTPS sync conflict matrix (memory/policy exchange already covered elsewhere; this slice is sealed-manifest VAL-13 classification only)
- Full identifier fuzz corpus beyond the malformed table
- Live multi-provider agent pilots and measured 95%/100% gates

## Known limits

No measured 95%. Live DB not migrated. Automated replay remains `not_implemented`. Fixture pilot scores are plumbing evidence only. The replica classification and `blocks_authority` are not yet called from replica sync or replay start, and the `cursor` adapter has no runner (raised in `EVIDENCE-RECHECK.md`, "Spec issues for the owner").
