# EVIDENCE: turn the savings gauntlet positive

Source state: uncommitted working tree on 60d9369. Spec: `/Users/gary/.cursor/plans/turn_savings_positive_ad04a2c6.plan.md` (approved by the user before implementation). Tier 2.

## Behaviour to test

| Behaviour | Test |
|---|---|
| Unchanged rule/skill bodies are listed, not resent; changed bodies resent | `ax-policy format::tests::{unchanged_always_rule_is_listed_not_resent, changed_rule_body_is_resent, unchanged_skill_is_listed_not_resent}` |
| IDE-loaded always-apply files skipped only when body identical and `alwaysApply: true` | `ax-policy ide_loaded::tests::*` (5) |
| Large always-apply skills summarized with `ax_skill` pointer | `format::tests::large_always_skill_is_summarized_with_ax_skill_pointer`, `small_always_skill_stays_inline` |
| CLI formatter output unchanged | `format::tests::default_options_match_legacy_formatter` |
| Per-connection session: new chat, TTL, initialize reset; connections isolated | `ax-mcp policy_session::tests::*` (6) |
| Second preflight on a connection skips bodies; private keys never leak | `server::policy_integration::second_preflight_on_a_connection_skips_unchanged_bodies` |
| New initialize resends | `server::policy_integration::new_initialize_resends_bodies` |
| `policy match --json` bodies once | `ax-cli commands::policy::tests::match_json_keeps_bodies_only_in_inject` |
| Preflight memories as titles | `ax-memory format::tests::match_titles_*` (2) |
| Explore: stopwords dropped, term coverage, data files ranked lower | `ax-db queries::tests::*` (7 new) |
| Rust/other doc comments indexed as docstrings | `ax-extraction languages::common::tests::*` (4) |
| `ax_node mode: signature` | `server::policy_integration::node_signature_mode_returns_location_and_declaration_only` |
| Neighbour lists capped, direct first | `ax-context explore_format::tests::{long_neighbor_list_is_capped_with_direct_edges_first, short_neighbor_list_is_unchanged}` |

## Gauntlet

- Tests: `cargo test -p ax-policy -p ax-memory -p ax-db -p ax-extraction -p ax-context -p ax-mcp -p ax-cli` → 440 passed, 0 failed (final run).
- Mutants (manual, restored; verified by rerunning): delivery recording removed → killed; initialize reset removed → killed; doc-comment adjacency check removed → killed.
- Tests that passed on the first run (no RED seen): `match_json_keeps_bodies_only_in_inject` and the two preflight integration tests. The integration tests were proven by the mutants above; the CLI strip test was not mutated.
- Clippy on changed files: no new warnings (`walk_nodes` warning was already there).
- Real execution: `scripts/bench-agent-efficiency/run-savings-gauntlet.sh` → exit 0, gate green, 10/10, net 158,205 / 215,769 (73.3%).
- `ax policy test`: `match_savings_gauntlet` OK; `inject_block` FAIL is pre-existing (no `agent-workflow` rule is indexed in this project).
- Supply chain: no new dependencies.

## Disclosed

- A doc comment was added to `call_tool_and_wrap` (accurate: it logs each call's savings estimate). It is the reason explore now finds that function for the benchmark query, together with the docstring extraction fix.
- A ranking test fixture used the anchor name `call_tool_and_wrap`, which inflated medium-explore's without-arm (7,904 → 22,338). It was renamed and the numbers above are from after the rename.
- The Cursor sandbox redirects `CARGO_TARGET_DIR`; builds must set `CARGO_TARGET_DIR=$PWD/target-dev`, or `target-dev/release/ax` stays stale.
- Not done: `ax ship --ci` wiring (needs a new quality-gate step type); CLI `callers`/`impact` text compaction and lean `ax_recall` (MCP output is already compact; the gauntlet measures MCP).
- Live Claude arm skipped: `claude` is not logged in.

## Round 3 (2026-09-28): every task above 50%

Spec: `docs/specs/savings-rules-50.md`, approved by the user ("Approve, implement and run the tests").

- Compact always-apply rules (`rule_inline_chars`, `AX_POLICY_RULE_INLINE_TOKENS`, default 250): tests `long_rule_is_sent_compact_with_directive_sections`, `long_rule_without_directive_sections_keeps_first_lines`, `short_rule_stays_full_with_rule_limit`. The first two were observed failing before the implementation. The third passed immediately (existing behaviour) and was proven non-vacuous by mutant 2.
- Legacy CLI output unchanged: `default_options_match_legacy_formatter` is still green.
- Mutants: (1) treat every `##` section as a directive section, killed by 2 tests; (2) always compact, killed by `short_rule_stays_full_with_rule_limit`. Both restored.
- Fixture change, disclosed: `session_fixture` in `server.rs` now puts its repeated lines under `## Rules`, so compaction keeps them and the test still measures session skipping alone. Assertions unchanged.
- Memory-title budget default is now 200 tokens (was 400).
- Gate is now at least 50% net per task and per feature (`MIN_NET_PCT`).
- Suites: ax-policy 217/217. ax-mcp policy_integration 6/6, and the rest of the ax-mcp, ax-db, ax-memory, ax-context, and ax-extraction suites are green.
- Final gauntlet (`python3 scripts/bench-agent-efficiency/gauntlet.py`, exit 0): net 173,674 of 218,624 (79.4%). Lowest tasks: medium-guard 55.1%, easy-rules 59.0%. Some `without` totals grew by 8 to 40 tokens because the source files they read grew. The `without:` hashes are unchanged.

## Round 4 (plan savings-2026-09-28-next.md)

Final fresh run after the last edit: `python3 scripts/bench-agent-efficiency/gauntlet.py`. Gate **green**, 21/21 tasks, without 802,700 / with 112,833 / net 689,867 (85.9%).
Report: `scripts/bench-agent-efficiency/out/summary.md`.

- memory-miss: RED was the gauntlet itself (anchor failed: filler memories returned, 177 tokens). Fix: `drop_weak_term_matches` clears matches when no hit covers any query term; `ax_recall` replies "No memories match '<q>'." Now 16 tokens, 97.5%.
- Suites: ax-memory, ax-mcp, ax-policy, ax-extraction green. `unchanged_catalog_and_memory_titles_are_sent_once_per_session` failed once in a parallel run, then passed alone and in 3 more full runs. Flaky, not fixed.
- Known limits: cross-crate qualified calls produce no graph edges (`ax_path` task uses an in-crate pair). Pre-existing failure `ax-usage savings::tests::cursor_transcript_path_filter` fails with this work stashed too.
- Not done: plan items 5 (intent-gated memory titles, review-language dedupe) and 6 (guard-intent answer). Baseline JSON for the round was not saved; before-values come from the baseline run reported in chat.
