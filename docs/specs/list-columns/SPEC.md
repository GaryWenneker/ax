# Shared list columns

Tier 2, authorized by the user's request and standing permission to implement and merge. Work in the isolated Git checkout.

Every shared ItemList and DataTable, including Rules, Skills, Memory, Nodes, Search, Unresolved, Logging, Savings, Prices, Stats, plugins and package previews, exposes independent column resizing, sorting and text filtering. Review queue uses the same controls on its heading columns. Single links, graph edges and prose cards without tabular data remain links/cards rather than acquiring artificial columns.

Drag a separator or use its arrow keys to resize, with bounded widths and accessible labels. Remember widths per page/schema in localStorage; tolerate unavailable or corrupt storage. Reset widths. Click a sortable heading through ascending/descending/original order. All active column filters intersect and never mutate source data. Numeric values sort numerically; original order and ties remain stable. Actions are not sortable/filterable. Existing row actions, selection, pagination and streaming continue to work. Client filtering/sorting applies to loaded rows, labelled accordingly, not unseen pages.

Logging retains its stacked headline/details, timestamps, direction indicators, tool badges, filters and inspector. Its dedicated layout exposes Event, Time and Context columns without applying policy/memory row overrides. Arrival order is the default; live batches use active sort/filter. Narrow screens retain cards; a Columns menu provides accessible sort/filter controls. Explicit resizing can use horizontal scrolling on wide viewports.

Tests: pure stable sort/filter, numeric/date comparison and width validation; browser resize/keyboard/reset/persistence, sort/filter/action preservation, populated logging/SSE/inspector, generic tables, long metadata and desktop/mobile regressions. RED precedes implementation, negative controls prove behavioral assertions. Strict TypeScript + frontend production build, relevant unit/site tests, review to zero findings, reproducible EVIDENCE and current screenshots. No backend/Docker/release/deploy.

Column filter overlays use the native top layer to escape list scrolling/clipping, close on Escape or outside focus, and keep the trigger keyboard behavior separate from log navigation. Separators retain a one-pixel system-highlight focus outline in forced-colors mode. Default widths are bounded at 64–1600px, with a 20px minimum for Review selection controls. Modern Chrome desktop/mobile are the automated browser targets.
