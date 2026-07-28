---
name: azdo-code-review
description: Constructive Azure DevOps pull request review and git hygiene — author checklist, reviewer checklist, merge rules, and comment resolution.
triggers: ["code review", "review PR", "pull request", "approve", "squash", "git hygiene", "PR comments", "merge"]
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
5. Self-review the diff as if you were the reviewer
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

### Git hygiene

- Prefer a clean history; squash when the team expects one commit per Story
- Avoid "fix comments" noise commits that bury the real change — amend or squash before merge if policy allows
- Do not force-push shared long-lived branches; force-push your feature branch only when the team allows rebase workflows
- Resolve **all** review threads before merge (`azdo-pr-policies`)

## As reviewer

### Priority order

1. **Correctness** — does this meet AC? regressions?
2. **Security** — secrets, authz, injection, unsafe defaults (`azdo-shift-left-security`)
3. **Data & migrations** — irreversible changes called out?
4. **Design** — boundaries, naming, duplication that will hurt the next Story
5. **Performance** — only when the change is on a hot path or AC requires it
6. **Style** — follow existing project conventions; do not bikeshed

### How to comment

- Be specific: file + behavior + suggested fix
- Prefer questions for unclear intent; prefer blocking comments for security/correctness
- Distinguish **blocking** vs **nit** explicitly
- Approve when residual nits are non-blocking and tracked

### Blocking reasons (must not approve)

- Secrets or credentials in the diff
- Missing tests for new logic (`azdo-tests-required`)
- CI red or required checks skipped without waiver
- Work item not linked
- Scope creep that should be another Story
- Unresolved critical threads

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
  1. Read work item + AC
  2. Skim diff for security/correctness first
  3. Leave prioritized comments
  4. Approve only when blocking items are done
```

## Related

| Resource | Role |
|---|---|
| `azdo-development` | Author DoD before PR |
| `azdo-testing` | Tests reviewers expect |
| Rules `azdo-pr-small-scope`, `azdo-pr-policies` | Size and merge gates |
| `azdo-pipelines` | What CI must stay green |
