# SPEC — Group multiselect filter + collapse/expand all

- Tier: 2
- spec approval: not obtained (autonomous run — user asked for a multiselect group filter and collapse all / expand all on Rules and Skills groups)
- Isolation: none (landing tree)
- New dependencies: none

## Goal

On **Policy → Rules** and **Policy → Skills**, operators can:

1. **Multiselect-filter** by catalog group (show only selected groups).
2. **Collapse all** / **Expand all** group folders.

## Scenarios

### F1 — Empty selection shows every group

Given search/other filters already applied
When no group ids are selected
Then every remaining item is shown, grouped as today

### F2 — Selecting groups is OR within groups

Given items in groups `testing` and `git`
When the operator selects only `testing`
Then `git` rows and the `git` folder are hidden
And `testing` items that still match search/level/tags remain

### F3 — Multiselect

When `testing` and `git` are both selected
Then both folders (and their matching items) are shown
And other groups are hidden

### F4 — Collapse all

When Collapse all is clicked
Then every **currently listed** group folder is collapsed (ids stored in existing `*-groups-collapsed` localStorage)
And item rows under those headers are hidden until expanded

### F5 — Expand all

When Expand all is clicked
Then the collapsed set is empty (all listed folders expanded)
And localStorage is updated

## Invariants

- Catalog, YAML `group:`, SQLite, and empty-group hiding stay unchanged
- Search / level / layer / tag filters still apply **before** the group filter
- Command Center **Ship** page is untouched
- No new npm/cargo packages

## Setup

- Files: `skillGroupFilter.ts` + node test, `PolicyGroupListControls.tsx`, PolicyRules/Skills pages, CSS, this spec, gauntlet extension, site/README docs as needed
- Git: no commits unless asked
