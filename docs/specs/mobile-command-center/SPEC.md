# Mobile Command Center

Base: `01602b5`, isolated worktree on `feat/mobile-command-center`.
Tier 2 UI change. Spec approval: not obtained (autonomous run requested).

## Acceptance criteria

1. Every route from `routes.ts` and every Settings section renders at 320, 390 and 768 pixels without page-level horizontal overflow or uncaught errors. Retired Sonar redirects to Stats.
2. Navigation opens as an accessible drawer; selecting a route or pressing Escape closes it and restores focus. Hidden navigation cannot receive keyboard focus.
3. Page filters and column sort/filter controls open in mobile sheets. Filtering and sorting retain their existing behavior; closing the sheet preserves selections.
4. Nodes, Files, Memory, Rules, Skills, Graph, Logging and quality details occupy the mobile viewport with a visible close action and independently scrollable content.
5. Settings navigation opens in a sheet and closes after section selection. All 14 sections remain reachable without a wall of tabs above the content.
6. Mobile lists show readable stacked rows and persistent row actions; desktop column resizing, sorting and filtering remain available.
7. Forms, project selection and status popups fit the viewport, support touch and keyboard dismissal, and keep footer actions reachable. Inputs avoid mobile browser focus zoom.
8. At 1280 pixels the desktop layout and existing browser checks continue to work.

## Setup and validation

Reuse installed React, Vite and Playwright dependencies; no new runtime dependency. Use the existing compiled-UI synthetic fixture server. Add regression tests in `crates/ax-web/web-ui/e2e/mobile-command-center.spec.ts`, audit runner and results in this directory, mobile screenshots in `site/public/screenshots/`, and update the Command Center guide and README in the same PR.

Run frontend production build, unit tests, relevant browser tests, route/Settings audits, documentation audit and review loop. Record skipped coverage/mutation/tooling layers explicitly. No backend/Docker/Netlify builds, deployments or workflow dispatches. Commit and push one completed change and open a PR; existing session authorization permits merge after applicable checks pass.

## Live verification

Requested URL: `https://admin.wenneker.dev`. Container Playwright returned `net::ERR_EMPTY_RESPONSE`; cloud browser returned `net::ERR_BLOCKED_BY_CLIENT`. Production is read-only. Local synthetic UI evidence does not verify production deployment, backend permissions or real phone keyboard behavior.

## Clarifications found during verification

- Desktop hover and outside-click inspector scenarios retain their desktop contract. Mobile inspectors cover the screen, with Close/Escape and persistent actions tested separately.
- The public log fixture has two records containing `workspace`; the single-result filter scenario uses the unique phrase `workspace sync ok`. The expected single-result assertion is unchanged.
- Unit tests that inspect sibling source files require their source tree next to transpiled tests. The test runner copies that tree into its temporary directory before bundling; no test assertions change.
- Rebasing onto `8235d81` preserves the newer Logging presentation and inspector fix. Rebuild and repeat all affected checks after the rebase.
- That rebase also expanded the public screenshot log from 10 to 15 records. The interaction test configures its own original 10-record stream, preserving its assertions while screenshot captures use the expanded fixture.
- Final visual review adds explicit mobile coverage for Review filters/comparisons, Savings period controls and full-width Markdown/Preview tabs. Their two regression tests fail against the pre-fix compiled `40c8673` UI and pass after the corrections. Mobile Memory metrics wrap into two columns; editors preserve their draft when switching tabs.
