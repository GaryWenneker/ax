# EVIDENCE: Links between rules, skills and memories, plus vault follow-ups

Spec: `docs/specs/obsidian-links.md` (approved: "Approved, start building").
Tier 3. Source state of the final gauntlet: `e33f2ac` on branch
`obsidian-links` (worktree `/Users/gary/io/ax-links`). Later commits touch only
docs and the rebuilt `web-ui/dist`.

Entry points:

- `scripts/links-gauntlet.sh`: every layer, exits 1 on any failure
- `scripts/links-mutants.sh`: the mutation runner
- `scripts/links-real.sh`: real execution

Tool versions: rustc 1.98.0, cargo 1.98.0, node v26.8.2.

## Open item from your answers

You picked "Other" without text for "What a link does". I built the two
options you picked (preflight and the Command Center). If you meant more,
say so.

## Spec to test mapping

| Clause | Verified by |
|---|---|
| L1 parse every form | `l1_parses_every_form`, `l1_link_does_not_span_lines`, `l1_non_ascii_text_around_links`, `l1_never_panics_on_any_short_input` |
| L2 code is not a link | `l2_code_is_not_a_link`, `l2_unclosed_backtick_is_literal`; mutants code-fence-skip, inline-code-skip |
| L3 path stays in its folder | `l3_path_links_stay_in_their_folder`; mutant path-folder |
| L4 bare-name order | `l4_bare_name_order`; mutant bare-name-order |
| L5 case and `.md` | `l5_case_and_md_suffix_are_ignored` |
| L6 missing | `l6_missing_target_is_none` |
| L7 shared page names | `l7_page_names_are_safe_and_unique`; vault mutant stem-unique |
| P1 linked skill delivered (revision 5) | `p1_matched_rule_delivers_linked_skills`; mutants one-hop, inject-rebuilt; real execution over MCP stdio |
| P2 no duplicate | `p2_already_matched_is_not_delivered_twice`; mutant no-duplicate |
| P3 one hop | `p3_links_inside_linked_items_are_not_followed`; mutant one-hop |
| P4 cap 5 | `p4_at_most_five_linked_items`, `follow_stops_at_cap`; mutant cap |
| P5 disabled or unapproved never delivered | `p5_disabled_or_unapproved_targets_are_never_delivered`; mutants disabled-target, unapproved-skill |
| P6 memories both ways | `p6_memories_link_both_ways` |
| P7, P8 no change without links | `p7_and_p8_nothing_to_follow_changes_nothing`; full `ax-mcp` suite |
| A1 to A4 | `a1_outgoing_links_are_parsed_and_resolved`, `a1_global_items_carry_their_project_id`, `a2_backlinks_cover_every_kind_sorted_by_kind_then_id`, `a3_unknown_item_is_404_and_bad_kind_is_400`, `a4_works_in_readonly_mode`; mutants backlink-scan, api-global-skills; real execution |
| U1 clickable links (revision 6) | TS `U1: resolved links become Markdown links…`, `opens rules, skills and memories…`, `U1 a linked item stays selected while the list loads`; mutant ts-selection-loading; browser: memory → rule click |
| U2 missing style | TS `U2: a missing link gets the missing style and tooltip` |
| U3 code stays code | TS `U3: links in fenced code…`; mutants ts-fence, ts-inline-code |
| U4 Linked from | browser screenshot: "Linked from (1) Link note" |
| V1 to V5 | `v1_…`, `v2_app_json_adds_only_missing_keys`, `v2_complete_app_json_is_served_byte_for_byte`, `v3_…`, `v4_…`, `v5_…`; four app-json mutants |
| G1 to G7 (G5 narrowed, revision 2) | `g1_…` to `g7_…` (15 tests in `dav_links.rs`); mutants global-delete-guard, global-draft, global-leader-row, global-copy-guard, global-folder-guard |
| N1 existing suites green | gauntlet test layer |
| N2 no real `global.db` / `ax.db` | `n2_tests_use_a_temp_global_db`; `links-real.sh` isolates `HOME` and `AX_GLOBAL_DB` |
| N3 no note files on disk | real-execution check; `n1_vault_writes_leave_no_files_on_disk` |
| N4 bodies never modified | TS `U3 and N4…`; link code only reads rows |
| N5 no new dependencies | `Cargo.lock` and `package-lock.json` unchanged by this branch |
| N6 `/dav` CORS, share token, readonly | existing vault tests; vault mutants cors-outside, share-gate, readonly-guard (12/12 killed) |

## Gauntlet results (final run at `e33f2ac`)

| Layer | Result |
|---|---|
| Tests: ax-web, ax-policy, ax-memory, ax-mcp, ax-db, ax-global-db, ax-core | 464 passed, 1 failed: the baseline `new_tools_smoke::cycles_api_path_handlers_work` ("project not initialized"), which fails before this change as well |
| Web-ui node tests | 89 pass, 0 fail |
| Types | `tsc --noEmit` exit 0; cargo builds clean |
| Suite health | new tests run 10 times in a row: 10/10 green |
| Clippy | 0 warnings in new and changed files (`-A clippy::invalid_regex` for a pre-existing error in ax-context) |
| rustfmt | `--check` clean on the six new Rust files |
| Mutation (links) | 25/25 killed |
| Mutation (vault, rerun) | 12/12 killed |
| Real execution | `links-real.sh`: 20/20 OK, release binary built at `e33f2ac`, temp project |
| Browser | memory → click `[[link-rule]]` → rule opens with "Linked from (1)"; served bundle `index-C3ZL00yy.js` |
| Coverage | skipped: `cargo llvm-cov` is not installed |
| Supply chain | nothing to audit: no new dependencies. No secrets in the diff. New capabilities: `GET /api/links` (read-only), and vault writes to `global.db` through the existing Command Center save code |

## Gauntlet checker controls

- Test layer: breaking one assertion in `links_api.rs` made the gauntlet exit 1 with "failing tests beyond the baseline: a1_outgoing_links_are_parsed_and_resolved". Restored afterwards.
- Mutation runner: it failed closed twice in practice. A rustfmt-changed literal gave "literal found 0 times", and a survivor gave exit 1.
- Real execution: before the link existed, preflight did not deliver the skill, and a full reply was confirmed by the presence of `link-rule`.

## What failed along the way, and the fix

- Mutant app-json-unchanged survived at first, because the test fixture was already in serde's canonical format. The fixture now uses tabs and a trailing newline. The assertion is unchanged.
- Real execution showed the inject budget: in a fresh project the always-apply content fills 16,000 characters, so a linked skill may be named in the "Skills omitted" line instead of having its body included (spec revision 5).
- The browser showed U1 broken for links to another page: the Rules and Skills pages cleared the selection while their list was still loading. Fixed with a RED test first (spec revision 6).
- g2: the existing `spawn_policy_dedup` deletes shadowed global rows when the hub opens. The test now asserts that the edit lands on the leader row.
- The repo's `post-checkout` hook exits 1, so `git checkout -- file` could not restore mutants. Both runners now restore from a copy, with a trap on exit and on signals.

## Landing in the main tree

- Your tree gained new preflight session code after the snapshot, so `crates/ax-mcp/src/tools.rs` was merged by hand. Links expand before `build_preflight_meta` and the session inject, and memories are recalled once, before both. Every other file applied cleanly. The pre-merge tree is saved as `refs/backup/pre-links`.
- `second_preflight_on_a_connection_skips_unchanged_bodies` failed after the merge. It measured this repo's live `ax.db`, where the second call had room for a linked skill that didn't fit the first time. At your choice, it now runs on a temp project with three always-apply rules and no links. Control: with the session skip disabled (`delivered: None`) it fails.
- In the main tree after the merge: `ax-mcp` 76/76, and ax-policy, ax-web, ax-core and ax-memory green. Web-ui 89 pass, `tsc` clean. The gauntlet and mutation numbers above come from the worktree, not from the merged tree.
- Rebuilt with `scripts/reinstall-cli.sh`. `ax web` on 7070 serves `index-C3ZL00yy.js`, matching `dist/index.html`.

## Known limits

- The accepted limits in the spec still apply: ambiguous bare names, the heading part of a link is ignored for delivery, no code links, and renames outside Obsidian don't rewrite links.
- Linked items get no priority inside the inject budget (revision 5).
- Coverage was not measured.
- Independent verification: not performed.
