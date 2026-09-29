---
name: design-first
description: Design before code. Use when building new features or components — clarify requirements before implementation.
triggers: ["build", "create", "new feature", "design", "architect", "plan", "implement from scratch"]
tags: ["design", "methodology"]
priority: 65
---
# Design Before Code

Do not jump into implementation. Clarify what you are building first.

## Process

1. **Understand context** — check existing code (`ax_explore`), docs, recent commits. Follow existing patterns.
2. **Ask one question at a time** — purpose, constraints, success criteria. Prefer multiple-choice.
3. **Propose 2-3 approaches** — with trade-offs and your recommendation. Lead with the recommended option.
4. **Present design** — scaled to complexity. A config change needs two sentences; a new subsystem needs architecture, data flow, error handling.
5. **Get approval** — do not write code until the user confirms the approach.

## Scope check

If the request spans multiple independent subsystems, flag it. Help decompose into sub-projects first. Each gets its own design-approve-implement cycle.

## Principles

- **YAGNI** — remove unnecessary features from all designs.
- **Isolation** — each unit has one purpose, clear interfaces, testable independently.
- **Existing codebases** — explore structure before proposing changes. Follow established patterns. Only improve code you are actively modifying.
