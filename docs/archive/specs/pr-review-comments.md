# SPEC: ask per comment before posting PR review comments

Status: draft, awaiting approval. Tier 2. The user asked for this to ship with the seeding.

## Goal

When the user asks the agent to review a colleague's pull request, the agent reviews it and then asks, **one comment at a time**, whether to post that comment. Every question offers one or more proposed comment texts, a "do not post" option, and free text. The agent always asks: nothing is posted without a yes for that specific comment.

## Behavior

A new global skill `pr-review-comments` (`crates/ax-policy/templates/skills/pr-review-comments/SKILL.md`, `seedVersion: 1`):

1. **When it applies:** someone else's pull request (GitHub, Azure DevOps, or any other host). A review of the agent's own change keeps the review loop.
2. **Review:** run steps 1 and 2 of `review-loop` (skill check, stack review). **Do not fix anything**: it is the colleague's code. Each finding becomes a draft comment with a location (`file:line`), a severity, and the skill section it violates.
3. **Ask per comment,** in order of severity, with the IDE's question tool (`AskQuestion` in Cursor, `AskUserQuestion` in Claude Code). Where no such tool exists, ask a numbered question in chat and wait for the answer. One question per comment, never a batch approval. Each question shows the location, the finding, and these options:
   - `Post: <proposed text 1>`, and optionally `Post: <proposed text 2>` (for example a shorter or a more explanatory wording);
   - `Do not post`;
   - free text: the user writes their own comment (the tool's "Other" field, or a reply in chat).
4. **Post** only the chosen or written text, as a line comment at that location, through the host's tool (`gh`, the Azure DevOps CLI or MCP, the GitKraken MCP). Never post a comment the user did not choose.
5. **Summary:** at the end, list what was posted, what was skipped, and any comment that failed to post (with the error).

Changes to existing seeded policy (so the pieces agree):

- `review-loop` SKILL.md goes to `seedVersion: 2`. "Also run it whenever the user asks for a code review" becomes: for the user's own change, run the full loop; for a colleague's pull request, load `pr-review-comments` instead, and do not fix findings.
- `old-coder-mandatory` goes to `seedVersion: 3`. Step 4 gets the same exception in one line.
- `pr-review-comments` is stored in `~/.ax/global.db` like `review-loop`, so `ax_skill("pr-review-comments")` works in every project.

## Tests (RED first)

- P1 `pr_review_comments_skill_pins_every_step`: the skill contains its trigger scope, "Do not fix", "one question per comment", `Do not post`, free text, `AskQuestion`, "Never post a comment the user did not choose", and the summary.
- P2 `global_seed_writes_pr_review_comments`: seeded to `~/.ax/global_policy/skills` and `~/.cursor/skills`.
- P3 `review_loop_hands_colleague_prs_to_pr_review_comments`: `review-loop` names `pr-review-comments` and has `seedVersion >= 2`; the rule names it and has `seedVersion >= 3`.
- **Changed assertions (behavior change, declared here):**
  - `review_loop_is_the_global_db_skill` becomes `global_db_skills_are_review_loop_and_pr_review_comments`.
  - The ax-cli test `seeded_skills_are_stored_once_as_company_skills` expects `["review-loop", "pr-review-comments"]`, one row each.

## Must not

- Post anything without a per-comment answer.
- Change the vendored `old-coder` skill.
- Overwrite a seeded file already at the current `seedVersion`.

## Setup plan

- Edit: the new skill template, `review-loop` SKILL.md, `old-coder-mandatory.mdc`, `seed.rs` (bundle plus tests), the ax-cli init test, the policy engine guide (global table and review loop section), and `scripts/review-loop-mutants.py` (add mutants).
- No new dependencies, no commits.
- Gauntlet: tests, mutants, reinstall, `ax install --yes --target cursor`, global.db check on a copy, `ax_skill("pr-review-comments")`, then the review loop.
- Directive capture: the seeded global skill and rule, plus the global.db row, are the durable record. No separate `ax_policy_capture` rule unless the user asks (it would say the same thing twice).
