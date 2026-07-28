# AZDO Full-Stack Developer Pack

Installable project-scope policy pack for Azure DevOps teams covering ticket refinement through production release.

## Install

```bash
ax policy pack install azdo-fullstack
```

Use `--force` to overwrite existing items with the same id/name.

## Contents

### Skills

Each skill is a full agent workflow (when to load, checklists, hard rules, related rules) — not a one-line summary.

| Skill | When |
|---|---|
| `azdo-refinement` | Story breakdown, DoR, hierarchy, vertical slices, right-sizing |
| `azdo-development` | Branching, commits, craftsmanship, migrations, code DoD |
| `azdo-testing` | Unit/integration/E2E, AC→tests, pipeline green (uses `tdd`) |
| `azdo-code-review` | Author/reviewer checklists, merge gates, git hygiene |
| `azdo-pipelines` | Multi-stage YAML, build-once, environments, IaC, secrets |
| `azdo-release` | Smoke, observability, flags, rollback, closing the work item |

### Rules

Traceability, security, Definition of Done, PR policies, CI/CD gates, and release verification.

## Relationship to built-in skills

- Prefer `design-first` when designing new features; this pack adds AZDO DoR / vertical-slice practice.
- Prefer `tdd` for red-green-refactor; `azdo-testing` adds pipeline and coverage expectations.
- Prefer `systematic-debugging` for production incidents; `azdo-release` covers smoke/rollback.

## Definition of Done checklist

- [ ] Story meets Definition of Ready before In Progress
- [ ] Work item ID in branch name and commits
- [ ] No secrets in git
- [ ] Lint/format clean; no new critical static-analysis issues
- [ ] Unit tests for new logic (happy path + edge cases)
- [ ] Pipeline green on the PR
- [ ] PR small/focused; comments resolved; work item linked
- [ ] Staging auto-deploy after merge; prod needs approval gate
- [ ] Smoke test after prod deploy; work item → Done

## CI example

See `docs/examples/azure-pipelines-ship.yml` for an ax ship CI snippet you can adapt.
