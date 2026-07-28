---
name: azdo-release
description: Production release verification, observability, feature flags, and rollback for AZDO teams.
triggers: ["release", "production", "smoke test", "Application Insights", "feature flag", "rollback", "App Configuration"]
tags: ["azdo", "release", "shared"]
priority: 67
enabled: true
status: approved
scope: project
share: true
---
# AZDO Release & Observability

For incidents, also use `systematic-debugging`.

## Practices

1. Monitor with Application Insights / Log Analytics
2. Prefer feature flags (e.g. Azure App Configuration) for safe rollouts
3. Smoke-test after every prod deploy
4. Move the work item to Done only after verification
5. Plan zero-downtime (slots / blue-green) and a rollback path

See rule `azdo-release-verification`.
