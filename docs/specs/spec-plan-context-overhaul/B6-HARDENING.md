# SPEC — Plan B Phase B6 hardening (first slice)

Tier 2. Executable hardening for context authority and replay. Does **not** claim 95% replay fidelity or complete B6 (full fuzz corpus, replica races, independent Plan C gate).

Owner continue order: implement and continue after Replay action.

## Goal

Prove scale and fail-closed behavior for resolve, manifest listing, and replay start under load-like fixtures.

## Behaviors

1. `resolve_context` with **120** distinct advisory candidates and a mid budget finishes without panic; selected count is ≤ budget; receipt lists rejects for budget overflow.
2. Wall time for that resolve stays under **50ms** on a debug test host (soft gate: fail if over 500ms to catch pathological regression).
3. `list_manifest_summaries` with **100** stored manifests for one project returns 100 rows and finishes under **2s**.
4. `start_replay_for_manifest` called twice on the same approved sealed manifest returns the **same** run id (idempotent seal / INSERT OR IGNORE).
5. Two concurrent `start_replay_for_manifest` calls on the same manifest still leave **one** `replay_runs` row with the same id.
6. `parse_replay_reference` rejects: empty object, missing session/turn/agent, blank strings, non-string ids, and does not treat Markdown instruction text as a session id when the field is missing.
7. Concurrent `approve_manifest` on one unapproved sealed manifest: exactly one call succeeds; a third call fails.
8. After the approver membership is set inactive, `approve_manifest` fails with approver role required and the manifest stays unapproved.

## Must not

- Migrate live project DB.
- Claim measured 95%/100% Plan C release gates.
- Invent provider chat ids.
- Add Criterion/cargo-bench crates unless already present (use unit tests with Instant).

## Setup

- Isolation: tests use temp pools only.
- Files: this SPEC, `context_authority.rs` tests, `EVIDENCE-B6.md`.
- No new dependencies.

## Tests

- `b6_resolve_handles_one_hundred_twenty_candidates`
- `b6_list_summaries_handles_one_hundred_manifests`
- `b6_start_replay_is_idempotent_for_the_same_manifest`
- `b6_concurrent_replay_starts_share_one_run_row`
- `b6_parse_replay_reference_rejects_malformed_payloads`

Added in the recheck (2026-11-22). The first two were already in the code for behaviors 7 and 8 but missing from this list:

- `b6_concurrent_manifest_approve_one_wins` (behavior 7)
- `b6_permission_change_blocks_approval` (behavior 8)
- `b6_parse_replay_reference_requires_the_turn` (behavior 6: a missing turn alone is rejected by name)
- `b6_resolve_handles_one_hundred_twenty_candidates` now prints the measured resolve time, so the 50ms target in behavior 2 is reported, not only the 500ms gate
