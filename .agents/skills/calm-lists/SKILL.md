---
name: calm-lists
description: "Build and review calm, compact, accessible list interfaces in the AX Command Center using the existing shared list primitives. Use for Rules, Skills, Memory, Logging, review queues and new collection views; preserve column resizing, sorting, filtering and responsive behavior."
alwaysApply: false
triggers: ["calm-lists", "calm lists", "list UI", "list layout", "ItemList", "ItemRow", "PolicyCalmList", "Command Center lists", "rustige lijsten", "mooie lijsten"]
tags: ["ui", "lists", "accessibility", "design-system"]
priority: 85
enabled: true
status: approved
scope: project
group: design
---

# Calm Lists — AX Command Center

## Mission

Produce consistent, elegant, information-dense AX lists: quiet visual hierarchy, fast scanning, predictable controls, keyboard accessibility, and usable layouts on narrow screens. Reuse the project design system, never introduce a parallel table or list system.

**Read and obey** `.agents/rules/calm-lists.mdc`, `.agents/rules/outline-badges.mdc`, and `.agents/rules/wcag-contrast.mdc` before changing UI. This skill explains HOW to implement; the rules remain authoritative.

## Canonical components

Inspect existing usages and component contracts before editing:

- `crates/ax-web/web-ui/src/components/ui/ItemList.tsx`
- `crates/ax-web/web-ui/src/components/ui/ListColumns.tsx`
- The existing `ItemRow`, `ItemGroupHeader` and `PolicyCalmList` definitions/usages
- `crates/ax-web/web-ui/src/lib/columnModel.ts`
- `crates/ax-web/web-ui/src/hooks/useColumnFit.ts`
- `crates/ax-web/web-ui/e2e/list-columns.spec.ts`
- `docs/specs/list-columns/SPEC.md`

For list pages use `ItemList className="calm-list"` and `ItemRow variant="graph"`, or the existing `PolicyCalmList` for policy pages. Memory may use the established `memory-vault-list` variant. Group headings use `ItemGroupHeader`. Do not switch those screens to `<table>` or `DataTable`; existing specialized numerical/data tables elsewhere may remain where appropriate.

## Visual contract

1. **Hierarchy:** strong primary label; at most one muted subtitle line by default; compact metadata to the right; descriptive labels instead of raw IDs when possible.
2. **Density:** compact but comfortably clickable; consistent row heights, alignment, separators and horizontal padding across pages. Avoid box-per-row cards, excessive borders, gratuitous shadows and glow.
3. **Badges:** restrained outline badges with stable widths where appropriate; semantic color only for actual status. Never use color alone to communicate meaning.
4. **Actions:** group checkbox, toggle and contextual actions in the existing row aside. Reveal secondary actions on hover AND keyboard focus; keep essential actions discoverable on touch.
5. **States:** deliberate loading/skeleton, empty, no-filter-results, error, selected, focused and disabled states. Show an actionable filter-reset affordance for empty filtered results where existing controls support it.
6. **Themes:** test light and dark; meet WCAG AA contrast for readable text and UI indicators. Honor reduced-motion and system high-contrast modes; visible but subtle focus treatment.
7. **Responsiveness:** no uncontrolled horizontal page overflow. At smaller widths retain the title, essential status and a touch-safe actions menu; reduce or stack secondary metadata using established responsive styling. Never rely on hover alone.
8. **Scale:** support long labels, localization, high row counts, loading and pagination/virtualization where needed without breaking keyboard navigation or row identity.

## Column behavior

Where a list exposes columns, retain the AX shared column model:

- Each meaningful column is independently resizable by pointer and keyboard within safe min/max widths.
- Persist user-set widths per list schema; recover gracefully from missing/corrupt storage; provide reset.
- Headers sort ascending, descending, then original order where supported. Stable ties and numeric sorting are required.
- Per-column text filters intersect without mutating source rows. The UI must distinguish filtering currently loaded rows from server-wide filtering.
- Never make action columns sortable/filterable.
- Changes must preserve row click navigation, multi-selection, keyboard actions, streamed updates and existing pagination.

## Workflow

1. Run AX preflight. Explore the actual list component, its styles and page usages before editing. Identify the existing pattern and any exceptions.
2. State the list's information hierarchy: title, subtitle, metadata, status, selection and actions; decide what collapses on mobile.
3. Reuse existing primitives and design tokens. Avoid page-specific CSS unless a reusable variant is genuinely needed.
4. Implement the smallest cohesive change without changing business logic or data semantics.
5. Review desktop + mobile and light + dark, including long text, zero rows, active filters, keyboard-only, touch and high contrast.
6. Run relevant typecheck, lint, unit tests and Playwright tests, especially list-columns and affected page flows. Follow the project's old-coder quality workflow for implementation changes.
7. Report the changed files, scenarios validated and any remaining gaps. Do not claim green tests unless actually executed.

## Acceptance checklist

- [ ] AX's canonical calm-list primitives are used on list pages.
- [ ] Title, subtitle, metadata, badges and controls follow a consistent layout.
- [ ] Keyboard focus and touch reveal all necessary actions.
- [ ] Light/dark, narrow screens, long titles and empty/filter/error states are usable.
- [ ] Sort, filter, resize, persistence/reset and row actions remain correct where present.
- [ ] No new page-specific list visual language or unnecessary dependencies.
- [ ] Accessible names, contrast and reduced-motion/high-contrast behavior are verified.
- [ ] Existing tests and relevant visual/browser checks are run and results reported.

## Related

- Rule: `.agents/rules/calm-lists.mdc`
- Rule: `.agents/rules/outline-badges.mdc`
- Rule: `.agents/rules/wcag-contrast.mdc`
- Specification: `docs/specs/list-columns/SPEC.md`
