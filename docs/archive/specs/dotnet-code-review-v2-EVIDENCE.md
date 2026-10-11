# EVIDENCE: dotnet-code-review v2 (spec `docs/specs/dotnet-code-review-v2.md`)

- **Spec approval:** "Approve, build it" (AskQuestion answer).
- **Tier:** 2.
- **Isolation:** none. Edits went into the working tree; nothing is committed.
- **Source state:** HEAD `76da9c6` plus uncommitted changes. The sha256 of the skill, `pack.toml`, `stack_catalog.rs`, `stacks.rs`, and `stack-review-mutants.py` concatenated is `7f80d25a518954431240e849270f42cc5102a7f56950ed5036ccfc6607c2c412`.
- **Final run:** 2026-09-23 11:56 CEST; rustc and cargo 1.98.0, Python 3.14.

## Behavior to test mapping

| Spec | Verified by |
|---|---|
| D1: 12 sections plus key phrases, including the blocker rules | `stacks::tests::dotnet_review_skill_covers_every_section` |
| D2: no duplicate bullet, full rule set (more than 50 bullets) | `stacks::tests::dotnet_review_skill_has_no_duplicate_bullets` |
| D3: `upgrade` rewrites an unedited older copy; lock records 1.2.0 | `stacks::tests::upgrade_rewrites_an_unedited_older_dotnet_review`, plus the real execution below |
| Hand-edited copy is not overwritten | Existing `stacks::tests::upgrade_skips_user_edit`, plus the real execution below |
| Frontmatter kept (name, triggers, scope, share) | Diff review. `apply` tests still find `dotnet-code-review` |
| Must not: the four `dotnet-*.mdc` rules change | `git diff` of those files shows only changes from before this task |

## RED and GREEN

- **RED:** all 3 new tests failed. D1 reported `lost "## 1. Naming and casing"`. D2 reported `got 37 bullets`. D3 reported that `template_version` was still 1.1.0. The rewrite part of D3 already passed, so the upgrade mechanism is pre-existing and kept as regression armor.
- **GREEN:** 167/167 ax-policy tests pass.

## Gauntlet

| Layer | Result |
|---|---|
| `cargo test -p ax-policy -p ax-cli` | 167 passed and 43 passed, 0 failed |
| Clippy (`-A clippy::invalid_regex`) | No hits in changed lines. `stacks.rs:360` (collapsible `if`) was already there before this change |
| `python3 scripts/stack-review-mutants.py` | **6/6 killed**, sources restored (sha256 verified). The first run killed 5/6: "async ban lost" survived, so I added the blocker rules to the pinned phrases |
| `python3 scripts/review-loop-mutants.py` (script changed by R1-1) | 12/12 killed |
| mypy | Skipped: not installed. Both scripts parse (`ast.parse`) |
| Real execution | See below |
| Dependencies | None added |

### Real execution

- **Old binary:** in a temp git repo, `ax policy stack apply dotnet .` wrote the 114-line skill and lock `templateVersion: 1.1.0`.
- **New binary:** after reinstall, `ax policy stack upgrade .` changed the file to 211 lines, byte-identical to the template, with lock `1.2.0`.
- **Edited copy:** after I appended "local edit", `upgrade` left the file alone.
- Both commands also print "project not initialized - run ax init". That comes from the ax.db import step, which this temp repo lacks; it predates this change and doesn't affect the files.

## Review rounds

| Round | Skills (status) | Findings | Fixed |
|---|---|---|---|
| 1 | `old-coder` usable, `python-review` usable, `rust-review` usable (loaded earlier this session; a re-call was blocked by auto-review), Markdown against the generic checklist | 1 minor | R1-1: the mutant runners' functions had no type annotations (python-review, "Types and API"). Fixed in `stack-review-mutants.py` and in `review-loop-mutants.py` from the previous task, whose loop reviewed only Rust. I also added a `read_bytes` helper so the runners stop opening files without closing them |
| 2 | same | 0 | — |

## Known limits

- D2 catches exact duplicate bullets only. I removed near-duplicates (the two directives restate most rules) by hand when merging.
- D1 pins section headings and selected rules, not every bullet. A rule outside the pinned list can be dropped without a test failing.
- The four `dotnet-*.mdc` rules still overlap with the skill (for example, the `Async` suffix and `ConfigureAwait`). They were out of scope.
