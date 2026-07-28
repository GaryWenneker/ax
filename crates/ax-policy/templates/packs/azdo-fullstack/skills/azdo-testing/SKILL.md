---
name: azdo-testing
description: Testing for AZDO full-stack work — unit, integration, E2E, and pipeline awareness. Prefer the tdd skill for red-green-refactor.
triggers: ["test", "unit test", "integration test", "E2E", "coverage", "Test Plans", "regression"]
tags: ["azdo", "testing", "shared"]
priority: 64
enabled: true
status: approved
scope: project
share: true
---
# AZDO Testing

Use the built-in `tdd` skill for the red → green → refactor cycle.

## Additional expectations

1. Unit tests for new logic (backend and frontend)
2. Integration tests at service boundaries when contracts change
3. E2E for critical user journeys when UI is in scope
4. Explicit edge-case and validation coverage — not only happy path
5. Understand how tests run in Azure Pipelines; keep the PR build green

See rules `azdo-tests-required` and `azdo-pr-policies`.
