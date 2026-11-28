# Evidence: second pass, fix/review2-web-server (Command Center server)

Base: `origin/main` at `4f417000`. Scope: `crates/ax-web`, plus `crates/ax-share` (web-ui-014 token store), `crates/ax-skill-catalog` (shared same-origin fetch) and `crates/ax-policy` (policy write lock for the dav_vault flake).

RED means a test failed on the code before the fix. Where the RED was a compile error (the tested helper did not exist yet), the row says so.

## Owner decisions and follow-ups

| Item | State | RED seen | Test(s) | Commit |
|---|---|---|---|---|
| web-ui-014 (server API and ax-share storage) | Fixed | yes | `ax-share config::tests::the_git_token_goes_to_the_credential_store_and_never_to_a_file_or_the_api`, `::a_token_in_ax_json_moves_to_the_credential_store_on_first_load`, `::a_token_in_the_old_token_file_moves_to_the_credential_store`, `::without_a_credential_store_the_old_token_is_read_and_no_new_token_is_written`, `::an_empty_token_keeps_the_stored_one_and_clear_token_removes_it`; `ax-web tests/share_config_token.rs::share_config_reports_token_set_and_never_the_token` | ee452438 |
| Skill catalog well-known origin (ax-web install and audit) | Fixed | yes | `skill_catalog_api::well_known_origin_tests::a_well_known_file_on_another_origin_is_refused`, `ax-skill-catalog remote::tests::well_known_file_urls_stay_on_the_index_origin` | 63414d3f |
| dav_vault "Database unavailable" / lost-update flake (b01, b16) | Fixed (causes found and removed; see notes) | yes | `ax-policy tests/save_vs_index.rs::an_index_running_during_a_save_keeps_the_saved_rule`, `workspace_state::background_task_tests::dropping_the_tasks_stops_them`, `tests/dav_vault.rs::the_isolated_home_covers_the_global_database`; dav_vault suite 20 runs in a row: 20/20 passed | ce639e60, 62a345af |

web-ui-014 details: tokens live in the OS credential store through the `keyring` crate (service `io.wenneker.ax.share`, key `<canonical project root>|<repoUrl>`). On load a token in `ax.json` or in the old `~/.ax/auth/git-share.json` moves to the store and is removed from the file (`ax.json` written atomically). Without a store the old location is read, a warning is logged once, and saving a new token fails instead of writing plain text. `GET /api/policy/share/config` returns `github.tokenSet`, never the token. The UI field is left to the web-ui pass; only `hasToken` was renamed to `tokenSet` and the placeholder reads "Token set (type to replace)". `AX_SHARE_SECRET_STORE=memory` keeps tokens in process memory for automated runs.

dav_vault flake: two causes were found and fixed. (1) An index pass (the hub's background watcher runs `index_policy`) racing a save could prune the new rule or write an older body back; seen in a stressed run as `skill_page_round_trip_and_body_only_save` returning the old body, and reproduced deterministically by the ax-policy race test. Index passes and policy saves/deletes now share one lock per project. (2) The fixture's seed `WebHub` was dropped without `close()`, leaving its watcher, sync, cleanup and dedup tasks running beside the test hub; a dropped `WorkspaceBundle` now aborts them. The suites also did not set `AX_GLOBAL_DB` (web-server-079). The literal "Database unavailable" response did not reproduce locally in 32 stressed runs (4 parallel binaries x 32 threads) before or after the fix, so no RED exists for that exact message; the contention that leads to it (leaked writers on the same project and global databases) is removed. Not done here, outside this area: the ax-memory `remember_with_id` retry and `FileLock::acquire_wait` in ax-utils; CI's `--test-threads=1` stays until those land.

## Worklist findings

| Finding | State | RED seen | Test(s) | Commit |
|---|---|---|---|---|
| web-server-057 | Deferred | n/a | n/a | n/a |
| web-server-067 | Fixed | yes | `sonar_proxy::tests::only_sonarqube_cookies_reach_the_sonarqube_host` | c7c980b4 |
| web-server-068 | Fixed | yes | `tests/vault_folders.rs::c5_sync_of_a_folder_with_indexing_off_is_refused`, `::d11_a_resync_after_a_project_switch_leaves_the_new_project_alone` | c40384bb |
| web-server-069 | Fixed | no: logging only, no observable output | clippy, `web_auth::tests::*`, `tests/web_auth_http.rs`, `tests/web_auth_gate.rs` | 578ee914 |
| web-server-070 | Fixed | yes | `web_auth::tests::every_loopback_origin_counts_as_local` | 578ee914 |
| web-server-071 | Fixed | yes | `web_auth::tests::a_full_ceremony_store_drops_only_the_oldest` | 578ee914 |
| web-server-072 | Fixed | yes | `web_auth::tests::sign_in_identity_prefers_id_then_email_and_refuses_a_shared_display_name` | 578ee914 |
| web-server-073 | Fixed | yes | `web_auth::tests::a_session_lookup_writes_last_seen_at_most_once_a_minute` | 578ee914 |
| web-server-074 | Fixed | yes | `tests/workspace_routes.rs::reading_the_current_project_does_not_write_the_recent_list` | c25189e0 |
| web-server-075 | Fixed | yes | `workspace::tests::init_output_keeps_flowing_after_a_line_that_is_not_utf8`, `::a_second_init_of_the_same_folder_is_refused_while_one_runs` | c25189e0 |
| web-server-076 | Fixed | yes | `tests/workspace_routes.rs::purge_forgets_the_project_in_the_configured_global_database` | c25189e0 |
| web-server-077 | Fixed | yes (compile: `dedup_once` did not exist) | `workspace_state::policy_dedup_tests::writable_web_reports_dedup_without_mutating_project`, `::readonly_web_does_not_dedup` | ab36e2ce |
| web-server-078 | Fixed | yes (the new `: ready` wait failed before the feed sent it) | `tests/changes_feed.rs::a_new_memory_produces_one_memory_event`, `::an_idle_project_sends_no_events` | ab36e2ce |
| web-server-079 | Fixed | yes | `tests/dav_vault.rs::the_isolated_home_covers_the_global_database` (shared `tests/common/mod.rs`, used by dav_vault, dav_mount, dav_share, dav_links, lan_exposure, changes_feed, graph_recent, context_authority_api, hub_smoke) | 62a345af |
| web-server-080 | Fixed | no: coverage gap; the new route tests pass on old code | `tests/hub_smoke.rs::hub_open_and_hot_switch` (two temp projects, isolated home), `tests/workspace_routes.rs::mkdir_takes_one_plain_folder_name`, `::read_only_mode_refuses_every_folder_change`, `::purging_a_missing_folder_removes_nothing_on_disk` | c25189e0 |

web-server-057 deferred: the concrete defects were fixed in the first pass. What is left is the effort-L split of `crates/ax-web/src/policy.rs` into modules with generic rule/skill handlers. It is a pure move, and the open PR #329 (fix/review-policy) changes the same file, so doing it now would conflict; it belongs after #329 merges.

## Docs

- `site/src/content/docs/guides/policy-sharing.md`: credential-store token storage, migration, no plain-text fallback, `tokenSet`, `AX_SHARE_SECRET_STORE=memory`.
- `site/src/content/docs/reference/web-api.md`: well-known origin rule, Sonar proxy cookies, vault folder sync 409, loopback `AX_WEB_AUTH_ORIGIN`, sign-in identity order, last-seen throttle, workspace route behavior (recent list, init limits, `AX_GLOBAL_DB` purge), change feed `: ready`.
