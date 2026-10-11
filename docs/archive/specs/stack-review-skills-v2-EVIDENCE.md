# EVIDENCE: stack review skills v2

- **Spec:** `docs/specs/stack-review-skills-v2.md`, approved by the user ("Approve, build it").
- **Tier:** 2. These are seeded content and version bumps; the stack engine is unchanged.
- **Source state:** HEAD `76da9c6` plus uncommitted changes (nothing committed). The sha256 of the 28 review skills, the 30 `pack.toml` files, `stack_catalog.rs`, `stacks.rs`, `scripts/stack-review-mutants.py`, and `policy-engine.md`, concatenated in sorted path order, followed by `nextjs-review/SKILL.md` (63 files), is `d0ee20004aab1797dc09b70bb3d9abc393c059abe19768165a7a3794aba183ab`.

## Spec revision after the build

The user approved one change to a "Must not" clause afterwards. The approval was the answer "Yes, add it" to: "May I add 'a Context `value`' to that nextjs-review rule?" nextjs-review line 82 now lists "a Context `value` whose provider rerenders often" as a reason for `useMemo`, which matches react-review line 71. The pack stays at 1.2.0, although the question said it would be bumped. 1.2.0 has not shipped, and `upgrade` also rewrites an unedited copy when the template hash changes (`stacks.rs:357`), so existing projects get the fix without a bump. After the change, every gauntlet layer below was rerun.
- **Toolchain:** cargo 1.98.0, Python 3.14.7. No dependency was added.

## Behaviour to test mapping

| Spec clause | Verified by |
|---|---|
| S1: at least 10 numbered sections, a final `Output format` with verdict, location, section, impact, and suggested code, at least 50 rules | `stacks::tests::every_stack_review_skill_is_complete` |
| S2: no rule appears twice in a skill | `stacks::tests::every_stack_review_skill_has_no_duplicate_bullets` |
| S3: a building skill names its base in the intro and shares no rule with it (9 pairs, including nextjs on react) | `stacks::tests::building_skills_do_not_repeat_their_base` |
| S4: `upgrade` rewrites every unedited older copy; the lock records 1.2.0 | `stacks::tests::upgrade_rewrites_every_unedited_older_review_skill`, plus the real run below |
| S5: catalog and `pack.toml` agree at 1.2.0 for all 30 packs | `stacks::tests::every_stack_pack_is_version_1_2_0` |
| Section plans from the spec table | Review loop (below). Not machine-checked beyond S1 |
| Frontmatter kept except `description` | Review loop; existing `apply` tests still find each skill by name |
| Must not: change dotnet-code-review or nextjs-review (revised: the one nextjs line above is allowed) | dotnet-code-review was not edited in this task, and nextjs-review only on that line. Their mtimes change on every mutant run, because the runner rewrites and restores them; it verifies sha256 after restore. Their own tests still pass |
| Must not: change the stack engine | The only `stacks.rs` change in this task is appended tests. The earlier hunks come from previous tasks |
| Must not: overwrite an edited copy without `--force` | Existing tests, plus the real run below |
| Must not: add a dependency | `Cargo.lock` has no change from this task |

## Gauntlet (final fresh run, after the last code edit)

| Command | Result |
|---|---|
| `env -u CARGO_TARGET_DIR cargo test -p ax-policy` | **178 passed, 0 failed** |
| `env -u CARGO_TARGET_DIR cargo test -p ax-cli` | **43 passed, 0 failed** |
| `cargo clippy -p ax-policy --tests`, filtered to `stacks.rs` and `stack_catalog.rs` | 1 warning, `collapsible_if` at `stacks.rs:360`. That line is outside this task's diff and the warning was already there |
| `python3 scripts/stack-review-mutants.py` | **20/20 killed**, sources restored (sha256 verified) |
| `bash scripts/reinstall-cli.sh` | `ax 4.12.0` built to `target-dev/release/ax`; the PATH shim points to it |
| Real run, nextjs, after the line change and a reinstall | `ax policy stack apply nextjs .` wrote `nextjs-review/SKILL.md` byte-identical to the template, and it contains the new Context `value` reason |
| Real run in a temp git project with `Cargo.toml` | `ax policy stack apply rust .` wrote `.agents/skills/rust-review/SKILL.md`, byte-identical to the template (94 rules); `.ax/stacks.lock.json` has `"templateVersion": "1.2.0"`. After a user edit, `ax policy stack upgrade .` left the edited copy alone |

The first final mutant run was 19/20 and failed closed. The TypeScript mutant's anchor `` `javascript-review` `` matched 3 times, because review rounds added two rule references. I re-anchored it to the unique intro sentence "This skill builds on `javascript-review`;", and the rerun gave 20/20. Both temp-project commands also printed "project not initialized - run ax init". That message comes from the ax.db import step and predates this change (see the nextjs EVIDENCE).

### The 9 new mutants

| Mutant | Killed by |
|---|---|
| pascal: section 10 removed | S1 |
| rust: `Output format` heading renamed | S1 |
| svelte: `**Location:**` label dropped | S1 |
| python: a rule duplicated | S2 |
| laravel: a `php-review` rule copied in | S3 |
| typescript: intro no longer names `javascript-review` | S3 |
| r: truncated to 49 rules | S1 |
| go: `pack.toml` left at 1.1.0 | S5 |
| kotlin: catalog left at 1.1.0 | S5, S4 |

The 11 earlier dotnet and nextjs mutants were also killed.

## Review loop

Reviewers were subagents that checked every skill against its section plan, the stack's facts, internal consistency, and conflicts with the base skill.

| Round | Findings | Scope |
|---|---|---|
| 1 | 116 | all 28 skills |
| 2 | 29 | all 28 skills |
| 3 | 16 | the 15 files changed in round 2 |
| 4 | 7 | 11 files |
| 5 | 6 | 6 files |
| 6 | 10 | 9 files |
| 7 | 11 | 7 files |
| 8 | 8 | 6 files |
| 9 | 4 | 5 files |
| 10 | 6 | 3 files |
| 11 | 3 | the 8 building skills |
| 12 | 3 | 3 files |
| 13 | 1 | 2 files |
| 14 | 1 | luau |
| 15 | **0** | luau |

Every skill had a clean review after its last edit. From round 3 on, only the files changed in the previous round were re-reviewed. Every finding was fixed.

Base-rule conflicts in building skills kept turning up one at a time. So each building intro now ends with two sentences. One says the building skill's rule wins over the base skill. The other says a base rule does not apply where the framework defines and consumes the construct itself. Round 13 caught a regression that one of my own merges had introduced (Luau server authority over client-owned character positions), and I fixed it.

## Result

28 skills now have 10 to 12 review sections plus Output format, with 73 to 103 rules each. All 30 packs are at 1.2.0. `policy-engine.md` describes this.

## Known limits

- The section content was written by subagents and reviewed by subagents of the same model family. Checking stack facts (API names, version cut-offs) this way shares blind spots; no human read the rules.
- S1 to S3 check structure and duplication, not whether a rule is correct or matches the section plan. Those rest on the review loop alone.
- The Must-not clause for dotnet and nextjs rests on the edit record and their tests, not on a hash from before the task. The earlier EVIDENCE hashes cover files that this task legitimately changed.
