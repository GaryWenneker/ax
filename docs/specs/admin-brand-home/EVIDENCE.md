# Admin branding, retired Quality Gates, and compact tags

Tier 2. Spec approval: not obtained (autonomous run under the user's standing implementation/merge authorization). Isolated checkout based on main `95821949ae6d375d6d6830fc5b786dff62770851`; this commit contains the verified sources and generated frontend. No dependency or backend changes.

## Acceptance mapping

| Behavior | Verification |
|---|---|
| Homepage wordmark font, weight, spacing, lime underscore; local font loads | `admin-brand-home.spec.ts`: header typography and font-loading assertions; website design coherence test |
| Semantic logo link returns to admin home with keyboard | Header test: accessible name, href `/`, Enter navigation |
| Quality Gates page/navigation removed; path, embed and hash bookmarks return home | Navigation retirement unit test and desktop/mobile bookmark tests; component/import and dynamic navigation removed |
| Rules/Skills tag chips fit labels, retain filtering, Light/Dark | Tag test: unequal intrinsic widths, active filter after click; scope badges stay 100px |
| Shared controls and stale connection removal remain functional | Existing `control-spacing.spec.ts`, both desktop/mobile projects |
| CLI quality gates and shared configuration APIs remain | No Rust/API changes; shared `shipApi` imports retained in Settings, SonarQube and App |

## Executed checks

Final command: `AX_CHROME_EXECUTABLE=<installed Chromium executable> sh docs/specs/admin-brand-home/verify.sh`.

- Navigation unit tests: **6 passed**, zero failures.
- Website suite: **55 passed**, zero failures; no website production build/deployment.
- Embedded frontend production build: **exit 0**, strict TypeScript and Vite.
- Targeted desktop/mobile browser suite: **10 passed**, zero failures (35.2 seconds).
- `git diff --check`, Node syntax check, Python compilation: **exit 0**.
- Negative controls: `python docs/specs/admin-brand-home/mutations.py` with the same Chromium environment. **3/3 killed**: restored retired route, incorrect logo font, fixed-width tags. Each control ran and failed the expected behavioral assertion; sources restored in `finally`. Final verification rebuilt the restored source afterward.

RED observations: `/ship` originally parsed as a valid page; the initial logo check caught the portal stylesheet forcing the underscore white; the initial tag check measured 100px for `graph`. Fixed the implementation rather than relaxing those assertions. Existing tests expecting an available Quality Gates page were updated to the user's explicit new removal contract.

Runtime: Node 24.19.0, Python 3.12.14, TypeScript 5.9.3, Vite 5.4.21, React 18.3.1; existing Playwright/Chromium installation. Archivo font is reused from the website, with its OFL license copied into public and generated assets. No new packages, credentials, persistent data or network capabilities.

## Review rounds

| Round | Skills | Findings | Fixed |
|---|---|---|---|
| 1 | old-coder, TypeScript, React, JavaScript, Python usable; CSS missing (generic review) | Portal override changed underscore color; obsolete page documentation; script import side effects | Specific brand color/weight rule, retired-page docs, Python main guard |
| 2 | Same skills and generic CSS/accessibility review | **0 open findings** | — |

## Limits

Browser inspected the live website wordmark: Archivo Variable, 29px, weight 700, -2.465px tracking, ink `rgb(240,243,237)` and lime `rgb(185,255,106)`. Live admin navigation failed with `ERR_BLOCKED_BY_CLIENT`; admin checks used the actual newly built local frontend with deterministic API fixtures, not the live backend.

No changed-line coverage threshold is configured; browser behavior assertions provide functional coverage, not a numeric coverage claim. No property framework added: route parsing is unchanged, with explicit retired path/hash cases. No new lint tool; TypeScript, build, syntax and diff checks ran. The complete backend and unrelated browser suites were not rerun for this frontend-only change. No independent agent verification performed. No native backend/Docker rebuild, release or deployment; the running admin instance must be rebuilt/restarted to serve the new embedded frontend.
