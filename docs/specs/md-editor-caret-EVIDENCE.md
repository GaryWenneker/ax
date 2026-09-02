## Evidence Report — Markdown source caret sync (Tier 2)

- Spec approval: obtained — user pasted `/Users/gary/io/ax/docs/specs/md-editor-caret.md` as the task contract (2026-09-01 revision including selection-ghosting)
- Source state: git `HEAD` `b0676ba29130ed93ea1e8f9db1db9ee5f22e5f82` plus uncommitted working-tree changes (this task). Re-run from that tree with the entry point below
- Toolchain: TypeScript 5.9.3 (`npx tsc --version` in `crates/ax-web/web-ui`), Playwright 1.62.0, `ax 4.6.0`
- Entry point: `bash /Users/gary/io/ax/tools/gauntlet-md-editor-caret.sh` (Windows: `tools/gauntlet-md-editor-caret.ps1` — tsc + Playwright; the bash script also runs the manual mutant)
- Independent verification: not performed (Tier 2)

Served bundle after rebuild: `index-BrX5xtho.css` on `http://127.0.0.1:7070` matched `crates/ax-web/web-ui/dist/index.html`.

Installed binaries (sha256 `f3579cc5d44e0ce10f6858fa82d135730944b20a98f741755962328824a36acc`):

- `/Users/gary/io/ax/target-dev/release/ax`
- `/Users/gary/.cargo/bin/ax`
- `/Users/gary/.local/bin/ax` (Cursor MCP `command`)

`MarkdownEditor.tsx` was not changed (CSS-only).

### Spec → Test mapping

| Scenario | Test | Status |
|---|---|---|
| overlay code and textarea share font metrics | `e2e/md-editor-caret.spec.ts` → `overlay code and textarea share font metrics` | **pass** |
| click the last source line places the caret at the document end | `click the last source line places the caret at the document end` | **pass** |
| click after a long wrapping bold line does not land mid-word | `click after a long wrapping bold line does not land mid-word` | **pass** |
| drag-select a wrapping paragraph does not ghost a second copy | `drag-select a wrapping paragraph does not ghost a second copy` | **pass** |

Must-not constraints (save/load/frontmatter, preview HTML, npm/Cargo deps, syntax colors, `--fs-base` / `--ui-scale`, leftover probe scripts): no implementation change outside overlay CSS + docs + this e2e/gauntlet; **n-a** as automated assertions except “no probe scripts” (tree has no `tmp-caret-probe.mjs`).

### Gauntlet layers (fresh run after last code edit)

Command: `bash /Users/gary/io/ax/tools/gauntlet-md-editor-caret.sh`

| Layer | Command / check | Result |
|---|---|---|
| Static types | `cd crates/ax-web/web-ui && npx tsc --noEmit` | exit 0 |
| Playwright (4 scenarios) | `npx playwright test e2e/md-editor-caret.spec.ts --project=desktop-chrome` | **4 passed (1.6s)** |
| Manual mutant | inject `pre > code { font-size: 14px; line-height: 18px }` then compare computed sizes | **killed**: overlay `14px` vs textarea `13px` |
| RED (pre-fix, same spec file) | metrics `14px` vs `13px`; wrap click `selectionStart` 204 vs expected 194; `::selection` fill not transparent | observed fail before CSS; not re-run after restore |
| Full cargo/npm unit suite | — | **skipped**: CSS + Playwright contract; no new TS production modules |
| Changed-line coverage | — | **skipped**: no CSS coverage gate in this repo; e2e exercises computed style + caret + selection |
| Property tests | — | **skipped**: not a parser/math surface |
| Lint/format | — | **skipped**: CSS-only + existing Playwright style; `tsc --noEmit` is the type gate |
| Supply chain | — | **skipped**: no new dependencies |
| Real execution | live `ax web --port 7070` during Playwright | **pass** (tests hit `/policy/rules/edit`) |
| Suite health / shuffle | — | **skipped**: four serial Playwright tests, 1 worker |

Checker negative control: the metrics scenario failed on the live bug (`code` 14px vs textarea 13px). The gauntlet mutant re-introduces that 14px overlay rule and exits 1 if sizes match.

### Fix (what the tests lock)

uiw paints glyphs on `pre > code` (library `14px` / `18px`) and the caret on `textarea`. Theme CSS now sets the same `font-size` (`--fs-base` × `--ui-scale`), `line-height: 1.5`, mono family, wrap, and `font-weight: 400` on text/pre/code/textarea. `::selection` and `::-moz-selection` are **separate** rules (a grouped selector is dropped by Chromium). Textarea `color` / `-webkit-text-fill-color` stay transparent with `caret-color: var(--text)` so selected glyphs do not ghost over the overlay.

### Known limits

- Playwright click tests use a 1280×800 viewport; wrap still depends on pane width in the editor chrome.
- `getComputedStyle(textarea, '::selection')` is only meaningful after a valid `::selection` rule (not grouped with `::-moz-selection`).
- Independent verification was not run.

### Confidence

Tier 2 with human-authored SPEC and a rerunnable gauntlet. Residual risk: other viewports or uiw upgrades that add a more specific overlay font rule.
