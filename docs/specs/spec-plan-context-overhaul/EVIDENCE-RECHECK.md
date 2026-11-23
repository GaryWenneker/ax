# EVIDENCE — Spec recheck (2026-11-22)

Owner request: "Check the specs, and I want you to recheck the changes and make sure everything works as promised in the spec."

Specs rechecked:

- `/Users/gary/io/ax/docs/specs/specs-hierarchy-ui/SPEC.md` (14 behaviors)
- `/Users/gary/io/ax/docs/specs/specs-hierarchy-ui/REPLAY-ACTION.md` (5 behaviors)
- `/Users/gary/io/ax/docs/specs/spec-plan-context-overhaul/B6-HARDENING.md` (8 behaviors)
- `/Users/gary/io/ax/docs/specs/spec-plan-context-overhaul/B6-REPLICA-AND-FIXTURE-PILOT.md` (5 behaviors)
- `/Users/gary/io/ax/docs/specs/spec-plan-context-overhaul/PLAN-C-PILOT-V1.md` (4 behaviors)
- The B0 threat table and the 03 plan, where they describe the same code

Tier 2. No spec behavior was changed. Test lists and status notes were appended to the specs where tests were added. None of the five specs has a sign-off of its final text: `SPEC.md` records "do it, you must" as its approval, but that message came before the clarification the spec is built on ("firt hierachy specs, from there the plans"), and the other four were implemented on "implement and continue." orders. Confidence is therefore limited to what the tests and real runs below show. Independent verification: not performed.

## Outcome

Every promised behavior is now covered by a test, a real run, or both. The final gauntlet passed in one fresh run after the last code edit (numbers below). Two earlier attempts at the same code state failed on a flaky test outside this work; see "Baseline flake".

Found and fixed during the recheck:

- SPEC.md behavior 11 ("does not rewrite an approved body in place") had no test. Added `revising_an_approved_plan_appends_a_draft_and_keeps_the_approved_row`. It is the only test that catches mutant M24, which quietly sets the approved revision back to draft when a new revision is saved.
- `the_hierarchy_flag_gates_list_preflight_and_capture` used the process-wide home directory that other tests change, and failed intermittently. It now holds the shared home lock with an isolated home (0 failures in 50 runs afterwards).
- The authority panel could show the replay receipt twice. It now shows it once (`the panel gates the Replay button and the start on the same check and shows the receipt once`).
- `ManifestAuthorityPanel.test.ts` was a vitest file that no script ran and that imported a `.tsx` file. It was deleted; its checks now live in `src/lib/manifestAuthority.test.ts`, which `npm test` runs.
- Missing regression tests were added for B6-HARDENING behavior 6, B6-REPLICA behaviors 2 and 3, Plan C pilot v1 behavior 1, the stale-replica row of the B0 threat table, and the replay HTTP routes (`crates/ax-web/tests/context_authority_api.rs`). The 120-candidate test now prints its time for B6-HARDENING behavior 2.
- Clippy and rustfmt were not clean on this work. Its files had 11 clippy findings (the CI job runs `cargo clippy --workspace --all-targets -- -D warnings`, so they would have failed it) and 63 rustfmt differences (59 in the four new files, 4 in `ax-usage/src/lib.rs` and `ax-web/src/specs.rs`, all on lines this work added). All were fixed without behavior changes: three needless borrows, one explicit deref, one `len() > 0`, a test-only helper placed after the test module, three complex tuple types replaced by type aliases (one reuses `spec_registry::RevisionColumns`), `#[allow(clippy::too_many_arguments)]` on `queue_finished_plan` and the `candidate` test helper (the repository's convention), and `cargo fmt -p ax-usage -p ax-web`.
- The mutation run first covered only behavior 11 of `SPEC.md`. Mutants M25 and M26 now cover behaviors 1 and 2 (the spec-only list and plans under the opened spec), and the coverage layer now also measures `hierarchyList.ts`.
- Stale documents: the 03 plan said document 04 was missing (it exists), the B6 test lists missed tests, the evidence column of three B0 threat-table rows claimed more than the code does, and the hierarchy UI evidence named an old bundle. In `B0-CONTRACT.md` only that evidence column and a new status section changed; the promised outcomes did not.

Not fixed, raised for the owner: see "Spec issues" and "Baseline flake" at the end.

## Source state

- `HEAD` `43496761db03ba3256bd390f6f06d2a941abd2d0`, with the feature uncommitted (nothing was committed).
- Code state `75e03f812c226b87fd0af692375ec27d49ff5685c64d8b581b46c539b6f26a1f` (sha256 over `git diff HEAD --binary` plus every untracked file, excluding `docs/` and the generated `crates/ax-web/web-ui/dist`). The gauntlet prints it at the start and the end and fails if it changes.
- Tools: cargo 1.100.0, rustc 1.100.0, node v26.13.1, tsc 5.9.3.

## Gauntlet

One command reruns every layer and fails closed at the first failing layer:

```bash
bash /Users/gary/io/ax/scripts/spec-recheck-gauntlet.sh
```

`LAYERS="clippy rustfmt"` runs selected layers; a partial run never prints `GAUNTLET OK`.

Final fresh run, after the last code edit:

| Layer | Command | Result |
|---|---|---|
| ax-usage, three times in a row (suite health) | `cargo test -q -p ax-usage --lib` | 343 passed, 0 failed, 1 ignored, in each of the 3 runs |
| ax-web | `cargo test -q -p ax-web --no-fail-fast` | 17 suites, 211 passed, 0 failed, 1 ignored |
| web-ui unit tests | `npm test` | 68 tests, 68 pass, 0 fail |
| Types | `npx tsc --noEmit` | no errors |
| Coverage of `manifestAuthority.ts` | node test coverage with 100% line, branch and function thresholds, plus a check that the file's row is present | 100% lines, branches, functions |
| Coverage of `hierarchyList.ts` | node test coverage with 100% line and function thresholds, plus the row check | 100% lines and functions; 60.87% branches, not enforced: every uncovered branch is in the payload parser (lines 9 to 33), while `hierarchySpecRows` and `plansForSpec` are fully covered |
| Lint | `cargo clippy -p ax-usage -p ax-web --all-targets` | 0 findings in changed files; 2 elsewhere (unused workspace dependencies `gix` and `self_update` in the unchanged `Cargo.toml`) |
| Format | `rustfmt --check --edition 2021` on the changed Rust files | clean |
| Mutation | `scripts/spec-recheck-mutants.sh` | 26 of 26 killed, after a clean run of all six runners; sources restored |
| Negative controls | C1 to C4 | each exits nonzero with its expected message |
| Code state | start equals end | unchanged, `GAUNTLET OK`, exit 0 |

This was the fifth full attempt; all five are listed under "Baseline flake". Attempts 1, 3 and 4 stopped at the ax-web layer on that flake, and attempt 2 passed at an earlier code state. Nothing was changed to work around the flake. Between attempts 1 and 2, the lint and format fixes were made and the two recheck scripts changed; between attempts 2 and 3, only the recheck scripts changed (M25, M26 and the `hierarchyList.ts` coverage check); attempts 3 to 5 ran on the same code. One more run, at attempt 1's code state, was stopped by hand about 86 seconds in, during its first ax-usage pass, to check whether a must-not lacked a test (an existing test covered it). Nothing was changed before attempt 1 restarted it, and the stopped run produced no result.

Changed Rust files checked by the lint and format layers: `ax-usage/src/{context_authority,lib,replay_validation,spec_hierarchy,spec_registry}.rs`, `ax-web/src/{context_authority_api,specs}.rs`, `ax-web/tests/context_authority_api.rs`.

## Mutation testing

Manual mutants from `/Users/gary/io/ax/scripts/spec-recheck-mutants.sh`. A mutant counts as killed only when a test reports a failure; a build or load error, an anchor that is missing or not unique, or a file not restored byte for byte fails the script. Each kill line names the failing tests. `ANCHORS_ONLY=1` checks every anchor without running tests; all 26 match exactly once. While the script was being written, one full run stopped after killing its first five mutants, because the next mutant's anchor matched 4 places in `context_authority.rs`. That was the uniqueness check working; the anchor was made unique before any reported run.

| Mutant | Bug it introduces | Failing tests in the final run |
|---|---|---|
| M1 | `blocks_authority` lets Stale through | `b6_blocks_authority_covers_every_state`, `b6_stale_blocks_authority` |
| M2 | Unauthorized stops failing closed | `a_conflict_is_fail_closed_and_not_authoritative`, `b6_blocks_authority_covers_every_state` |
| M3 | IntegrityFailed stops failing closed | `a_conflict_is_fail_closed_and_not_authoritative`, `b6_blocks_authority_covers_every_state` |
| M4 | Approved replica disagreement reported as Stale | `b6_replica_manifest_sync_classifies_conflict_and_stale` |
| M5 | Turn id becomes optional | `b6_parse_replay_reference_requires_the_turn` |
| M6 | Replay start skips the manifest seal | `a_broken_seal_cannot_start_a_replay` (HTTP) |
| M7 | A partial bundle counts as prepared | `fixture_pilot_refuses_a_partial_bundle` |
| M8 | A prepared bundle is never reused | `fixture_pilot_refuses_a_tampered_frozen_task_on_reuse`, `fixture_pilot_scores_a_prepared_bundle_in_place` |
| M9 | Fixture run skips the definition seal | `fixture_pilot_refuses_a_tampered_frozen_task_on_reuse` |
| M10 | `cursor` adapter dropped | `pilot_task_v1_freezes_the_stratum_criteria_and_both_adapters` |
| M11 | Pilot stratum changed | `pilot_task_v1_freezes_the_stratum_criteria_and_both_adapters` |
| M12 | Secret criterion no longer critical | six pilot tests, including `fixture_pilot_scores_all_arms_without_claiming_release` |
| M13 | Read-only hub starts a replay | `a_readonly_hub_refuses_to_start_a_replay` (HTTP) |
| M14 | Replay ignores a start in progress | `a replay starts only from an intact seal while no other replay is starting` |
| M15 | Replay ignores a broken seal | same |
| M16 | Button not gated by `canStartReplay` | `the panel gates the Replay button and the start on the same check and shows the receipt once` |
| M17 | Start not gated by `canStartReplay` | same |
| M18 | Receipt shown twice | same |
| M19 | Integrity label changed | `uncertainty labels stay readable without inventing confidence` |
| M20 | `not_tested` shown as a pass | `scorecard never invents a confident pass for not_tested` |
| M21 | Stale replica check removed | `a_stale_replica_is_stale_after_integrity_and_before_conflict` |
| M22 | Stale replica checked after conflict | same |
| M23 | Plan revision rewrites the latest row in place | eight `spec_hierarchy` tests, including `revising_an_approved_plan_appends_a_draft_and_keeps_the_approved_row` |
| M24 | Revising demotes the approved revision | only `revising_an_approved_plan_appends_a_draft_and_keeps_the_approved_row` |
| M25 | The spec list mixes in plan rows | `hierarchySpecRows shows specs only so plans stay under the opened spec` |
| M26 | Plans come from the first spec, not the opened one | same |

Kills are attributed to whichever tests fail, so 26 of 26 validates the suite as a whole. Each mutant runs one runner (one test module, the HTTP test file, the two authority UI test files, or the hierarchy list test file). Within that runner, 17 mutants (M5 to M11, M13 to M19, M21, M22, M24) are caught only by tests this recheck added, and M20 only by `replayScorecard.test.ts`, which `npm test` did not run before this recheck.

## Negative controls and their proofs

Each home-grown check was shown to fail on a known-bad input, and each control was shown to depend on the defence it guards.

| Check | Negative control | Proof the control is not vacuous |
|---|---|---|
| Mutation runner: a surviving mutant fails the run | C1, a comment-only change, must survive and make the script exit nonzero | With the final kill-count check removed, C1 exits 0 (`SURVIVED: C1`, `mutants killed: 0/1`) |
| Anchors must occur exactly once | C2, an anchor that is not in the file, must fail with `anchor found 0 times` | With the count check removed, C2 fails only through the "did not change" backstop, without that message, so the control turns red |
| A Rust build error is not a kill | C3, a type error, must end in `ERROR: C3` | With the cargo test-failure check replaced by `true`, C3 is counted as killed and the script exits 0 |
| A TypeScript load error is not a kill | C4, a syntax error, must end in `ERROR: C4` | Without the `SyntaxError` exclusion, node reports the file as a failing test, C4 is counted as killed, and the script exits 0 |
| Coverage row must be present | node with `--test-coverage-include=src/lib/manifestAuthority.ts` but only `replayScorecard.test.ts` | node exits 0 and prints `all files | 100.00` without a `manifestAuthority.ts` row; the row check fails. This is why the check exists |
| `hierarchyList.ts` coverage | (1) the same command with only `replayScorecard.test.ts`; (2) only the first hierarchy test | (1) node exits 0 with `all files | 100.00` and no `hierarchyList.ts` row, and the row check fails; (2) node exits 1 on the line threshold (36.59% lines) |
| Clippy findings in changed files fail the layer | a `clone_on_copy` warning appended to `spec_hierarchy.rs` (restored byte for byte) | the layer failed and named the injected line (it also exposed the 11 real findings fixed above) |
| Code state | an untracked file in `scripts/`, then one in `docs/` | the hash changed for `scripts/`, stayed equal for `docs/`, and returned after removal |
| `ANCHORS_ONLY` mode stops on a missing anchor | `ANCHORS_ONLY=1 CONTROLS=1 ONLY=C2` exits 2 with `anchor found 0 times` | With that mode's count check disabled (in a temporary copy), the same run prints `anchor ok: C2` and exits 0 |

A negative control proves that one known-bad case reaches the failure path. It does not prove the check recognizes every possible violation; the clippy gate, for example, only looks at the listed changed files.

## Spec to evidence

### SPEC.md (hierarchy UI, specs first, then plans)

The browser walkthrough ran on a temporary project with an isolated home and global database (`ax-walkthrough.ARymHlXm2w` in the OS temp folder), never on the live project. Seed data: specs `spec-ledger` and `spec-empty` (no plans), plans `plan-ledger-a` and `plan-ledger-b`, and two sealed manifests (`turn-intact`, `turn-broken` with a broken seal). `hierarchy_v2` was switched on in that temporary database only. A fetch recorder in the page captured request bodies. Screenshots: `/Users/gary/io/ax/docs/specs/specs-hierarchy-ui/walkthrough-spec-list.png`, `/Users/gary/io/ax/docs/specs/specs-hierarchy-ui/walkthrough-plan-open.png`, `/Users/gary/io/ax/docs/specs/specs-hierarchy-ui/walkthrough-authority-tab.png`. The walkthrough used the build that embeds `index-CmAv1Egz.js`, before the lint and format changes; those changes do not touch the web UI and do not change behavior (suite and mutants rerun after them). Behavior 1 was checked again on the final release binary, because the earlier list screenshots show only one row. The page hides rows only through the search box and the column filters; with neither set, both specs are listed.

| # | Behavior | Test | Real run |
|---|---|---|---|
| 1 | List shows one row per spec, no plan rows | `hierarchySpecRows shows specs only so plans stay under the opened spec` | rerun on the final binary with an empty search: `spec-empty` (no plans) and `spec-ledger`, no plan rows; `/api/specs` returned both specs |
| 2 | `hierarchySpecRows` and `plansForSpec` (unknown spec gives `[]`) | same test, including `plansForSpec(nodes, 'missing')` | — |
| 3 | The page uses `hierarchySpecRows` | — | `Specs.tsx` builds the list with it; list showed no plans |
| 4 | Opening a row loads the spec revisions and shows Agreement | — | `GET /api/specs/{id}/revisions` recorded; Agreement tab active |
| 5 | Tabs Agreement, Plans, Authority | — | seen, see screenshot |
| 6 | Breadcrumb `Spec › {planId}` with a way back to the Plans list | — | `Spec › plan-ledger-a`; back returned to the list with the blade open |
| 7 | Deep links `?spec=` and `?spec=&plan=` | — | both opened the expected tab and plan |
| 8 | Escape and outside click close the blade | — | both closed it |
| 9 | Plans tab lists id and latest version; empty state | — | `spec-ledger` listed its plans; `spec-empty` showed "No plans under this spec yet." |
| 10 | Choosing a plan loads plan revisions and shows the editor | `list_plan_revisions_returns_ordered_history` | `GET /api/specs/plan/plan-ledger-a/revisions` recorded; editor shown |
| 11 | Save posts `planId`, `basedOn`, `body`, `sessionId: "web-ui"`, `turnId: "local"`; approved bodies are not rewritten | `revising_an_approved_plan_appends_a_draft_and_keeps_the_approved_row` (only M24 killer) | recorded body had exactly those fields; the database holds `plan-ledger-a` v2 as a draft based on v1, with v1 unchanged |
| 12 | Alternative plan posts the listed fields, reloads, opens the new plan | — | recorded body had the listed fields; `plan-mwrxfce3` ("Plan C") was created under `spec-ledger` as a `web-ui`/`local` draft and opened |
| 13 | Authority tab shows the panel; not duplicated above the list | — (no test checks where the panel is placed) | exactly one panel on the page, inside the Authority tab |
| 14 | Legacy mode keeps the flat list and the page-level panel | `a flat spec payload stays a flat list` (the list data only) | flag off showed the flat list and the panel above it |

Must not:

| Clause | Evidence |
|---|---|
| No live database migration, no forced `hierarchy_v2` on the live project | every UI and CLI check used temporary projects; nothing migrated the live database or changed its flags |
| No 95% replay claim | `scorecard never invents a confident pass for not_tested`; `uncertainty labels stay readable without inventing confidence` |
| No in-place rewrite of an approved body | `revising_an_approved_plan_appends_a_draft_and_keeps_the_approved_row` and mutants M23, M24 |
| Do not remove or weaken the `hierarchyList` or SpecsDetail tests | the three `hierarchyList` tests run in `npm test`. `specsPage.test.ts`, which also reads `SpecsDetail.tsx`, was last changed on 2026-11-21, before this spec, and passes. The deleted `ManifestAuthorityPanel.test.ts` was neither; it never ran (see Outcome) |
| No new npm dependency | `package-lock.json` is identical to `HEAD`, and the only `package.json` change is the `test` script, which now runs seven more test files than at `HEAD` (two added by this recheck, five by earlier work in this feature) |
| No commit or deploy | none made |

### REPLAY-ACTION.md

| # | Behavior | Test | Real run |
|---|---|---|---|
| 1 | `POST /api/context-authority/replay` uses the stored identity and inserts a `replay_runs` row | `a_replay_uses_the_stored_identity_and_claims_no_score` (HTTP), `start_replay_for_manifest_uses_stored_identity_and_stays_not_tested` | the walkthrough database holds one run for `session-1`/`turn-intact`/`agent-ax` (started 29 s after seeding; the seed helper starts none) |
| 2 | Automated stays `not_implemented`, score `not_tested`, no invented pass | same tests, `a_manifest_stores_model_tools_and_commit_and_replay_stays_not_implemented` | that run is `not_implemented`, `not_tested`, `unapproved` |
| 3 | `GET /api/context-authority/replay/{id}` returns the stored receipt | `a_replay_uses_the_stored_identity_and_claims_no_score`, `an_unknown_manifest_or_replay_is_missing` | — |
| 4 | Replay button disabled for a broken seal; notice via `formatReplayNotice` | `a replay starts only from an intact seal while no other replay is starting`, `a_broken_seal_cannot_start_a_replay` | broken-seal row shows a disabled Replay button, intact row an enabled one (screenshot) |
| 5 | `prepare_v2_pilot_arms` makes G/A/B/C; `format_scorecard_lines` claims no release unless Pass | `prepare_v2_pilot_arms_makes_g_a_b_c`, `format_scorecard_lines_do_not_claim_release_on_fail` | — |

Must not: transcripts are refused by `store_replay_run` (`a_manifest_stores_model_tools_and_commit_and_replay_stays_not_implemented`); invented provider ids fail (`a_wrong_agent_or_invented_provider_id_fails_replay`); no 95% claim (UI tests above); a read-only hub cannot start a replay (`a_readonly_hub_refuses_to_start_a_replay`), and the routes use only the existing `ensure_*` schema helpers.

### B6-HARDENING.md

| # | Behavior | Test |
|---|---|---|
| 1 | 120 candidates, selected within budget, rejects listed | `b6_resolve_handles_one_hundred_twenty_candidates` |
| 2 | Under 50 ms target, 500 ms hard gate | same test asserts the 500 ms gate and prints the time: 0.57 ms, 0.07 ms, 0.07 ms in three debug runs. The 50 ms target itself is not asserted |
| 3 | 100 manifest summaries under 2 s | `b6_list_summaries_handles_one_hundred_manifests` |
| 4 | Same manifest, same run id | `b6_start_replay_is_idempotent_for_the_same_manifest` |
| 5 | Concurrent starts leave one row | `b6_concurrent_replay_starts_share_one_run_row` |
| 6 | Malformed references rejected | `b6_parse_replay_reference_rejects_malformed_payloads`, `b6_parse_replay_reference_requires_the_turn` |
| 7 | Concurrent approve: exactly one wins | `b6_concurrent_manifest_approve_one_wins` |
| 8 | Inactive approver blocked | `b6_permission_change_blocks_approval` |

Must not: temp pools only; no 95% claim; no invented provider ids (`a_wrong_agent_or_invented_provider_id_fails_replay`); no new crates (no dependency change in `Cargo.toml` or `Cargo.lock`).

### B6-REPLICA-AND-FIXTURE-PILOT.md

The CLI smoke ran the final release binary (`ax 10.0.2`, embedding `index-CmAv1Egz.js`). It was built at code state `51c34a9e…`; the only later edits are to the two recheck scripts in `scripts/`, which are not part of the binary. The smoke used temporary directories with an isolated home and databases (`ax-pilot-smoke-final.pJcxznH95W` in the OS temp folder). An earlier smoke on the previous build (`ax-pilot-smoke.lGY9An3nX6`) gave the same results.

| # | Behavior | Test | Real run |
|---|---|---|---|
| 1 | Replica seal classification | `b6_replica_manifest_sync_classifies_conflict_and_stale`, `a_stale_replica_is_stale_after_integrity_and_before_conflict` | — |
| 2 | `blocks_authority` for Conflict, Unauthorized, IntegrityFailed, Missing, Stale | `b6_blocks_authority_covers_every_state`, `b6_stale_blocks_authority` | — |
| 3 | Score prepares or reuses a bundle, writes `evidence-<arm>.json`, reports fixture, 0 live runs, gate not claimed | `fixture_pilot_scores_all_arms_without_claiming_release`, `fixture_pilot_scores_a_prepared_bundle_in_place`, `fixture_pilot_refuses_a_tampered_frozen_task_on_reuse`, `fixture_pilot_refuses_a_partial_bundle` | prepare then score wrote all four evidence files in place; scoring a fresh directory prepared and scored it; after prepare, a changed criterion weight in `frozen-task.json` was refused with `definition seal mismatch` (exit 1) and a bundle with only arm G with `arm workspace already exists: G` (exit 1), both with no evidence written; each arm folder holds only its own evidence file |
| 4 | G pass, A fail (critical), B fail (one functional), C blocked; never a release claim | same tests | verdicts G pass, A fail, B fail (33.3% weighted: the weight-2 criterion failed), C blocked; every evidence file says `releaseGate: "not claimed"` |
| 5 | `ax replay-pilot-score <dir>` prints JSON and exits 0 | — | `adapter: "fixture"`, `liveAgentRuns: 0`, `releaseGate: "not claimed"`, exit 0 |

### PLAN-C-PILOT-V1.md

| # | Behavior | Test | Real run |
|---|---|---|---|
| 1 | `pilot-ts-list-filter` in `typescript-react`, two noncritical criteria and critical `no-secret-in-ui` | `pilot_task_v1_freezes_the_stratum_criteria_and_both_adapters` | `frozen-task.json` lists `filter-matches-query` (2.0), `empty-query-shows-all` (1.0), `no-secret-in-ui` (critical) |
| 2 | Bundle with G/A/B/C, `frozen-task.json`, adapters `fixture` and `cursor` | `pilot_bundle_writes_frozen_task_without_claiming_release`, same test as 1 | arms and file present |
| 3 | A critical failure reports gate not claimed | `format_scorecard_lines_do_not_claim_release_on_fail`, `fixture_pilot_scores_all_arms_without_claiming_release` | arm A |
| 4 | `ax replay-pilot-prepare <dir>` prints `releaseGate: "not claimed"` | — | exit 0, `ok: true`, `releaseGate: "not claimed"`, adapters `fixture` and `cursor`, arms G, A, B, C |

Must not: no 95% claim; no golden code in arm workspaces (`v2_arm_workspaces_are_separate_and_reject_golden_overlap`; no arm file in the smoke bundle mentions golden code or provider ids); no invented provider ids.

## Baseline flake (not part of this work)

Three of the five full gauntlet attempts failed at the ax-web layer, each on one test, each with the same cause:

| Attempt | Code state | Failing test |
|---|---|---|
| 1 | before the lint and format fixes | `changes_feed::an_idle_project_sends_no_events` (`changes_feed.rs:44`, `assertion failed: res.status().is_success()`) |
| 2 | `51c34a9e…` | none; every layer passed |
| 3 | `75e03f81…` | `changes_feed::a_new_memory_produces_one_memory_event` (same line) |
| 4 | `75e03f81…` | `dav_links::v3_app_json_that_is_not_an_object_is_served_unchanged` (`dav_links.rs:108`, `status 500 Internal Server Error: {"error":"Database unavailable"}`) |
| 5 | `75e03f81…` | none; every layer passed (the run reported above) |

- Rate at the final code state, running the built test binaries alone 25 times each: `changes_feed` failed 3 times, always with this signature; `dav_links` did not fail. With 17 test binaries in the ax-web layer, a gauntlet run hits it often.
- Reproduced with a temporary diagnostic test (deleted afterwards): four parallel subscriptions on a fresh global database failed in 1 of 60 sequential and 1 of 80 parallel process runs, always with `500 {"error":"Database unavailable"}`.
- Cause: `web_auth::gate` opens the global database on every request and turns any error into that 500. Concurrent first opens of a new SQLite file race on the switch to WAL mode (`ax_db::connect_options`): 13 of 2,400 concurrent opens of fresh databases failed with `open sqlite …: (code: 5) database is locked`. On an existing database, 0 of 1,600 failed.
- Not caused by this work: `ax-db/src/lib.rs`, `ax-global-db/src/lib.rs`, `web_auth.rs`, `changes_feed.rs`, `dav_links.rs` and `Cargo.lock` are identical to `HEAD`. This work's changes on the request path add a role mapping, a module declaration and a nested `/api/context-authority` router; none of them opens the global database, and the error happens in `web_auth::gate` before any handler runs. The rate at `HEAD` was not measured.
- Product impact: only the first parallel requests on a machine that has no global database yet.
- Not fixed here. The gauntlet does not tolerate it: a failure still fails the layer, and `--no-fail-fast` keeps other test binaries running so a flake cannot hide them. Possible fixes: open the global pool once in `web_auth` instead of per request, or create the file and switch it to WAL with a single connection before opening the pool. At the owner's request a fix is drafted in `docs/specs/global-db-first-open/SPEC.md`: retry the global pool open on lock errors, as the project database open already does. It waits for approval and nothing is implemented yet.

## Spec issues for the owner

These were not changed, because they change what a spec promises or need a decision:

1. The `cursor` adapter is listed in the pilot bundle (PLAN-C behavior 2) but has no runner.
2. A replica with an empty seal while both sides are approved classifies as `Conflict`, not `Missing`; the spec only defines the empty primary seal.
3. The replica classification and `blocks_authority` are not called from replica sync or replay start. B6 goal 1 ("fail closed when a replica's sealed manifest disagrees") is only a pure function today. The B0 threat table now says so.
4. Reusing a bundle checks `frozen-task.json` against its own definition seal, not against the pinned v1 task, so a tamper that also recomputes the seal would be scored.
5. The 50 ms resolve target is printed, not asserted (only the 500 ms gate is).
6. Nothing in the product calls `store_manifest` yet, so the Authority tab and Replay have no data in real projects.
7. In hierarchy mode the list row shows the spec id instead of its title, and the Version column is empty.
8. The authority panel shows overlapping uncertainty badges (for example `unapproved` and `Unapproved`, `not run` and `Replay not run`).
9. No product path creates a plan-less hierarchy spec, so the Plans empty state only appears for imported legacy specs.
10. Minor: the blade animates again when a plan opens.
11. Minor: on first load the Specs page briefly shows the legacy layout ("No specs stored yet" and the page-level authority panel) until the hierarchy payload arrives.
12. The hierarchy list takes its project id from the first spec by id (`SELECT project_id FROM specs ORDER BY id LIMIT 1`), so if specs from more than one project share a database, only that project's specs are listed.

## Known limits

- No measured 95% or 100% replay claim; fixture pilot scores are plumbing evidence only.
- B6-HARDENING covers concurrency and approval permissions, which old-coder rates Tier 3. This recheck added no failure model, property-based tests or adversarial pass, so those parts carry Tier 2 evidence only.
- The live `ax web` on port 7070 ran a binary older than the replay routes until 2026-11-22. The owner then approved a restart on the final release binary, knowing that the first Specs page view would create `context_manifests` and `replay_runs` in the live project database. After the restart:
  - the served bundle is `index-CmAv1Egz.js`, the file `dist/index.html` names;
  - `GET /api/context-authority/manifests` returned `200 {"manifests":[],"projectId":"ax"}`;
  - both tables exist with 0 rows;
  - the Specs page shows "No sealed manifests yet." in its Context authority section.

  The live list stays the legacy flat list because `hierarchy_v2` is off there. Its startup log warns "background watch skipped: database incident. Run ax db repair", which `ax web` already logged on 2026-11-18, before this work; no repair was run.
- Temporary walkthrough and smoke folders that contain `.ax` were left in the OS temp folder on purpose.
