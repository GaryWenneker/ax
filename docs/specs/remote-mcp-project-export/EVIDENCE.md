# Local validation — 2026-10-12

Validated on Linux, from main `86237398b9f00f996626a8c7a9a7195b58879360`.

| Check | Result |
|---|---|
| `cargo test -p ax-global-db -p ax-mcp -p ax-web -p ax-cli --locked` | 545 tests passed |
| Final `cargo test -p ax-mcp -p ax-web remote --locked` | 32 tests passed, including added origin normalization |
| Final `cargo test -p ax-web remote_mcp --locked` | 3 API/login guard tests passed after reusing local owner bootstrap |
| Scoped strict Clippy, all targets, no dependencies | Passed for global-db, MCP, web and CLI; final web/CLI rerun passed |
| Web UI TypeScript + Vite production build | Passed |
| UI theme/contrast and activity model tests | 24 passed |
| Desktop/mobile Playwright sharing and login flows | 22 executions passed; includes generated copies of the three existing login specs |
| Type checking the new sharing browser spec | Passed |
| Documentation tests | 34 passed |
| Local Astro/site production build and share cards | Passed; screenshot assets included |
| `cargo build --release -p ax-cli --locked` | Passed with rebuilt ax-web and current embedded UI |
| Isolated install: executable path + `ax --version` | Verified `ax 8.1.0` |
| Installed release CLI + real owner API + browser | Passed |

The installed-binary smoke test used two initialized fake projects with the same folder name, separate remote IDs and a fake HTTPS publisher. It registered both projects through the new CLI command, then opened one project in the real Command Center. The API returned only that project's publication and generated the correct Cursor public-client-ID configuration. The new page also loaded in embedded IDE mode without browser exceptions. The served HTML was byte-identical to the current frontend build and referenced `/assets/index-CXRn7xoc.js`, which contained the new sharing card.

Security and correctness tests cover canonical roots and symlink aliases, distinct publishers and similar project names, transactional conflicts, import versus full reconciliation, live configuration conflicts, lease takeover/old cleanup, local Owner/CSRF/bearer/share guards, oversized imports, stale workspace/revision rejection, foreign publication IDs, schema-specific presets, secret exclusion, redirects, private addresses, bounded metadata and timeouts. Fake-provider regression tests cover OAuth/PKCE/resource/project authorization and token refresh.

Browser tests cover exact clipboard/download bytes, URL-only ChatGPT output, explicit publisher selection, Cline proxy default, missing native registration, clipboard denial/manual selection, delayed responses after workspace switch, embedded mode and modal registration/removal. Updated desktop/mobile screenshots are committed with the guide.

No GitHub workflow was dispatched, no installer/release was published, and no Netlify build or deployment was run. Production client/provider logins outside the local fake-provider fixtures, native macOS/Windows builds and user-account availability in third-party clients are not claimed.

To use a source checkout locally after merging, rebuild/reinstall Ax and restart Command Center. `git pull` alone does not replace an already installed binary. The source can be rebuilt using the repository's `scripts/reinstall-cli.sh` on macOS/Linux; use the documented local release workflow on Windows.

## Usability follow-up validation

From main `80d8ce9`: 101 ax-web library tests, 2 transfer-parser tests, strict web/CLI Clippy, TypeScript/Vite build, 26 desktop/mobile browser executions (including generated legacy login specs), 34 documentation tests and the local site build passed. Updated local/remote overview and import-dialog screenshots accompany the guide.

The rebuilt release was installed into an isolated test prefix. Real API and embedded browser checks verified local pinned configuration, path-free portable project configuration, native public-ID remote transfer and the paste flow. The serving binary's HTML matched the current frontend build (`index-ttYnblpS.js`). A stale release artifact was detected by this runtime check, removed and rebuilt before publication. No installer release, workflow or deployment was triggered.
