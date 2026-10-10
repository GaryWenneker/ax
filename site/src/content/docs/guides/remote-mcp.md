---
title: Remote MCP and browser login
description: Run Ax on your own computer and retrieve project context from ChatGPT or another IDE through authenticated HTTPS.
---

Remote MCP exposes Ax's existing context tools through an authenticated HTTP endpoint. Ax can run on your Mac while ChatGPT or an IDE on another computer retrieves its rules, graph, memories and working context. The remote connection uses the same [Smart Output](/guides/smart-output/) dispatcher as local stdio.

This guide describes the remote MCP feature. Availability requires a binary built from the feature or a release containing it. It does not claim that a local build has already been published or that your OAuth provider has been configured.

## What runs where

| Component | Responsibility |
|---|---|
| Ax on your computer | Owns registered project roots, database access, tools, project grants and context sessions |
| Your existing Cloudflare Tunnel | Routes a public HTTPS hostname to the local Ax remote listener |
| OAuth authorization server | Signs users in, grants consent and issues Ax access/refresh tokens |
| ChatGPT | Connects as an OAuth MCP client and invokes the permitted tools |
| Another IDE | Connects directly over HTTP when supported, or uses Ax's authenticated stdio bridge |
| Netlify documentation site | Hosts this guide; it does not run your local Ax instance |

The Mac must remain powered on and reachable. Sleep, a stopped tunnel or a disconnected network makes the local server unavailable. Remote MCP does not copy your project to Netlify or turn Ax into a language model.

## Configure the server

Initialize each project normally with `ax init`. Create a private JSON configuration outside your repository. Paths and subjects below are examples; replace them with your actual canonical project root and OAuth subject.

```json
{
  "public_url": "https://ax.example.com",
  "oauth": {
    "issuer": "https://identity.example.com/",
    "jwks_uri": "https://identity.example.com/.well-known/jwks.json"
  },
  "projects": [
    {
      "id": "my-project",
      "root": "/Users/you/work/my-project",
      "subjects": ["provider|your-user-id"],
      "write_subjects": []
    }
  ],
  "api_keys": [],
  "allowed_origins": []
}
```

- `public_url` is the stable external HTTPS origin, without a path or query. It is also the OAuth resource/audience. Local ephemeral port numbers never become the public resource identifier.
- `issuer` must exactly match the access token's `iss`, including a significant trailing slash. The configured JWKS endpoint supplies public signing keys. This implementation accepts **RS256 JWT access tokens with `kid`**; opaque tokens and other algorithms are not supported.
- `subjects` grants the named OAuth identities access to this project. No implicit access is granted to other signed-in users.
- `write_subjects` must be a subset of `subjects`. Memory/policy writes additionally require the `ax:write` token scope.
- `allowed_origins` accepts explicit HTTPS browser origins. Requests without an Origin header can still come from authenticated native clients; present foreign Origin headers are rejected.
- Each root must already contain `.ax/ax.db`. Project IDs must be unique and URL-safe. Remote calls cannot override a root or switch the Command Center's current workspace.

Start the dedicated remote listener:

```bash
ax mcp remote serve --config /absolute/path/remote-mcp.json --port 0
```

Port `0` asks the operating system for an available local port. Startup emits JSON with the **actual assigned port**, local URL, public base URL and each project MCP URL. It binds to `127.0.0.1`. An explicitly requested occupied port fails without terminating the other listener.

For the example, connect clients to:

```text
https://ax.example.com/projects/my-project/mcp
```

Keep the existing Command Center local. Do not expose all of `ax web` as the remote MCP service; its workspace switching and administration APIs serve a different purpose.

## Dynamic ports and your existing Cloudflare route

If Cloudflare's configured TCP upstream contains the selected port, it must be updated whenever that port changes. Printing a new port alone does not update a pre-existing tunnel.

On macOS and Linux, a stable Unix socket can keep the tunnel's upstream constant while the HTTP listener uses a dynamic TCP port:

```bash
ax mcp remote serve \
  --config /absolute/path/remote-mcp.json \
  --port 0 \
  --socket /absolute/path/ax-remote.sock
```

Point the existing tunnel's service at:

```yaml
service: unix:/absolute/path/ax-remote.sock
```

Ax forwards the socket's HTTP traffic to the actual assigned TCP listener. The socket is owner-only; run `cloudflared` under the same user or arrange an appropriate local permission setup. Ax refuses to overwrite an existing socket. After an unclean shutdown, verify that its owner is stopped before removing a stale socket.

Windows uses the TCP upstream path; Unix sockets in this feature are macOS/Linux only. Dynamic port allocation addresses listener collisions. It does not guarantee that every VPN, firewall, DNS or routing configuration will allow the tunnel's network connection.

The server does not rewrite your Cloudflare configuration or create another tunnel. OAuth discovery and authenticated MCP traffic must reach Ax through the route. If you also use Cloudflare Access, configure it so its separate browser login or custom service-token headers do not block the MCP OAuth client.

## OAuth provider setup

Use an established OAuth provider. Ax implements the resource server and native client; it does not provide its own account database or authorization server.

Configure the provider to:

1. Issue RS256 access tokens for resource/audience `https://ax.example.com`, with `iss`, `aud`, `sub`, `exp` and space-separated `scope` claims.
2. Define `ax:context` and optional `ax:write` scopes. Grant the actual user access to the configured resource and scopes.
3. Publish OAuth authorization-server metadata or OIDC discovery with the exact issuer, authorization endpoint, token endpoint and `code_challenge_methods_supported: ["S256"]`.
4. Register a **public native Ax client** using Authorization Code + PKCE S256 and no embedded client secret. Its loopback callback is `http://127.0.0.1:<assigned-port>/callback`. The provider must support changing loopback ports for native clients. A provider that requires one fixed exact callback port needs configuration compatible with this native-client requirement.
5. Permit `offline_access` and refresh-token rotation if you want automatic refresh. Ax can also connect without a refresh token; reconnect when its access token expires.
6. Configure a separate ChatGPT client using the provider's supported registration method and the exact client metadata/redirect URL shown by ChatGPT. Provider support for CIMD, DCR or pre-registration is a deployment responsibility.

Ax's public discovery endpoint is:

```text
https://ax.example.com/.well-known/oauth-protected-resource
```

It advertises the canonical resource, configured authorization server and `ax:context`. Unauthenticated MCP requests receive `401` plus a `WWW-Authenticate` metadata challenge. Tokens are validated on every request, followed by project and tool permission checks. A token for GitHub, a Cloudflare Access cookie or a login ID token is not automatically an Ax access token.

## Browser login from CLI or Settings

The interaction follows a familiar desktop account-connection flow:

1. Enter the project MCP URL and native client ID.
2. Select **Connect with browser**, or run the CLI login command.
3. Sign in and consent in the system browser.
4. Return to Ax. Ax exchanges the code, verifies access to the selected MCP project and stores credentials in the OS credential store. You can cancel a pending sign-in from Settings or with Ctrl+C in the CLI.

```bash
ax mcp remote login \
  --url https://ax.example.com/projects/my-project/mcp \
  --client-id YOUR_NATIVE_CLIENT_ID

ax mcp remote status

ax mcp remote logout \
  --url https://ax.example.com/projects/my-project/mcp
```

Add `--write` to CLI login only when memory/policy writes are wanted and granted. The Settings connection requests context access by default.

In Command Center, open **Settings → Sharing → Remote Ax connections**. The IDE's embedded Command Center uses this same component. Browser blocking has an explicit sign-in link fallback. Invalid state or issuer is rejected; cancellation, timeout, missing project permission and unavailable credential storage produce errors rather than a connected confirmation.

Access and refresh tokens are kept in the OS credential store, with no plaintext fallback. A local metadata file records connection URLs/client IDs/expiry, without tokens. “Credentials saved” is stored state, not a continuous network health check. Disconnect removes local credentials and attempts provider revocation when the provider publishes that endpoint.

The Settings login API accepts only the local Command Center Host/Origin and requires a CSRF token for changes. It is unavailable in a read-only share session.

## Connect an IDE on another laptop

If your MCP client supports remote HTTP with OAuth, enter the project URL and use its own supported login flow. Ax does not assume every editor supports the same custom headers or configuration syntax.

For stdio-only clients, first log in on that laptop, then configure its MCP process as:

```json
{
  "mcpServers": {
    "ax-remote": {
      "command": "ax",
      "args": [
        "mcp", "remote", "proxy",
        "--url", "https://ax.example.com/projects/my-project/mcp"
      ]
    }
  }
}
```

The bridge sends authenticated HTTPS requests, preserves JSON-RPC responses, forwards the transport session and refreshes a saved token before expiry when possible. Its logs go to stderr; stdout carries MCP responses only. Expired transport sessions require reconnecting the bridge.

An IDE that can securely supply a custom Bearer header can alternatively use a long random API key. Add only its SHA-256 hex hash, a unique key ID and explicit project IDs to `api_keys`:

```json
{
  "id": "other-laptop",
  "sha256": "REPLACE_WITH_64_HEX_CHARACTERS",
  "projects": ["my-project"],
  "write": false
}
```

This is not a valid literal key hash. Generate and store the original key outside version control. API-key access does not provide the browser login experience and is **not** the ChatGPT authentication route.

## Connect ChatGPT

Use the project URL, such as `https://ax.example.com/projects/my-project/mcp`, rather than the origin or local port.

1. Open **ChatGPT Plugins**, select **+ → Add custom MCP server**, and enter a name such as **Ax — my-project**.
2. Choose the public HTTPS connection and enter the project MCP URL.
3. Select OAuth. Use the provider's supported client registration method: CIMD, DCR, or a predefined client. Copy the exact client metadata URL and callback URL shown by ChatGPT into your provider's configuration when needed. The native Ax CLI client is a separate registration.
4. Complete the connection and sign in at the OAuth provider. Grant `ax:context`; request `ax:write` only for accounts with an explicit write grant in Ax.
5. Review the discovered tools, install the resulting plugin, and start a new conversation with it selected using `@`.
6. Ask for `ax_preflight`, then retrieve one known symbol or memory. Check that the active project is correct.

Account and workspace settings control custom MCP availability. See the [official connection guide](https://developers.openai.com/plugins/deploy/connect-chatgpt) if your surface differs. After upgrading Ax, restart the server, open its ChatGPT connection and select **Refresh**, then start a new conversation. Refresh updates discovery; it does not configure the authorization server.

After connecting, invoke `ax_preflight` first each turn, then the graph, policy or memory tools needed for the task. A successful server build does not establish that ChatGPT is already connected; verify the real connection independently.

ChatGPT's documented MCP authentication flow uses Authorization Code + PKCE. It cannot be configured to send your arbitrary customer API key as a substitute for this login.

## Tools, projects and conversations

The remote server exposes graph/context retrieval, preflight, rules/skills, scoped conversation context and memory retrieval. `ax_remember` and policy-capture save need both `ax:write` and an explicit write grant. Index/sync, LSP, ship, global sharing and shell-running tools are unavailable remotely in this feature. Listing a smaller catalog does not authorize hidden tool calls: direct calls use the same permission checks. Protected tools advertise OAuth security schemes, and denied write calls return the MCP authentication challenge used by ChatGPT.

Each transport session is bound to the authenticated principal and selected project. Project A's session cannot be used on project B or by another principal. Sessions are capped and expire after 30 minutes of inactivity.

Remote chat IDs are issued by the server. Pass the `session` from preflight on subsequent calls. Omit it for a new chat. Foreign chat IDs are rejected; local Cursor hooks are never used to identify a remote conversation. Keep the returned context hash only while the model still holds its snapshot.

The HTTP endpoint supports Ax's existing **2025-03-26, 2025-06-18 and 2025-11-25** Streamable HTTP behavior with initialize and a transport session. It does not claim the revised 2026-07-28 protocol. POST returns JSON; notifications return empty 202; authenticated DELETE closes the transport session. There is no standalone GET notification stream.

## Smart Output, preflight and token use

The remote layer changes transport and authorization, not Ax's default output representation. PR 8 introduced the Smart Output response architecture:

- `content.text` contains compact Markdown/plain text, numbered source where applicable and actionable errors.
- `structuredContent` contains tool-specific IDs, locations, counts and context-budget diagnostics, without repeating large bodies by default.
- Preflight selects mandatory policy, matched skills, relevant memory references and working context. Unchanged rules can be skipped within the same retained chat context.
- Recall by text returns bounded summaries and memory IDs. Recall by ID returns the complete memory.
- Large cached answers can be expanded on demand. Cache and durable ownership checks remain project-bound; legacy unscoped records do not receive guessed owners.
- After compaction, call `ax_preflight` with the same durable `session` and `context_reset: true` (or a new `context_epoch`). Required policy and working notes are resent. Retrieve additional missing content with `ax_rules`, `ax_skill`, `ax_recall` or `ax_expand`.

See [Smart Output](/guides/smart-output/), [Memory Vault](/guides/memory/) and [MCP Server reference](/reference/mcp-server/) for the full contract.

HTTP/authentication/tunnel transport does not need a second language model. Model-context cost comes from tool definitions and results the client includes in its prompt. Ax itself uses CPU, RAM, database storage and networking. Existing optional explore synthesis can incur an external model call when configured. Exact RAM use, token savings and provider/tunnel fees require measurements; transmitted response bytes are not automatically billed token usage.

## Verify your tunnel before signing in

The startup JSON reports the actual local port and project URLs. For TCP, point your existing tunnel service to `http://127.0.0.1:ACTUAL_PORT`. For macOS/Linux's stable socket, use `unix:/absolute/path/ax-remote.sock`. Route the entire hostname to the dedicated listener so discovery and project paths both arrive at Ax.

```bash
curl --fail https://ax.example.com/.well-known/oauth-protected-resource
curl -i -X POST https://ax.example.com/projects/my-project/mcp \
  -H 'Content-Type: application/json' \
  -H 'Accept: application/json, text/event-stream' \
  --data '{"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2025-11-25","capabilities":{},"clientInfo":{"name":"manual-check","version":"1"}}}'
```

Discovery returns the configured resource/issuer, never credentials. The unauthenticated POST must return HTTP 401 and a challenge pointing to your public discovery URL. Cloudflare sign-in HTML, a 502, or the Command Center UI indicates a routing/access issue to resolve before OAuth.

Run native browser login on the laptop that will use the connection: its callback is local to that laptop. Logging in on the server Mac does not sign in another IDE automatically.

## Troubleshooting and verification

| Result | Meaning and next step |
|---|---|
| 401 | Missing/invalid/expired token or wrong signature/issuer/audience. Reauthorize against the advertised issuer/resource. |
| 403 | Account/project/scope, Origin or chat ownership denied. Inspect the explicit grant and requested scope. |
| 404 for session | Unknown/expired transport session. Reconnect; do not reuse a session from another project. |
| 400 protocol mismatch | Send the version negotiated by initialize with subsequent requests. |
| 429 | Session/concurrency limit reached. Close unused sessions or retry after active requests finish. |
| Credential store unavailable | Unlock/configure the OS credential store. Ax does not write plaintext credentials as a workaround. |
| Tunnel cannot reach Ax after restart | Update a TCP upstream to the emitted actual port, or use the stable Unix socket option on macOS/Linux. |
| OAuth works but connection fails | Check Ax's subject/project grant and that the access token targets the configured Ax resource. |

Test a real deployment in order: local server readiness; unauthenticated metadata challenge; browser login; authenticated initialize/list/preflight; two-project isolation; then the actual IDE and ChatGPT connections. Repository interface tests and local builds cover implementation behavior, while real-provider, Cloudflare and ChatGPT checks need the deployment's actual configuration.

## Protocol references

- [OpenAI plugin authentication](https://developers.openai.com/plugins/build/auth)
- [MCP Streamable HTTP 2025-11-25](https://modelcontextprotocol.io/specification/2025-11-25/basic/transports)
- [MCP protocol changes in 2026-07-28](https://modelcontextprotocol.io/specification/2026-07-28/basic/transports/streamable-http)
- [Cloudflare Tunnel service protocols](https://developers.cloudflare.com/cloudflare-one/networks/connectors/cloudflare-tunnel/routing-to-tunnel/protocols/)
