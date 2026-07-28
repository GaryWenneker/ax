---
name: azdo-code-review
description: Constructive PR review and git hygiene for Azure DevOps pull requests.
triggers: ["code review", "review PR", "pull request", "approve", "squash", "git hygiene"]
tags: ["azdo", "pr", "shared"]
priority: 66
enabled: true
status: approved
scope: project
share: true
---
# AZDO Code Review

## As author

1. Keep PRs small (`azdo-pr-small-scope`)
2. Clean commit history; squash when the team expects it
3. Link the work item; ensure CI is green
4. Resolve all review comments before merge

## As reviewer

Review for architecture, readability, performance, and security. Be specific and constructive. Block on security or correctness; prefer suggestions for style.
