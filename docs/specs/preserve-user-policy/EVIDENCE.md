# EVIDENCE: Preserve user-customized rules and skills

**SPEC:** `/Users/gary/io/ax/docs/specs/preserve-user-policy/SPEC.md`  
**Approval:** user replied `approve` (2026-11-09)  
**Source state:** working tree on branch after preserve-user-policy implementation (not yet tagged)

## Spec → test map

| Scenario | Result |
|----------|--------|
| A1 global seed keeps hand edit without force | `seed_global_keeps_hand_edited_rule_without_force` PASS |
| A2 create missing | existing `seed_writes_defaults_once` / global seed tests PASS |
| A3 sync --fix missing only / keep drift | `sync_fix_restores_missing_startup`, `sync_fix_keeps_drifted_startup_without_force` PASS |
| A4 sync --force restores drift | `sync_fix_restores_drifted_startup_template` (uses `sync_instructions_with_force(..., true)`) PASS |
| A5 IDE force gate | `repairs_stale_cursor_ax_rule` uses `seed_ide_agent_workflow_with_force(..., true)`; default seed create-missing PASS |
| A6 global.db insert-if-absent | `seeded_skills_are_stored_once_as_company_skills` PASS (custom body survives second store) |
| A7 english-only template | `crates/ax-policy/templates/rules/english-only.mdc` no longer hardcodes Dutch PR comments |

## Commands (final)

```bash
cargo test -p ax-policy --lib -- seed_ sync_fix sync_ide write_skill global_db_skills older_seeded current_review
# 25 passed

cargo test -p ax-policy --lib -- repairs_stale seeds_ creates_ skips_
# ide_seed subset PASS

cargo test -p ax-cli --bin ax -- seeded_skills_are_stored
# 1 passed
```

## Layers

| Layer | Result |
|-------|--------|
| Unit tests (changed) | PASS (commands above) |
| Mutation | skipped (manual behavior covered by A1/A3/A6 red→green style tests) |
| Docs | policy-engine, troubleshooting, CLI reference updated (English) |

## Known limits

- Marked-section upserts (`AGENTS.md`, Copilot/Windsurf ax blocks) still refresh the ax-managed marker region; dedicated rule/skill files are create-missing.
- Product template upgrades for global policy require an explicit future `--force` path on seed (CLI today: `ax policy sync --fix --force` for project managed files).
