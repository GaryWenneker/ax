# Command Center pages — selection, zip, broken views

Tier 2. Spec approval: not obtained (autonomous run; user listed behaviors and asked to fix in the browser).

## Behaviors

### S1 row selection (skills and rules)

- Plain click on a row: that row is the only selected item and (if project origin) opens the inline editor.
- Ctrl/Cmd click: toggle that row in a multi-selection; inline editor closes when more than one row is selected.
- Shift click: select the inclusive range from the last plain/cmd anchor through the clicked row among currently visible rows.
- Checkbox in the header toggles all visible rows.
- Right-click uses the current multi-selection when the row is already selected; otherwise it selects that row then shows the menu. Menu actions apply to every selected row they are valid for.

### S2 click outside

- Pointer down outside the inline editor panel (and outside the context menu) closes the editor and shows only the list. A click on another row still follows S1.

### S3 zip package

- Package modal has **Include global.db copies**. Off: only this-project shareable items. On: global copies are listed and selectable.
- Global copies that are not on disk cannot be packed; the UI still lists them and packing project items together with a warning is allowed when mixed — selected global-only ids are omitted from the zip request and shown in an error if the user selected only those.
- Restore modal has **Also copy restored items into global.db** (off by default). On: after a successful restore, each written rule/skill is relocated to global.db.

### S4 sonar / agent

- Command Center sidebar has no SonarQube or Agent items.
- `/sonar` and `/agent` redirect to `/stats`.
- Settings has no “show agent terminal” control.

### S5 prices

- Catalog is grouped by provider with summary cards (model count, min input rate). Selecting a model still shows price-over-time.

### S6 unresolved

- Kind filter pills are clickable and update the list.
- URL kind only overwrites the filter when the query param is present.
- The list scroller is the IntersectionObserver root so infinite load does not stampede.
- Clicking outside the detail blade closes it.

### S7 savings

- Page renders when `pricing` or `agent_sessions` is missing (empty arrays / fallback copy).

## Invariants

- English UI strings.
- Agents still match only project ax.db.
- Existing zip format unchanged.

## Setup

- Isolation: current branch.
- Tests: `crates/ax-web/web-ui/src/gitShare.test.ts` (node:test).
- No new npm dependencies.
