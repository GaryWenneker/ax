# SPEC — one calm list style across Command Center

Status: draft, awaiting approval. Tier 2 (UI refactor, no data or API change).

## Goal

Rules, Skills, and Logging lists look and behave like the Memory list. They use the same
component (`ItemList` + `ItemRow variant="graph"` in `components/ui/PageLayout.tsx`), so
future pages inherit the style instead of copying it.

## Behaviors

1. **Rules list** renders each rule as an `ItemRow variant="graph"`:
   title = rule id, subtitle = `level · scope · priority N`, badges = level (error/warning/info
   icon), scope (repo/globe/organization/person/lock), up to 3 tags + `+N`; aside = the existing
   enable toggle. Clicking a row opens the same blade as today.
2. **Skills list**: same, with title = skill name, subtitle = first line of the description.
3. **Groups** (the current `policy-skill-group-row` table rows) become a quiet section header
   inside the `ItemList` (small uppercase gray label + count, chevron to collapse). Collapse
   state keeps its current localStorage key.
4. **Logging list** rows use `ItemRow variant="graph"`: title = message, subtitle =
   `domain · tool · duration`, meta = relative time, badges = level + inbound/outbound trace.
   Live streaming still appends rows.
5. All three lists: scroll inside their own panel, selected row = accent highlight + 1px
   `--accent-text` border, hover changes only the background (rule `outline-badges`).
6. No table borders, zebra stripes, filled chips, or column headers remain on these pages.

## Must NOT change

- Any API call, request shape, or data shown (only layout).
- Filters, search, keyboard navigation, blade open/close, toggle behavior.
- Existing vitest suites stay green; `policyBladeMotion` tests unchanged.

## Tests (RED first)

- `src/components/ui/PageLayout.test.tsx`: `ItemGroupHeader` renders label, count, and toggles.
- `src/pages/PolicyRules.test.tsx`, `PolicySkills.test.tsx`, `Logging.test.tsx`: render with a
  fixture; assert `.page-item--graph` rows exist, no `<table>`, badges carry `.badge-icon`,
  clicking a row selects it (`.page-item--selected`), toggle still calls the API mock.

## Gauntlet

vitest, `tsc --noEmit`, eslint, `npm run build` (exit 0), 3 manual mutants (drop `variant`,
drop selected class, drop group collapse), `bash scripts/reinstall-cli.sh` + served bundle hash
check, browser screenshot of each page.

## Setup plan

- Isolation: branch `calm-list-style` from the current tree (the working tree has many
  uncommitted changes; a worktree would miss them).
- New files: the test files above and a small `ItemGroupHeader` in `PageLayout.tsx`.
- No new dependencies.
