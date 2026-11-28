# Evidence: Command Center UI (fix/review-web-ui)

Scope: `crates/ax-web/web-ui`, plus the CI browser job, two site guide pages and the site design test that read the split stylesheets. Base commit: `9dca60e4`. This PR changes no Rust source, so it owns no crate for `cargo test` or `cargo clippy`.

## Findings

States follow the spec: fixed (RED then GREEN, or the named check when no failing test is possible), not a defect, or deferred for an owner decision. Tests are the test files each fix commit added or changed; paths are relative to `crates/ax-web/web-ui`.

| Finding | State | Tests | Commit |
|---|---|---|---|
| web-ui-001 | fixed | `e2e/logging-text.spec.ts`, `src/e2eHygiene.test.ts` | 8f81a53a |
| web-ui-002 | fixed | `e2e/titlebar-wave.spec.ts`, `src/edgeGlow.test.ts` | 76e6f060 |
| web-ui-003 | fixed | `src/streamGraph.test.ts` | 6576ffa0 |
| web-ui-004 | fixed | `e2e/auth-gate-transient.spec.ts`, `src/lib/authSession.test.ts` | cbf3e06a |
| web-ui-005 | fixed | `e2e/continuity-draft.spec.ts`, `src/lib/continuity.test.ts` | 46bb3594 |
| web-ui-006 | fixed | `e2e/file-preview-race.spec.ts` | 0f7e97fa |
| web-ui-007 | fixed | `e2e/file-tree-filter-expansion.spec.ts` | c1c27487 |
| web-ui-008 | fixed | `e2e/project-switch-failure.spec.ts` | 31a80fb4 |
| web-ui-009 | fixed | `e2e/logging-trace-cap.spec.ts`, `src/lib/mcpTrace.test.ts` | 45de95da |
| web-ui-010 | fixed | `e2e/logging-history-failure.spec.ts` | d38df42c |
| web-ui-011 | fixed | `e2e/policy-graph-unsaved.spec.ts` | 44b234e4 |
| web-ui-012 | fixed | `e2e/policy-property-name.spec.ts` | 284ff128 |
| web-ui-013 | fixed | `e2e/policy-comma-values.spec.ts`, `src/lib/policyList.test.ts` | e4d64a67 |
| web-ui-014 | needs owner decision | none: fix belongs to ax-share/ax-web; owner must choose token storage, migration and API shape | - |
| web-ui-015 | fixed | `e2e/policy-restore-diff-race.spec.ts` | d123a9e9 |
| web-ui-016 | fixed | `e2e/workspace-request-failures.spec.ts`, `src/workspaceApi.test.ts` | 56da570f |
| web-ui-017 | fixed | `e2e/workspace-request-failures.spec.ts`, `src/workspaceApi.test.ts` | 56da570f |
| web-ui-018 | fixed | `e2e/remote-mcp-login-poll.spec.ts` | 5b2d37b2 |
| web-ui-019 | fixed | `src/lib/replicationErrors.test.ts` | f69a6c39 |
| web-ui-020 | fixed | `src/components/agent/ptyReconnect.test.ts` | b2fc23d9 |
| web-ui-021 | fixed | `src/components/agent/agentTerminalModel.test.ts` | 96e9d460 |
| web-ui-022 | fixed | `src/components/agent/agentTerminalModel.test.ts` | 96e9d460 |
| web-ui-023 | fixed | `src/components/ui/NewArrivalPill.test.ts` | b5cc9934 |
| web-ui-024 | fixed | `site/tests/design-coherence.test.ts`, `src/components/continuityPanel.test.ts`, `src/edgeGlow.test.ts`, `src/globalStyles.test.ts`, `src/lib/policyBladeMotion.test.ts`, `src/listScroll.test.ts`, `src/liveChase.test.ts`, `src/pages/specWorkspacePage.test.ts`, `src/pages/specsPage.test.ts` | 2255e55f |
| web-ui-025 | fixed | `src/cssTokens.test.ts` | ba34c704 |
| web-ui-026 | fixed | `src/cssTokens.test.ts` | ba34c704 |
| web-ui-027 | fixed | `src/gitShare.test.ts`, `src/lib/nav.test.ts`, `src/testScript.test.ts` | d974a79b |
| web-ui-028 | fixed | `src/lib/policyProperties.test.ts` | 9fadc369 |
| web-ui-029 | fixed | `src/lib/routes.test.ts`, `src/lib/sourceHighlight.test.ts`, `src/lib/symbolTree.test.ts`, `src/pipelineState.test.ts`, `src/smallHelpers.test.ts` | 36414ce0 |
| web-ui-030 | fixed | `src/lib/sharedEventSource.test.ts` | 60ab254b |
| web-ui-031 | fixed | `src/lib/sharedEventSource.test.ts` | e344fc29 |
| web-ui-032 | fixed | `src/lib/fileTree.test.ts` | 5eaa23a2 |
| web-ui-033 | fixed | `src/lib/graphAsOf.test.ts` | 0e99c1b9 |
| web-ui-034 | fixed | `src/lib/specSession.test.ts`, `src/memoryRefresh.test.ts` | e809f273 |
| web-ui-035 | fixed | `src/lib/policyRowKeys.test.ts` | 62b1c737 |
| web-ui-036 | fixed | `src/lib/latestLoad.test.ts` | c0cd3e8e |
| web-ui-037 | fixed | `e2e/ship-config-load.spec.ts`, `src/shipApi.test.ts` | 55adbf84 |
| web-ui-038 | not a defect | none: pages/SonarQube.tsx is never rendered (App.tsx redirects /sonar to Stats); proxy risk is server-side | - |
| web-ui-039 | fixed | `e2e/specs-unsaved.spec.ts` | f7867eb7 |
| web-ui-040 | fixed | `src/wikilinks.test.ts` | c2f7a69c |
| web-ui-041 | fixed | `e2e/graph-obsidian.spec.ts`, `e2e/policy-graph.spec.ts`, `src/lib/graphProbe.test.ts` | 8d08c76a |
| web-ui-042 | fixed | `e2e/access-workflows.spec.ts`, `e2e/column-accessibility.spec.ts`, `e2e/control-spacing.spec.ts`, `e2e/focus-row-density.spec.ts`, `e2e/list-columns.spec.ts`, `e2e/remote-mcp-publish.spec.ts`, `e2e/replication-setup.spec.ts`, `e2e/specs-unsaved.spec.ts`, `src/e2eHygiene.test.ts` | a1ee447b |
| web-ui-043 | fixed | `e2e/logging-layout.spec.ts`, `src/lib/columnModel.test.ts` | 639a4284 |
| web-ui-044 | fixed | `e2e/memory-files.spec.ts` | cb09228d |
| web-ui-045 | fixed | `e2e/mobile-smoke.spec.ts` | 82c40159, 361865e1 |
| web-ui-046 | not done (partly) | CI now runs the 36 mocked specs against vite preview (3208713d); no backends.ts or webServer split yet | 3208713d |
| web-ui-047 | fixed (with web-ui-103) | `src/lib/chunkReload.test.ts`, `src/lib/sourceHighlight.test.ts` | ceb98c00 |
| web-ui-048 | fixed | `src/lib/routes.test.ts` | 10c7d876 |
| web-ui-049 | fixed | `src/apiRequest.test.ts`, `src/components/agent/agentTerminalModel.test.ts`, `src/lib/mcpReload.test.ts` | 4ae65b99 |
| web-ui-050 | fixed | tsc --noEmit, npm test (dead code removed) | 30309e8a |
| web-ui-051 | fixed | `src/lib/sse.test.ts`, `src/sseStreams.test.ts`, `src/workspaceApi.test.ts` | 77c592f5 |
| web-ui-052 | fixed | `e2e/access-settings.spec.ts`, `src/components/accessSettings.test.ts` | 0412132b, b459b85c |
| web-ui-053 | fixed | `src/lib/activityStreaks.test.ts` | a36a93e4 |
| web-ui-054 | fixed | `src/components/forceGraphCanvas.test.ts` | 25e248ce |
| web-ui-055 | fixed | `src/lib/graphDrag.test.ts` | 9814cf8d |
| web-ui-056 | fixed | `src/lib/hoverTip.test.ts` | 00d711d9 |
| web-ui-057 | fixed | `src/lib/refreshingCache.test.ts` | b25bda19 |
| web-ui-058 | fixed | `src/lib/itemLinks.test.ts` | 81dae386 |
| web-ui-059 | fixed | `src/lib/rowNeighbors.test.ts` | d185c4ab |
| web-ui-060 | fixed | `src/lib/lruCache.test.ts` | fd1aa1a1 |
| web-ui-061 | fixed | `src/memoryDetail.test.ts` | b7745900 |
| web-ui-062 | fixed | `src/lib/frameCoalesce.test.ts` | c64f6069 |
| web-ui-063 | fixed | `src/lib/shareSettings.test.ts` | e3a7a11e |
| web-ui-064 | fixed | `src/lib/shareSettings.test.ts` | e3a7a11e |
| web-ui-065 | not done (partly: two parts committed) | `src/lib/mouseDrag.test.ts`, `src/lib/traceKindBadge.test.ts` | 973da4f8, ffc2a65a |
| web-ui-066 | fixed | `e2e/project-browser-browse.spec.ts`, `src/lib/projectBrowse.test.ts` | dd714784 |
| web-ui-067 | fixed | `src/components/remoteHostSetup.test.ts` | fb505ab7 |
| web-ui-068 | fixed | `src/lib/safeNavigation.test.ts` | b44e583c |
| web-ui-069 | fixed | `src/components/settingsSourceShape.test.ts`, `src/lib/replicationAdvertise.test.ts` | 09eebd78, 90d1d826 |
| web-ui-070 | fixed | `src/lib/settingsAction.test.ts` | 48673df4 |
| web-ui-071 | fixed | `src/lib/specCatalog.test.ts` | f3c56740 |
| web-ui-072 | fixed | `src/lib/policyAudit.test.ts` | fa87d70c |
| web-ui-073 | fixed | `src/lib/skillCatalog.test.ts` | 042f3446 |
| web-ui-074 | fixed | `src/lib/tokenPath.test.ts` | a6345e88 |
| web-ui-075 | fixed | `src/lib/workspaceSwitch.test.ts` | 170145f1 |
| web-ui-076 | fixed | `e2e/continuity-panel.spec.ts`, `e2e/project-switch-trash.spec.ts`, `src/components/continuityPanel.test.ts`, `src/components/projectSwitchChrome.test.ts` | b9f2490a |
| web-ui-077 | fixed | `e2e/label-filter-keyboard.spec.ts` | 473b0804 |
| web-ui-078 | fixed | `e2e/column-filter-listeners.spec.ts` | 2988f4bb, 8c92dc54 |
| web-ui-079 | fixed | dead code removed; `tsc --noEmit` (theme type now requires portal layout), `src/lib/themes.test.ts` | 5b1e3a76 |
| web-ui-080 | fixed | `src/edgeGlow.test.ts`, `src/lib/manifestAuthority.test.ts` | 909c4671 |
| web-ui-081 | fixed | `src/styleHygiene.test.ts` | 573e6cac |
| web-ui-082 | fixed | `src/pages/specWorkspacePage.test.ts`, `src/styleHygiene.test.ts` | c0aeef97 |
| web-ui-083 | fixed | `src/styleHygiene.test.ts` | 33dd860d |
| web-ui-084 | fixed | `src/styleHygiene.test.ts` | b971da0e |
| web-ui-085 | fixed | `src/styleHygiene.test.ts` | bc966b96 |
| web-ui-086 | no commit; state not recorded | - | - |
| web-ui-087 | no commit; state not recorded | - | - |
| web-ui-088 | fixed | `src/styleHygiene.test.ts` | 0fee5d64 |
| web-ui-089 | fixed | `src/styleHygiene.test.ts` | f09c2c48 |
| web-ui-090 | no commit; state not recorded | - | - |
| web-ui-091 | fixed | `src/lib/accessApi.test.ts` | f077bfd2 |
| web-ui-092 | fixed | duplicate types removed; `tsc --noEmit` | 5b8b50fe |
| web-ui-093 | fixed | `src/lib/graphObsidian.test.ts` | d4d1e75e |
| web-ui-094 | fixed | `src/lib/loggingPresentation.test.ts` | 918cb348 |
| web-ui-095 | fixed | `src/lib/traceEntryMemo.test.ts` | b35abbe4 |
| web-ui-096 | fixed | `src/lib/traceClassify.test.ts` | afefd76b |
| web-ui-097 | fixed | `src/lib/mcpTrace.test.ts` | 0640a50b |
| web-ui-098 | fixed | `src/lib/placePopover.test.ts` | 5298c032 |
| web-ui-099 | fixed | `src/lib/policyBodyView.test.ts` | 07389d93 |
| web-ui-100 | no commit; state not recorded | - | - |
| web-ui-101 | fixed | `src/lib/routes.table.test.ts`, `src/lib/routes.test.ts` | b8e3ae5f |
| web-ui-102 | fixed | `src/lib/sharedEventSource.test.ts` | e2dd2dfa |
| web-ui-103 | fixed | `src/lib/chunkReload.test.ts`, `src/lib/sourceHighlight.test.ts` | ceb98c00 |
| web-ui-104 | fixed | `src/lib/policyBladeMotion.test.ts`, `src/lib/specSession.test.ts`, `src/listScroll.test.ts`, `src/savingsPortal.test.ts`, `src/tagOneline.test.ts` | 6be86bb1 |
| web-ui-105 | fixed | `src/lib/specWorkspace.test.ts` | 82c981b5 |
| web-ui-106 | fixed | `src/lib/symbolTree.test.ts` | 0534afb6 |
| web-ui-107 | fixed | `src/lib/themes.test.ts`, `src/policyPackage.test.ts` | b63f423b |
| web-ui-108 | fixed | `src/lib/unifiedDiff.test.ts` | 56e0f8c5 |
| web-ui-109 | fixed | `src/lib/live.test.ts` | 1a535506 |
| web-ui-110 | fixed | `src/smallHelpers.test.ts` | 2e14b4d4 |
| web-ui-111 | fixed | `src/lib/pwaOptIn.test.ts` | 41bb5fb6 |
| web-ui-112 | fixed | `src/memoryConversation.test.ts`, `src/memoryRowCounts.test.ts` | 566e028d |
| web-ui-113 | fixed | `e2e/graph-stale-load.spec.ts` | 5db01624 |
| web-ui-114 | fixed | `e2e/graph-selection-idle.spec.ts` | a0f218ca |
| web-ui-115 | fixed | `src/lib/recallGate.test.ts` | 8ac51c19 |
| web-ui-116 | fixed | `src/lib/policyEditorLoad.test.ts` | 76b4b184 |
| web-ui-117 | fixed | `src/lib/policyActions.test.ts` | 7e5664a2 |
| web-ui-118 | fixed | pure move; `tsc --noEmit`, npm test, mocked e2e set | 3f5ae4d6 |
| web-ui-119 | fixed | `src/lib/policyFolderDraft.test.ts` | b1875b69 |
| web-ui-120 | fixed | `src/lib/clickableRow.test.ts` | 6acb9d2b |
| web-ui-121 | fixed | `src/lib/latestLoad.test.ts` | 9cd8c8d7 |
| web-ui-122 | fixed | uses the latest-gate covered by `src/lib/latestLoad.test.ts` | 63c14b28 |
| web-ui-123 | fixed | `src/apiRequest.test.ts`, `src/lib/latestSaver.test.ts` | 014c3a40 |
| web-ui-124 | fixed | `src/lib/sonarFrame.test.ts` | cfdc0a3e |
| web-ui-125 | fixed | `src/pageApis.test.ts`, `src/pages/specsPage.test.ts` | edda4e75 |
| web-ui-126 | fixed | `src/lib/specDocLoad.test.ts`, `src/pages/specsPage.test.ts` | ebe108a3 |
| web-ui-127 | fixed | `src/pages/savingsMetrics.test.ts` | f6258c55 |
| web-ui-128 | fixed | `src/pages/savingsMetrics.test.ts` | f6258c55 |
| web-ui-129 | fixed | `e2e/specs-page.spec.ts`, `src/lib/specsPageModel.test.ts`, `src/pages/specWorkspacePage.test.ts`, `src/pages/specsPage.test.ts` | 700888be |
| web-ui-130 | fixed | `src/pipelineState.test.ts` | 8f654ed6 |
| web-ui-131 | fixed | `src/apiFetch.test.ts` | b63a4454 |
| web-ui-132 | fixed | `src/postSse.test.ts` | 7f0f6e58 |
| web-ui-133 | fixed | `src/apiFetch.test.ts`, `src/vaultMount.test.ts` | 910c24f6 |

Notes on single findings:

- **web-ui-014** (share token in plain text in `ax.json` and returned unredacted by `GET /share/config`): the fix belongs to the ax-share and ax-web server crates and needs owner choices: where the token lives (OS keychain or a 0600 file under `~/.ax`), whether tokens already in `ax.json` are migrated on load or on the next save, and whether the API returns `tokenSet: true` or a masked prefix. The UI change after that is small (a write-only field with Replace and Clear).
- **web-ui-038**: `pages/SonarQube.tsx` is never rendered, because `App.tsx` redirects `/sonar` to Stats. The remaining risk is the server's same-origin Sonar proxy, which strips Sonar's CSP and X-Frame-Options; that belongs to the ax-web server PR.
- **web-ui-046**: CI now runs every spec that mocks all its API calls (36 specs) against the built UI. Not done: a `backends.ts` that classifies every spec as mocked, fixture or live, a Playwright `webServer` for the mocked set, and a hygiene test that every spec is classified.
- **web-ui-047**: done by the same commit as web-ui-103: every page is a `React.lazy` chunk.
- **web-ui-048**: finished in this round. RED: the new routes test failed with `/policy/rules?id=team-style` instead of `/policy/rules?id=team-style&origin=global&projectId=7`. GREEN after `legacyEditRedirect` kept every route field.
- **web-ui-065**: two parts are committed (one badge table for Logging filter chips, which fixed the hook and agent chips reading LOG; one shared mouse-drag hook for the four resize handles). The batch marked the finding as not fully done; the rest of its record was cut off before this step.
- **web-ui-086, web-ui-087, web-ui-090, web-ui-100**: no commit in this branch names them, and the batch results that reached this step were cut off before them, so their state is not recorded here. See open items.

## Review rounds

| Round | Findings | Fixed |
|---|---|---|
| 1 | 3 | 3 |
| 2 | 8 | 8 |
| 3 | 1 | 1 |
| 4 | 0 | 0 |

## Commands run in the final check

All in `crates/ax-web/web-ui` unless noted. Numbers are from this run.

| Command | Result |
|---|---|
| `npm test` (node test runner over `src/**/*.test.ts`) | 870 tests, 870 pass, 0 fail |
| `npx tsc --noEmit` | pass |
| `npm run build` (`tsc && vite build`) | pass |
| Mocked browser set from `.github/workflows/ci.yml` (36 specs) against `vite preview` on 127.0.0.1:5190, `--project=system-chrome` | 127 passed, 0 failed (run twice: before and after the final rebuild) |
| Same set with `--project=desktop-chrome` | not run: the bundled Playwright Chromium is not installed on this machine (all 127 failed at browser launch); system Chrome was used instead |
| `e2e/mobile-smoke.spec.ts -g "fit the phone"`, `--project=mobile-chrome` with system Chrome | 1 passed (failed before 361865e1, see below) |
| `e2e/mobile-smoke.spec.ts`, other 7 tests | 7 failed: they need a running ax web server (`/api/stats` with live data); not counted |
| `GIT_DIR=/nonexistent npx playwright test --list` | 828 tests in 58 files collected |
| `node --experimental-strip-types --test tests/design-coherence.test.ts` (in `site/`) | 2 pass, 0 fail |
| `cargo fmt --all --check` (repo root) | clean |
| `cargo clippy` / `cargo test` | not run: this PR changes no Rust crate |

Fixed during the final check:

- `10c7d876` web-ui-048 (see above).
- `361865e1` the phone-width Logging test answered the trace stream only once, and the status bar used that snapshot before the lazy Logging page connected, so no rows rendered. It now uses `routeTraceStream`, like the other Logging specs.
- `0c193732` rebuilt `dist/`, which the server embeds.

## Open items

1. web-ui-014 needs the owner's decision on share token storage, migration and API shape (server crates).
2. web-ui-038: the same-origin Sonar proxy in ax-web strips Sonar's CSP and X-Frame-Options; needs a server-side choice (separate origin or port).
3. web-ui-046 is only partly done (see above).
4. web-ui-065 is only partly done; confirm what remains.
5. web-ui-086, web-ui-087, web-ui-090 and web-ui-100 have no commit and no recorded state in this branch; their state must be confirmed.
6. Specs that need a live ax web server or live project data were not run. Specs left out of CI because they still fail against the current UI: focus-row-density, list-columns, logging-text, policy-review-density, and the live tests in mobile-smoke.
7. The `desktop-chrome` project could not be run locally (no bundled Chromium); CI installs it.
