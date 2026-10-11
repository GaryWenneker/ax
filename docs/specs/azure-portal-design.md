# Azure Portal layout for Command Center

Status: implemented on `feature/azure-portal-design`; validation evidence is tracked separately.

## Goal
Add selectable Azure Portal Light and Dark styles to Ax, based on the approved visual concepts. Preserve existing presets and actual Ax functionality. Use independent CSS and Ax components: Azure's deployed private stylesheets are not a stable component API.

## Behavior
- Settings → Appearance offers Azure Portal Light and Azure Portal Dark. The selection persists in browser storage and applies immediately without changing the workspace configuration.
- Azure themes use a full-width 48px header, flat surfaces, blue active accents, compact primary navigation, and Segoe UI typography. Existing presets retain their workbench chrome.
- The portal search opens an existing page by exact label, or searches indexed code. Submitting another query while Search is open updates results.
- Settings has a second, filterable navigation beside the primary menu. Groups: General, Connections, Data. Every item opens an existing settings surface. Only the active panel is visible; switching sections preserves draft edits.
- Appearance contains theme, savings visibility, timezone and Smart Output information. Pipeline and PR settings remain under Command Center with their existing save action. Sharing, policy sync, remote MCP, vault, OKF, plugins, embeddings and policy sharing remain operational.
- On phones the settings menu wraps above content and primary navigation opens as a drawer. IDE embed retains its docked sidebar and omits browser header.
- Theme changes update native form color scheme and Markdown editor colors. Status bar uses neutral portal chrome and the same real data/actions.
- Every page uses the portal content system: flat headers, command bars, panels, filters, tables, logs, status strips, modals and detail blades. Rules and Skills expose separate level/scope/priority columns; Memory exposes kind/source/update columns. Existing row actions remain available.
- Graph includes a page header, readable light-mode labels and community colors, with redraw on theme changes. Agent terminal colors follow theme changes without reconnecting the session.

## Visual references
- https://learn.microsoft.com/en-gb/azure/azure-portal/azure-portal-dashboards
- https://microsoft.design/articles/designing-for-scale-and-complexity/

## Validation
UI production build; 14 existing theme contrast tests; six browser checks with mocked API fixtures for all 20 page routes in both themes, theme persistence, settings navigation, retained drafts, search, phone width and IDE embed. The route check includes offline/error states and detects uncaught JavaScript errors. Screenshots are implementation previews with fixture data, not evidence of running backend integrations. No backend integration or Rust build was performed for this frontend change.

Run browser checks with `npx playwright test e2e/azure-portal.spec.ts` while the UI dev server is listening on the test base URL. Set `AX_PORTAL_SCREENSHOTS=1` to capture desktop previews and the phone settings view under `/tmp/ax-portal-previews`.

## Implementation previews

Demo API fixtures, captured from the implemented UI:

- [Memory light](../design/azure-portal/azure-light-memory.png) · [Memory dark](../design/azure-portal/azure-dark-memory.png)
- [Rules light](../design/azure-portal/azure-light-policy-rules.png) · [Rules dark](../design/azure-portal/azure-dark-policy-rules.png)
- [Graph light](../design/azure-portal/azure-light-graph.png) · [Graph dark](../design/azure-portal/azure-dark-graph.png)
- [Stats light](../design/azure-portal/azure-light-stats.png) · [Stats dark](../design/azure-portal/azure-dark-stats.png)
- [Phone settings](../design/azure-portal/azure-light-phone.png)
