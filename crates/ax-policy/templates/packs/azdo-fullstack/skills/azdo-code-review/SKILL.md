---
name: azdo-code-review
description: Constructive Azure DevOps pull request review and git hygiene — author checklist, reviewer checklist (story scope, sibling patterns, both bounds, named tests), merge rules, and comment resolution.
triggers: ["code review", "review PR", "pull request", "approve", "squash", "git hygiene", "PR comments", "merge", "beoordelen"]
tags: ["azdo", "pr", "shared"]
priority: 66
enabled: true
status: approved
scope: project
share: true
---
# AZDO Code Review

Use when creating, reviewing, or merging Azure DevOps pull requests. Keep delivery safe without blocking on taste alone.

## When to load

- Opening a PR for a Story/Bug
- Acting as reviewer on a teammate's PR
- Deciding approve vs request-changes
- Cleaning history before merge (squash / rebase)

## As author

### Before you open

1. PR matches **one** Story or tightly related fix (`azdo-pr-small-scope`)
2. Title explains the why; description links AC / work item
3. Work item linked in Azure DevOps (required by `azdo-pr-policies`)
4. CI green on the latest commit
5. Self-review the diff as if you were the reviewer (run the reviewer checklist below on your own diff)
6. Screenshots or sample requests for UI/API changes when helpful

### Description template

```markdown
## Work item
#<id> — <title>

## Summary
<1–3 bullets: what and why>

## How to test
- [ ] <AC-derived check>
- [ ] <edge case>

## Notes
Risks / feature flags / follow-ups
```

Work item references: number only (`17173`) or full AzDO URL. Never `AB#`.

### Git hygiene

- Prefer a clean history; squash when the team expects one commit per Story
- Prefix commits with the work item number (`17173 Clamp BG search page size`)
- Avoid "fix comments" noise commits that bury the real change — amend or squash before merge if policy allows
- Do not force-push shared long-lived branches; force-push your feature branch only when the team allows rebase workflows
- Resolve **all** review threads before merge (`azdo-pr-policies`)

## As reviewer

Review the diff against the **user story**, not against “nice architecture.” Leave a short, **line-anchored** comment for every finding. A summary-only review is incomplete.

Use `ax_explore` / graph tools to find sibling implementations before commenting on a new helper, cache, or flag.

### Priority order

1. **Story scope** — does this meet AC, and only AC?
2. **Correctness** — regressions; **both ends** of every bound
3. **Security** — secrets, authz, injection, unsafe defaults (`azdo-shift-left-security`)
4. **Data & migrations** — irreversible changes called out?
5. **Sibling pattern** — same job already done in this repo?
6. **Performance** — only when the change is on a hot path or AC requires it
7. **Style** — follow existing project conventions; do not bikeshed

### Story scope

- Extra cache, extra endpoint, extra UI: either drop it or constrain it (TTL, flag, test) and say it is out of story.
- Do not accept “while we were here” without an explicit call-out.

### Both bounds

- A max without a min is incomplete (`pageSize > Max` but `pageSize == 0` still forwarded).
- Check `0`, empty, negative, overflow, too-short input.
- After clamp/reject, related flags (`pageSizeSpecified`, optional SOAP fields) must stay consistent.

### Sibling pattern

- Search for the same job already done (cache, paging, mapping, `RefreshCache`).
- Duplicated private helper → shared helper, or say why not.
- If a sibling honours a flag (`AccessControl.RefreshCache`, cancellation, access control), this change must too. Name the sibling in the comment.

### Test for the gap

- Every bound or flag you mention needs a **named unit test** for that case (`azdo-testing`).
- “Tests exist” is not enough. “Add a test with pageSize 0” is the bar.
- Do not ask for a test you would not write yourself.

### Language

- New comments, test names, and review text: **English**. Flag Dutch comments in test or production code.

### Config defaults

- TTLs and limits must match the story and similar settings. Hour-long cache for typeahead is a finding if similar data is 5 minutes or the story never asked for cache.

### How to comment

- One or two sentences. File + line + behavior + suggested fix.
- Prefer questions for unclear intent; prefer blocking comments for security/correctness/scope.
- Distinguish **blocking** vs **suggestion** explicitly.
- Group related nits on the same line into one thread.
- Approve when residual nits are non-blocking and tracked.

### Comment shapes

| Finding | Comment |
|---|---|
| Upper clamp only | Bound only covers the top. `pageSize` 0 still goes to the backend. Add a unit test with pageSize 0. |
| Extra cache | Cache was not in the story. If we keep it, cap TTL (now 60 minutes). |
| Copied helper | Same as `<SiblingType>`. Shared helper if we keep this. |
| Flag ignored | Cache ignores `accessControl.RefreshCache`. `<SiblingMethod>` does not. |
| Dutch in tests | No Dutch comments. |
| Commits | Prefix commits with the work item number. |

### Blocking reasons (must not approve)

- Secrets or credentials in the diff
- Missing tests for new logic (`azdo-tests-required`), including the specific bound/flag you found
- CI red or required checks skipped without waiver
- Work item not linked
- Scope creep that should be another Story
- Unresolved critical threads
- Bound only on one side; sibling flag (`RefreshCache`, cancellation) ignored

## Merge checklist

- [ ] Required reviewers approved
- [ ] All conversations resolved
- [ ] Build validation green
- [ ] Work item linked; state update planned (Active → Resolved/Closed per team process)
- [ ] Squash/merge option matches repo policy
- [ ] Post-merge: staging deploy expected (`azdo-pipelines`)

## Agent workflow

```text
Author path:
  1. Confirm DoD (azdo-development / azdo-dod-code)
  2. Push branch; open PR with template
  3. Watch pipeline; fix failures before asking for review

Reviewer path:
  1. Read work item + AC (scope first)
  2. Diff; ax_explore for sibling patterns
  3. Both bounds + named tests + flags
  4. Line-anchored comments; blocking vs suggestion
  5. Approve only when blocking items are done
```

## Related

| Resource | Role |
|---|---|
| `azdo-development` | Author DoD before PR |
| `azdo-testing` | Tests reviewers expect (named edge cases) |
| Rules `azdo-pr-small-scope`, `azdo-pr-policies` | Size and merge gates |
| `azdo-pipelines` | What CI must stay green |
