# EVIDENCE — Client name cleanup (v5.0.1)

- **Spec:** `docs/specs/client-name-cleanup.md`, approved 2026-09-23 ("Approve, build it and release v5.0.1"). Revisions 1–4 are listed at the end of the spec; none changes the approved behaviors.
- **Tier:** 2 (normal).
- **Source state:** staged tree `6f104bc2fd39e78c35108134d49cc1253403583f` on top of `30fac85`; the release commit is tagged `v5.0.1`.
- **Isolation:** none — the change was made on `main` in the working tree, as declared for this repo's release flow. The untracked `.agents/` and `test-smoke/.agents/` copies were not added to git.
- **Terms file:** `~/.ax/redact-terms.txt` (outside the repo, 22 terms). All numbers below come from the final run after the last code edit.

## Behavior → verification

| Spec item | Verified by |
|---|---|
| B1 gate fails on a tracked hit, prints no term, fails closed | `scripts/check-client-names.sh`; controls below |
| B2 defaults: no wiki, folder name, no client values | `config::tests::defaults_without_ax_json_have_no_wiki_and_no_client_values` |
| B2 ax.json keys override defaults; `wiki_root_url` falls back to `wiki_remote` | `config::tests::ax_json_values_override_defaults`, `explicit_wiki_root_url_wins_over_remote` |
| B2 dry run without `wiki_remote` → `not-configured`, exit 0, workspace still scanned, no clone | `tests::dry_run_without_wiki_remote_skips_wiki_and_scans_workspace` |
| B2 `--skip-wiki-pull` without `wiki_remote` is not an error | `tests::skip_wiki_pull_without_wiki_remote_is_not_an_error` |
| B2 products parser: header row, separator rows, `--` cells, skip list | `scan::tests::products_parses_plain_and_bold_rows_and_skips_header`, `products_skips_configured_values_and_double_dash_cells`, `products_missing_file_is_empty` |
| B2 generic titles and tags, 8 memories, stable ids | `memories::tests::titles_are_generic_and_use_the_catalog_name`, `ids_are_stable_so_existing_rows_update_in_place`, `master_index_tags_are_generic` |
| B2 skill only when configured | `memories::tests::skill_is_mentioned_only_when_configured` |
| B2 no-wiki wording (revision 3) | `memories::tests::master_index_says_when_no_wiki_is_configured` |
| B2 `SyncReport` JSON renames | `tests::report_json_uses_generic_field_names` |
| B3 empty default share URL, blank URL stays blank | `config::tests::default_onedrive_url_is_empty` |
| B3 OneDrive error names the key | `providers::onedrive::tests::pull_onedrive_without_share_url_names_the_ax_json_key` |
| B4 skills English and generic | gate (0 hits) + Dutch word scan (0 hits) + copies byte-identical to templates |
| B5 fixtures renamed, assertions unchanged | existing tests in `ax-quality`, `ax-remote`, `ax-share`, `ax-mcp`, `ax-policy`, `ax-web` pass (except baseline) |
| B6 specs neutral | gate (0 hits) |
| B7 capture refuses without terms, removes rows, fails on leftovers | capture controls below; recaptured `cc-memory-vault.png` (3 rows removed) and `cc-graph.png` (2 removed) |
| B7 SonarQube cards unreadable | `cc-sonarqube-dark.png` blurred with `sharp` (box 1350,730 1120×830 at 2560×1600), checked by eye |
| Must not change: existing tests | full suite: zero new failures (below) |
| Must not change: configured sync still imports | real execution with a local git wiki: `wikiAction: cloned`, 2 pages, 1 integration, 1 product |
| Must not change: CLI flags | `main.rs` diff touches doc strings only |

## Gauntlet (final run)

| Layer | Command | Result |
|---|---|---|
| Full suite | `env -u CARGO_TARGET_DIR cargo test --workspace --no-fail-fast` | 703 passed, 4 failed. 3 are the known Windows-only baseline (`ax-quality` `bootstrap::tests::{legacy_prefix_from_workspace_folder, resolves_placeholder_to_folder_name}`, `ax-usage` `savings::tests::cursor_transcript_path_filter`). The 4th, `ax-usage` `savings::tests::transcript_import_does_not_wipe_state_tokens` ("no such table: agent_session_log"), is in an untouched crate and passed 3 of 3 isolated reruns: flaky under workspace load, pre-existing |
| Changed crates | `cargo test -p ax-docs-catalog` / `-p ax-share` | 15/15 and 24/24 pass |
| Types | `npx tsc --noEmit -p .` in `crates/ax-web/web-ui` | exit 0 |
| Frontend production build | `npm run build` in `crates/ax-web/web-ui` | exit 0 |
| Lint | `cargo clippy -p ax-docs-catalog -p ax-share --all-targets -- -A clippy::invalid_regex` | no warning on a changed line. `-A clippy::invalid_regex` is needed because `ax-context/src/directory.rs:155` (untouched) fails that deny-by-default lint and stops the build |
| Format | `rustfmt --check` on `crates/ax-docs-catalog/src/*.rs` | clean |
| Coverage on changed lines | — | **skipped**: no coverage tool installed, and installing one was not in the approved setup plan. Mutation below is the substitute |
| Mutation | `bash scripts/mutants-client-cleanup.sh` | 15/15 killed. The script checks that the unmutated tests pass first, that each mutant applied, and that each file is restored from the index |
| Real execution | `ax docs-catalog sync --dry-run [--json]` in a fresh workspace, with and without a local git wiki | as in the table above |
| Supply chain | — | no new dependencies. Capability diff: the capture script now reads one file from `~/.ax` |
| Suite health | isolated reruns of `ax-usage` (3×) | see the flaky test above |

## Negative controls

| Checker | Known-bad input | Result | Non-vacuous proof |
|---|---|---|---|
| gate, content | tracked file holding the first term | exit 1, `content: nc-gate-fixture.txt:1` | gate copy with the content grep disabled: exit 0 on the same file |
| gate, path | tracked file whose name holds the first term | exit 1, `path: nc-<term>.txt` | — |
| gate, terms | missing file / comments only | exit 1 with a message | — |
| capture, terms | `AX_REDACT_TERMS_FILE=/nonexistent` | exit 1, not readable | — |
| capture, leftovers | term on a button outside any row | exit 1, image hash unchanged | script copy with the check disabled wrote the image |
| capture, filter | `AX_SHOTS=bogus.png` | exit 1 | — |

Limits: the gate checks only git-tracked text and file names, not binary content, and only the listed spellings. The capture check cannot see text drawn on a `<canvas>`; the graph canvas was checked by eye.

## Review rounds

| Round | Skills (status) | Findings | Fixed |
|---|---|---|---|
| 1 | `old-coder` usable, `rust-review` usable, `typescript-review` usable, `javascript-review` usable, `react-review` usable; bash: generic checklist | 6 (2 major, 4 minor) | R1-1 Dutch trigger in `pr` description; R1-2 `.ax.json` fallback not in spec → revision 4; R1-3 gate path grep swallowed errors; R1-4 comments-only terms file failed silently; R1-5 mutation run lacked a baseline; R1-6 rustfmt |
| 2 | same | 1 (minor) | R2-1 no `bash` example in the new `cli.md` section |
| 3 | same | 0 | — |

## Other

- The shared-pack copies in `.ax/policy/shared/skills/` still hold the old text; they come from the team share and are replaced on the next pull.
- Spec approval: obtained before implementation.
