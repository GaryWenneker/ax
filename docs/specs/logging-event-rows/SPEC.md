# Readable Logging event rows

Source base: `01602b519855144d01ae1d9adc3f898c890db060` (main).
Isolation: a clean main-based branch `fix/logging-event-rows`, reusing installed locked dependencies in this isolated checkout.
Tier: 2, frontend presentation bug fix. Spec approval: not obtained (autonomous run under the user's instruction to fix the appearance).

## Acceptance criteria

1. Reproduce the user's real-shaped Workspace sync, Memory capture-git and Ship start/ok lines. Show readable action titles and a quiet facts line, without `[ax]`, axe emoji, duplicated category labels or raw `tool=`/`duration_ms=` strings in the row. Unknown messages retain their text; the underlying log entry and inspector remain unchanged.
2. At 393, 900, 1366 and 2048 px, each shared 12 px row marker has a dedicated gutter with at least 12 px between its right edge and event text. Shared column rows do not draw an unrelated-event dotted timeline through the text. Event and subtitle align; no row/page horizontal overflow at default widths.
3. Keep independent column resizing, sorting/filtering, raw-field search, kind/tool filters, live arrivals, keyboard selection, selected border and inspector/raw payload access. Related-call grouping still uses existing association metadata.
4. Capture current desktop/mobile Logging and its detail inspector from the compiled UI with synthetic real-shaped input, record provenance and inspect actual PNGs. Update README and Logging website guidance in the same change. Run a second diff/screenshot review and resolve findings.

## Setup and verification

No production dependency, HTTP/backend/CLI contract change, Netlify/backend/Docker build, release, suspended workflow dispatch, production setting mutation or server restart. Restore an installed Chrome headless-shell in temporary storage if workspace maintenance removed the prior browser. Use existing TypeScript/Vite build, Node unit tests and Playwright fixtures. Add presentation unit tests and real-line browser regression checks; observe RED before implementation. Run relevant shared list/accessibility/live/inspector checks, local site tests and documentation audit. Keep the rerunnable verification entry point and EVIDENCE in this directory. Local compiled-preview evidence does not update the embedded UI in the running Ax binary.

## Screenshot review clarification

The same column-gutter defect affects the inspector Steps list. It must use the shared 40 px gutter and readable action presentation too. The inspector header/Tool field must prefer the selected entry's own tool, falling back to its MCP call group only when the selected step has no tool. Keep call grouping and navigation unchanged; this fixes misleading presentation without changing log parsing or group membership. A mixed MCP/domain browser fixture must reproduce both defects before the repair. Inspector captures must scroll the original raw line into view instead of claiming to show content below the crop.

On a 393 px screen, short domain rows place Time and Context beside each other below the title/facts, with their centers aligned within 2 px and a row height at most 72 px. Long Context labels wrap within the available panel width; this preserves the same columns while avoiding four separate lines for routine events.
