# Focus glow and responsive rows — evidence

Tier 2. Spec approval: not obtained (autonomous run under standing implementation/merge authorization). Isolated checkout based on main `f17811b85e76a3f365ae179b9ee34f9de9f387da`; this commit contains the verified source, generated frontend, documentation and screenshots. No dependency or backend changes.

## Acceptance mapping

| Behavior | Executable check |
|---|---|
| Soft accent glow, zero thick outlines, no duplicate inner glow | `focus-row-density.spec.ts`: fields and keyboard buttons, Light/Dark |
| Keyboard focus retained in system high contrast | High-contrast test: solid 1px system outline |
| Focus does not resize or shift layout | Full bounding-box equality after scrolling the target into view |
| Wide Rules/Skills/Memory rows compact; narrow rows labelled and wrapped | Density test at 2048px and 393px, metadata alignment and overflow assertions |
| Long metadata stays inside panel | Overflow test at 2048, 1366, 900 and 393px |
| Column headings retain space | Header content fits its column in all three lists |
| Memory header buttons fit mobile and retain accent skin | Three buttons remain within panel; Capture color matches the existing accent Home control |
| Logo, retired routes, badge alignment, controls and connection cleanup retained | Existing brand and control-spacing browser tests, navigation/theme unit suites |

## Final verification

Command: `AX_CHROME_EXECUTABLE=<installed Chromium executable> AX_DENSITY_SCREENSHOTS=1 sh docs/specs/focus-row-density/verify.sh`.

- Package unit suite: **10 passed**, zero failures.
- Navigation and theme suites: **18 passed**, zero failures.
- Website suite: **55 passed**, zero failures.
- Strict TypeScript/Vite production frontend build: **exit 0**.
- Desktop/mobile browser suite: **22 passed**, zero failures (49.6 seconds).
- Manual negative controls: **3/3 killed** — removed focus glow, disabled container queries, removed high-contrast focus. Each compiled mutant ran and failed the expected behavioral test; sources restored in `finally`; final verification rebuilt restored sources.
- Source whitespace, Node syntax and Python compilation checks: **exit 0**. Generated Vite output is excluded from whitespace checks because vendored template/grammar literals legitimately contain whitespace.

Measured wide row height: **34px** for Rules, Skills and Memory. Narrow fixtures: Rules/Skills approximately **128px**, Memory approximately **122px**, with labelled metadata and visible actions. Original wide fixture was **73.39px**. Narrow panels intentionally use more vertical space instead of horizontal scrolling.

All seven current screenshots were generated from the freshly built local frontend, including focus glow and wide/mobile Rules, Skills and Memory. Visual inspection confirmed the pale header logo, separated headings, a subtle focus glow, and complete mobile header buttons.

## Failures resolved and review

Initial test setup used a screenshot-era placeholder that differed from current source; changed to the observed accessible combobox name before the meaningful RED run. RED then reproduced 2px outlines, 2px high-contrast outlines, and 73.39px wide rows. Further RED checks caught long tags spilling into adjacent columns, medium grids too narrow for their fixed columns, cramped headings, clipped mobile actions, the general link rule recoloring the logo, and wrapper-induced loss of button accent styling.

One mobile geometry check initially treated native scrolling to an offscreen control as a layout shift. The test now brings the target into view before measurement; its full bounding-box equality assertion is unchanged. Final fresh verification passed.

| Round | Skills | Findings | Fixed |
|---|---|---|---|
| 1 | old-coder, TypeScript, React, JavaScript, Python usable; CSS missing, generic CSS/accessibility review | Long metadata overflow, duplicate wrapper ring, logo ink override | Column/tag constraints, one container glow, wordmark excluded from general link skin |
| 2 | Same | Heading widths and mobile action clipping | Readable column widths, 675px narrow breakpoint, paired/wrapping actions |
| 3 | Same | Wrapper selectors missed button skin; focus measurement included native scrolling | Header skin recognizes action pairs; target scrolled into view before unchanged geometry assertion |
| 4 | Same | **0 open findings** | — |

## Runtime and limits

Node 24.19.0, TypeScript 5.9.3, Vite 5.4.21, React 18.3.1, Python 3.12.14 and existing Playwright/Chromium. No new packages or runtime permissions. Preview runner allocates an unused loopback port and checks that served HTML equals the freshly built index. API fixtures replace network boundaries, not the frontend under test.

Google's reference page presented an unusual-traffic verification challenge, so its exact implementation was not inspected. The requested soft glow follows the user's description and supplied Ax screenshot. No live admin deployment or verification, native backend/Docker rebuild, or Mac/Windows runtime validation performed. The owner must rebuild/restart Ax to expose the new embedded UI.

No configured changed-line coverage threshold or separate CSS linter; no numeric coverage claim. Explicit breakpoint cases replace property testing for this static layout change. Unrelated backend/browser suites were not rerun. Independent agent verification not performed. These checks are functional evidence, not absolute proof.
