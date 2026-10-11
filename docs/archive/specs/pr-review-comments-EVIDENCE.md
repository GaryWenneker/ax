# EVIDENCE: ask per comment before posting PR review comments

Spec: `docs/specs/pr-review-comments.md`. Spec approval: "Approve, build it". Tier 2 (new seeded skill plus two template edits; no runtime code paths beyond the existing seed and global.db store).

Source state: uncommitted working tree on `76da9c6`. `pr-review-comments/SKILL.md` sha256 `7fececb95e4c3b64…`, `seed.rs` sha256 `1058537e171c309b…`. Toolchain: cargo 1.98.0, Python 3.14.7.

## What changed

- New global skill `crates/ax-policy/templates/skills/pr-review-comments/SKILL.md` (seedVersion 1): review a colleague's PR without fixing, draft one comment per finding with one or two proposed texts, ask one question per comment (`Post: <text>`, `Do not post`, free text) with `AskQuestion` / `AskUserQuestion`, post only what the user chose, then summarize.
- `seed.rs`: added to `GLOBAL_SKILL_BUNDLES` with `global_db: true`, so install seeds it into `~/.ax/global_policy/skills/`, `~/.cursor/skills/`, and `~/.ax/global.db`.
- `review-loop` skill (seedVersion 2 → 3) and `old-coder-mandatory` rule (seedVersion 2 → 3) hand colleague PRs to `pr-review-comments`. The version bumps make existing installs upgrade unedited copies.
- Docs: `site/src/content/docs/guides/policy-engine.md` (global skills table and a "Reviewing a colleague's pull request" section).

## Behavior → test mapping

| Spec item | Verified by |
|---|---|
| Skill exists with every step and the per-comment question | `seed::tests::pr_review_comments_skill_pins_every_step` |
| Seeded to global policy and Cursor skills | `seed::tests::global_seed_writes_pr_review_comments` |
| Stored in global.db next to review-loop | `seed::tests::global_db_skills_are_review_loop_and_pr_review_comments`, `commands::init::tests::seeded_skills_are_stored_once_as_company_skills`, the blocking-store test in `init.rs` |
| review-loop and the rule hand colleague PRs over, with seedVersion bumps | `seed::tests::review_loop_hands_colleague_prs_to_pr_review_comments` |
| Must not: break the review-loop for the user's own change | existing `review_loop_skill_pins_every_step`, `old_coder_rule_requires_the_review_loop` (still green) |

## Gauntlet (final fresh run after the last code edit)

| Layer | Command | Result |
|---|---|---|
| Tests ax-policy | `env -u CARGO_TARGET_DIR cargo test -p ax-policy` | 173 passed, 0 failed |
| Tests ax-cli | `env -u CARGO_TARGET_DIR cargo test -p ax-cli` | 43 passed, 0 failed |
| Clippy (changed files) | `cargo clippy -p ax-policy -p ax-cli --tests` | no warnings in `seed.rs`, `init.rs`, `installer/mod.rs`; one pre-existing warning in `ide_seed.rs:425` (not touched by this task) |
| Mutation | `python3 scripts/review-loop-mutants.py` | 20/20 killed, sources restored (sha256 verified) |
| Real execution | reinstall, then `ax install --yes --target cursor` | printed "Stored review-loop, pr-review-comments in ~/.ax/global.db"; both SKILL.md copies exist; installed review-loop and rule at seedVersion 3; `ax_skill("pr-review-comments")` over MCP returned the full skill |
| Types | covered by `cargo test` compilation | ok |
| Property tests, supply chain | not applicable | no parsing logic, no new dependencies |

## Failures and how they were resolved

- GREEN first failed on `review_loop_hands_colleague_prs_to_pr_review_comments`: the test expected a backticked `` `pr-review-comments` `` in the rule, but the rule names skills as `ax_skill({ name: "pr-review-comments" })` like its other steps. The assertion now checks that exact call, which is stricter, not weaker.
- Two existing mutant anchors went stale: `global_db: true` now appears twice, and the rule moved to seedVersion 3. Both mutants were re-anchored.
- Mutation first ended at 18/20. "Batch approval allowed" and "no free text option" survived, because the pin only checked the loose phrase "free text". The pin now checks `- free text: the user writes their own comment` and `Never ask for a batch approval of several comments`. The rerun killed 20/20.
- The global.db row was not checked with sqlite, because auto-review blocked reading `~/.ax/global.db`. The install output and `ax_skill` over MCP stand in for that check.

## Review rounds

| Round | Skills | Findings | Result |
|---|---|---|---|
| 1 | rust-review (seed.rs, init.rs tests), python-review (mutant entries), generic checklist (Markdown) | 0 | done |

## Known limits

- The skill asks every question first and then posts. The spec does not say whether to post after each answer or at the end.
- The tests pin the skill's wording. Whether an agent actually asks one question per comment depends on the agent following the skill, and no test here can check that.
