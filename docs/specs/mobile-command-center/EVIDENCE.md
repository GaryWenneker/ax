# Mobile Command Center verification

Verified on 2026-10-13 against the production frontend bundle and public synthetic API/SSE fixtures. The implementation is rebased onto `8235d81`, retaining the Logging improvements from PR #55. No runtime dependency was added.

## Result

Navigation, filters, sorting, Settings selection, help, status panels, Review comparisons and period controls use mobile drawers or sheets. Inspectors use the available viewport with their own scrolling and close action. Forms keep actions reachable, lists expose touch actions, and Markdown editing uses full-width source/preview tabs. Desktop controls retain their existing presentation.

| Check | Result |
| --- | --- |
| TypeScript and Vite production build | Pass |
| Expanded frontend unit suite | 173 pass |
| Website unit tests | 55 pass |
| Desktop Playwright regression suite | 30 pass |
| Mobile Playwright interaction suite | 12 pass |
| Route and Settings layout audit | 132 pass: 33 targets at 320, 390, 768 and 1280 pixels |
| Audit errors, failed responses, clipping and page overflow | Zero |
| Documentation audit | Pass |
| Mobile documentation screenshots | 16 fresh captures, isolated contexts, image and bundle hashes in the manifest |
| Manual mutation checks | 3 of 3 killed: Escape dismissal, inspector width, form focus containment |
| Source/documentation whitespace check | Pass; generated vendor bundle excluded |

Run the full local verification with:

```sh
AX_CHROME_EXECUTABLE=/path/to/chrome-headless-shell node docs/specs/mobile-command-center/verify.mjs
AX_CHROME_EXECUTABLE=/path/to/chrome-headless-shell node docs/specs/mobile-command-center/mutate.mjs
```

The browser is Chromium 138, with Playwright 1.62.0, React 18.3.1 and TypeScript 5.9.3. `verification-results.json` records each verification command and exit code; `final/results.json` records every audited target. The audit excludes intentionally scrollable content and collapsed details. Its deliberate 400-pixel overflow negative control exits with failure. Unit tests are bundled with the existing esbuild dependency and their sibling source files; their assertions are unchanged.

## Interaction evidence

The mobile suite covers all 14 Settings sections, filter persistence, sorting, full-screen Nodes/Memory/Logging/Rules/Skills details, Files previews, Graph controls/settings, help, the navigation drawer, Logging filters, activity and quality sheets, project selection, small-screen forms, backdrop dismissal, Review comparisons, Savings periods and Markdown/Preview draft preservation. Keyboard focus containment, Escape, focus restoration, inert hidden navigation and minimum touch targets are checked.

The original five mobile regression tests failed against the original UI before implementation. The final Review/Savings and editor tests failed against the earlier compiled `40c8673` UI, then passed after their fixes. Mutation checks compile each altered implementation, require the matching browser test to fail, and restore the original source and production bundle.

## Review record

| Review | Findings resolved |
| --- | --- |
| Initial functional and stack review | Logging Close selected the wrong button; Files/Graph overlays were missing from adaptation; callback refs were written during render; desktop Graph toolbar overflowed. |
| Route and visual review | Memory rows were unreachable inside a fixed-height layout; the status footer overlaid inspectors; screenshot contexts leaked selected rows; intermediate generated assets remained. |
| Review after rebasing | Preserved Logging marker gutters and time/context placement from #55; gave mobile tests their own 10-event fixture while screenshots retain the expanded 15-event fixture. |
| Final visual review | Savings heading overlapped controls; Review filters/comparisons were inline; editor source occupied half the screen; Memory metrics needed wrapping. Added regression coverage and inspected the new captures. |
| Final review | No unresolved findings in the changed UI under the tested fixture states. |

## Limits and baseline failures

The requested live URL, `https://admin.wenneker.dev`, was attempted with Playwright and the cloud browser. Access failed with `net::ERR_EMPTY_RESPONSE` and `net::ERR_BLOCKED_BY_CLIENT`, respectively. These results verify the local frontend with synthetic data; they do not verify a deployed server, authentication, backend writes, permissions, physical-device keyboards or WebKit/Safari.

Additional legacy `app-feel.spec.ts` and `blade-dismiss.spec.ts` probes produced 11 failures and 18 passes. The same 11 failures reproduced against the unchanged `01602b5` compiled baseline. Their assertions were not relaxed. Mobile hover/outside-click assumptions are covered by the explicit mobile sheet/dialog contract in the new suite instead.

Source-mapped changed-line coverage and lint were skipped: this repository does not configure a frontend coverage runner or ESLint/stylelint. Mutation checks provide focused sensitivity evidence, not a coverage percentage. The whitespace check excludes Vite-generated `dist`, whose vendor output contains whitespace inside generated text. No backend, Docker or Netlify build/deployment ran. GitHub workflows remain manual-only; no workflow was dispatched. Publication and merge update repository source and the checked-in frontend bundle; deployment is owned by the repository owner.

See [FEATURE-MATRIX.md](FEATURE-MATRIX.md) for the audited surfaces and [SPEC.md](SPEC.md) for acceptance criteria.
