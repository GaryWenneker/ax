---
title: Command Center HTTP API
description: Every HTTP endpoint served by ax web / ax ship --watch, grouped by area.
banner:
  content: |
    This page describes part of the full ax, which is being phased out. <a href="/guides/phase-out/">ax-admin becomes the only visual interface</a>.
---

`ax web` (and `ax ship --watch`) serve the Command Center UI and a JSON API on the same port (default `7070`). Every path below is relative to that server, for example `http://127.0.0.1:7070/api/stats`.

- Requests from the local machine need no token. Through `ax share`, every `/api`, `/dav` and `/ax` request needs the share token (see [Share Command Center](/guides/share/)).
- The `Host` header must name this server: `localhost` or a loopback address, an IP address when the server is bound beyond loopback, the host of `AX_WEB_AUTH_ORIGIN`, or a name listed in `AX_WEB_ALLOWED_HOSTS` (comma separated) or in `web.allowed_hosts` in `~/.ax/config.json`. Any other `Host` gets `403`. This stops DNS rebinding, where a page on another domain points its name at this computer. The list is read when the server starts.
- Endpoints ending in `/stream` or `/events` answer with Server-Sent Events (SSE).
- UI routes like `/policy/rules` or `/graph` return the single-page app and work as bookmarks. Unknown `/api/` routes return an error rather than the HTML application shell.

## Graph and index

| Method | Path | Purpose |
|---|---|---|
| GET | `/api/stats` | Index counts for the active project |
| GET | `/api/version` | Running ax version |
| GET | `/api/nodes` | Symbol list with kind and file filters |
| GET | `/api/node/{id}` | One symbol with source, callers and callees |
| GET | `/api/graph` | Graph data for the Graph page |
| GET | `/api/graph/stream` | Graph data as SSE (large graphs) |
| GET | `/api/graph/export` | Portable export (HTML / JSON / other formats) |
| GET | `/api/insights` | Communities, god nodes, surprising connections |
| GET, PUT | `/api/domain-graph` | Read or save `.ax/domain-graph.json` |
| GET | `/api/files` | Indexed file tree (lazy per folder) |
| GET | `/api/files/roots` | Top-level folders and files |
| GET | `/api/search` | Full-text symbol search |
| GET | `/api/source` | Source text of a file range (indexed files only, up to 8 MiB; never `.env*`, `.git/` or `.ax/`) |
| GET | `/api/unresolved` | Unresolved references |
| GET | `/api/unresolved/summary` | Unresolved counts per kind |
| POST | `/api/unresolved/reconcile` | Re-run resolution for unresolved references |

List routes take `limit` and `offset`. `limit` is kept between 1 and the route's maximum, and a negative `offset` starts at the first row.

## Policy (rules and skills)

| Method | Path | Purpose |
|---|---|---|
| GET, POST | `/api/policy/rules` | List rules / create a rule |
| GET, PUT, DELETE | `/api/policy/rules/{id}` | Read, save, delete a rule |
| PATCH | `/api/policy/rules/{id}/enabled` | Turn a rule on or off |
| PATCH | `/api/policy/rules/{id}/storage` | Move a rule between project and global storage |
| GET | `/api/policy/rules/{id}/revisions` | Revision history |
| POST | `/api/policy/rules/{id}/revisions/{revId}/restore` | Restore a revision |
| GET, POST | `/api/policy/skills` | List skills / create a skill |
| GET, PUT, DELETE | `/api/policy/skills/{name}` | Read, save, delete a skill |
| PATCH | `/api/policy/skills/{name}/enabled` | Turn a skill on or off |
| PATCH | `/api/policy/skills/{name}/storage` | Move a skill between project and global storage |
| GET | `/api/policy/skills/{name}/revisions` | Revision history |
| POST | `/api/policy/skills/{name}/revisions/{revId}/restore` | Restore a revision |
| POST | `/api/policy/relocate` | Move several items at once |
| POST | `/api/policy/copies/delete` | Delete duplicate copies |
| POST | `/api/policy/match` | Which rules and skills match a prompt (Match page) |
| POST | `/api/policy/capture` | Save a durable directive as a rule |
| POST | `/api/policy/reindex` | Re-index policy from disk |
| POST | `/api/policy/export` | Export policy files to `out_dir`, a folder inside the project (default `.ax/policy/export`); absolute paths, `..` and links out of the project get 400 |
| GET | `/api/policy/pack/status` | Git pack sync status |
| POST | `/api/policy/pack/export` | Export a pack |
| POST | `/api/policy/pack/import` | Import a pack |
| POST | `/api/policy/package` | Build a zip package |
| POST | `/api/policy/package/preview` | Preview a package before restore |
| POST | `/api/policy/package/diff` | Per-item diff against a package |
| POST | `/api/policy/package/restore` | Restore selected items from a package |
| GET | `/api/policy/review` | Staged imports waiting for review |
| GET | `/api/policy/review/{id}` | One staged item |
| POST | `/api/policy/review/{id}/approve` | Accept a staged item |
| POST | `/api/policy/review/{id}/reject` | Reject a staged item |
| GET | `/api/policy/skills/catalog/search` | Search the public skill catalog |
| POST | `/api/policy/skills/catalog/install` | Install a skill from a catalog source |
| GET | `/api/policy/skills/catalog/installed` | Skills installed from the catalog |
| GET, PUT | `/api/policy/skills/catalog/settings` | Install reporting and the publish repository |
| GET | `/api/policy/skills/catalog/audits`, `/api/policy/rules/catalog/audits` | Latest audit reports |
| POST | `/api/policy/skills/catalog/audit` | Audit a skill before installing it |
| GET, POST | `/api/policy/skills/{name}/audit` | Read or run a skill audit |
| POST | `/api/policy/rules/{id}/audit` | Run a rule audit |
| POST | `/api/policy/skills/{name}/publish` | Publish one skill to the publish repository |

Read-only mode (`ax share`, `AX_WEB_READONLY=1`) refuses every catalog route that writes: install, settings, audits and publish. The publish repository must be an `https://` or `ssh://` URL, an scp-style `user@host:path`, or a folder inside the project (relative folders are relative to the project, optionally prefixed with `well-known:`); other values get `400`.

Install and audit from a well-known URL follow the same rule as `ax skill install`: every file the index lists must be relative to the index or on the index's own origin (scheme, host and port). A file on another server, a switch from `https` to `http`, and a redirect that leaves that origin are refused.
| GET, PUT | `/api/policy/settings` | Policy settings (folder, storage mode) |

### Remote policy share

| Method | Path | Purpose |
|---|---|---|
| GET, PUT | `/api/policy/share/config` | Share source configuration |
| GET | `/api/policy/share/status` | Last sync result |
| POST | `/api/policy/share/sync` | Sync now |
| POST | `/api/auth/microsoft/device/start` | Start Microsoft device-code sign-in |
| POST | `/api/auth/microsoft/device/poll` | Poll the device-code sign-in |
| GET | `/api/auth/microsoft/status` | Sign-in status |
| PUT | `/api/auth/microsoft/config` | Tenant / client settings |
| DELETE | `/api/auth/microsoft` | Sign out |

The Microsoft sign-in routes follow the same share-token and sign-in rules as the rest of `/api`. Starting or polling sign-in, changing the client and signing out work only on a writable server, from a browser on this computer at the loopback address; other callers get `403`. While the server's environment sets `AX_MS_CLIENT_ID`, that value is used for sign-in and `PUT /api/auth/microsoft/config` answers `409`.

See [Remote Policy Share](/guides/policy-sharing/).

## Specs

| Method | Path | Purpose |
|---|---|---|
| GET | `/api/specs` | List specs and imported plans |

The list imports finished plans from the plan folders on disk at most once every 30 seconds. `GET /api/specs?refresh=1` (or `refresh=true`) rescans the folders first; the Specs page sends it. A read-only Command Center lists what is stored without importing.

## Memory and links

| Method | Path | Purpose |
|---|---|---|
| GET, POST | `/api/memory` | List memories / create a memory |
| GET, PUT, DELETE | `/api/memory/{id}` | Read, save, delete a memory |
| PATCH | `/api/memory/{id}/enabled` | Include or exclude a memory from preflight |
| GET | `/api/memory/recall` | Hybrid search |
| GET | `/api/memory/embed-status` | Embedding status: `backend` is `hash` and `model` is `hash-v1` |
| POST | `/api/memory/capture-git` | Capture memories from recent commits |
| GET | `/api/memory/{id}/file-changes` | Files changed in the turn behind a memory |
| GET | `/api/memory/{id}/file-links` | Files a memory links to |
| GET | `/api/memory/{id}/diff` | Diff for a memory's file change |
| GET | `/api/memory/{id}/image` | Image attached to a memory |
| GET | `/api/links` | Resolve `[[links]]` |
| GET | `/api/links/graph` | Link graph for the link picker and policy graph |

## Vault drive (WebDAV)

| Method | Path | Purpose |
|---|---|---|
| any | `/ax/…`, `/dav/…` | WebDAV view of rules, skills and memories (`/ax` is the mounted volume name) |
| GET, POST, DELETE | `/api/dav/mount` | Mount status / mount / unmount the vault drive |
| POST | `/api/dav/mount/open` | Open the mounted drive in the file manager |
| GET, PUT | `/api/vault/folders` | Extra folders shown under `folders/` |
| POST | `/api/vault/folders/{name}/sync` | Sync one folder |
| DELETE | `/api/vault/folders/{name}` | Remove a folder |
| POST | `/api/vault/folder-picker` | Native folder picker |

See [Obsidian Vault](/guides/obsidian-vault/).

## Ship (quality gate)

| Method | Path | Purpose |
|---|---|---|
| GET | `/api/ship/events` | Pipeline events (SSE) |
| GET | `/api/ship/status` | Branch, last report, running step |
| GET | `/api/ship/impact` | Changed files and impacted tests |
| POST | `/api/ship/command` | Run evaluate / draft PR. Both are refused in read-only mode |
| GET, PUT | `/api/ship/config` | Read or save `.ax/ship.toml` |
| GET, PUT | `/api/ship/review-language` | Review comment language |

The `/api/ship/sonar/*` endpoints (discover, install, start, stop, bootstrap, setup, token, scan, exclude, and the `/api/ship/sonar/ui` proxy) still exist for the optional `sonar` gate step. The Command Center no longer has a page or buttons for them.

## Usage, savings, pricing and MCP logging

| Method | Path | Purpose |
|---|---|---|
| GET | `/api/usage/savings` | Savings dashboard data |
| POST | `/api/usage/savings/import` | Import IDE session logs |
| GET | `/api/usage/savings/call/{id}` | One recorded tool call |
| POST | `/api/usage/tokenize` | Token count for a text (body up to 512 KiB; text beyond the tokenizer's input cap is answered as truncated) |
| GET | `/api/usage/pricing` | Model price catalog |
| GET | `/api/usage/pricing/status` | Last price sync |
| GET | `/api/usage/pricing/history` | Price over time |
| GET | `/api/usage/pricing/agents` | Agent / model mapping |
| POST | `/api/usage/pricing/sync` | Sync prices now |
| GET | `/api/usage/mcp-trace/events` | Live MCP verbose stream (SSE) |
| GET | `/api/usage/mcp-trace/chunk` | Older log lines |
| GET | `/api/usage/mcp-trace/path` | Path of today's log file. When this project has no verbose log, the local owner's own writable Command Center follows the most recently written log of another recent project; read-only (share) viewers and other callers only ever see this project's log |
| GET | `/api/usage/mcp-quality` | Quality metrics (Q slide-out) |
| GET | `/api/usage/mcp-quality/events` | Quality updates (SSE) |
| POST | `/api/usage/mcp-audit` | Run the audit (`ax mcp audit`). `session` is a session id or one of this project's agent transcript files; other paths get 400. Refused in read-only mode |

## Workspace

| Method | Path | Purpose |
|---|---|---|
| GET | `/api/workspace/current` | Active project |
| GET | `/api/workspace/recent` | Recent and discovered projects |
| POST | `/api/workspace/recent/add` | Add a project to the recent list |
| GET | `/api/workspace/browse` | Browse folders in the project browser |
| POST | `/api/workspace/mkdir` | Create a folder |
| POST | `/api/workspace/init/stream` | Run `ax init` in a folder (SSE) |
| POST | `/api/workspace/switch` | Switch the active project |
| GET | `/api/workspace/purge-plan` | What a project purge would delete |
| POST | `/api/workspace/purge` | Delete a project's ax data |

These routes work on folders anywhere on this computer; there is no list of allowed folders. Paths must be absolute. With web sign-in off, they answer only the browser of the person at this computer: a connection from this computer, without a proxy header, to `127.0.0.1`, `localhost` or `[::1]` on the Command Center port, with no `Origin` from another page. Anything else gets 403. With sign-in on, they need an Admin or Owner session. The routes that write, `recent/add` included, are refused in read-only mode.

Purging the database of the project that is open closes it first: its background tasks stop and every connection to `.ax/ax.db` closes, and the reply has `"disconnected": true`. Until another project is opened with `switch`, project routes under `/api` and `/dav` answer 409 with "No project open"; the workspace, access, sign-in and action routes keep working.

## IDEs and agents

| Method | Path | Purpose |
|---|---|---|
| GET | `/api/agent/status` | Detected IDEs / agents and their ax wiring |
| POST | `/api/agent/install`, `/api/agent/install/stream` | Connect ax (MCP + hooks) to an IDE or agent |
| POST | `/api/agent/cli/install/stream` | Install an agent CLI |
| POST | `/api/agent/uninstall` | Remove ax from an IDE or agent |
| PUT | `/api/agent/config` | Terminal mode and preferred agent |
| GET, POST | `/api/agent/profiles` | List / create agent profiles |
| PUT | `/api/agent/profiles/active` | Set the active profile |
| PUT, DELETE | `/api/agent/profiles/{agent}/{id}` | Edit / delete a profile |
| POST | `/api/agent/profiles/{agent}/{id}/auth/stream` | Sign a profile in (SSE with the CLI's output lines; stopped after 15 minutes or when the stream closes) |
| POST | `/api/agent/profiles/{agent}/{id}/authenticated` | Mark a profile as signed in |

`/api/agent/chat/stream` and `/api/agent/pty/ws` belonged to the removed Agent page. The `config` and `profiles` endpoints still exist, but no Command Center screen uses them anymore.

In read-only mode every `/api/agent` request other than `GET` and `HEAD` returns 403. `/api/agent/chat/stream` starts an agent with tool access in the project, so like the terminal socket it answers only the local browser (a loopback peer on the loopback address); other callers get 403. An external agent run is stopped after 30 minutes or as soon as the browser closes the stream. Closing the `/api/agent/pty/ws` socket kills the CLI it started.

## Operations and integrations

| Method | Path | Purpose |
|---|---|---|
| GET | `/api/ops/mcp-health` | Shared MCP daemon health |
| POST | `/api/ops/mcp-reload` | Restart the MCP daemon (**Reload MCP**). Only from the local owner's own Command Center page; refused in read-only mode |
| GET | `/api/lsp/status` | Language-server status |
| POST | `/api/lsp/enrich` | Add exact edges from language servers |
| GET | `/api/plugins` | Extractor plugins |
| GET | `/api/okf/config` | OKF settings from `ax.json` |
| POST | `/api/okf/export`, `/api/okf/validate`, `/api/okf/publish` | Build, check, publish an OKF bundle |
| POST | `/api/docs-catalog/sync` | Refresh the docs catalog |
| GET | `/api/share/status` | `ax share` status |
| GET | `/api/actions/events` | Live action stream (SSE) |
| POST | `/api/actions/publish` | Publish an action to the stream |
| GET | `/api/reset-client-cache` | Clear a stale service worker / browser cache |

## Interactive Command Center sign-in and WebAuthn

Interactive human sessions use an opaque, HttpOnly, SameSite cookie. Passkeys are verified server-side against the configured relying party ID and origin. Browser identities must not be confused with MCP client tokens.

| Method | Endpoint | Purpose |
|---|---|---|
| GET | `/api/web-auth/status` | Whether interactive sign-in is enabled |
| GET | `/api/web-auth/session` | Active member and roles, or 401 |
| POST | `/api/web-auth/setup` | Initial local Owner password setup before enabling sign-in |
| POST | `/api/web-auth/config` | Trusted-local Owner enable/disable; revokes web sessions |
| POST | `/api/web-auth/login` | Sign in using identity and password |
| POST | `/api/web-auth/logout` | Revoke current session |
| GET | `/api/web-auth/passkeys` | Current user's registered passkeys (metadata only) |
| POST | `/api/web-auth/passkeys/registration/start` | Create WebAuthn registration challenge |
| POST | `/api/web-auth/passkeys/registration/finish` | Verify and store public-key credential |
| POST | `/api/web-auth/passkeys/authentication/start` | Begin passkey sign-in |
| POST | `/api/web-auth/passkeys/authentication/finish` | Verify assertion and establish session |
| PATCH | `/api/web-auth/passkeys/{id}` | Rename own passkey |
| DELETE | `/api/web-auth/passkeys/{id}` | Revoke own passkey |

`GET /api/access/team` returns both `role` (strongest-role compatibility value) and `roles` (authoritative role set). The `POST /api/access/team` and `PATCH /api/access/team/{id}` bodies accept `roles: ["admin", "member"]` and remain compatible with the legacy `role` input. An active member cannot have zero roles; only an Owner may grant or remove ownership, and the final active Owner cannot be demoted or disabled.

Authentication may be disabled for trusted local installations; remote Command Center data access stays protected. Configure `AX_WEB_AUTH_ORIGIN` to the exact stable HTTPS origin used for passkeys; localhost development is supported. Browser clients must not send credentials to a different host.

With sign-in on, every `/api` request needs a session, and the gate applies the member's role:

- **Changes** (any method other than `GET` and `HEAD`, and every WebSocket) need a role that may write; a Read-only member gets 403. A change from another computer must carry an `Origin` that is this server's own page: `AX_WEB_AUTH_ORIGIN` when it is set, otherwise the request's `Host` over HTTPS (or over HTTP when no proxy reports `X-Forwarded-Proto: https`). Any other `Origin`, including a sibling subdomain, or none at all, gets 403.
- **Management and this computer** need Admin or Owner: `/api/agent` (IDE installs, agent chat and the agent terminal), `/api/workspace`, `/api/lsp`, `/api/dav/mount`, `/api/auth/microsoft`, `/api/remote-mcp`, `/api/access`, `/api/ops`, `/api/ship`, `/api/plugins`, `/api/policy/share`, `/api/vault` and `/api/actions`.

## Team, clients and replication access

These endpoints enforce the local-owner session or a client credential with the corresponding `admin:team`, `admin:clients` or `admin:nodes` scope. Mutations are rejected in read-only mode. Local browser mutations use the CSRF value from `/api/access/session` in `X-Ax-Access-Csrf`. Token values are returned only when created/revealed; keep them out of logs and screenshots. A client credential acts with the role of the member it belongs to, so only an Owner can create, issue, rotate or change the scopes of a client that belongs to an Owner; anyone else gets 403.

| Method | Path | Purpose |
|---|---|---|
| GET | `/api/access/session` | Retrieve the local access-session CSRF value |
| GET, POST | `/api/access/team` | List or add members |
| PATCH | `/api/access/team/{id}` | Change a member's role/status |
| GET, POST | `/api/access/clients` | List or add applications and scopes |
| PATCH | `/api/access/clients/{id}/scopes` | Change application permissions |
| POST | `/api/access/clients/{id}/token` | Issue a credential |
| POST | `/api/access/clients/{id}/rotate` | Replace client credentials |
| POST | `/api/access/clients/{id}/revoke` | Revoke the client |
| POST | `/api/access/credentials/{id}/revoke` | Revoke one credential |
| GET | `/api/access/nodes` | List peers and local role/status |
| GET, POST | `/api/access/nodes/local` | Read or configure/start the local replication service |
| GET | `/api/access/nodes/local/interfaces` | List local advertisement interfaces |
| POST | `/api/access/nodes/local/stop` | Stop the local service |
| POST | `/api/access/nodes/local/credential` | Reveal the saved local replication credential |
| POST | `/api/access/nodes/local/remove` | Remove local configuration, preserving synchronized project data |
| POST | `/api/access/nodes/local/discover` | Inspect discovered Primary candidates |
| POST | `/api/access/nodes/{id}/revoke` | Block a peer |
| POST | `/api/access/nodes/{id}/remove` | Remove a revoked peer record |
| POST | `/api/access/nodes/{id}/reenroll` | Allow a blocked peer to enroll again |

Starting the local service as a replica enrolls with the Primary and applies its snapshot, which can take minutes. During that time `GET /api/access/nodes/local` keeps answering and reports `"setting_up": true`; a second start, stop or remove is refused with 409 until setup finishes.

## Local remote-MCP connection login

Use these on the hosting computer through local Command Center. They manage the local OAuth client connection, not public server access grants. Mutations require `X-Ax-Csrf` from the status response.

| Method | Path | Purpose |
|---|---|---|
| GET | `/api/remote-mcp` | Saved connections and CSRF value (the client uses the trailing-slash form) |
| POST | `/api/remote-mcp/login` | Begin browser login for a project URL and public client ID |
| GET | `/api/remote-mcp/login/{id}` | Poll the login attempt |
| POST | `/api/remote-mcp/login/{id}/cancel` | Cancel a pending attempt |
| POST | `/api/remote-mcp/logout` | Remove a saved connection and attempt provider revocation |

## SonarQube service controls

These are host operations, not passive documentation examples. Streaming variants return progress via SSE. Use the configured credentials and local host access for service administration.

| Method | Path | Purpose |
|---|---|---|
| GET | `/api/ship/sonar/discover` | Inspect runtimes, containers and reachability. Outside read-only mode it also starts project provisioning when SonarQube is enabled and reachable |
| POST | `/api/ship/sonar/install` | Install the configured service |
| POST | `/api/ship/sonar/install/stream` | Install with progress |
| POST | `/api/ship/sonar/start` | Start the service |
| POST | `/api/ship/sonar/start/stream` | Start with progress |
| POST | `/api/ship/sonar/stop` | Stop the service |
| POST | `/api/ship/sonar/stop/stream` | Stop with progress |
| POST | `/api/ship/sonar/bootstrap` | Bootstrap project setup |
| GET | `/api/ship/sonar/setup` | Read project, scanner and token setup status |
| POST | `/api/ship/sonar/validate-login` | Check a SonarQube login. `host` must be the configured SonarQube host or a loopback address (other hosts get 400); refused in read-only mode |
| GET | `/api/ship/sonar/validate-token` | Validate the current token |
| POST | `/api/ship/sonar/regenerate-token` | Replace the scanner token |
| POST | `/api/ship/sonar/scan` | Run a scan |
| POST | `/api/ship/sonar/scan/stream` | Scan with progress |
| POST | `/api/ship/sonar/exclude` | Update scan exclusions |
| GET | `/api/ship/sonar/ui/info` | Proxy connection information |
| ANY | `/api/ship/sonar/ui/{*path}` | Proxy a SonarQube UI subpath |

The UI proxy sends each request with the configured SonarQube admin login. Only `GET` and `HEAD` pass in every case. Any other method returns 403 in read-only mode, on a cross-site request (`Sec-Fetch-Site` other than `same-origin`), and when the `Origin` header is missing or names a different host than the request. The SonarQube page opened in the Command Center meets these rules on its own. Of the browser's cookies, only SonarQube's own `JWT-SESSION` and `XSRF-TOKEN` reach the SonarQube host; the Command Center's share token and login session cookies are dropped.

The other `/api/ship` writes follow the same `Origin` rule when they carry no JSON body (for example `sonar/start`, `sonar/stop`, `sonar/regenerate-token` and `sonar/scan` without a body), because another website can send such a request without a CORS preflight. Writes with a `Content-Type: application/json` body are not affected, so scripts and the desktop client can call them without an `Origin` header.

## Live data refresh

| Method | Path | Purpose |
|---|---|---|
| GET | `/api/changes` | Shared SSE notifications for graph, memory, policy and usage changes |
| GET | `/api/graph/recent` | Recently indexed nodes/edges for live graph updates |

## Project publication and remote host setup

These local-owner endpoints live under `/api/remote-mcp/publications`. They require local Command Center access; mutations use `X-Ax-Csrf`. Export requests bind the active workspace and configuration fingerprint, so switching projects or changing configuration can reject stale requests. Exports contain public connection metadata; local exports can contain this computer's paths. Host actions manage the hosted service, while deleting a publication removes only its export registration.

| Method | Path | Purpose |
|---|---|---|
| GET | `/api/remote-mcp/publications` | List this workspace's registered publications (client uses trailing slash) |
| POST | `/api/remote-mcp/publications/import` | Register an existing server configuration |
| GET | `/api/remote-mcp/publications/host` | Read managed host status |
| POST | `/api/remote-mcp/publications/host/setup` | Generate/import managed host configuration |
| POST | `/api/remote-mcp/publications/host/action` | Start, stop or remove managed host configuration |
| GET | `/api/remote-mcp/publications/host/authorizations` | List project connection requests/grants |
| POST | `/api/remote-mcp/publications/host/authorizations/{request_id}` | Decide, revoke or remove a request/grant |
| POST | `/api/remote-mcp/publications/local/export` | Generate local app configuration for this computer or a portable project entry |
| POST | `/api/remote-mcp/publications/{id}/export` | Generate configuration for the selected receiving app |
| POST | `/api/remote-mcp/publications/{id}/connection` | Generate portable connection details |
| POST | `/api/remote-mcp/publications/{id}/check` | Check endpoint reachability without claiming authenticated access |
| DELETE | `/api/remote-mcp/publications/{id}` | Remove a publication's export registration |
