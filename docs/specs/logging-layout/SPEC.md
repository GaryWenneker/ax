# Restore logging layout

Tier 2 regression fix, authorized by the request to restore the page and standing implementation/merge instructions. Work in the isolated repository checkout.

Preserve logging event headlines, timestamps, badges and direction metadata in the existing stacked layout at all panel widths. Generic compact-row overrides must not change the logging stream or inspector call steps. Preserve live delivery, filtering and opening event details. Keep compact policy and memory rows.

Use real frontend rendering with only network boundaries mocked: populated SSE batch, log metadata and configuration. Test desktop/mobile, long headlines, row alignment, filtering and details. Establish RED before CSS changes; restore logging-specific layout by narrowing shared selectors. Run strict production build and relevant desktop/mobile regressions. Record actual evidence and review results. No backend build or deployment.

The expanded column request is implemented under [the shared list-column contract](../list-columns/SPEC.md). Its verify.sh and EVIDENCE.md cover this restoration and the dedicated logging columns.
