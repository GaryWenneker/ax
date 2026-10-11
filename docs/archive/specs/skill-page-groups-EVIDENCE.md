## Evidence Report — Command Center skill groups (Tier 2)

- Spec approval: obtained from user (`ik wil dat je de app aanpast zodat dit zo werkt`) for `/Users/gary/io/ax/docs/specs/skill-page-groups.md`
- Source state: working tree (uncommitted); `cargo` and `node` were **not** on PATH in the implementation environment (`~/.cargo/bin` contains only `ax`)
- Toolchain: not executed here — rerun `/Users/gary/io/ax/tools/gauntlet-skill-groups.sh` on a machine with Rust and Node
- Entry point: `/Users/gary/io/ax/tools/gauntlet-skill-groups.sh`
- Independent verification: not performed
- Spec approval correlation: human approved the spec; gauntlet numbers below are **unverified** in this session

### Spec → Test mapping

| Scenario | Test | Status |
|---|---|---|
| Catalog has a fixed order | `catalog_has_fixed_order` in `crates/ax-policy/src/skill_groups.rs` | unverified |
| Empty groups omitted from the list | `empty_groups_are_omitted_from_visible_list` | unverified |
| Empty groups remain assignable | `empty_groups_remain_in_catalog_for_editor` | unverified |
| Explicit group wins over aliases | `explicit_group_wins_over_aliases` | unverified |
| Legacy skill without group uses aliases | `legacy_skill_without_group_uses_aliases` | unverified |
| No group and no alias is ungrouped | `no_group_and_no_alias_is_ungrouped` | unverified |
| Ungrouped node hidden when unused | `ungrouped_node_hidden_when_unused` | unverified |
| Group nodes expand and collapse | UI `aria-expanded` + `toggleCollapsed` in `skillGroups.ts` / `PolicySkills.tsx` | unverified (no browser; no cargo) |
| Filter does not show empty groups | `visibleSkillGroups` on filtered `visible` list | unverified |
| Create skill can persist empty group | `parse_skill_group_roundtrip` + editor `group` select | unverified |
| GET /skills stays compatible | `skill_row_json_keeps_name_and_tags` | unverified |
| Invalid group id is not a 500 | `invalid_group_id_resolves_to_ungrouped` | unverified |
| Schema upgrade (skills) | `crates/ax-db/tests/migration_v18.rs` | unverified |
| Seeded rule ids via aliases | `seeded_rule_ids_resolve_via_aliases` | unverified |
| Rule YAML group roundtrip | `parse_rule_group_roundtrip` | unverified |
| Schema upgrade (rules) | `crates/ax-db/tests/migration_v19.rs` | unverified |
| Rules list grouping UI | `PolicyRules.tsx` + `visibleRuleGroups` | unverified |

### Gauntlet layers

| Layer | Command | Result |
|---|---|---|
| Unit tests | `tools/gauntlet-skill-groups.sh` | **unverified** — `cargo: command not found` |
| Types | `npx tsc --noEmit` in `crates/ax-web/web-ui` | **unverified** — `node: command not found` |
| Catalog copies | `diff` of the two `skill-groups.json` files | **unverified** (files were copied from the same source) |
| Mutation | skipped: toolchain missing | skipped |
| Coverage fail-under | skipped: toolchain missing | skipped |
| Real UI | `scripts/rebuild-web.ps1` / `ax web` | **unverified** — cannot embed `web-ui/dist` without cargo |

### Known limits

- Command Center will serve the new UI only after a web rebuild (`scripts/rebuild-web.ps1` on Windows, or `npm run build` in `web-ui` plus `cargo build --release -p ax-cli` where the toolchain exists).
- Matching / `ax_preflight` does not use `group`.
