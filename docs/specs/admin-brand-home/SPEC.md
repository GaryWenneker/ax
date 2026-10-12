# Admin brand and retired Quality Gates

Match the public homepage wordmark: bundled Archivo Variable, weight 700, tracking -.085em, graphite text and lime underscore. Both admin wordmarks are semantic links to the admin homepage, with keyboard and modified-click support. Keep other interface typography unchanged.

Remove Quality Gates navigation, page component and route. Retired path and hash bookmarks return to Stats. Keep CLI gates and shared Settings/Sonar APIs intact. Build the embedded frontend and verify routing and typography; no backend rebuild, release or deployment.

## Added request: tag chip widths

The supplied Skills screenshot shows metadata tags padded to a uniform width. Shared tag chips must fit their icon and label, wrap between chips on narrow screens, and retain filtering. Equal widths remain for level/scope/source badges. Acceptance: `graph` is narrower than `methodology` in Rules and Skills in both themes, labels remain visible, and clicking a tag activates filtering.

Setup: existing isolated checkout, installed Node/Vite/Playwright only; no dependency changes. Reuse the website's local Archivo font under its OFL license. Persist verification in this spec folder and regression tests in `e2e/admin-brand-home.spec.ts`; update existing route tests to the explicitly changed retired-page contract. Autonomous execution; formal spec approval not obtained.

Verification files also include `verify.sh`, `ui-check.mjs`, `mutations.py` and `EVIDENCE.md`. Three negative controls restore the retired route, replace the logo font, and restore fixed tag widths. They mutate only the isolated checkout and always restore source. Final verification rebuilds the restored frontend.
