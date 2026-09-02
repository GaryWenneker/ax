# SPEC — Git-shared `.agents` for rules and skills

**Tier:** 2 (normal)  
**Spec approval:** not obtained as a separate review (autonomous run after the user said to implement the attached plan).  
**Isolation:** work on the current branch (no worktree) — gauntlet needs existing `target-dev/` and crate deps.

**No new dependencies.**

## Setup plan

| Item | Path / command | Why |
|---|---|---|
| Spec | `/Users/gary/io/ax/docs/specs/agents-git-share.md` | Executable contract |
| Evidence | `/Users/gary/io/ax/docs/specs/agents-git-share-EVIDENCE.md` | Gauntlet report |
| Gauntlet | `/Users/gary/io/ax/tools/gauntlet-agents-git-share.sh` | One entry point |
| Tests | `crates/ax-policy/src/agents_share.rs`, hierarchy/store/index/ide_seed tests | RED/GREEN |
| Git | no checkpoint commits unless the user asks | user rule |

## Canonical layout

| Kind | Path | Git |
|---|---|---|
| Shared project/workspace rules | `<root>/.agents/rules/*.mdc` | tracked |
| Shared project/workspace skills | `<root>/.agents/skills/<name>/SKILL.md` | tracked |
| Private project | `<root>/.ax/policy-private/` | ignored (`policy-private/` in `.ax/.gitignore`) |
| Private user | `~/.ax/private_policy/` | never in repo |
| Company | `~/.ax/global_policy/` | never in repo |
| Inactive (local only) | `<root>/.ax/policy-inactive/{rules,skills}/` | ignored (`policy-inactive/` in `.ax/.gitignore`) |
| Legacy one-release read | `<root>/.ax/policy/{rules,skills}` | still indexed if present; new writes go to `.agents` |
| Pending / packs | `<root>/.ax/policy/pending`, `shared` | unchanged |

Workspace shared files live at the workspace root `.agents/`, not `.ax/policy`.

## Behaviors

### A1 — Project write path

Given a temp project root `P`  
When `policy_dir_for_scope(P, Project)` is called  
Then the path is `P/.agents`  
And `ensure_scope_dirs(P, Project)` creates `P/.agents/rules` and `P/.agents/skills`.

### A2 — Workspace write path

Given a workspace root `W` with `ax.json` members and member `W/svc`  
When `policy_dir_for_scope(W/svc, Workspace)` is called  
Then the path is `W/.agents`.

### A3 — Private paths unchanged

`policy_dir_for_scope(P, PrivateProject)` is `P/.ax/policy-private`  
`policy_dir_for_scope(P, PrivateUser)` ends with `.ax/private_policy`  
`policy_dir_for_scope(P, Company)` ends with `.ax/global_policy`.

### A4 — Gitignore entries

When `ensure_private_gitignore(P)` runs (also on inactive writes)  
Then `.ax/.gitignore` contains both `policy-private/` and `policy-inactive/`.

### A5 — Layer merge: agents wins over legacy, private wins over both

Given `P/.ax/policy` and `P/.agents` both exist as dirs  
When `policy_layers(P)` runs  
Then a Project layer for `.ax/policy` appears before a Project layer for `.agents`.  
Inactive dir `.ax/policy-inactive` appears after shared Project layers and before PrivateProject.  
PrivateProject still appears after Project.

### A6 — Migrate legacy files

Given `P/.ax/policy/rules/foo.mdc` and no `P/.agents/rules/foo.mdc`  
When `migrate_legacy_policy_to_agents(P)` runs  
Then `P/.agents/rules/foo.mdc` exists and the legacy file is gone.  
Pending (`pending/`) and `shared/` under `.ax/policy` are not moved.  
If destination already exists, leave the destination and still remove or skip the source without overwriting (skip source when dest exists).

### A7 — Disable moves shareable items out of `.agents`

Given an enabled project rule file `P/.agents/rules/demo.mdc`  
When the item is written with `enabled: false` via `resolve_shareable_write_dir` + `relocate_rule_file`  
Then `P/.agents/rules/demo.mdc` does not exist  
And `P/.ax/policy-inactive/rules/demo.mdc` exists  
And the inactive file frontmatter has `enabled: false`.

### A8 — Enable moves back

Reverse of A7: enabled write restores `.agents/rules/demo.mdc` and deletes the inactive copy.

### A9 — Private never lands in `.agents`

`resolve_shareable_write_dir(P, PrivateProject, true)` is `P/.ax/policy-private`  
Even when `enabled` is false, private stays under `policy-private` (not `.agents`, not required to use inactive).

### A10 — Export skips private, company, disabled

`is_git_export_candidate(scope, enabled)` is true only for packable scopes (project, workspace) with `enabled == true`.  
`export_policy_to_files` writes only those candidates into `out_dir/rules` and `out_dir/skills`.

### A11 — Leak gate (fail closed)

`agents_share_violations(P)` returns a non-empty list when `.agents/rules/x.mdc` has `enabled: false` or `scope: private_project` / `private_user`.  
Empty list when only enabled project/workspace files sit under `.agents`.  
Negative control: a known-bad file is detected.

### A12 — IDE bootstrap pointers

Seeded `AGENTS.md` / Gemini / Copilot / Windsurf / Cline blocks and Cursor/Claude/Continue templates contain `.agents/rules/` and `.agents/skills/`  
and do not instruct agents to load `.agents/` as a whole or `.ax/policy-inactive/` / `.ax/policy-private/` as team policy.

### A13 — Cursor skills: symlink when possible

`link_cursor_skills_to_agents(P)` after skills exist at `P/.agents/skills/foo/SKILL.md`  
creates `P/.cursor/skills/foo` as a symlink to `../../.agents/skills/foo` when the OS allows it.  
If symlink fails, the function returns an error string (caller may copy); the test asserts symlink success on this Unix host.

## Must not

- Filename prefixes as the isolation mechanism.
- Put private or inactive under `.agents/`.
- Change pack `is_packable` for private/company (still not packed).
- Stop indexing `.ax/policy/{rules,skills}` for one release (legacy read).
- New crates/npm/cargo dependencies.

## Gauntlet mapping

| Behavior | Check |
|---|---|
| A1–A6, A9, A11, A13 | `cargo test -p ax-policy --lib agents_share` + hierarchy tests |
| A7–A8 | `cargo test -p ax-policy --lib agents_share::tests::disable_` |
| A10 | `cargo test -p ax-policy --lib agents_share::tests::git_export` + index export test |
| A12 | `cargo test -p ax-policy --lib ide_seed` grep assertions + gauntlet grep |
| Leak negative control | gauntlet script plants a bad file, expects nonzero, then restores |
| Mutation | gauntlet script (5 mutants) |
