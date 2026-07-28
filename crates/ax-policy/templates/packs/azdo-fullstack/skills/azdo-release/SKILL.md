---
name: azdo-release
description: Production release for Azure DevOps teams — smoke tests, observability, feature flags, zero-downtime, rollback, and closing the work item only after verification.
triggers: ["release", "production", "smoke test", "Application Insights", "feature flag", "rollback", "App Configuration", "deploy prod", "blue green"]
tags: ["azdo", "release", "shared"]
priority: 67
enabled: true
status: approved
scope: project
share: true
---
# AZDO Release & Observability

Use when promoting to production, verifying a deploy, planning rollback, or closing a work item after release. For production incidents after go-live, also load `systematic-debugging`.

## When to load

- Prod deploy is about to run or just finished
- Adding smoke checks or health endpoints
- Introducing feature flags / App Configuration
- Deciding whether a Story can move to **Done**
- Preparing a rollback or slot swap

## Preconditions

1. Same artifact that passed Staging (`azdo-build-once-deploy-many`)
2. Prod Environment approval completed (`azdo-prod-approval-gate`)
3. Known smoke plan and rollback owner
4. Work item ID tracked for the release notes / deployment record

## Release sequence

```text
1. Confirm Staging smoke green on the artifact
2. Obtain Prod approval (Environment check)
3. Deploy artifact (slots / blue-green when available)
4. Run smoke checklist (below)
5. Watch dashboards / alerts for the burn-in window
6. Enable or expand feature flag if used
7. Move work item to Done only after verification
8. If smoke fails → rollback (do not "fix forward" blindly)
```

## Smoke test (minimum)

Customize per service; do not mark Done without an equivalent:

- [ ] Health / readiness endpoint returns success
- [ ] Primary user journey for the Story succeeds in Prod
- [ ] Auth-sensitive path still enforces access control
- [ ] No error storm in Application Insights / Log Analytics
- [ ] Dependency checks (DB, queue, external API) healthy
- [ ] Feature flag state matches the release plan

Automate smoke in the Prod stage when practical; keep a manual fallback list in the work item or release notes.

See rule `azdo-release-verification`.

## Observability

1. **Application Insights / Log Analytics** — dashboards and alerts for new failure modes
2. Correlate deploy time with exception rate, latency, and dependency failures
3. Ensure new code paths emit useful logs (no secrets) and key metrics
4. Know who gets paged; do not deploy when on-call coverage is unclear

## Feature flags

Prefer flags (e.g. Azure App Configuration) for risky or gradual rollouts:

- Ship dark → validate → enable for internal → percentage → all
- Flag name and default documented in the PR / work item
- Removal of stale flags tracked as follow-up work
- Never use flags as a substitute for fixing a broken Prod deploy — rollback the artifact if smoke fails

## Zero-downtime and rollback

| Strategy | When |
|---|---|
| Deployment slots / blue-green | App Service / similar — swap only after smoke on staging slot |
| Rolling update | Kubernetes / scale sets — watch pod health |
| Backward-compatible migrations | DB changes that must not break the previous binary |
| Hard rollback | Redeploy previous artifact; disable flag; communicate status |

Always answer before deploying: **How do we undo this in under N minutes?**

### Rollback triggers

- Smoke failure
- Error rate or latency breach vs baseline
- Data corruption risk
- Security regression

## Closing the work item

Move to **Done** only when:

- [ ] Prod smoke passed (`azdo-release-verification`)
- [ ] No unresolved Sev-1/Sev-2 from this deploy
- [ ] Work item updated with deploy evidence (link/run ID)
- [ ] Follow-ups (flag cleanup, docs) filed if needed

## Agent workflow

```text
1. Confirm artifact ID/version from Staging
2. Restate smoke + rollback plan to the user
3. After deploy: run/list smoke checks
4. Inspect telemetry for the burn-in window
5. Recommend Done vs rollback with evidence
```

## Related

| Resource | Role |
|---|---|
| `systematic-debugging` | Incidents after release |
| `azdo-pipelines` | Approval and promotion path |
| Rules `azdo-release-verification`, `azdo-prod-approval-gate`, `azdo-build-once-deploy-many` | Gates |
