---
name: azdo-refinement
description: Azure DevOps refinement — vertical slices, DoR, and work-item hierarchy. Use when preparing tickets or starting a story.
triggers: ["refinement", "user story", "ticket", "DoR", "Definition of Ready", "Epic", "Feature", "story points", "split story"]
tags: ["azdo", "refinement", "shared"]
priority: 68
enabled: true
status: approved
scope: project
share: true
---
# AZDO Refinement

Complements `design-first` with Azure DevOps board practice.

## Skills to apply

1. **Vertical slices** — break work into deliverable frontend + backend + tests slices (not horizontal layers).
2. **Board mastery** — Epics → Features → User Stories → Tasks/Bugs with correct parent links.
3. **Right-sizing** — if a story exceeds ~3–5 days mid-sprint, split it immediately.

## Checklist before coding

- [ ] DoR met (see rule `azdo-no-spec-no-start`)
- [ ] Story linked to Feature/Epic
- [ ] Acceptance criteria clear
- [ ] Estimate recorded (points or hours)

Ask the user for the work item ID before creating a branch.
