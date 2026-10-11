# EVIDENCE — Choose IDEs during `ax init`

- SPEC: `docs/specs/init-ide-selection.md` (Tier 2). Approved with revised A6 ("Yes, build it"). Revision 1 (the A7 correction, the N2 finding) and A10 (`AX_INIT_IDES`) were added during implementation and not approved separately.
- Source state: uncommitted work on top of `b7caeb1`. No commits. Isolation: none.

## Behavior → test

| Behavior | Test |
|---|---|
| A1 menu, groups, "found" | `ide_choice::groups_cover_every_target_once_with_editors_first`; menu rendering shared with stacks (`stack_menu_text_is_stable`); interactive run: not automated (see limits) |
| A2 saved in `ax.json`, other keys kept | `config::project_ides_round_trip_and_keep_other_keys`, `init_ides::a2_a5_a6_…` |
| A3 saved list wins, even empty | `ide_choice::saved_list_wins_even_when_empty`, `init_ides::a3_…` |
| A4 connects exactly the chosen ones; none → says so | `init_ides::a2_a5_a6_…`, `a3_…` |
| A5 rerun keeps the choice | `init_ides::a2_a5_a6_…` |
| A6 dropped saved IDEs are removed, only those, files listed once | `ide_choice::only_saved_ides_that_were_dropped_are_removed`, `init_ides::a2_a5_a6_…`, `init_ides::a6_only_saved_ides_are_removed` |
| A7 no terminal: found IDEs, nothing saved | `init_ides::a7_…` |
| A8 typed fallback, unknown id names the valid ids | `ide_choice::typed_choice_parses_ids_none_and_empty`, `init_ides::a8_…` |
| A9 unknown saved ids warned and ignored | `ide_choice::unknown_saved_ids_are_split_out`, `init_ides::a9_…` |
| A10 `AX_INIT_IDES` | every `init_ides` test that passes an answer |
| N1 installer unchanged | no edits to installer write logic; `ide_choice.rs` is new and pure |
| N2 stack menu identical | `stack_menu_text_is_stable` (written before the refactor, green before and after) |
| N3 no new dependencies | no manifest changes |

## Gauntlet (final run, after the last code edit)

| Layer | Command | Result |
|---|---|---|
| Rust tests | `env -u CARGO_TARGET_DIR cargo test -q --no-fail-fast -p ax-installer -p ax-policy -p ax-cli` | 346 passed, 0 failed |
| Clippy | `cargo clippy -q -p ax-installer -p ax-policy -p ax-cli --all-targets` | 0 warnings in the changed files (one existing warning in `ax-remote`) |
| Mutation | `env -u CARGO_TARGET_DIR bash scripts/init-ides-mutants.sh` | 16/16 killed |
| Real execution | reinstalled CLI; `AX_INIT_IDES="cursor claude" ax init`, then `AX_INIT_IDES=cursor ax init` in a temp project with a temp HOME | saved `{"ides":["cursor","claude"]}`; the second run printed "Removed ax from Claude Code: …" and left Cursor unchanged |
| Supply chain | no new dependencies | n/a |

RED evidence: `ide_choice` 5/5 failed on stubs; `project_ides` failed on a stub; `init_ides` failed 4/6 before `AX_INIT_IDES` existed. The two that passed early (A7, A9) are covered by the mutants for `asked_over: None`, `is_detected` and the unknown-ids warning. The removed-files check failed with the dedup removed, then passed with it restored.

## Review rounds

| Round | Findings | Resolution |
|---|---|---|
| 1 | The removed-files line listed `~/.claude/settings.json` (and `hooks.json`) twice | order-preserving dedup, a RED/GREEN test, and a mutant |
| 2 | 0 | — |

## Known limits

- The interactive menu itself (key handling) is not driven by a test. It is the stack menu's code, shared, and the typed and `AX_INIT_IDES` paths are tested. Run `ax init` once in a terminal to see it.
- Behavior change for CI: a first `ax init` without a terminal now connects only the found IDEs. Before, it connected all 13 targets.
- The stack and IDE menus don't align their id column. This is an existing bug (`{:<12}` on a colored string), kept unchanged per N2.
- Independent verification: not performed.
