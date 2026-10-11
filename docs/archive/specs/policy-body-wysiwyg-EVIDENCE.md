# EVIDENCE — WYSIWYG mode and `[[` link picker for rule and skill bodies

Spec: `docs/specs/policy-body-wysiwyg.md` (Revision 2 approved: "Yes, build it (picker in both views)"; Revision 3 records changes found during implementation).
Tier 2 (UI feature plus one additive API field). Source state: HEAD `b7caeb1` plus the uncommitted working tree (no commits, per the user). All numbers below come from one final run after the last code edit.

## Behaviour to test

| Spec | Test |
|---|---|
| W1 three views, remembered | `policyBodyView.test.ts` (loads `wysiwyg`); e2e `W1 three views, the choice is remembered` |
| W2 formatting saved as Markdown | e2e `W2 toolbar bold writes **`, `W2 Cmd+I writes *`, and `W2/W4 a real edit…` |
| W3 no rewrite without an edit | e2e `W3 opening in WYSIWYG does not rewrite the body` (with a body TipTap would normalise) |
| W4 links, comments, tables, code and tasks survive | e2e `W2/W4 a real edit…`: 9 exact substrings plus a whitespace-tolerant table pattern |
| W5 correct body after a switch | e2e `W5 switching items in WYSIWYG…` and `W5 a restored revision…` |
| W6 dark theme, WCAG, keyboard | CSS uses theme variables; kind badges are outlined (`#8fb4ff`, `#d9b3ff`, `#f0c674` on `#181818`, all above 4.5:1); toolbar buttons have `aria-label` and `aria-pressed`; the picker uses `aria-controls`, `aria-expanded` and `aria-activedescendant` (e2e asserts them in the Markdown view) |
| W7 picker in both views | unit: `filterLinkTargets` (id or title, case-insensitive, current item excluded, max 20), `linkText`, `openLinkQuery`, `isLinkTarget`; Rust `w7_link_target_resolves_back_to_the_item`; API test `g1_…` (`target` on memory, project and global nodes); e2e: Markdown-view insert, Esc closes, WYSIWYG-view insert, "Links unavailable" on HTTP 500 |
| N1 Markdown and Preview unchanged; same Save format and API | existing specs `policy-graph.spec.ts` pass; the API change is one additive field |
| N2 no new network calls | the picker reads only the existing `GET /api/links/graph`, once per page |

## Gauntlet (final run)

| Layer | Command | Result |
|---|---|---|
| Unit (web) | `node --experimental-strip-types --test src/lib/linkPicker.test.ts src/lib/policyBodyView.test.ts` | 16/16 pass |
| Rust | `cargo test -p ax-policy` | 223 passed, 0 failed |
| API | `cargo test -p ax-web --test links_api` | 7/7 pass |
| Types | `npx tsc -b --noEmit` | exit 0 |
| Production build | `npm run build` | exit 0 |
| Browser | `playwright test e2e/policy-wysiwyg.spec.ts e2e/policy-graph.spec.ts e2e/blade-dismiss.spec.ts --project=system-chrome` | wysiwyg 11/11, policy-graph all pass; blade-dismiss has 6 pre-existing failures (below) |
| Mutation (manual) | `scripts/wysiwyg-mutants.sh` | **16/16 killed** |
| Lint | `cargo clippy -p ax-policy -p ax-web --all-targets` | 0 warnings in changed files; `-D warnings` stops in the untouched `ax-types` crate (pre-existing) |
| Supply chain | `npm audit --omit=dev` | 0 vulnerabilities |
| Real run | reinstall, `ax web`, `curl /` gives 200; the served bundle `index-DSM5DTYt.js` matches `dist/index.html`; `/api/links/graph` has 249/249 nodes with `target` (for example `memories/<title>`, and bare `english-comments-in-code-only` for a unique global rule) |

### Mutant script

The script is fail-closed:
- it requires every suite to pass before mutating;
- each mutant must change its file (checked with `cmp`), or the script stops;
- browser mutants run against a Vite dev server that proxies `/api` to `ax web`.

Mutants cover the self-exclusion, the 20-row cap, title matching, the open-query boundaries, the link text, the stored view, `link_target` (bare name and memory prefix), wikilink and HTML-comment serialisation, no emit on mount, the initial content, Enter-to-insert, `aria-activedescendant`, the payload guard, and Esc staying closed.

The script had a bug, which I fixed. Restoring a file with `mv` put back an mtime older than the mutant build, so cargo reused the mutant binary. That produced a false "baseline fails". The script now runs `touch` on each file it restores.

## Pre-existing failures (baseline)

`e2e/blade-dismiss.spec.ts` fails in 6 tests: click-away and re-click on `/memory`, `/logging`, `/nodes` and `/unresolved`. With every changed file reverted to its pre-change content, served through Vite, the same 6 fail. The six files were then restored and checked byte-identical with `cmp`. None of those pages mount a changed component. Not fixed (out of scope).

A related existing issue: mouse-clicking **Restore** in the History modal counts as a click outside the blade and closes it. The W5 restore test presses Restore with the keyboard for that reason.

## Deviations from the spec's plan

- Unit tests use `node:test`; the repo has no vitest.
- The Markdown round-trip fixtures run in Playwright, because TipTap needs a DOM.
- `@tiptap/extension-list` provides the task list (TipTap v3), instead of separate task-list packages.
- Revision 3 changes:
  - W3 no longer mentions Save, because Save is never disabled on open;
  - W4 accepts padded table columns;
  - W5 gained two concrete checks.
- Dead code removed. A `value`-to-editor sync effect could never run: every body change unmounts the card while it loads. A mutant survived on it and was classified dead rather than killed with a contrived test. `WysiwygEditor` now documents that it reads `value` once at mount, and the W5 tests prove both real replacement paths still work.

## Review rounds

| Round | Skills (status) | Findings | Fixed |
|---|---|---|---|
| 1 | `old-coder`, `old-coder-api`, `rust-review`, `typescript-review`, `react-review` usable | 5 (1 major, 4 minor): R1-1 ref written during render and setState inside updaters; R1-2 unvalidated graph payload; R1-3 picker not linked to its input for assistive tech; R1-4 component file too large, nodes moved out; R1-5 misleading strikethrough icon | all; R1-2 via RED test `isLinkTarget`, R1-3 via an e2e assertion plus a mutant |
| 2 | same | 1 minor: R2-1 `as` casts without a reason | fixed |
| 3 | same | 0 | — |

API gates for `/api/links/graph`:
- ✓ Boring;
- ✓ Compatibility (one additive field);
- N/A Authentication and Authorization (a local, loopback-only read, unchanged);
- N/A Idempotency (a read);
- ✓ Blast radius (same cost as before: one resolver index per request);
- N/A Pagination (bounded local policy set, unchanged);
- ✓ Expensive fields;
- ✓ No implementation leakage (`target` is the user-facing link text).

## Files

New:
- `src/components/WysiwygEditor.tsx`, `src/components/LinkPicker.tsx`;
- `src/lib/linkPicker.ts` with its test, `src/lib/wysiwygNodes.ts`;
- `e2e/policy-wysiwyg.spec.ts`;
- `scripts/wysiwyg-mutants.sh`.

Changed:
- `MarkdownEditor.tsx`, `PolicyBodyCard.tsx`, `Policy{Rule,Skill}InlineWorkspace.tsx`;
- `lib/policyBodyView.ts` with its test, `index.css`;
- `crates/ax-policy/src/links.rs` (`LinkIndex::link_target`);
- `crates/ax-web/src/links_api.rs` (`target` field) with its test;
- docs: `site/.../guides/command-center.md`, `README.md`.

Spec approval: obtained for Revision 2; Revision 3 is shown to the user with this report.
