---
name: azdo-development
description: Full-stack Azure DevOps development — craftsmanship, branching, commits, APIs, migrations, and Definition of Done before PR. Use when implementing a Ready story.
triggers: ["implement", "feature branch", "migration", "Entity Framework", "API", "fullstack", "SOLID", "coding", "start coding"]
tags: ["azdo", "development", "shared"]
priority: 62
enabled: true
status: approved
scope: project
share: true
---
# AZDO Development

Use after refinement: the Story meets Definition of Ready and you have a work item ID. Pair with `tdd` / `azdo-testing` while coding and `azdo-code-review` before merge.

## When to load

- Implementing a User Story or Bug
- Creating or naming a feature branch
- Adding DB migrations or API contracts
- Checking whether code is ready for PR (DoD)

## Preconditions

1. Work item ID known (ask if missing)
2. DoR satisfied — otherwise switch to `azdo-refinement`
3. Understand the vertical slice and AC you are delivering this PR

## Branching

Prefer short-lived branches from trunk/`main` (or the team's documented GitFlow).

| Kind | Pattern | Example |
|---|---|---|
| Feature | `feature/<id>-<slug>` | `feature/16295-export-csv` |
| Bugfix | `bugfix/<id>-<slug>` | `bugfix/16302-null-invoice` |
| Hotfix | `hotfix/<id>-<slug>` | `hotfix/16310-login-500` |

Rules:

- ID is the Azure DevOps work item number (no `AB#` prefix unless the team tool requires it for linking)
- Slug is short kebab-case from the Story title
- Rebase or merge main often; keep PRs small (`azdo-pr-small-scope`)
- Delete the branch after merge

## Commits

Follow `azdo-traceability`:

- Include the work item ID in every commit message (team convention, e.g. `#16295` or `16295: …`)
- Prefer small, purposeful commits over end-of-day dumps
- Do not commit secrets, credentials, or personal access tokens (`azdo-shift-left-security`)
- Do not commit generated noise unless the repo already tracks it

## Craftsmanship

1. **SOLID / DRY** — change the smallest clear design; avoid drive-by refactors outside the Story
2. **APIs** — stable contracts, explicit validation, consistent error shapes; version or feature-flag breaking changes
3. **Frontend** — predictable state, loading/error paths, no silent failures
4. **Boundaries** — keep domain logic out of controllers/UI glue when the codebase already separates them
5. **Observability** — log/metric hooks for new failure modes when production will need them

### Migrations

- Prefer automated migrations only (EF Core, Flyway, Liquibase, etc.)
- Never treat ad-hoc production SQL as the primary delivery path
- Migrations must be reviewable, forward-only when possible, and include rollback notes when destructive
- Test migrations against a realistic schema in CI or a local reset script
- Coordinate data backfills as separate, monitored steps when large

## Definition of Done — code (before opening PR)

Meet rule `azdo-dod-code`:

- [ ] Formatter / linter clean for touched files
- [ ] No new compiler warnings introduced
- [ ] No new critical static-analysis findings (e.g. SonarQube)
- [ ] New logic covered by unit tests (happy path + edge cases)
- [ ] AC for this slice are demonstrably met
- [ ] No secrets or credentials in the diff
- [ ] Work item linked; branch/commits carry the ID

Then open a focused PR (`azdo-code-review`, `azdo-pr-policies`).

## Agent workflow

```text
1. Confirm work item ID + AC
2. Create/checkout feature/<id>-slug from up-to-date main
3. Implement with tdd / azdo-testing
4. Run local lint/tests that CI will run
5. Self-check DoD + security
6. Push and open PR linked to the work item
```

## Hard rules

- Follow `azdo-traceability` for branch and commit IDs
- Follow `azdo-shift-left-security` — no secrets in code or pipeline logs
- Meet `azdo-dod-code` before opening a PR
- Do not expand scope beyond the Story AC without updating the work item

## Related

| Resource | Role |
|---|---|
| `azdo-refinement` | DoR and slicing before code |
| `tdd` / `azdo-testing` | Test-first and pipeline expectations |
| `azdo-code-review` | PR author/reviewer checklist |
| Rules `azdo-traceability`, `azdo-dod-code`, `azdo-shift-left-security` | Gates |
