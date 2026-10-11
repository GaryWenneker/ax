# Remote MCP and browser OAuth

Status: implementation and validation in progress. Original baseline: main `730699d`, Ax 8.0.0. Integration baseline: main `3fe6546` (PR #10).

## Goal

Run Ax on one machine and use its project context from ChatGPT and another IDE through the owner's existing Cloudflare Tunnel. Preserve the existing local stdio interface and Smart Output dispatcher. A browser sign-in started from CLI or Command Center must connect without pasting access tokens.

## Decisions

- Add a dedicated remote HTTP server under `ax mcp remote serve`; do not expose the existing unrestricted Command Center API through the tunnel.
- Use an established OAuth authorization server. Ax is the resource server and native client, not a new identity provider. The deployment supplies issuer, audience, JWKS and native client registration. Test with a mock issuer; report real-provider and ChatGPT checks separately.
- Authorization Code + PKCE S256, random state, exact issuer validation, loopback callback on an OS-assigned port, short-lived access tokens, refresh rotation when offered. Store credentials in the OS credential store; no plaintext fallback. CLI and the embedded IDE/web Settings use the same connection service.
- One public HTTPS base URL; project URLs `/projects/{id}/mcp`. JWT audience is the configured base resource. Public protected-resource metadata advertises the actual issuer and required scope. ChatGPT's provider registration/callback comes from its connection management page, not a hardcoded redirect.
- Native client uses pre-registered client ID. ChatGPT uses the issuer's supported CIMD/DCR/pre-registration. Never treat a GitHub login token or Cloudflare Access login cookie as an Ax access token.
- Bind TCP to `127.0.0.1:0` by default; read `listener.local_addr()` after binding. Explicit occupied ports fail without killing processes. Report actual port and public URL in machine-readable runtime state.
- On macOS/Linux an optional stable Unix socket forwards to the dynamic listener. Cloudflare can target `unix:/absolute/socket/path`; no fixed TCP gateway port is needed. Also support ordinary TCP upstream configuration, with explicit restart instructions if its port changes. Do not rewrite the user's existing Cloudflare configuration.
- Register projects by stable ID and canonical local root. Grant OAuth subjects explicit project access. No default access. A route never switches the global WebHub or accepts an arbitrary root supplied by a caller.
- Validate token signature, allowed RS256 algorithm, issuer, resource audience, expiry/nbf, scopes and project grant on every request. Optional hashed API keys are IDE-only, separately identified and project-scoped; ChatGPT uses OAuth.
- Preserve legacy Streamable HTTP revisions already supported by Ax (2025-03-26, 2025-06-18, 2025-11-25). Do not claim support for the changed 2026-07-28 protocol without implementing it. Return the actual supported version. Legacy transport sessions are bound to principal and project, bounded and idle-expiring.
- Per-connection engines preserve preflight delivery state. Remote chat IDs are server-issued, connection-owned and never inferred from local Cursor hooks. Reinitialize or new preflight without a chat ID sends required context again. Foreign session and cache handles fail.
- Remote tool discovery and invocation share a strict allowlist. Context retrieval, policy, graph and scoped session tools are available. Memory/policy writes require additional scope and grant. Shell-running/index/ship/LSP/global-sharing tools remain inaccessible remotely in this feature.
- Return the existing text + structured metadata MCP envelope; do not duplicate response bodies or invoke an extra model for remote transport. Existing optional explore AI behavior stays documented.
- Bound request size, sessions and concurrency; validate Origin when present; use no-store responses; keep credentials and authorization codes out of logs. Health returns only readiness.

## Interfaces

CLI commands: `ax mcp remote serve --config FILE [--port 0] [--socket PATH]`, `ax mcp remote login --url HTTPS_PROJECT_MCP --client-id ID`, `status`, `logout`, and `proxy` for stdio-only IDEs. Proxy attaches stored credentials, refreshes when needed, preserves JSON-RPC and forwards a remote transport session.

Command Center Settings: server URL and client ID, Connect, Connected/Expired/Not connected, account/project identity when verified, Disconnect, readable errors and browser fallback link. Settings backend is loopback-only, rejects foreign Origin/Host and uses a per-page CSRF token for mutations. Never display tokens. Existing IDEs embed this same Settings screen, avoiding divergent login implementations.

HTTP: public protected-resource discovery; authenticated project MCP POST/DELETE; GET can return 405 because no standalone notifications are advertised. `initialize` returns a legacy transport session. `tools/list`, `tools/call`, `ping`, and initialized notification obey supported-version rules. Invalid tokens 401 with metadata challenge; insufficient project grant/scope 403; tool write-scope errors carry `isError` and `_meta.mcp/www_authenticate`; unknown session 404; bad protocol/request 400; no notification body on 202.

## File map

Extend `crates/ax-mcp` with `remote/{config,auth,server,client,credentials}.rs` and export the module. Reuse `server::handle_request`, `engine.rs` and `smart_output.rs`. Add a remote-context flag rather than reading local hook state. Wire commands in `crates/ax-cli/src/main.rs`; add scoped Settings API in `crates/ax-web`, and `RemoteMcpSettingsSection.tsx` in its web UI. Add integration tests, website guide `site/src/content/docs/guides/remote-mcp.md`, sidebar navigation and links from Smart Output/MCP reference.

## Implementation order

1. Commit this specification on `feature/remote-mcp-oauth` before implementation.
2. Implement configuration validation, auth/project ACL, HTTP/session routing and dynamic listener.
3. Implement native browser login, secure credentials, refresh/logout and remote stdio bridge.
4. Connect CLI and the shared IDE/web Settings UI.
5. Extend website docs for this feature and PR 8's preflight/recall/cache/compaction behavior.
6. Run interface tests and build checks; fix failures; review the resulting diff; publish a PR with precise evidence. Never merge while checks are pending.

## Acceptance tests

| Area | Required evidence |
|---|---|
| Dynamic listener | Port 0 reports a nonzero assigned port; occupied port leaves original listener alive; restart resolves new actual port; Unix socket forwards HTTP on Unix |
| Auth | Missing, expired, wrong issuer/audience/signature/algorithm and ungranted tokens fail before engine access; discovery challenge is actionable |
| Isolation | Principal/project/session mismatch, unknown project, root overrides, foreign chat and cache IDs fail; simultaneous project connections stay separate |
| Tool ACL | Listing and direct invocation deny the same tools; read scope cannot remember/save; unknown tools cannot bypass discovery |
| Protocol | Real HTTP initialize/list/preflight/ping/error/notification/delete cycle; supported version enforced; stdio regression tests |
| OAuth client | PKCE/state/issuer rejection, cancellation/timeout, token exchange, refresh rotation, secure-storage failure and logout tested with local mock provider/storage |
| Settings | Browser link, connect completion/error/disconnect, no secret display, foreign Origin rejected; actual UI build and focused browser tests |
| Build/docs | Rust relevant tests + workspace build/clippy; UI TypeScript/build; website tests and Netlify build command; PR head CI status reported explicitly |

## Operational limits and documentation

The owner supplies OAuth issuer configuration and real project roots, and keeps the Mac awake for availability. Dynamic ports avoid bind collisions, not every VPN routing problem. External provider/tunnel charges and exact RAM/token savings require deployment measurements. No additional model is needed for transport/auth. Instrument request bytes and latency, and distinguish model context tokens from whole-wire bytes and billed usage. Netlify publishes website docs only after the branch is merged/deployed; a local successful site build is not a live deployment.

## Primary references

- https://developers.openai.com/plugins/build/auth
- https://modelcontextprotocol.io/specification/2025-11-25/basic/transports
- https://modelcontextprotocol.io/specification/2026-07-28/basic/transports/streamable-http (compatibility boundary)
- https://developers.cloudflare.com/cloudflare-one/networks/connectors/cloudflare-tunnel/routing-to-tunnel/protocols/
- https://docs.rs/oauth2/5.0.0/oauth2/
