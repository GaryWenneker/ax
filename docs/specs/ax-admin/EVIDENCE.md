# ax-admin: evidence

Report for [SPEC.md](SPEC.md). Every number below comes from one run of
`tools/gauntlet-ax-admin.sh --mutation` on the source state named here, started
after the last code edit.

## Source state and how to rerun

| | |
|---|---|
| Commit | `727cd2b7` on branch `feature/ax-admin` (working tree clean) |
| Base | `main` at `9010913d` |
| Toolchain | rustc 1.100.0 (0ef1bc386 2026-11-09), cargo 1.100.0 (3acc79c0c 2026-11-05), Node v26.13.1, Playwright 1.64.1 with the system Chrome |
| Host | macOS (Darwin 27), arm64 |
| One command | `tools/gauntlet-ax-admin.sh --mutation` (the mutation layer takes most of the time; leave out `--mutation` for a quick run) |

The script fails on the first layer that fails. It needs `cargo`, `node`/`npm`,
Google Chrome, `pwsh` and `python3`.

## Spec approval

- The spec was approved by the owner on 2026-11-26 with the words "Approve spec".
- Review changed 18 points after approval. They are listed in the spec as V1 to
  V18, with the approved text left as it was. The owner answered "accepted" on
  2026-11-26 when asked to review V1 to V18 and to decide whether browser
  sign-in tokens may stay shared with `ax mcp remote login`.
- Tier 3 (credentials and authentication). Isolation: a branch in the owner's
  checkout, as the spec's setup plan states; no worktree.

## Criteria and the checks that verify them

Rust tests: `crates/ax-admin/tests/admin.rs` (A, B, C), `crates/ax-mcp/tests/remote_admin.rs`
and `crates/ax-mcp/src/remote/auth.rs` (E). Browser story: `crates/ax-admin/web-ui/e2e/admin.spec.ts`.

| Criterion | Verified by |
|---|---|
| A1 no connection: connect screen only, remote API closed | `a1_without_a_connection_the_remote_api_is_closed`; browser "A1" |
| A2 address and credentials required, nothing written | `a2_a_connection_needs_an_address_and_credentials`; browser "A2, A4, A5"; UI `connectionInput.test.ts` |
| A3 valid address and key save, activate and work | `a3_a_valid_address_and_key_are_saved_activated_and_usable`; browser "A3" |
| A4 wrong key reported, nothing saved | `a4_a_wrong_key_is_reported_and_not_saved`; browser "A2, A4, A5" |
| A5 unreachable, not Ax, redirect: distinct errors, redirect never followed | `a5_unreachable_non_mcp_and_redirecting_addresses_are_distinct_errors` (target server receives nothing); browser covers unreachable only (V16) |
| A6 several connections, switching changes what answers | `a6_switching_changes_which_server_answers`; `a_call_for_a_connection_that_is_no_longer_active_is_refused`; browser "A6, E9" and "a page that still shows another environment" |
| A7 remove deletes entry and credential, another becomes active | `a7_removing_deletes_the_entry_and_its_key`; browser "A7" |
| A8 restart restores connections and the active one | `a8_a_restart_restores_connections_and_the_active_one`; `a8_editing_keeps_the_key_unless_a_new_one_is_given`. Browser: a page reload only (V16) |
| A9 no key or token in file, responses or log | `a9_no_key_reaches_the_configuration_file_or_any_response` (file and every response, API keys and a sign-in token); process log by `tools/ax-admin-e2e.sh` (V18) |
| A10 sign-in saved only after success; cancel keeps nothing | `a10_browser_sign_in_saves_only_after_it_succeeds`, `a10_a_finished_sign_in_is_saved_once_and_removed_with_its_tokens`, `a10_tokens_of_a_sign_in_nobody_keeps_are_removed`, `a10_a_sign_in_the_page_never_saved_is_dropped_with_its_tokens`; UI `signIn.test.ts`. With a stand-in sign-in backend (V12) |
| B1 protocol negotiation, session and version on later calls | `b1_the_negotiated_protocol_version_and_session_are_sent_on_later_calls` |
| B2 expired session reopened once | `b2_an_expired_session_is_reopened_once` |
| B3 rejected OAuth token refreshed once; API key not retried | `b3_a_rejected_oauth_token_is_refreshed_once_and_an_api_key_is_not` (stand-in token source; the real forced refresh is not exercised, V12) |
| B4 unlisted tool refused locally | `b4_b5_only_listed_tools_are_called_and_unknown_listed_tools_work` |
| B5 pages follow the tool list; unknown tools usable | same test; UI `pages.test.ts`; browser "A3" and "A6, E9" |
| B6 timeout and size limit | `b6_slow_and_oversized_answers_are_errors`; `the_servers_timeouts_and_busy_answers_are_told_apart` |
| C1 bound to loopback | `c1_the_listener_is_bound_to_loopback` |
| C2 foreign Host or Origin refused | `c2_other_hosts_and_origins_are_refused` |
| C3 changes need the request token | `c3_changes_need_the_request_token` (every changing route, including sign-in completion) |
| C4 no-store on every response | `c4_responses_are_never_cached` |
| D1 one binary, no indexing engine | gauntlet L7 (`cargo tree`, with a control that finds the engine in `ax-mcp`) and L9 |
| D2 `install-admin.sh` | `scripts/test-install-admin.sh` (gauntlet L11) |
| D3 `install-admin.ps1` | `scripts/test-install-admin-ps1.sh` under PowerShell 7 on macOS (gauntlet L11); the Windows PATH step is not executed |
| E1 non-admin credentials unchanged | `e1_non_admin_credentials_see_the_recorded_list_and_cannot_call_admin_tools`, `e1_the_non_admin_tool_list_is_the_admin_list_without_the_admin_tools`, `the_admin_scope_is_advertised_only_where_a_token_can_carry_it` (V1) |
| E2 admin key works on its own project only | `e2_admin_key_lists_and_calls_admin_tools_on_its_own_project_only`, `e2_an_admin_subject_needs_the_scope_and_the_grant`, `e2_admin_tools_are_described_as_admin_scoped_without_project_arguments`, unit tests `admin_grant::*` |
| E3 admin needs write access | `e3_admin_grants_require_write_access` |
| E4 old configuration files load | `e4_configuration_without_admin_fields_loads_and_grants_nothing` |
| E5 no other project path | `e5_admin_calls_cannot_select_another_project_root`; `policy_reload_is_the_only_policy_action_available_remotely` (V2) |
| E6 sync and index update the project | `e6_sync_and_index_update_the_pinned_project`, `e6_two_concurrent_index_requests_both_finish_with_a_consistent_index`, `an_index_run_finishes_after_its_request_timed_out`, `calls_do_not_queue_behind_a_running_index`, `index_changing_calls_are_limited_and_reads_are_not`; browser "E6, E9" (see V16 for what it can show) |
| E7 call log: newest first, paged, bounded, no bodies, own project | `e7_logs_are_newest_first_paged_bounded_scoped_and_without_bodies` |
| E8 works in the headless build | gauntlet L8 (headless build) and L10 (the browser story runs against that binary). `ax-mcp` has no optional features, so its tests in L3 and L4 are the headless tests |
| E9 admin pages only with the grant; operations ask first | browser "A3", "A6, E9", "E6, E9"; UI `pages.test.ts` |
| E10 longer client timeout for admin calls | `mcp::tests::admin_tools_get_ten_minutes_and_other_tools_do_not`; `b6_slow_and_oversized_answers_are_errors` |
| N1 non-admin behaviour and old configuration unchanged | E1 and E4 checks; existing `tests/remote_mcp.rs` passes (gauntlet L3) |
| N2 `ax`, `ax web`, slim build unchanged; existing tests pass | gauntlet L3, L4, L5. `tests/remote_mcp.rs` received two added struct fields (`admin_subjects`, `admin`) in its fixture so that it compiles; no assertion was changed |
| N3 no per-version text in docs | by inspection of the changed documents |

## Gauntlet results

| Layer | Command (inside the gauntlet) | Result |
|---|---|---|
| L1 UI types | `npm run typecheck` (application; tests, browser story and Playwright config under the strict options) | passed |
| L1 UI unit tests | `npm test` | 34 passed, 0 failed |
| L1 awaited assertions | pattern check on `e2e/*.spec.ts`, with control | no un-awaited locator assertion |
| L1 production build | `npx vite build` | exit 0 |
| L2 ax-admin, ax-remote-client | `cargo test -p ax-admin -p ax-remote-client` | 45 passed, 0 failed (8 unit + 29 integration; 8 in ax-remote-client) |
| L3 remote MCP | `cargo test -p ax-mcp --test remote_admin --test remote_mcp` | 16 passed and 14 passed, 0 failed |
| L4 full suites | `cargo test -p ax-mcp -p ax-usage --no-fail-fast` | 13 test binaries, 593 passed, 2 failed, both in the baseline; 0 new failures |
| L5 ax-web, ax-cli | `cargo check -p ax-web -p ax-cli`; `cargo test -p ax-web remote` | builds; 5 passed, 0 failed |
| L6 lint | `cargo clippy -p ax-admin -p ax-remote-client -p ax-mcp -p ax-usage --all-targets -- -D warnings` | 0 warnings |
| L7 D1 | `cargo tree -p ax-admin -e normal` | 204 crates; none of `ax-extraction`, `ax-db`, `ax-core`, `ax-mcp`, `tree-sitter` |
| L8 headless build | `cargo build -p ax-cli --no-default-features --features "minimal-grammars mimalloc"` | builds; `ax web` answers that it is not in this build |
| L9 binary | `cargo build -p ax-admin`; `ax-admin version` | `ax-admin 10.1.1` |
| L10 real run | `tools/ax-admin-e2e.sh` (headless ax + ax-admin + Chrome) | 9 passed, 0 failed; no key in ax-admin's files or log |
| L11 installers | `scripts/test-install-admin.sh`, `scripts/test-install-admin-ps1.sh` | all checks passed (7 and 5 steps); `site/public` copies identical |
| L12 encoding | UTF-8 without BOM on every changed file | ok |
| L13 documentation site | `npm test` in `site/` | 71 passed, 0 failed |
| L14 mutation | `tools/ax_admin_mutants.py --self-test`, then all mutants | 43 of 43 killed |

The run ended with `GAUNTLET PASSED`, exit code 0.

Changed-line coverage was not measured with a coverage tool (none is set up in
this repository). The mutation layer stands in for it on the security and
correctness checks; it does not cover every changed line.

### Baseline

Two `ax-mcp` tests fail on `main` at `9010913d` as well. They were run there:
`cargo test -p ax-mcp --lib unchanged_catalog_and_memory_titles` and
`cargo test -p ax-mcp --test ice_preflight`. Neither touches code changed here.
They are not fixed in this change.

- `server::policy_integration::unchanged_catalog_and_memory_titles_are_sent_once_per_session`
- `preflight_returns_exact_indexed_manifest_line`

### Mutation

Hand-written mutants, each one plausible bug in a check
(`tools/ax_admin_mutants.py`). A mutant counts as killed only when the code
built and a test failed. The runner first proves it reports a survivor: an
equivalent mutant (a changed comment) must come back as SURVIVED.

Result: **43 of 43 killed**; self-test: `S00 ... SURVIVED`, as required.

| Mutant | The bug it plants | Killed by |
|---|---|---|
| M01 | C2: any Host header is accepted | c2_other_hosts_and_origins_are_refused |
| M02 | C2: any Origin is accepted | c2_other_hosts_and_origins_are_refused |
| M03 | C3: changes need no request token | c3_changes_need_the_request_token |
| M04 | C1: listener bound to every interface | c1_the_listener_is_bound_to_loopback |
| M05 | A9: the key is written into the connection name | a9_no_key_reaches_the_configuration_file_or_any_response |
| M06 | A7: removing a connection keeps its key | a7_removing_deletes_the_entry_and_its_key |
| M07 | A5: redirects are followed with the key | a5_unreachable_non_mcp_and_redirecting_addresses_are_distinct_errors |
| M08 | B4: unlisted tools are sent to the server | b4_b5_only_listed_tools_are_called_and_unknown_listed_tools_work |
| M09 | B2: an expired session is not reopened | b2_an_expired_session_is_reopened_once |
| M10 | B3: an API key is retried like an OAuth token | b3_a_rejected_oauth_token_is_refreshed_once_and_an_api_key_is_not |
| M11 | B6: a streamed answer has no size limit | b6_slow_and_oversized_answers_are_errors |
| M12 | E10: admin calls get the short timeout | b6_slow_and_oversized_answers_are_errors |
| M13 | A9: configuration file readable by others | store::tests::configuration_file_is_private |
| M14 | E1: admin tools callable without the grant | e1_non_admin_credentials_see_the_recorded_list_and_cannot_call_admin_tools |
| M15 | E1: admin tools listed without the grant | e1_non_admin_credentials_see_the_recorded_list_and_cannot_call_admin_tools, e1_the_non_admin_tool_list_is_the_admin_list_without_the_admin_tools |
| M16 | E1: a key with the admin scope needs no admin grant | remote::auth::admin_grant::a_key_needs_the_scope_the_grant_and_write_access |
| M24 | E1: the admin scope is not required | remote::auth::admin_grant::a_key_needs_the_scope_the_grant_and_write_access, remote::auth::admin_grant::a_subject_needs_the_scope_and_a_place_in_admin_subjects |
| M25 | E1: any write subject is an admin subject | remote::auth::admin_grant::a_subject_needs_the_scope_and_a_place_in_admin_subjects |
| M17 | E2: a key works on every project | e2_admin_key_lists_and_calls_admin_tools_on_its_own_project_only |
| M18 | E3: admin without write access is accepted | e3_admin_grants_require_write_access |
| M19 | E3: admin subjects need no write access | e3_admin_grants_require_write_access |
| M20 | E5: a call may name another project path | e5_admin_calls_cannot_select_another_project_root |
| M21 | E7: the call log shows every project | e7_logs_are_newest_first_paged_bounded_scoped_and_without_bodies |
| M22 | E7: the call log page size is unbounded | e7_logs_are_newest_first_paged_bounded_scoped_and_without_bodies |
| M26 | a saved key is sent to a changed address without asking for it again | a_saved_key_is_never_sent_to_another_address |
| M27 | a call for another connection than the active one is run | a_call_for_a_connection_that_is_no_longer_active_is_refused |
| M28 | every policy action is available remotely | policy_reload_is_the_only_policy_action_available_remotely |
| M29 | ax:admin is advertised to every client | the_admin_scope_is_advertised_only_where_a_token_can_carry_it |
| M30 | an index run is dropped when its request times out | an_index_run_finishes_after_its_request_timed_out |
| M31 | index-changing calls have no slot limit | index_changing_calls_are_limited_and_reads_are_not |
| M32 | the old key is not restored when a save fails | a_failed_save_leaves_keys_and_connections_as_they_were |
| M33 | memory changes although the file was not written | store::tests::a_failed_save_changes_nothing_in_memory |
| M34 | tokens of a sign-in cancelled too late are kept | a10_tokens_of_a_sign_in_nobody_keeps_are_removed |
| M35 | a failed second sign-in signs the saved connection out | a10_tokens_of_a_sign_in_nobody_keeps_are_removed |
| M36 | sign-in status saves the connection without the request token | a10_browser_sign_in_saves_only_after_it_succeeds |
| M37 | the timeout after which an index run continues is not marked | an_index_run_finishes_after_its_request_timed_out |
| M38 | a finished sign-in that no page saved keeps its tokens | a10_a_sign_in_the_page_never_saved_is_dropped_with_its_tokens |
| M39 | a late-cancelled sign-in removes the tokens of a newer attempt | a10_tokens_of_a_sign_in_nobody_keeps_are_removed |
| M43 | an address stays marked after its leftover tokens were removed | a10_a_sign_in_the_page_never_saved_is_dropped_with_its_tokens |
| M42 | ending any sign-in attempt signs out tokens ax-admin never stored | a10_browser_sign_in_saves_only_after_it_succeeds |
| M40 | calls queue for minutes behind a running index | calls_do_not_queue_behind_a_running_index |
| M41 | a proxy's timeout page is taken for a still-running index | the_servers_timeouts_and_busy_answers_are_told_apart |
| M23 | E2: admin tools lack the admin scope in discovery | e2_admin_tools_are_described_as_admin_scoped_without_project_arguments |

Not covered by a mutant: the guard that keeps a client from being cached while
its connection changes (no deterministic test, V17), and the browser code
(TypeScript is not mutated).

### Checks written for this change, and their negative controls

| Check | Proven able to fail by |
|---|---|
| `cargo tree` engine check (L7) | The same pattern must find the engine crates in `ax-mcp` |
| Awaited-assertion check on the browser story (L1) | The pattern must match a sample un-awaited assertion |
| Full-suite baseline (L4) | Fails on a build error and when fewer than 12 test binaries report |
| `install-admin.sh` test | Tampered archive, unusable binary, missing checksum and a pinned release without the asset must each be refused |
| Mutation runner | Self-test with an equivalent mutant |

## Review rounds

Two independent reviewers in fresh contexts, read-only, each round on the
updated diff. `ax_skill` returned only metadata for the review skills in this
session, so the reviewers read the skill files from `.agents/skills/`.

| Round | Skills (status) | Findings | Fixed |
|---|---|---|---|
| 1 | `old-coder`, `old-coder-api`, `rust-review`, `typescript-review`, `react-review` (bodies read from disk) | 52 (15 major, 37 minor) | all, or disclosed as V1 to V14 |
| 2 | same | 17 (3 major, 14 minor) | all, or disclosed as V15 to V17 |
| 3 | same | 10 minor, plus one remainder | all, V18 |
| 4 | same | 5 (1 major, 4 minor) | all |
| 5 | same | 1 minor (Rust); 0 (UI) | R5-1 |
| 6 | `rust-review`, `old-coder-api` | 0 | — |

Independent verification in the sense of the old-coder skill (a blind verifier
given only the contract and the source state) was not performed.

## What went wrong on the way, and how it was resolved

- **Two index requests in one server stalled for three minutes.** Two sessions
  of one process waited on each other's database and file locks. The remote
  server now runs one index-changing call per project at a time.
- **A search during a remote index hung for two minutes.** A read can sync
  changed files, so it is a writer too. Calls now share a per-project lock that
  an index run takes alone, and a call that meets a running index is told to
  come back after two seconds (V3).
- **Review found that a saved key could be sent to another address**, that the
  policy tool could restore global policy on the host, and that a timed-out
  index run was dropped halfway. All three are fixed and have a test and a mutant.
- **A fix for leftover sign-in tokens could sign out `ax mcp remote login`.**
  Found in round 4, fixed so that ax-admin only removes tokens it left itself.
- **One browser assertion was never awaited.** Once awaited it failed, and
  showed that a running Ax picks up new files by itself. The test now claims
  only what it can prove (V16).
- **The first full gauntlet run stopped on a mistake in the gauntlet script**
  (a pipeline that failed on `ax web`'s exit code), and one mutant pointed at a
  line a later fix had changed. Both were corrected before the run reported here.

## Known limits

- No automated test exercises the real browser sign-in, the forced token
  refresh or the OS credential store. They are tested through stand-ins (V12).
- The Windows user-PATH change of `install-admin.ps1`, and the release workflow
  job `build-admin`, were not executed.
- No run against a real Azure or Vercel deployment. The real run is against a
  headless Ax on loopback.
- No automated test for dropping a stale reload answer in the UI, for the
  sign-in wait view as a component, or for an `axc_` client token at an admin
  tool (V17).
- Whether a browser offers to save the API key field cannot be verified here (V18).
- During an index run started through ax-admin, other calls to that project are
  refused with "try again" (V3). That is new for agents using the same project.
- The built-in authorization server cannot grant admin access; an API key or an
  external provider is needed. Reading the existing code, the built-in server
  also rejects the `offline_access` scope that browser sign-in requests; that
  code is outside this change and was not exercised.
- `ax-mcp` has two pre-existing failing tests (see Baseline).

## Environment changes made

- New crates `crates/ax-admin` and `crates/ax-remote-client`; `Cargo.lock` updated.
- `npm install` in `crates/ax-admin/web-ui` (adds `node_modules`, ignored). One
  development package beyond the spec's list: `@types/node` (V10).
- Build output under `target-dev/`, including a separate headless build in
  `target-dev/headless/`.
- Checkpoint commits on `feature/ax-admin` (see `git log main..feature/ax-admin`); nothing was pushed.
- The gauntlet and the browser run use temporary directories for `HOME`, the
  usage database and the ax-admin configuration. The repository's `.ax` folder
  and the owner's OS credential store were not touched.
