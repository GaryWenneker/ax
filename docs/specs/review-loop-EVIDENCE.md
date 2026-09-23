# EVIDENCE: review loop (spec `docs/specs/review-loop.md`, rev 2)

- **Spec approval:** "Approve, build it" (AskQuestion answer on spec rev 2).
- **Tier:** 2 (policy seeding plus one new global.db write path).
- **Isolation:** none. Edits went into the working tree as the spec's setup plan says. Nothing is committed.
- **Source state:** HEAD `76da9c6` plus uncommitted changes. The sha256 of the changed files, concatenated in this order, is `fd61464c4b93563253564cbdb6056e3c713bf7dda70ed504d338c9bc67c70da5`:
  - `init.rs`, `installer/mod.rs`
  - `seed.rs`, `lib.rs`
  - `old-coder-mandatory.mdc`, `review-loop/SKILL.md`
  - `review-loop-mutants.py`
- **Toolchain:** rustc 1.98.0 and cargo 1.98.0.
- **Final run:** 2026-09-23 11:47 CEST, after the last code edit.

## Behavior to test mapping

| Spec | Behavior | Verified by |
|---|---|---|
| R1 | `old-coder-mandatory` names the review loop, calls `ax_skill("review-loop")`, and has no round cap | `seed::tests::old_coder_rule_requires_the_review_loop` |
| R2 | `review-loop` is seeded to `~/.ax/global_policy/skills` and `~/.cursor/skills` | `seed::tests::global_seed_writes_review_loop_everywhere`, plus the real install below |
| R3 | The skill pins its four steps, the statuses, zero findings, no cap, and the review rounds table | `seed::tests::review_loop_skill_pins_every_step` |
| R4 | A higher `seedVersion` overwrites the file, hand edits included; only frontmatter counts | `seed::tests::seed_version_decides_upgrades`, `older_seeded_old_coder_rule_is_upgraded_once`, `seed_version_only_counts_the_frontmatter` |
| R5 | Install and init store `review-loop` in global.db once, as a company skill; a failure is an error, not a panic | `commands::init::tests::seeded_skills_are_stored_once_as_company_skills`, `unusable_global_db_is_an_error_not_a_panic`, `blocking_store_works_from_inside_a_runtime` |
| R5 | Only `review-loop` goes to global.db (not old-coder) | `seed::tests::review_loop_is_the_global_db_skill` |
| R6 | `ax_skill("review-loop")` works in a project without a local copy | Real execution: MCP call in `/Users/gary/io/ax`, which has no `.agents/skills/review-loop` or `.cursor/skills/review-loop` |
| R7 | The rule is CRITICAL | `old_coder_rule_requires_the_review_loop` checks the template |
| Must not | Vendored `old-coder` / `old-coder-api` SKILL.md change | `git status` on both template dirs: empty |
| Must not | A file already at the current seedVersion is overwritten | `seed::tests::current_review_loop_skill_is_not_overwritten` |
| Must not | Install or init fails when global.db fails | `run_installer` matches the result and prints a note, with no `?`. `unusable_global_db_is_an_error_not_a_panic` covers the error path |

## Gauntlet

| Layer | Command | Result |
|---|---|---|
| Tests | `env -u CARGO_TARGET_DIR cargo test -p ax-policy -p ax-cli` | ax-policy 164 passed, 0 failed. ax-cli 43 passed, 0 failed |
| Lint | `cargo clippy -p ax-policy -p ax-cli --all-targets -- -A clippy::invalid_regex` | Zero hits in changed lines. `init.rs` lines 356, 366, and 529 already failed before this change (telemetry mutex across await; saturating sub). With `-D warnings`, clippy 1.98 fails first in untouched crates (ax-installer, ax-telemetry, ax-types). That failure predates this change |
| Format | `rustfmt --check` on changed files | The repo is not rustfmt-clean; unrelated lines in the same files already differ. New code follows the surrounding style, and I did not reformat whole files |
| Mutation | `python3 scripts/review-loop-mutants.py` | **12/12 killed**; sources restored, verified by sha256 |
| Coverage | Not run | ax has no coverage tool set up. Every new function is exercised by a named test above, and mutation confirms the tests assert on it |
| Real execution | `ax install --yes --target cursor`, twice | See below |
| Supply chain | No new dependencies | `tokio`, `dirs`, and `sqlx` were already ax-cli dependencies |

### Mutation history

The first run killed 9 of 12:

- **"seedVersion read from the body too" survived.** No test covered this. I added `seed_version_only_counts_the_frontmatter`.
- **"store skips the upsert" survived.** The mutant was equivalent: a `continue` at the end of the loop changes nothing. I replaced it with "store writes the rules table".
- **"skill stops before zero findings" did not apply,** because the phrase appeared twice. I used a precise anchor. It then survived, because the pin test matched `zero findings` in the frontmatter description. I tightened the pin to the loop-ending sentence.

One run was invalid: I launched it in the same batch as a test edit. The runner snapshotted `seed.rs` before that edit and then restored the old snapshot. I reapplied the edit and reran.

### Real execution

- **Before the first install:** `~/.ax/global_policy/rules/old-coder-mandatory.mdc` had no `seedVersion`, `~/.cursor/skills/review-loop` did not exist, and global.db had no `review-loop` row.
- **First install** printed "Seeded 1 baseline Cursor skill(s)", then "Seeded 1 global policy file(s)", then "Stored review-loop in ~/.ax/global.db". Afterwards:
  - the rule has `seedVersion: 2` and the "… REVIEW LOOP → EVIDENCE" line;
  - both `review-loop/SKILL.md` copies exist;
  - global.db (inspected on a copy) has one row: project `/Users/gary/.ax`, `scope=company`, `sourcePath=global.db`, and a body containing `## 4. Repeat`.
- **Second install** printed only the "Stored" line and seeded no files. The global.db row count stayed at 1.
- **`ax_skill("review-loop")` over MCP** returned the full skill body.

## Review rounds

| Round | Skills (status) | Findings | Fixed |
|---|---|---|---|
| 1 | `old-coder` usable, `rust-review` usable; Markdown templates and docs against the generic checklist | 3 minor | R1-1: `ax init` stored twice, because init also calls `run_installer`. I removed the init call site, and the installer now prints the success line. R1-2: the global.db open error dropped its cause. My first fix added the path, but `open_pool` already adds it, so I narrowed the fix to `{e:#}` (full error chain) and dropped the equivalent mutant. R1-3: a test changed `AX_GLOBAL_DB` for the whole process. I split out `store_seeded_skills_blocking_at` and test it with explicit paths |
| 2 | same | 1 minor | R2-1: `ensure_project` and upsert errors also dropped their cause. Now `{e:#}` |
| 3 | same | 2 minor | R3-1: tests used `let _ =` without saying why (rust-review, Errors). Now a commented `cleanup` helper. R3-2: `seed_version` picked up the old doc comment of `seeded_content_needs_upgrade`. I removed the stale lines |
| 4 | same | 0 | — |

Not a finding for this diff: `run_installer` is synchronous filesystem work called from async `ax init`. That was already the case before this change. The new store keeps the same shape and runs on its own thread and runtime.

## Known limits

- **Parallel edits during the task.** Around 11:44–11:45, someone else changed files this task also touches: a `frontend-production-build` global rule (template, `GLOBAL_RULE_TEMPLATES` entry, and a test in `seed.rs`) and a `## Production build` section in the review-loop template. I did not write or review those changes. The final run includes them and is green. The review-loop template kept `seedVersion: 1` after that edit. That is fine while it is unreleased, but a machine that already has v1 will not receive later edits unless the version is bumped.
- **Init output.** The global.db line now comes from the installer (stderr, plain text), not from an init `ok_line`.
