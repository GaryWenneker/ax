---
name: azdo-refinement
description: Azure DevOps refinement — vertical slices, Definition of Ready, work-item hierarchy, and right-sizing. Use when preparing tickets, splitting stories, or starting a story.
triggers: ["refinement", "user story", "ticket", "DoR", "Definition of Ready", "Epic", "Feature", "story points", "split story", "backlog", "grooming"]
tags: ["azdo", "refinement", "shared"]
priority: 68
enabled: true
status: approved
scope: project
share: true
---
# AZDO Refinement

Complements `design-first` with Azure DevOps board practice. Use this skill before coding when a ticket is vague, oversized, or not yet Ready.

## When to load

- Preparing or refining a User Story / Bug / Task
- Splitting work that will not fit a sprint
- Moving a story toward **In Progress**
- Asking whether acceptance criteria are good enough

## Work-item hierarchy

Keep the board navigable and reporting accurate:

| Level | Purpose | Agent action |
|---|---|---|
| Epic | Multi-sprint outcome | Confirm parent exists; do not invent orphan stories |
| Feature | Shipable capability under an Epic | Link stories here |
| User Story | Vertical slice with clear AC | Primary unit of delivery |
| Task / Bug | Implementation or defect under a Story | Create only when the Story is Ready |

Every Story must have a parent Feature or Epic before **In Progress** (rule `azdo-no-spec-no-start`).

## Vertical slices (not layers)

Break work into deliverable slices that each include what is needed to demo value:

1. **Slice** = thin end-to-end path (UI + API + persistence + tests) for one acceptance path
2. Prefer several small Stories over one horizontal "backend then frontend" Feature
3. Each Story should be independently testable and mergeable
4. Shared foundation work belongs in an earlier Story with its own AC — not a vague "tech spike" without outcome

### Split signals (do this immediately)

- Estimate exceeds ~3–5 days mid-sprint
- Multiple unrelated acceptance criteria
- Blocked on external dependency that others can proceed without
- Mix of research and delivery — separate spike (time-boxed) from delivery Story

## Definition of Ready (DoR)

A Story may move to **In Progress** only when all are true:

1. Clear functional description (who / what / why)
2. Explicit acceptance criteria (testable, not slogans)
3. UX/UI designs or wireframes linked when UI is in scope
4. Linked to parent Feature or Epic
5. Dependencies called out (API contracts, feature flags, data migrations)
6. Rough estimate recorded (points or hours per team convention)

If any item fails → refine first. Do not start a branch for incomplete DoR.

### Acceptance criteria quality

Good AC are binary and demoable:

- Given / When / Then or checklist form
- Cover happy path **and** one meaningful edge or error path
- Avoid "works well" / "user friendly" without observables
- Note non-functional needs when they matter (auth, perf budget, a11y)

## Agent workflow

```text
1. Ask for the Azure DevOps work item ID (or URL)
2. Summarize title, type, parent, state, AC
3. Score DoR — list gaps explicitly
4. Propose splits or AC rewrites if needed
5. Only after DoR passes: proceed to azdo-development (branch naming)
```

### Questions to ask the user

- What is the work item ID?
- Who is the primary user / persona?
- What must be true to call this Story done in a demo?
- Any UI mock / API contract / environment dependency?
- Target sprint and estimate?

## Checklist before coding

- [ ] DoR met (`azdo-no-spec-no-start`)
- [ ] Story linked to Feature/Epic
- [ ] Acceptance criteria clear and testable
- [ ] Estimate recorded
- [ ] Work item ID known for branch/commits (`azdo-traceability`)
- [ ] Out-of-scope items parked on backlog or separate Stories

## Related

| Resource | Role |
|---|---|
| `design-first` | Product/UX design before large builds |
| `azdo-development` | Branching and implementation after Ready |
| `azdo-testing` | How AC become tests |
| Rule `azdo-no-spec-no-start` | Hard gate for In Progress |
| Rule `azdo-traceability` | ID in branch and commits |
