---
name: azdo-pipelines
description: Azure Pipelines YAML, multi-stage CI/CD, and infrastructure-as-code basics.
triggers: ["pipeline", "YAML", "CI/CD", "deploy", "Bicep", "Terraform", "ARM", "Azure Pipelines"]
tags: ["azdo", "cicd", "shared"]
priority: 65
enabled: true
status: approved
scope: project
share: true
---
# AZDO Pipelines

## Practices

1. Multi-stage YAML: build → test → deploy (Dev/Staging/Prod)
2. **Build once, deploy many** — one artifact promoted across environments
3. Staging close to production (environment isolation)
4. IaC for Azure resources (Bicep / Terraform / ARM) when infrastructure changes

## Gates

- Staging: auto after merge to main
- Prod: manual approval on AZDO Environment (`azdo-prod-approval-gate`)

Optional ax CI snippet: `docs/examples/azure-pipelines-ship.yml`.
