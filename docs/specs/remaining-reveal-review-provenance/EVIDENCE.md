# Verification evidence

Implemented against main `e20d0b0de8154b99513dd408e6306c0e3d9b94ad` on 2026-10-12. The concurrent phone landing/menu fix was rebased cleanly and preserved. The user authorized implementation, follow-up requests and merge without a separate human spec approval round.

## Behavior and verification

- Homepage reveal: mounted tagged sections, actual scroll visibility, keyboard exposure, Pause, reduced-motion preference, explicit Play, no JavaScript and share-image preparation. Desktop 1440px and mobile 390px checks use local Astro dev only. Scroll-reveal module line/function/branch coverage: 100% each.
- Website install: all four manually selectable platforms offer script commands, not binary archives. macOS/Linux/WSL2 use the shell installer; Windows uses PowerShell. Browser checks exercise copying and denied-clipboard fallback in both viewports. Native execution on macOS/Windows was not available. Browsers cannot execute local terminal commands; users copy, paste and run them. Short screens/open commands can scroll.
- Documentation: exactly the homepage CMY screen CSS, live motion and translucent readable content surfaces. Light ink is dark and Dark ink is light. No production website build or deployment ran.
- Agent UI: removed Activity navigation, route and page. Fixed-path and legacy-hash bookmarks open Stats in desktop/mobile. IDE integration/backend agent APIs remain available.
- Appearance: selected preset is passed directly to accent application. Light/Dark palette names, swatches and reload persistence verified. Existing all-accent tests check contrast and unchanged header/footer surfaces.
- Density: all standard PageHero pages, including Graph/Settings, share title, expandable About, actions and compact spacing. Memory workflow/tag help starts collapsed; redundant kind legend removed because rows already show Kind. Row-header Y <300px desktop/<430px mobile. Logging retains its compact native command bar. Shared header tests cover Rules, Skills, Nodes, Files, Unresolved, Stats, Search, Graph, Settings, Prices, Review, Sync and Quality Gates. Standard semantic badge classes use 100px widths across pages; Rules/Skills varied labels verified.
- Policy source: hash-bound atomic sidecars record local pack, scrubbed remote, node/project/cluster/revision and snapshot policy ID. Missing/stale/corrupt/oversized/symlink metadata returns an honest unknown origin. Approve/reject clean up the sidecar. Same-ID rule/skill, snapshot 409, restart persistence and actual CLI import/HTTP review checked.
- Replication compatibility: existing two transport properties remain unchanged. A new original-ID property was rejected during review because older peers would fingerprint it as a local edit and create feedback. Sources report the validated snapshot identifier rather than inventing a native identifier.
- Evergreen documentation: README version-history list removed; stale standalone/autonomous/appearance specifications corrected. The no-per-version-updates rule was already stored through Ax MCP in the preceding merged work.

## Checks

Rerunnable entry: `verify.sh` in this directory. Set CARGO/RUSTFMT if needed and AX_CHROME_EXECUTABLE to local Chrome. It builds the embedded UI before the CLI and compares served index/assets to their rebuilt bytes.

Affected Rust suites: ax-policy 257, ax-share 25, ax-web 103, CLI server tests 25: **410 passed**. The server suite includes real three-node graph/source/memory/log/conversation synchronization and no-feedback/retention checks. Strict Clippy on four affected crates/all targets passed. Rust format checks passed. The backend did not change during UI follow-ups; tests were not repeated merely for CSS edits.

Final embedded UI: TypeScript + Vite build and CLI embedding passed. **24 desktop/mobile Playwright cases passed**: retired routes, palette, Memory density, shared headers, badge widths, control spacing/dropdowns, revoked connection removal, Quality Gates navigation, every accent's contrast/persistence, Savings alignment and review bulk/partial failure. Real CLI/HTTP provenance smoke passed. Theme/navigation unit suite: **13 passed**. Website unit suite after rebase: **55 passed**.

Mutation verification: homepage selector removal, stale sidecar hash acceptance and remote username leakage: **3/3 killed**, exact original bytes restored. Coverage thresholds applied to the actual reveal module only, not claimed for Rust or the whole UI.

## RED, refactor and review

Observed RED cases included missing homepage reveal, hard-coded unknown provenance, sidecar missing/stale/kind collision, README history, share-image visibility, SSH scrubbing, and older-peer transport extension. Follow-up RED included retired navigation and Memory row position (355px, then 339px desktop), followed by compact card header/legend fixes. The previous desktop row threshold was retained. Clipboard browser checks originally hit Astro's dev toolbar on mobile; checks exclude that dev-only surface and wait for actual clipboard status. A concurrent mobile fixed-height change exposed short-screen clipping; scrolling was restored for short screens/open instructions.

Review rounds corrected strict Clippy fixture initialization/helper placement; the replication compatibility issue; Python import side effects, explicit UTF-8, SQLite connection cleanup and optimized-assert execution; shared preset application; and Light docs ink. JavaScript, TypeScript, React, Rust, Python and API compatibility checklists were applied. **Final verdict: APPROVED, 9/10, zero unresolved findings** within the tested scope. A dedicated CSS review skill was unavailable, so generic layout/accessibility review and browser checks were used. No independent reviewer agent or human approval round was represented as having occurred.

## Limits and baseline findings

The broad unrelated WebDAV integration test `skill_page_round_trip_and_body_only_save` failed once on both the unchanged baseline main and current checkout; isolated/current integration reruns passed. No test was weakened. Broad CLI execution attempted an optional external pricing request and automatic approval review rejected it; the safer affected server suite and pricing-seeded owned HTTP fixtures were used. No bypass/retry of that external request was made.

Real LAN/interface broadcast, native macOS/Windows, external HTTPS/OAuth providers and full-workspace/all-CLI testing were not verified in this follow-up. Prior merged end-to-end OAuth/replication work is historical evidence, not represented as freshly rerun here. No release, CI dispatch, production site build or deployment was performed. Old reviews with absent provenance cannot have their historical source reconstructed.
