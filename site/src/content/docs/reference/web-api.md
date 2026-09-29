---
title: Command Center HTTP API
description: Every HTTP endpoint served by ax web / ax ship --watch, grouped by area.
---

`ax web` (and `ax ship --watch`) serve the Command Center UI and a JSON API on the same port (default `7070`). Every path below is relative to that server, for example `http://127.0.0.1:7070/api/stats`.

- Requests from the local machine need no token. Through `ax share`, every `/api`, `/dav` and `/ax` request needs the share token (see [Share Command Center](/guides/share/)).
- Endpoints ending in `/stream` or `/events` answer with Server-Sent Events (SSE).
- Any other path that is not listed here returns the single-page app, so UI routes like `/policy/rules` or `/graph` work as bookmarks.

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
| GET | `/api/source` | Source text of a file range |
| GET | `/api/unresolved` | Unresolved references |
| GET | `/api/unresolved/summary` | Unresolved counts per kind |
| POST | `/api/unresolved/reconcile` | Re-run resolution for unresolved references |

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
| POST | `/api/policy/export` | Export policy files |
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

See [Remote Policy Share](/guides/policy-sharing/).

## Memory and links

| Method | Path | Purpose |
|---|---|---|
| GET, POST | `/api/memory` | List memories / create a memory |
| GET, PUT, DELETE | `/api/memory/{id}` | Read, save, delete a memory |
| PATCH | `/api/memory/{id}/enabled` | Include or exclude a memory from preflight |
| GET | `/api/memory/recall` | Hybrid search |
| GET | `/api/memory/embed-status` | Embedding backend status |
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
| POST | `/api/ship/command` | Run evaluate / draft PR |
| GET, PUT | `/api/ship/config` | Read or save `.ax/ship.toml` |
| GET, PUT | `/api/ship/review-language` | Review comment language |

The `/api/ship/sonar/*` endpoints (discover, install, start, stop, bootstrap, setup, token, scan, exclude, and the `/api/ship/sonar/ui` proxy) still exist for the optional `sonar` gate step. The Command Center no longer has a page or buttons for them.

## Usage, savings, pricing and MCP logging

| Method | Path | Purpose |
|---|---|---|
| GET | `/api/usage/savings` | Savings dashboard data |
| POST | `/api/usage/savings/import` | Import IDE session logs |
| GET | `/api/usage/savings/call/{id}` | One recorded tool call |
| POST | `/api/usage/tokenize` | Token count for a text |
| GET | `/api/usage/pricing` | Model price catalog |
| GET | `/api/usage/pricing/status` | Last price sync |
| GET | `/api/usage/pricing/history` | Price over time |
| GET | `/api/usage/pricing/agents` | Agent / model mapping |
| POST | `/api/usage/pricing/sync` | Sync prices now |
| GET | `/api/usage/mcp-trace/events` | Live MCP verbose stream (SSE) |
| GET | `/api/usage/mcp-trace/chunk` | Older log lines |
| GET | `/api/usage/mcp-trace/path` | Path of today's log file |
| GET | `/api/usage/mcp-quality` | Quality metrics (Q slide-out) |
| GET | `/api/usage/mcp-quality/events` | Quality updates (SSE) |
| POST | `/api/usage/mcp-audit` | Run the audit (`ax mcp audit`) |

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
| POST | `/api/agent/profiles/{agent}/{id}/auth/stream` | Sign a profile in (SSE) |
| POST | `/api/agent/profiles/{agent}/{id}/authenticated` | Mark a profile as signed in |

`/api/agent/chat/stream` and `/api/agent/pty/ws` belonged to the removed Agent page. The `config` and `profiles` endpoints still exist, but no Command Center screen uses them anymore.

## Operations and integrations

| Method | Path | Purpose |
|---|---|---|
| GET | `/api/ops/mcp-health` | Shared MCP daemon health |
| POST | `/api/ops/mcp-reload` | Restart the MCP daemon (**Reload MCP**) |
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
