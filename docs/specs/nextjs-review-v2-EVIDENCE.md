# EVIDENCE: nextjs-review v2 (spec `docs/specs/nextjs-review-v2.md`)

- **Spec approval:** "Approve, build it" (AskQuestion answer). That approval includes the nine corrections to the directive listed in the spec.
- **Tier:** 2.
- **Isolation:** none. Edits went into the working tree; nothing is committed.
- **Source state:** HEAD `76da9c6` plus uncommitted changes. The sha256 of the skill, `pack.toml`, `stack_catalog.rs`, `stacks.rs`, and `stack-review-mutants.py` concatenated is `d16df8967b959dab458376b1a06a5e3f367976e6ef20b6e29ff49a8ae008465a`.
- **Final run:** 2026-09-23 12:05 CEST; rustc and cargo 1.98.0.

## Behavior to test mapping

| Spec | Verified by |
|---|---|
| N1: 12 sections plus key phrases, including the Server Action and Route Handler auth rules | `stacks::tests::nextjs_review_skill_covers_every_section` |
| N2: no duplicate bullet, none shared with `react-review`, more than 50 bullets | `stacks::tests::nextjs_review_skill_has_no_duplicate_bullets` |
| N3: `upgrade` replaces an unedited older copy; lock records 1.2.0 | `stacks::tests::upgrade_rewrites_an_unedited_older_nextjs_review`, plus the real run below |
| Hand-edited copy is kept | `stacks::tests::upgrade_skips_user_edit`, plus the real run below |
| Must not: `react-review` and the nextjs and react `.mdc` rules change | Not touched in this task |

## RED, GREEN, and refactor

- **Test-structure refactor first** (dotnet tests onto `stack_file_body`, `bullets`, `assert_no_duplicate_bullets`, and `upgrade_from_older_copy`). Assertions are unchanged. Afterwards 14/14 stack tests passed and the dotnet mutants still scored 6/6.
- **RED:** all 3 new tests failed. N1 reported `lost "## 1. Structure and naming"`. N2 reported `got 10 bullets`. N3 failed on the lock version.
- **GREEN:** 170/170 ax-policy tests pass.

## Gauntlet

| Layer | Result |
|---|---|
| `cargo test -p ax-policy -p ax-cli` | 170 passed and 43 passed, 0 failed |
| Clippy | No hits in changed lines; `stacks.rs:360` was already there before this change |
| `python3 scripts/stack-review-mutants.py` | **11/11 killed** (6 dotnet, 5 nextjs). The first run killed 10/11: "Server Action auth check dropped" survived because "checks the session" also matched the Route Handler rule. Both rules are now pinned with their full phrase |
| Real run | See below |
| Dependencies | None |

### Real run

- **Old binary:** in a temp git repo, `ax policy stack apply nextjs .` wrote `nextjs-review` (33 lines) and `react-review`. The lock read `nextjs 1.1.0, react 1.1.0`.
- **New binary:** after reinstall, `ax policy stack upgrade .` changed the file to 166 lines, byte-identical to the template. The lock read `nextjs 1.2.0, react 1.1.0`.
- **Edited copy:** after I appended "local edit", `upgrade` kept the edit.
- **Final binary:** after the round 1 fix and a last reinstall, `apply --force` wrote the final template, byte-identical.
- The "project not initialized" message comes from the ax.db import step in a repo without `ax init`. It predates this change.

## Review rounds

| Round | Skills (status) | Findings | Fixed |
|---|---|---|---|
| 1 | `old-coder`, `rust-review`, and `python-review` usable; skill text against the generic checklist, including a technical accuracy pass | 2 minor | R1-1: the `upgrade_from_older_copy` doc comment left out the returned temp dir (rust-review, Structure). R1-2: "every Route Handler checks the session" would flag webhooks and deliberately public endpoints. Webhooks verify a signature instead, and a public endpoint says so in a comment. This revises the approved wording and is disclosed here |
| 2 | same | 0 | — |

## Known limits

- The overlap check against `react-review` compares exact bullets. I removed rewordings of react-review rules (derived state, Context, props into state) by hand.
- N1 pins headings and selected rules, not every bullet.
- A few rules depend on the installed version (`params` as a promise, `useActionState`). The skill tells the reviewer to check `package.json` and the Next.js types, but no test checks that the reviewer does so.
