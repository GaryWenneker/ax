---
name: azdo-testing
description: Azure DevOps testing expectations — unit, integration, E2E, edge cases, and keeping Azure Pipelines green. Prefer the tdd skill for red-green-refactor.
triggers: ["test", "unit test", "integration test", "E2E", "coverage", "Test Plans", "regression", "pipeline tests", "flaky"]
tags: ["azdo", "testing", "shared"]
priority: 64
enabled: true
status: approved
scope: project
share: true
---
# AZDO Testing

Use the built-in `tdd` skill for the red → green → refactor cycle. This skill adds Azure DevOps / full-stack delivery expectations so tests prove the Story and keep CI green.

## When to load

- Implementing or changing logic under a Story
- Adding or fixing CI test jobs
- Investigating flaky pipeline failures
- Mapping acceptance criteria to test cases

## Pyramid (what to write)

| Layer | When required | Focus |
|---|---|---|
| Unit | New/changed logic (backend and frontend) | Behavior, edge cases, pure rules |
| Integration | Contract or persistence boundaries change | Real DB/API/testcontainers where practical |
| E2E | Critical user journey in UI scope | Happy path + one failure path for that journey |
| Manual / Test Plans | Compliance or exploratory needs | Link plans to the work item when the team uses them |

Prefer fast, deterministic unit tests. Escalate up the pyramid only when a lower layer cannot catch the risk.

## Map acceptance criteria → tests

For each AC item:

1. Write at least one automated test that would fail if the AC is unmet
2. Add an edge/validation case (empty input, auth failure, concurrency, timezone, etc.)
3. Name tests after the behavior, not the implementation
4. Keep fixtures minimal — huge setup usually means the API under test is too wide

### Minimum bar for a Story with logic

- [ ] Happy-path unit coverage for new branches
- [ ] At least one edge or negative case per non-trivial validation
- [ ] Integration test when a public contract or schema changes
- [ ] E2E smoke for the primary UI path when UI is in scope
- [ ] Tests run the same way locally and in Azure Pipelines

See rules `azdo-tests-required` and `azdo-pr-policies`.

## Pipeline awareness

1. Know which pipeline job runs which suite (PR vs main vs nightly)
2. Keep PR builds fast — reserve long E2E for post-merge or nightly when the team agreed
3. Flaky tests are defects: quarantine with a linked Bug, then fix or delete — do not ignore
4. Failures on main after merge are stop-the-line; fix or revert before new feature work
5. Optional quality gate: adapt `docs/examples/azure-pipelines-ship.yml` for `ax ship --ci`

## Working with `tdd`

```text
For each behavior in the AC:
  RED   — failing test that expresses the AC
  GREEN — minimal production code
  REFACTOR — clean names/duplication; stay green
Then run the suite CI will run before opening the PR.
```

Do not "test after" for new logic. Tests written only after the fact often pass immediately and prove nothing (`tdd` hard rules).

## Anti-patterns

| Anti-pattern | Do instead |
|---|---|
| Only happy-path tests | Add edge/error cases from AC |
| Snapshot-everything UI | Assert user-visible outcomes |
| Sleeping for async | Wait on conditions / fake clocks |
| Hitting real prod APIs in unit tests | Fake/stub or dedicated test env |
| Skipping tests to go green | Fix product or test; never disable without a Bug |

## Agent workflow

```text
1. List AC → intended test cases
2. Load tdd; implement RED/GREEN/REFACTOR per case
3. Run the PR pipeline suite locally (or closest equivalent)
4. Confirm azdo-tests-required / azdo-dod-code
5. Note residual manual checks in the PR description
```

## Related

| Resource | Role |
|---|---|
| `tdd` | Red-green-refactor discipline |
| `azdo-development` | DoD before PR |
| `azdo-pipelines` | Where tests run in YAML |
| Rules `azdo-tests-required`, `azdo-pr-policies`, `azdo-dod-code` | Gates |
