# EVIDENCE — Git-shared `.agents` for rules and skills

**Spec:** `/Users/gary/io/ax/docs/specs/agents-git-share.md`  
**Tier:** 2  
**Spec approval:** not obtained (autonomous run after “Implement the plan as specified”)  
**Source state:** tree dirty on `b0676ba29130ed93ea1e8f9db1db9ee5f22e5f82` (this change uncommitted)  
**Entry point:** `/Users/gary/io/ax/tools/gauntlet-agents-git-share.sh`

## Behavior mapping

| Spec | Test |
|---|---|
| A1 project write `.agents` | `agents_share::tests::project_write_path_is_agents` |
| A2 workspace `.agents` | `agents_share::tests::workspace_write_path_is_workspace_agents` |
| A3 private/company paths | `agents_share::tests::private_and_company_paths_unchanged` |
| A4 gitignore | `agents_share::tests::gitignore_lists_private_and_inactive` |
| A5 layer order | `agents_share::tests::layers_legacy_then_agents_then_inactive_then_private` |
| A6 migrate | `migrate_moves_rules_not_pending`, `migrate_skips_when_destination_exists` |
| A7/A8 disable/enable move | `disable_relocate_leaves_agents`, `enable_relocate_restores_agents` |
| A9 private never `.agents` | `private_write_dir_never_agents` |
| A10 export filter | `git_export_candidate_filters` |
| A11 leak gate | `leak_gate_detects_disabled_and_private` |
| A12 bootstrap pointers | `ide_seed::tests::upserts_agents_and_gemini_markers` + gauntlet grep |
| A13 cursor symlink | `cursor_skill_symlink_points_at_agents` |

## Gauntlet (fresh run via entry point)

Command: `bash /Users/gary/io/ax/tools/gauntlet-agents-git-share.sh`

| Layer | Result |
|---|---|
| `cargo test -p ax-policy --lib` | **107 passed**, 0 failed |
| Bootstrap grep | pass (`.agents/rules/` and `.agents/skills/` in templates) |
| Negative control | leak test went **red** after `if !doc.frontmatter.enabled` was stubbed to `if false && …`; restored |
| Manual mutation | **5/5 killed**: export-ignores-enabled, leak-skip-disabled, inactive-never, agents-dir-typo, project-dir-legacy |
| Types/lint | skipped as separate layer: rustc via cargo test; no extra clippy run |
| Coverage fail-under | skipped: no diff-cover config for this crate in the gauntlet |
| Property tests | skipped: path layout not a parser/round-trip domain beyond existing parse tests |
| Supply chain | skipped: no new dependencies |
| Real execution | `cargo test` exercises fs migrate/symlink on this Darwin host |

## Known limits

- Filename prefixes were not implemented (per spec).
- Cursor does not natively load `.agents/rules`; team rules still arrive via MCP; skills may symlink.
- This repo’s `.ax/policy/{rules,skills}` were moved to `.agents/` on disk; `.ax/` remains gitignored except `!.ax/policy/**`, so `.agents/` is the new committed tree (add those files at commit time).
