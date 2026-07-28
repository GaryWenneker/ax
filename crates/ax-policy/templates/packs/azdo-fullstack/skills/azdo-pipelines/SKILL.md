---
name: azdo-pipelines
description: Azure Pipelines YAML — multi-stage CI/CD, build-once deploy-many, environments, approvals, secrets, and infrastructure-as-code basics.
triggers: ["pipeline", "YAML", "CI/CD", "deploy", "Bicep", "Terraform", "ARM", "Azure Pipelines", "environment", "artifact"]
tags: ["azdo", "cicd", "shared"]
priority: 65
enabled: true
status: approved
scope: project
share: true
---
# AZDO Pipelines

Use when authoring or changing Azure Pipelines YAML, deployment stages, environments, or infrastructure-as-code that feeds those pipelines.

## When to load

- Adding/editing `azure-pipelines*.yml` or templates
- Wiring build, test, or deploy stages
- Introducing environments, approvals, or service connections
- Changing how artifacts move across Dev / Staging / Prod

## Core model: build once, deploy many

Rule `azdo-build-once-deploy-many`:

1. **Build** produces a versioned artifact (package, image, zip) once
2. **Test** runs against that artifact (or its build outputs)
3. **Deploy** promotes the **same** artifact to Dev → Staging → Prod
4. Never rebuild from source for Prod with different flags than Staging

If Staging and Prod diverge in how the binary is produced, you no longer have a trustworthy promotion path.

## Recommended multi-stage shape

```text
Stage: Build
  - restore / compile
  - unit + fast integration tests
  - publish artifact / push image (immutable tag)

Stage: Deploy_Dev (optional)
  - deploy artifact
  - smoke

Stage: Deploy_Staging
  - deploy same artifact
  - smoke / integration
  - auto after merge to main (typical)

Stage: Deploy_Prod
  - Azure Environment with manual approval (azdo-prod-approval-gate)
  - deploy same artifact
  - smoke (azdo-release)
```

### PR vs CI

| Pipeline | Purpose |
|---|---|
| PR validation | Build + tests that must be green before merge |
| CI on main | Build artifact + deploy Staging (and maybe Dev) |
| Release / Prod stage | Approval-gated promotion of the main artifact |

Keep PR pipelines fast. Move long E2E/nightly suites out of the critical PR path when the team agreed.

## YAML practices

1. Prefer template files for repeated jobs; keep root pipeline readable
2. Pin task major versions; avoid floating `@latest` in production pipelines
3. Use variable groups / Key Vault / secret variables — never commit secrets (`azdo-shift-left-security`)
4. Parameterize environment names, resource IDs, and connection names
5. Set `trigger` / `pr` paths thoughtfully to avoid useless runs
6. Fail fast: compile and unit tests before expensive deploys
7. Publish test results and code coverage so AzDO shows them on the PR

## Environments and approvals

- Staging: typically automatic after successful main CI
- Prod: **manual approval** on an Azure DevOps Environment (`azdo-prod-approval-gate`)
- Restrict who can approve Prod; do not use a personal PAT as the only control
- Use checks (branch control, business hours) when the org requires them

## Infrastructure as code

When infrastructure changes with the feature:

- Prefer Bicep / Terraform / ARM in-repo with the app change or a linked IaC repo
- Plan/apply in pipeline with clear review of the plan output
- Do not click-ops Prod as the primary path
- Keep state and backend config out of git secrets

## Quality gate (optional ax)

Adapt `docs/examples/azure-pipelines-ship.yml` to run `ax ship --ci` as a PR or main check when the repo uses ax for index/TIA/policy gates.

## Agent workflow

```text
1. Identify whether change is PR validation, artifact build, or deploy
2. Preserve build-once artifact identity across stages
3. Wire secrets via Variable Group / Key Vault only
4. Ensure Prod has Environment approval
5. Document how to re-run / roll forward in the PR
```

## Checklist

- [ ] Artifact built once; deploys reuse it
- [ ] PR pipeline covers compile + required tests
- [ ] Staging auto path understood
- [ ] Prod approval gate present
- [ ] No secrets in YAML or logs
- [ ] IaC changes reviewed with the app change when relevant

## Related

| Resource | Role |
|---|---|
| `azdo-testing` | What CI must run |
| `azdo-release` | Post-deploy smoke and rollback |
| Rules `azdo-build-once-deploy-many`, `azdo-prod-approval-gate`, `azdo-pr-policies` | Gates |
| `docs/examples/azure-pipelines-ship.yml` | Optional ax ship snippet |
