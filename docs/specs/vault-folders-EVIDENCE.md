# EVIDENCE — Vault folders in Settings

- SPEC: `docs/specs/vault-folders.md` (Tier 3). Spec approval: "Implement the plan as specified" (the plan). Revision 1 (C5, C6, D8, U3) came out of review round 1 and was not approved separately.
- Source state: uncommitted work on top of `b7caeb1`. No commits, as agreed.
- Isolation: none. This works in the current tree because the change builds on other uncommitted work.

## Behavior → test

| Behavior | Test |
|---|---|
| C1 | `c1_list_round_trips_and_starts_empty` |
| C2 | `c2_bad_paths_are_rejected` (relative, `src`, missing, a file) |
| C3 | `c3_bad_or_duplicate_names_are_rejected` |
| C4 | `c4_sync_delete_and_readonly`, `c4_turning_index_on_imports_and_off_removes` |
| C5 | `c4_only_the_local_browser_may_use_the_folder_api` |
| C6 | `c4_saving_an_unchanged_folder_does_not_resync_it` |
| D1, D2 | `d1_d2_folders_are_listed_and_readable` (also: symlinks are not listed) |
| D3 | `d3_writes_reach_the_disk` |
| D4 | `d4_escapes_are_refused`, `dav::folders::dot_dot_is_refused_even_when_it_stays_inside` |
| D5, D6 | `d5_d6_folder_roots_are_protected_and_unknown_names_missing` |
| D7 | `d7_writes_resync_an_indexed_folder` |
| D8 | `d_folders_are_local_only_in_the_drive` (GET, PROPFIND, PUT, and MOVE into `folders/`) |
| Read-only | `d_readonly_refuses_folder_writes` |
| S1–S6 | `s1_…` to `s6_…` in `crates/ax-memory/tests/folder_sync.rs`, plus `s4_symlinks_are_not_followed` |
| U1, U3 | `e2e/vault-folders.spec.ts` (add with a bad path shows an alert and a description, then add, sync summary, DAV GET, recall, toggle off, remove) |
| U2 | `src/memoryCategory.test.ts` (`doc` → "Folder doc"), `src/vaultMount.test.ts` (sync summary text) |
| N1 | D4 tests plus the resolver mutants below |
| N2 | the existing `dav_vault`, `dav_mount`, `dav_links` and `dav_share` suites are unchanged and green |
| N3 | no manifest changes for new crates or npm packages |

## Gauntlet (final run, after the last code edit)

| Layer | Command | Result |
|---|---|---|
| Rust tests | `env -u CARGO_TARGET_DIR cargo test -q --no-fail-fast -p ax-memory -p ax-web -p ax-mcp` | 295 passed, 1 failed (see the flake below) |
| Flake check | `cargo test -q -p ax-mcp --lib` (parallel and `--test-threads=1`), plus the single test 4× | 81/81 both times, 4/4 |
| Clippy | `cargo clippy -q -p ax-memory -p ax-web --all-targets` | 0 warnings in the changed files |
| Types | `npx tsc --noEmit -p .` | exit 0 |
| Lint (TS) | eslint | skipped: web-ui has no ESLint config |
| Unit (TS) | `node --test src/*.test.ts src/lib/*.test.ts` | 188/188 |
| Mutation | `env -u CARGO_TARGET_DIR bash scripts/vault-folders-mutants.sh` | 23/23 killed. Each mutant is checked with `cmp` to prove it changed the file. They cover the resolver, the symlink skip, name and path validation, the readonly and local-only API guards, the unchanged-folder filter, the DAV local-only guard and its MOVE destination, root protection, and the sync filters, limits, no-op and stale deletion |
| Real execution | `reinstall-cli.sh`, `ax web`; served bundle `index-D0mTzRMu.js` = `dist/index.html` | OK |
| E2E | `npx playwright test e2e/vault-folders.spec.ts e2e/memory-files.spec.ts e2e/calm-lists.spec.ts --project=system-chrome` | 11 passed |
| Supply chain | no new dependencies | n/a |
| Capability diff | new: reads and writes to directories the user configures, and a background tokio task for resync | covered by C5, D4, D8 and N1 |

Failed test: `server::policy_integration::unchanged_catalog_and_memory_titles_are_sent_once_per_session` fails only in the combined run. It runs `ax_preflight` twice against the live repo's `ax.db` and asserts that the index snapshot didn't change between the calls. The running ax daemon re-indexes files that were just edited, so the snapshot does change. The test passes in every isolated run. It predates this change, and this change doesn't touch the snapshot code.

## Failure model (Tier 3)

| Mode | Layer |
|---|---|
| Path escape (`..`, absolute part, symlink out) | D4 tests, resolver unit test, 3 mutants |
| A foreign web page or LAN client reads or indexes the disk | C5 and D8 tests, 3 guard mutants |
| An index gets out of sync with the files | S2, S3, D7 tests, and the no-op and stale mutants |
| Unbounded import | S4 limits, size and count mutants |
| Blocking the async runtime | disk walks and DAV folder operations run in `spawn_blocking` (not measured by a test) |

## Review rounds

| Round | Findings | Resolution |
|---|---|---|
| 1 | R1-1 blocker: the folder API (behind permissive CORS) and `/dav/folders` were reachable by foreign pages or remote clients. R1-2 major: blocking `std::fs` in async code. R1-3 minor: resync errors were swallowed. R1-4 minor: the modal had no `aria-describedby`. R1-5 minor: errors had no `role="alert"`. R1-6 minor: PUT re-synced every indexed folder | R1-1: local-only guard on every API route and on DAV (source and destination), RED→GREEN. R1-2: `spawn_blocking`. R1-3: `tracing::warn`. R1-4 and R1-5: `useId` descriptions and alerts, checked by e2e. R1-6: sync only new or changed folders, RED→GREEN |
| 2 | 0 | — |

## Known limits

- A symlink swapped in between the check and the file open (TOCTOU) is not defended against. The resolver canonicalizes the path and then opens it.
- The Memory page has no `doc` filter chip, only a category color and label plus the `folder:<name>` tag.
- Changes made outside the vault are picked up at the next sync (start, Sync now, or a write through the vault). There is no file watcher.
- The startup sync runs in the background, so recall can briefly miss docs right after `ax web` starts.
- Independent verification: not performed. The self-review and the gauntlet are the only assurance.
