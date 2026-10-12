# Compact review and peer compatibility evidence

Base main: `e3093501866f3de5fee9bf357a61ab04498188db`. Isolated branch `fix/review-compatibility-density`. User authorized autonomous continuation and merge; no separate human SPEC approval or independent verifier was represented as performed.

## Live finding

The supplied Cloudflare interface served `index-DWds_loA.js` and `index-DpVx-2vM.css`, still had Agent navigation and displayed v8.1.0. Its first three collapsed review rows measured 136.75px and all 80 items displayed Comparison unavailable. Direct backend JSON inspection was blocked by the browser/network, so its precise backend commit was not established. These observations prove an older frontend, not an exact primary version. Comparison is computed from staged local/current policy; updating only a primary cannot repair missing comparison support in the serving web process. Version labels can match across unreleased builds.

## Behavior → checks

- Compact queue: 80 records with long IDs/descriptions. Collapsed rows measured **72px desktop** and **142px mobile** (touch actions retained). Full Current/Incoming text remains available through Inspect, without horizontal overflow. Updated screenshots are checked into the documentation.
- Legacy/incomplete comparison: one queue-level warning, unavailable Current is explicitly unknown, preview is explicitly partial, and approval is disabled even for an effect-only response without current content. Snapshot guards and partial-failure bulk decisions remain verified.
- Review response: additive `serverVersion`/`comparisonAvailable` metadata. Real CLI-import/HTTP smoke checks metadata, source, restart persistence, stale attribution, conflict 409 and cleanup. Served index/asset bytes match the rebuilt embedded UI.
- Peer transport: optional version/protocol headers on requests/responses plus enrollment version. Same complete protocol continues with a readable app-version warning; missing legacy version becomes unknown. Explicit protocol mismatch or incomplete snapshot stops before applying data. Strict snapshot fields/revisions are unchanged.
- Persisted warning: status survives reopening, clears after matching success and retains the last successful revision after failure. Settings → Replication nodes displays it and warning service state.
- Hostile metadata: bounded safe characters in version/protocol labels; numeric protocol cases 0–15 accept only 3. Unsupported protocol 4 returns conflict in the real authenticated three-node HTTP fixture without changing native policy or graph counts. Existing graph/source/memory/log/conversation exchange and no-feedback checks remain intact.
- No automatic server upgrade, process restart, production website build, release or CI dispatch.

## Final fresh gauntlet

Entry: `sh docs/specs/review-peer-compatibility/verify.sh` with CARGO/RUSTFMT/AX_CHROME_EXECUTABLE configured. Toolchain: Rust/Cargo 1.99; Node 24.19; repo-pinned TypeScript/Vite/Playwright. Source state is the containing Git commit/tree; generated bundle and screenshots are part of that state.

| Layer | Result |
|---|---|
| ax-web affected full library suite | 103 passed |
| CLI replication/server suite | 27 passed |
| Strict Clippy, both affected crates/all targets | passed, no warnings promoted to errors |
| Edited Rust format / Git whitespace checks | passed |
| TypeScript + Vite embedded UI build | exit 0 |
| CLI rebuild + served bundle byte comparison | passed |
| Desktop/mobile Playwright | 30 passed |
| Real CLI/HTTP execution | passed |
| Website tests | 55 passed |
| Manual mutation checks | 3/3 killed by failing tests; original source bytes restored |

Mutants: accept incompatible protocol; hide version mismatch; drop persisted warning. The persisted runner refuses to count compile failures as kills and restores bytes in finally. No dependency changes.

## Review rounds

| Round | Skills/status | Findings | Fixed |
|---|---|---|---|
| 1 | old-coder/API/Rust/TypeScript/React/Python usable; dedicated CSS skill unavailable, generic/browser review | incomplete comparison could imply No current item; effect-only approval; unsafe/raw peer labels | explicit unknown/preview labels, full-field approval guard, bounded peer labels |
| 2 | same | warning fixture targeted Policy sync instead of Replication nodes; whole native export revision was not a stable preservation invariant | corrected route; assert persisted policy content and graph counts, retain conflict and full exchange assertions |
| 3 | same | 0 unresolved findings | — |

The first protocol test was RED because peer_warning was absent. Old UI failed compact-row/global-warning tests. Intermediate Rust edits had a match-brace/type error and the preservation test initially referenced private constants; these were fixed without claiming tests passed. The export-revision test was corrected openly: fresh captures carry dataset metadata, while persistence/data preservation is the intended invariant.

API review: existing HTTP/JSON surfaces, first-party UI and externally deployed peer nodes. Optional headers/response fields preserve legacy omission behavior; existing auth/project/node binding, no redirects, bounded body reads, exact acknowledgements and snapshot validation remain. No endpoints/types were renamed and no strict snapshot fields were added. Read-only metadata has constant cost. Existing review-list pagination/rate-limit limitations were not expanded or claimed solved. Actual mixed-release LAN/native OS/external deployment remains unverified.

## Limits

No Rust changed-line coverage tool, randomized suite, property-based generator or independent verifier ran. Enumerated protocol cases, real transport tests and mutations provide narrower evidence; independent verification is declared **not performed** for Tier 3. Native Mac/Windows and real LAN were not available. The historical unrelated WebDAV intermittent failure and optional pricing-request rejection are documented in the preceding spec; broad all-CLI tests were not retried here. Tests used owned temporary projects and isolated pricing caches. No live reviews were approved/rejected and no user service was upgraded or restarted. The shared live process still requires a new build/release and restart; merge alone does not change its binary.
