# SPEC: Rules and Skills tables are one-liners

**Tier:** 2 (UI density)
**Spec approval:** not obtained (autonomous; user asked for compact one-liners)

## Behaviors

### B1 — Tags do not wrap (revised 2026-09-01)

Table rows stay one line via CSS `flex-wrap: nowrap`. **Every tag still renders** — do not slice to two pills or `+N`. A rule with tags `[mcp, preflight, cursor]` shows three badges.

### B1-old (superseded)

`TagList compact` / `compactTagItems` hid tags after refresh; that was a defect.

### B2 — CSS forbids wrap in the tags cell

`.policy-table-tags .policy-view-tags` uses `flex-wrap: nowrap` (not `wrap`).

### B3 — Row padding is compact

`.page-table--dense td` padding-top is `2px` (not `5px` or more).

### B4 — Meta TagList unchanged

Detail pane `TagList` still lists every tag (wrap allowed outside the table).

## Invariants

- Filter click on a visible tag still works.
- Git-share dot, enabled toggle, storage toggle remain.
- Windows reinstall / MCP paths unchanged.

## Setup

No new dependencies. Files: this spec, `policyListUtils.ts`, `PolicyMetaView.tsx`, `PolicyRules.tsx`, `PolicySkills.tsx`, `index.css`, `tagCompact.test.ts`, `tools/gauntlet-policy-table-oneline.sh`.

### B5 — Table must not crush columns (2026-09-01)

`.page-table { width: 100% }` plus many columns made IDs wrap and tags stack. `table.policy-table` uses `width: max-content` (and `min-width: 100%`) so each row stays one line; the wrapper already scrolls horizontally.

IDs, scope badges, and tags use `white-space: nowrap` and tags use `flex-flow: row nowrap`.
