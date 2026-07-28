---
name: azdo-development
description: Full-stack development practices for AZDO teams — craftsmanship, branching, and migrations.
triggers: ["implement", "feature branch", "migration", "Entity Framework", "API", "fullstack", "SOLID"]
tags: ["azdo", "development", "shared"]
priority: 62
enabled: true
status: approved
scope: project
share: true
---
# AZDO Development

## Practices

1. **Craftsmanship** — SOLID/DRY, safe REST/GraphQL APIs, efficient frontend state.
2. **Branching** — short-lived feature branches from trunk/main (or team GitFlow). Name: `feature/<id>-slug`.
3. **Migrations** — automated DB migrations only (EF, Flyway, Liquibase, etc.); never manual prod SQL as the primary path.

## Hard rules

- Follow `azdo-traceability` for branch and commit IDs
- Follow `azdo-shift-left-security` — no secrets in code
- Meet `azdo-dod-code` before opening a PR
