# Remote MCP project configuration export

Status: implemented; local validation recorded in EVIDENCE.md. No installer release or deployment is implied by merging this change.

## Problem and research result

Settings previously exposed outbound browser login under Remote MCP. It had no way to copy the active project's inbound MCP configuration. Project folder names, replication IDs and browser origins cannot reliably identify a remote MCP endpoint.

The existing server owns the authoritative HTTPS origin, OAuth resource/issuer and explicit `projects.id` → canonical root mapping. Client formats differ: Cursor and Claude Code use `mcpServers`; VS Code's workspace file uses `servers`; Cline's direct HTTP type is `streamableHttp`. ChatGPT consumes a URL through its custom app OAuth flow. Public native Ax, Cursor, VS Code and ChatGPT OAuth registrations are separate.

Primary client references:

- https://cursor.com/docs/mcp
- https://code.visualstudio.com/docs/agents/reference/mcp-configuration
- https://docs.cline.bot/mcp/mcp-overview
- https://code.claude.com/docs/en/mcp
- https://developers.openai.com/plugins/deploy/connect-chatgpt

The original research specification also covers credential/grant separation and provider-dependent compatibility. This implementation records only confirmed public client IDs; unconfirmed registrations remain unknown. It does not assume client metadata registration or dynamic registration is supported by a particular provider.

## Implemented solution

Settings → Connections → Remote MCP shows **Share this project**, followed by **Connect to another Ax**. The same React component is used in embedded IDE mode.

A local SQLite catalogue stores publishers, canonical project bindings and instance-scoped runtime leases. It never stores access/refresh tokens, API key hashes, OAuth subjects, client secrets or the complete server configuration. Canonical roots are internal matching metadata and are excluded from browser responses and client exports.

`ax mcp remote register --config FILE --name LABEL` registers metadata without starting the server or signing in. Optional native/Cursor/VS Code public client ID flags record pre-registered IDs. `serve` reconciles all configured project bindings automatically before network startup and preserves existing explicit labels/client IDs. Browser import accepts a strict server JSON object, with a 256 KiB request limit, and registers only the active project's binding.

Publisher identity derives from the validated canonical origin and exact OAuth issuer. Publication identity includes the publisher and remote project ID. Different roots with the same folder name remain distinct. A remote ID bound to another root requires explicit local registration removal. Reconciliation and conflicts are transactional; a fresh live lease rejects a different server snapshot.

Leases begin only after discovery, listener and optional socket bridge succeed. Heartbeats run every 15 seconds and become unknown after 45 seconds. Cleanup is conditional on instance ID so an older process cannot mark a newer lease stopped. Stopped configurations remain copyable.

## API and authorization

All publication routes require a writable loopback browser session and an active local Owner. Authorization occurs before body extraction. Foreign Host/Origin, cross-site requests, bearer credentials, MCP sessions and share credentials are rejected. Mutations require CSRF. Responses, including errors, use `Cache-Control: no-store`.

| Route below `/api/remote-mcp` | Behavior |
|---|---|
| GET `/publications/` | List only active canonical-root bindings, with workspace ID and CSRF |
| POST `/publications/import` | Validate JSON; register active root only |
| POST `/publications/{id}/export` | Generate an allowlisted client/transport configuration |
| POST `/publications/{id}/check` | Bounded unauthenticated public endpoint probe |
| DELETE `/publications/{id}` | Remove local export registration only |

Mutation/export requests bind an opaque workspace ID and public configuration revision. Export revisions include public client metadata. The hub read lock prevents workspace switching midway through a request. Foreign publication IDs and stale revisions fail closed. Frontend switch handling aborts pending requests, clears preview/feedback and ignores late responses. Copy and download revalidate the backend snapshot.

The probe accepts no caller-supplied URL, follows no redirects, pins resolved public addresses, sends no credentials, invokes only `initialize`, limits metadata to 16 KiB and bounds total duration to 5 seconds. A matching 401 OAuth challenge plus protected-resource metadata reports reachable/sign-in required, without claiming project authorization. Private-network endpoints are still copyable.

## Client output and limits

Pure backend generation produces OAuth HTTP presets or JSON-array stdio arguments. VS Code gets `.vscode/mcp.json` schema; Cursor gets `.cursor/mcp.json`; Claude Code gets `.mcp.json`; Cline defaults to the Ax proxy and has a direct `streamableHttp` preset with an explicit compatibility requirement. ChatGPT returns a URL, without a JSON download.

Exports contain no secrets and do not issue credentials, grant access, change tool permissions, enable writes or start services. Native login commands are POSIX quoted and available only when a known public native Ax ID is registered. Windows stdio configuration uses the same JSON argument array; PowerShell users adapt quoted login arguments as documented. Missing IDE registrations are explained, with the proxy available as fallback.

Actual external OAuth compatibility remains dependent on the receiving client surface/version, account availability and provider registration. Local fake-provider tests validate Ax's auth boundary and transport; they do not claim live authorization in every third-party client.

## Acceptance criteria

- Active root matches explicit server IDs; same names, symlink aliases and multiple publishers behave correctly.
- Import registers only the active project; CLI reconciliation removes omitted bindings.
- Root conflicts and live snapshot changes fail without partial writes.
- Lease takeover and old cleanup preserve the newer instance.
- Owner, CSRF, readonly, bearer/share, stale workspace and foreign publication cases fail closed.
- Client JSON schemas, command arguments, public ID separation and secret exclusion are tested.
- Copy/download bytes equal the preview; clipboard denial offers manual selection.
- Workspace switching clears old content, including delayed responses and embedded IDE mode.
- Local runtime, public reachability and user authorization have distinct wording.
- Documentation and desktop/mobile screenshots accompany the feature.
- Frontend is rebuilt before the Ax binary; workflows/releases/deployments remain suspended.
