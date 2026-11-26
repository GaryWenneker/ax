# ax-admin: standalone remote admin application

Status: approved by the owner on 2026-11-26 ("Approve spec"). Changes made after approval are listed under "Revisions after approval" at the end; the text above that section is the approved text.

## Problem

The Command Center (`ax web`) is embedded in the full ax binary. The headless
slim build (called "atlas" in the request; `ax-slim`, `ax mcp remote serve`)
has no interface, so a deployment in a container or sandbox cannot be
inspected or managed from a UI.

## Decisions taken with the owner (2026-11-26)

| Question | Answer |
|---|---|
| What is "atlas"? | The headless slim build. |
| Form of the application | A local binary that serves the UI on loopback and opens the browser (same model as `ax web`). |
| Management scope | First answer: only the tools the remote MCP server already exposes. Revised by the owner in the next message: where management is missing, extend the remote MCP, and keep that extension small. See "Remote MCP admin extension". |
| Credentials | API key / client token (bearer) and the existing OAuth browser login. |

"Keep it slim" is read here as: the extension must run in the headless slim
build (nothing from `ax-web`), reuse tools that already exist instead of
adding new ones, and add as little server code as possible.

## Solution

A new binary, `ax-admin`, with its own installer and release assets. It is
not part of `ax` and does not need ax installed.

```text
browser (loopback only)  ->  ax-admin (local process)  ->  HTTPS  ->  ax mcp remote serve
   UI, no secrets              MCP client, credential store          /projects/<id>/mcp
```

- The only way ax-admin reaches an environment is the remote MCP endpoint
  (`https://host/projects/<id>/mcp`, Streamable HTTP). It uses no `ax web`
  API, no replication API, no SSH and no local `.ax` database.
- MCP calls are made by the native process, so the server needs no CORS and
  no `allowed_origins` entry. This works with every ax version that has
  remote MCP, full or slim. The management pages additionally need a server
  that has the admin extension below and a credential with the admin grant.
- The UI is built from what the connected server lists in `tools/list`.
  A page whose tool is not listed is hidden, so older and newer servers both
  work.

### Remote MCP admin extension

The local stdio MCP server already has the management tools; the remote
server just does not offer them. The extension exposes a fixed list of those
existing tools behind a new permission. Only one tool is new.

| Tool | Status | Use in ax-admin |
|---|---|---|
| `ax_sync` | exists | Incremental re-index |
| `ax_index` | exists | Full rebuild (`force`) |
| `ax_policy_index` | exists | Reload rules and skills from disk |
| `ax_files` | exists | Indexed files |
| `ax_cost`, `ax_tool_economics`, `ax_budget`, `ax_optimization_advice` | exist | Usage and savings |
| `ax_logs` | **new** | Read-only, paged view of the MCP call log (`mcp_call_log` in `ax-usage`): time, tool, duration, sizes, outcome. No arguments or result bodies |

Rules:

- New scope `ax:admin`. An admin tool is listed and callable only when the
  token has that scope **and** an explicit admin grant for the project:
  `"admin": true` on an API key (requires `"write": true`), or the subject in
  a new per-project `admin_subjects` list (must be a subset of
  `write_subjects`). Both default to empty, so existing configuration files
  load unchanged and grant nothing new.
- Without the grant, `tools/list` and every response are byte-for-byte what
  they are today. A direct call to an admin tool is denied with the same
  permission error as other hidden tools.
- The existing project pinning applies: a call cannot pass `projectPath`.
  Index and sync take the existing `.ax/ax.lock`, so a second writer waits.
- Protected-resource metadata advertises `ax:admin`.
- Still not available remotely: `ax_ship`, `ax_lsp`, `ax_diagnostics`,
  neural training tools, spec editing, and anything that runs a shell
  command or changes server configuration.
- Limit of this version: the built-in OAuth server (set up from Command
  Center, which the slim build does not have) keeps issuing only
  `ax:context` and `ax:write`. Admin access is granted through an API key or
  an external OAuth provider's subject. Adding admin to the Command Center
  approval screen is a follow-up.

Server code touched: `crates/ax-mcp/src/remote/{server,auth,config}.rs`, the
`ax_logs` tool in `crates/ax-mcp/src/tools.rs` with a query in `ax-usage`,
and their tests. No change in `ax-web`.

### Components and files

| Path | Content |
|---|---|
| `crates/ax-admin/` | Binary crate `ax-admin`: CLI, loopback HTTP server, MCP client, connection store, embedded UI |
| `crates/ax-admin/web-ui/` | Vite + React UI. Imports the Command Center stylesheet and theme tokens from `crates/ax-web/web-ui/src` so both look the same; no fork of those files |
| `crates/ax-remote-client/` | OAuth browser login and token store moved out of `ax-mcp/src/remote/{client,credentials}.rs`, re-exported from `ax_mcp::remote` so existing paths, behaviour and tests stay unchanged. Reason: ax-admin must not link the indexing engine |
| `install-admin.sh`, `install-admin.ps1` | Separate installers (macOS/Linux, Windows) |
| `scripts/package-admin-release.sh` | Packages `ax-admin-<os>-<arch>` archives |
| `.github/workflows/release.yml` | `build-admin` job; assets added to `SHA256SUMS`; installers copied to `site/public/` |
| `site/src/content/docs/guides/remote-admin.md` + sidebar, `guides/remote-mcp.md` (admin grant), `reference/mcp-server.md` (`ax_logs`, admin tools), `reference/cli.md`, `README.md`, `slim/README.md` | Documentation of current behaviour |
| `crates/ax-admin/tests/`, `crates/ax-admin/web-ui/e2e/`, `tools/gauntlet-ax-admin.sh` | Tests and the one entry point that reruns every layer |

New dependencies: none outside what the workspace and
`crates/ax-web/web-ui/package.json` already use (axum, reqwest with rustls,
keyring, include_dir, clap, tokio, serde, sha2, url; react, vite, typescript,
codicons, highlight.js). `ax-mcp` is a dev-dependency of ax-admin only, to
start a real remote server in tests.

### CLI

```bash
ax-admin                 # serve on 127.0.0.1:7080 and open the browser
ax-admin --port 0        # any free port; the URL is printed
ax-admin --no-open
ax-admin connections     # list saved connections (no secrets)
ax-admin version
```

It binds to `127.0.0.1` only. There is no flag for another bind address.

### Connections

A connection is: name, project MCP URL, authentication (`api_key` or
`oauth`), and for OAuth the public native client ID and whether write access
is requested.

- Metadata is stored in `~/.ax-admin/connections.json` (mode 0600). It never
  contains a key or token.
- An API key is stored in the OS credential store (service
  `io.wenneker.ax.admin`). OAuth tokens use the existing ax remote token
  store. There is no plaintext fallback: without a usable credential store
  the save fails with a clear error.
- The URL must be `https://…/projects/<id>/mcp`. `http://` is accepted only
  for a loopback host.
- "Paste connection details" accepts the JSON that Command Center's
  **Copy connection details** produces and fills URL and client ID.

### UI behaviour

1. **First start, no saved connection:** only the "Connect to an Ax
   environment" screen is shown. Required: URL and credentials (an API key,
   or a client ID plus browser sign-in). Name is optional and defaults to the
   host and project. The rest of the UI is not reachable until one
   connection has been tested and saved.
2. **Connections button** in the title bar, always visible. It opens the
   connections panel: list of saved environments with status, Add, Edit,
   Test, Remove, and Switch. The active environment is shown in the status
   bar. Switching reloads the tool list and the pages.
3. **Pages**, each shown only when its tools are listed by the server:

| Page | Tools |
|---|---|
| Status | `ax_status`, `ax_cache_status` |
| Search | `ax_search` |
| Nodes | `ax_node`, `ax_callers`, `ax_callees`, `ax_impact` |
| Explore | `ax_explore` |
| Insights | `ax_insights`, `ax_report` |
| Rules | `ax_rules` |
| Skills | `ax_skill` |
| Memory | `ax_recall`, `ax_history`; **Add memory** only when `ax_remember` is listed |
| Sessions | `ax_session`, `ax_durable` |
| Operations (admin) | `ax_sync`, `ax_index`, `ax_policy_index`; each asks for confirmation and shows the result |
| Files (admin) | `ax_files` |
| Usage (admin) | `ax_cost`, `ax_tool_economics`, `ax_budget`, `ax_optimization_advice` |
| Logging (admin) | `ax_logs` |
| Tools | Any listed tool: a form generated from its input schema, and the raw result |

   Results render the tool's `content.text` as Markdown with numbered source,
   and `structuredContent` as a collapsible tree. The Tools page is what
   keeps the app useful against a server with tools this version does not
   know.
4. Lists, selected rows and badges follow the `outline-badges` rule; colours
   follow `wcag-contrast`. All text is English.

### Local API (loopback, used by the UI only)

| Route | Behaviour |
|---|---|
| GET `/api/connections` | Saved connections, active id, CSRF token. No secrets |
| POST `/api/connections/test` | Runs `initialize` + `tools/list` with the given URL and credential; saves nothing |
| POST `/api/connections` | Tests, then saves. For OAuth, saved after sign-in completes |
| PUT / DELETE `/api/connections/{id}` | Edit / remove; remove deletes the stored credential |
| POST `/api/connections/{id}/activate` | Switch environment |
| POST `/api/login`, GET / DELETE `/api/login/{id}` | Start, poll and cancel browser sign-in |
| GET `/api/remote/info` | Server name, version, protocol version, tool list of the active connection |
| POST `/api/remote/tools/{name}` | Calls one listed tool with JSON arguments |

## Acceptance criteria

Connections and first start

- A1. With no `connections.json`, GET `/` shows the connect screen and no
  navigation; `/api/remote/*` returns 409 `no_active_connection`.
- A2. Saving with an empty URL or no credential is rejected (400) and
  nothing is written.
- A3. A valid URL and API key against a real `ax mcp remote serve` saves the
  connection, makes it active and shows the Status page.
- A4. A wrong API key gives "The server rejected these credentials" (from
  the 401) and saves nothing.
- A5. An unreachable host, a non-MCP response (HTML, 502) and a redirect each
  give a distinct error and save nothing. A redirect is never followed.
- A6. Two connections can be saved; Switch changes the active one, and the
  tool list and pages come from the newly active server.
- A7. Remove deletes the metadata entry and the stored credential. Removing
  the active connection activates another one, or returns to the connect
  screen when none is left.
- A8. After a restart the saved connections and the active one are restored
  and the app opens on the Status page without asking again.
- A9. `connections.json`, every API response and the process log never
  contain an API key or token (checked by a test that greps for the test
  key).
- A10. Browser sign-in: starting it opens the authorization URL, the
  connection is saved only after the token exchange succeeds, and Cancel
  leaves nothing saved.

MCP client

- B1. `initialize` negotiates 2025-11-25, falling back to 2025-06-18 and
  2025-03-26; the session id and negotiated version are sent on later calls.
- B2. A 404 for an expired transport session causes one re-initialize and
  one retry; a second failure is reported.
- B3. A 401 on an OAuth connection triggers one token refresh and one retry.
  A 401 on an API-key connection is reported as rejected credentials.
- B4. A tool that the server did not list is refused locally (404) without a
  network call.
- B5. A server whose `tools/list` lacks `ax_remember` shows no Add memory
  control; a server that lists an unknown tool shows it on the Tools page.
- B6. Requests have a timeout and a response size limit; exceeding either is
  an error, not a hang.

Remote MCP admin extension

- E1. A context-only or write-only credential gets exactly the same
  `tools/list` as before the change (compared against a recorded list), and
  a direct call to each admin tool is denied.
- E2. An API key with `"admin": true` lists and can call the admin tools for
  its projects only; the same key on another project is denied.
- E3. A configuration with `"admin": true` but `"write": false`, or an
  `admin_subjects` entry that is not in `write_subjects`, is rejected at
  start-up.
- E4. A configuration file without the new fields loads and behaves as
  before.
- E5. An admin call that passes `projectPath` is rejected; index and sync
  act on the pinned project root only.
- E6. `ax_sync` and `ax_index` through remote MCP update the index of a temp
  project (a new symbol becomes findable with `ax_search`).
- E7. `ax_logs` returns the latest calls newest first, is paged and bounded
  (limit capped), and contains no argument or result bodies.
- E8. The extension works in the headless build:
  `cargo test -p ax-mcp --no-default-features` for the affected tests, and
  `cargo build -p ax-cli --no-default-features --features "minimal-grammars mimalloc"` succeeds.
- E9. In ax-admin the admin pages are hidden with a non-admin credential or
  an older server, and shown with an admin credential. Operations ask for
  confirmation before calling `ax_index`.
- E10. Admin calls use a longer client timeout (10 minutes) than other
  calls; other calls keep the short one.

Loopback security

- C1. The listener is bound to 127.0.0.1.
- C2. A request with a non-loopback `Host`, or a foreign `Origin`, gets 403.
- C3. Every POST/PUT/DELETE without the CSRF token gets 403.
- C4. All API responses carry `Cache-Control: no-store`.

Distribution

- D1. `cargo build -p ax-admin` produces one binary with the UI embedded.
  It does not link `ax-extraction`, `ax-db` or tree-sitter (checked with
  `cargo tree`).
- D2. `install-admin.sh` installs, upgrades and uninstalls `ax-admin` from a
  release archive, verifies the SHA-256 and never touches an installed `ax`
  (tested against a local archive through the download-base override).
- D3. `install-admin.ps1` does the same on Windows.

Must not change

- N1. For credentials without the admin grant, the remote MCP server's tool
  list, responses and authorization are unchanged, and existing
  configuration files stay valid (E1, E4). The stdio MCP server is unchanged
  apart from the added `ax_logs` tool.
- N2. `ax`, `ax web`, the slim build and their installers behave as before.
  Existing tests of `ax-mcp` (including `tests/remote_mcp.rs`) and `ax-web`
  pass unmodified after the client code is moved.
- N3. No What's-new or per-version text in docs.

## Out of scope

- Server settings, prices, the rule/skill editors, specs, the graph canvas,
  LSP, ship and the agent terminal. Rules can still be added through
  `ax_policy_capture` with write access.
- Admin grants through the built-in OAuth approval screen (see the limit
  above).
- A hosted (static site) version and CORS on the server.
- Making the server reachable. `ax mcp remote serve` binds to loopback; the
  deployer still provides the HTTPS route (ingress sidecar, tunnel). Netlify
  cannot run the ax binary; it can only host static content.
- Signed `.dmg`/`.msi` packages and self-update. Upgrade by re-running the
  installer.

## Failure model (Tier 3: credentials and authentication)

| Failure | Check |
|---|---|
| Secret written to disk, log or API response | A9 |
| Credential sent to a redirect target or a different host | A5, plus a test that the bearer header only goes to the saved origin |
| Another site in the browser drives the local API | C2, C3 |
| Listener reachable from the network | C1 |
| Stale or wrong connection used after Switch | A6 |
| Hang on a dead server | B6 |
| Moving the login code breaks `ax mcp remote login` | N2 |
| Admin tools reachable without the grant, or on another project | E1, E2, E3 |
| Remote caller re-indexes a path outside the project | E5 |
| Two re-index requests corrupt the index | Existing `.ax/ax.lock`; a test starts two `ax_index` calls and checks both finish and the index is consistent |
| Log tool leaks prompt or source content | E7 |
| Extension pulls web code into the slim build | E8 |

Known limits to be recorded in EVIDENCE: no test against a real Azure or
Vercel deployment, the release workflow job and the Windows installer are
not executed locally, and browser sign-in is tested against the built-in
OAuth server of a local ax, not an external provider.

## Setup plan (authorised by approving this spec)

- Work on branch `feature/ax-admin` in this checkout. No worktree: a fresh
  worktree has no `node_modules` or `target-dev`, and the gauntlet needs both.
- Commit this spec at approval, then checkpoint commits per green step. No
  push and no pull request unless asked.
- `npm install` in `crates/ax-admin/web-ui` (packages listed above only).
- Tests start `ax mcp remote serve` on a loopback port against a temp
  project created with `ax init` in a temp directory. The repository's own
  `.ax` folder is not touched.
- Gauntlet: `cargo test` for ax-admin, ax-remote-client, ax-mcp and ax-web;
  `cargo clippy -D warnings`; `tsc`; UI unit tests; Playwright for A1-A8;
  manual mutants on the security checks (C2, C3, A9, B4, E1, E2, E5); the
  headless build and tests of E8; one real run of
  the built binary against a local remote MCP server; review loop; EVIDENCE
  at `docs/specs/ax-admin/EVIDENCE.md`.

## Revisions after approval

The approved text above is unchanged. Implementation and two independent
reviews showed these points to be wrong or incomplete; each is listed here so
the difference from the approved contract is visible. They await the owner's
review.

| # | Approved text | What is implemented, and why |
|---|---|---|
| V1 | "Protected-resource metadata advertises `ax:admin`." | Advertised only when an external provider is configured and a project has an admin subject. The built-in authorization server rejects scopes it does not issue, so advertising it to everyone would break clients that request every advertised scope (N1). |
| V2 | `ax_policy_index` is exposed. | Only `action: "index"` is available remotely. The restore action writes global policy files on the host, which is outside "the pinned project" and "nothing that changes server configuration". |
| V3 | "Index and sync take the existing `.ax/ax.lock`, so a second writer waits." | That lock alone is not enough inside one server process: two sessions stalled until a three-minute lock timeout. The remote server adds a per-project lock (tool calls share it, index-changing calls take it alone), a limit of four index-changing calls at once, and lets an index run finish after its request times out. A call that meets a running index is answered 503 after two seconds instead of queueing, so the other projects keep their request slots; this is new behaviour for context clients during an admin-started index. |
| V4 | Server gives admin calls 600 s (implied by E10). | 570 s on the server, 600 s in ax-admin, so the client sees the server's answer. All nine admin tools get the long client timeout, not only three. |
| V5 | Local API table: `GET /api/login/{id}` polls and saves. | `GET` only reports; `POST /api/login/{id}/complete` saves and needs the request token (C3). |
| V6 | `POST /api/remote/tools/{name}` takes arguments. | It also takes the connection id the page shows and refuses (409) when another connection is active, so a result cannot land under the wrong environment (A6). |
| V7 | Edit: "PUT … Edit". | A saved key is sent only to the address it was saved for: changing the address requires the key again, and a saved connection is tested as saved. |
| V8 | Remote failures were not given status codes. | 502 unreachable or not Ax, 504 timeout or still running, 429 busy, 422 refused by the environment. |
| V9 | "each [operation] asks for confirmation" (page table) versus E9 naming only `ax_index`. | All three operations ask first and name the environment; the Tools page asks for the same three tools. |
| V10 | "New dependencies: none outside …" | One development-only package was added to the UI, `@types/node`, to type-check the unit tests and the browser story. Test-only crates `futures-core` (already in the lock file) and `jsonwebtoken` were added as dev-dependencies of ax-admin. |
| V11 | Browser sign-in asks for context and optionally write. | It can also ask for `ax:admin`, otherwise an admin subject could never reach the management pages through sign-in. A rejected token is refreshed for real (B3) through a small addition to the shared client code. |
| V12 | Known limits: "browser sign-in is tested against the built-in OAuth server of a local ax". | Not done. The sign-in flow is tested with a stand-in backend; the real browser flow, token refresh and the OS credential store are not exercised by any automated test. |
| V13 | Memory "Add memory" inline; Logging as a table. | "Add memory" and the durable store open in a dialog and the call log is a calm list, following the Command Center's `modal-forms` and `calm-lists` rules. |
| V14 | "`structuredContent` as a collapsible tree". | Implemented as a tree that opens level by level (the first version showed formatted JSON). |
| V15 | A10: "Cancel leaves nothing saved". | Also true when the page disappears: a sign-in that finishes while no page saves it is dropped after one minute with its tokens, and a cancel that arrives in the moment of saving removes the connection again. |
| V16 | Gauntlet: "Playwright for A1-A8". | The browser story covers A1-A4, A6, A7, E6, E9 and one case of A5. A8 as a process restart, the other A5 cases and A10 are covered by Rust tests. E6 shows that sync and rebuild run and that new code is in the index afterwards; it cannot show the code was absent before, because a running Ax watches its projects and picks up new files by itself. |
| V17 | (not in the spec) | No automated test covers three UI mechanisms: dropping a stale reload answer, the sign-in wait view as a component, and the client cache guard in ax-admin against a change racing a request. They are covered by reading and, for the first two, by the pure logic tests in `signIn.test.ts`. No test sends an `axc_` client token at an admin tool; such tokens never carry the admin scope. |
