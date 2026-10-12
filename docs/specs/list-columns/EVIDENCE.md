# Logging restoration and shared list columns — evidence

Tier 2. Authorized implementation and merge; no separate spec approval was requested. Isolated checkout based on main `bf84fa44f2f6e502b4706b8dc5e7c9cfd8f463bf`. This change includes the verified source, generated frontend, current documentation and screenshots. No dependency, backend or workflow changes.

## Acceptance mapping

| Behavior | Executable check |
|---|---|
| Stable ascending/descending/original order; numeric, currency, percentage and magnitude comparison | `columnModel.test.ts`: sorting is stable; formatted numbers and numeric suffixes |
| Conjunctive, case-insensitive filters preserve original input | Model filter and immutability assertions |
| Width bounds, corrupt storage tolerance, drag/keyboard resizing, persistence and reset | Model width tests and `list-columns.spec.ts`: column drag keyboard resizing persists and resets independently |
| Sorting/filtering preserves policy row actions | Headings sort numeric values; opening the existing full-page rule editor |
| Mobile controls and readable cards | Mobile Columns menu test and existing narrow-panel density/overflow cases |
| Native table columns grow and shrink independently | Shared data tables test, measured widths of the changed and neighboring columns |
| Nodes, Search and Unresolved adopt shared controls | Dedicated shared-list integration test |
| Review filters govern bulk selection and retain comparisons | Review column filters test |
| Memory enabled filters match implicit legacy defaults and explicit disabled records | Memory enabled filters test |
| Logging headline and details stay stacked and left aligned | `logging-layout.spec.ts`: geometry at 2048, 1366, 900 and 393px |
| Logging kind filters, full inspector and visible-order keyboard navigation remain functional | Logging filter, sorted navigation and inspector assertions |
| Actual stream arrivals obey active column sort/filter | Persistent same-origin loopback SSE fixture; new event arrives while stream remains open |
| Meaningful outbound headlines and compact context badges | Populated logging fixtures and compact kind-chip assertions |
| Header Enter/Escape stays separate from log navigation | Logging heading filter keyboard test |
| Separators remain visible in system high contrast; filters avoid clipping | `column-accessibility.spec.ts`: forced-colors focus and rightmost popup geometry |
| Previous logo, retired routes, glow, spacing, responsive rows and connection cleanup remain intact | Existing brand, control-spacing and focus-row-density browser tests; navigation/theme suites |

Shared ItemList and DataTable components also cover Savings, Prices, Stats, plugins and package previews. Single links, trees, graph edges and prose cards retain their appropriate presentation. Sorting/filtering is explicitly labelled as applying to loaded rows; it does not imply server-wide results across unseen pages. Actions and graphical distribution columns are not sortable/filterable.

## Final verification

Run from the repository root, using the existing installed Chromium executable:

```sh
AX_CHROME_EXECUTABLE=/tmp/ax-finish-site-final/chrome-headless-shell-linux64/chrome-headless-shell \
AX_COLUMN_SCREENSHOTS=1 AX_DENSITY_SCREENSHOTS=1 \
sh docs/specs/list-columns/verify.sh
```

The executable path is environment-specific; substitute another existing compatible Chromium binary when reproducing. Final run completed with **exit 0** after the last product-source edit.

- Frontend package unit suite: **10 passed**, zero failures.
- Navigation, retirement, themes and column model suites: **24 passed**, zero failures; **34 frontend unit tests total**.
- Website unit suite: **55 passed**, zero failures.
- Strict TypeScript and Vite production frontend build: **exit 0**.
- Desktop/mobile browser suite: **52 passed**, zero failures.
- Behavioral negative controls: **4/4 killed** — sorting removed, filters disabled, width bounds removed, logging stack alignment broken. Each mutant failed the expected assertion. Source restoration runs in `finally`; final verification rebuilds the restored source.
- Node syntax, Python compilation and source whitespace checks: **exit 0**. Generated Vite output is excluded from whitespace checks because bundled grammar/template literals contain intentional whitespace.

Wide Rules, Skills and Memory rows remain **34px**. Narrow fixture rows wrap with labelled values: approximately **128px** for Rules/Skills and **122px** for Memory. Logging now has separate Event, Time and Context columns; the event headline/details share the same left edge at every tested width. Explicit user resizing may introduce horizontal scrolling on wide panels instead of silently redistributing neighboring columns.

Nine current screenshots were generated from the fresh build: existing focus and responsive policy/memory images plus desktop/mobile logging. The logging images were visually inspected after the final run: meaningful outbound titles, compact context badges, readable event details, valid zero-valued synthetic metrics, and working live-stream status. Screenshot fixtures contain synthetic public data.

## RED checks, corrections and review

The original compact-row CSS reproduced approximately **199.19px** of logging headline/detail misalignment. The new resize behavior was absent before implementation; model tests failed while the model module was missing. Further RED checks caught numeric policy priorities parsed as `NaN`, native-table shrinking redistributed neighboring columns (67.73px instead of the intended 90px), empty outbound headlines, overwide kind chips, header keyboard actions opening log events, unsupported formatted numeric comparisons, invisible high-contrast separators, clipped rightmost filters (right edge 1520px versus list edge 1367px), and implicit enabled Memory records incorrectly filtered as disabled.

Fixture and runner failures were corrected explicitly:

- The rule editor is an existing full page, not a dialog. The test targets its actual heading with a valid rule document and `outgoing`/`backlinks` API fixture; a missing links fixture had caused an asynchronous `undefined.map` failure.
- A cross-origin route rewrite was blocked by the browser. The runner now owns a same-origin loopback HTTP/SSE proxy; it does not bypass external-site protections. Only transient startup 502 responses are retried; served HTML must still match the fresh built index exactly.
- Empty quality fixtures produced invalid screenshot metrics. Valid empty snapshots now yield numeric zero values.
- The Memory fixture initially failed to match the real query-bearing API URL. After correcting the route, the unchanged expected two enabled rows received one, exposing the product bug. `enabled !== false` now agrees with the existing legacy UI semantics.
- ES2020 rejected `replaceAll`; the implementation uses regex replacement. React 18 JSX types lack `popover`; a typed DOM API in a layout effect enables native top-layer panels without suppressing TypeScript or changing dependencies.

The full-label regression initially failed on the old defaults. An initial width increase still clipped the combined Enabled / actions label (126px of text in a 120px button); the final defaults reserve space for labels and sort arrows. The narrower-panel card fallback now starts at 745px. The existing heading-box check had allowed inner text ellipsis, so the strengthened check measures the actual text button at 1366/2048px, both initially and after sorting. The targeted regression then passed in both browser projects.

Behavioral assertions and resize tolerances were retained when correcting implementation and fixtures. Final staging caught trailing blank lines in two new components, which unstaged whitespace checks had not included. Those lines were removed; the full final verifier was rerun with all new files staged.

| Review round | Findings | Resolution |
|---|---|---|
| 1, after initial 42-test run | R1: empty outbound labels; fixed-width logging chips; header/global keyboard conflict; formatted numeric comparison; inappropriate header ARIA context | Headline fallback, scoped compact chip styles, controls own keyboard handling, numeric parser, appropriate table/div semantics |
| 2, after 46-test run | R2: separator focus absent in forced colors; rightmost filter clipped by scrolling container | One-pixel system Highlight outline; clamped native top-layer filter panel |
| 3, during subsequent verification | R3: incomplete editor boundary fixture; Memory column values disagreed with implicit enabled semantics | Complete links fixture; query-aware Memory regression and `!== false` fix |
| 4, source review and 52-test run | No open findings in the source checks completed at this point | Final visual review followed |
| 5, final screenshot review | R4: default Priority, Updated, Enabled and combined Enabled / actions labels were clipped by the added controls | Wider automatic columns and 745px card fallback; explicit full-label assertions at 1366/2048px, including active sort arrows |
| 6, final source/screenshot review and fresh 52-test run | **0 open findings** | All acceptance and regression checks green |

Applied usable old-coder, review-loop, TypeScript, React, JavaScript and Python guidance. CSS review skill was unavailable; generic CSS/accessibility review covered layout, keyboard focus, overflow, breakpoints and forced-colors behavior. Independent agent review was not performed.

## Source identity and runtime

Product-source SHA256: `74da1a0bbe181682d96410eceb45dbe54bdeb80dc71864369997c7c38acdc5e2`. This hashes the sorted changed/new non-test files under `crates/ax-web/web-ui/src/`, concatenating each UTF-8 relative path, NUL, file bytes and NUL. The documentation report itself and generated assets are excluded to avoid a circular hash. The committed Git tree also includes the exact generated frontend that was tested.

Node **24.19.0**, TypeScript **5.9.3**, Vite **5.4.21**, Playwright **1.62.0**, Python **3.12.14**; existing React/Chromium dependencies. No new package or runtime permission. The runner allocates unique loopback ports, validates the freshly built index, serves a real persistent SSE connection and shuts down its owned processes. API fixtures replace network boundaries; the UI and column model are real.

Vite retains its existing large-chunk advisory; build succeeded. No configured changed-line coverage threshold, separate CSS linter, numeric coverage claim or property-testing framework. Existing functional tests and explicit geometry/edge cases provide evidence, not absolute proof. Backend/native/Docker builds, workflow dispatch, releases and deployments were not performed. The live admin requires the owner to pull main and rebuild/restart Ax to load the new embedded frontend. Older browsers without native popover support retain the inline details fallback; automated targets are modern Chrome desktop/mobile.
