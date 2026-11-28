---
title: Remote MCP and browser login
description: Serve an Ax project over authenticated HTTPS, so that ax-admin, ChatGPT or an IDE on another computer can use its tools.
---

Remote MCP exposes Ax's existing context tools through an authenticated HTTP endpoint. Ax can run on a server, in a container or on your own computer while ChatGPT, an IDE or [ax-admin](/guides/remote-admin/) on another computer retrieves its rules, graph, memories and working context. The remote connection uses the same [Smart Output](/guides/smart-output/) dispatcher as local stdio.

`ax mcp remote serve` is part of the [headless build](/guides/headless-linux/). A server is set up with a configuration file; see [Configure the server](#configure-the-server). A client gets in with an API key or with a browser sign-in at an external OAuth provider. For a ready-made server for one repository, see [Deploy to the cloud](/guides/headless-linux/#deploy-to-the-cloud).

This guide describes the remote MCP feature. Availability requires a binary built from the feature or a release containing it. It does not claim that a local build has already been published or that your OAuth provider has been configured.

The full ax, which is being phased out, can also generate a host from its Command Center, with a built-in authorization server where the owner approves each browser sign-in; see [Command Center](/guides/command-center/). A headless Ax has no place to approve those sign-ins.

## What runs where

| Component | Responsibility |
|---|---|
| Ax on your computer or server | Owns registered project roots, database access, tools, project grants and context sessions |
| Your existing Cloudflare Tunnel, or another HTTPS proxy or ingress | Routes a public HTTPS hostname to the local Ax remote listener |
| OAuth authorization server | Signs users in, grants consent and issues Ax access/refresh tokens |
| ChatGPT | Connects as an OAuth MCP client and invokes the permitted tools |
| Another IDE | Connects directly over HTTP when supported, or uses Ax's authenticated stdio bridge |
| ax-admin | Connects with an API key or a browser sign-in and shows the environment; see [ax-admin](/guides/remote-admin/) |
| Netlify documentation site | Hosts this guide; it does not run your local Ax instance |

The computer that runs Ax must remain powered on and reachable. Sleep, a stopped tunnel or a disconnected network makes the local server unavailable. Remote MCP does not copy your project to Netlify or turn Ax into a language model.

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
- `issuer` must exactly match the access token's `iss`, including a significant trailing slash. The configured JWKS endpoint supplies public signing keys. Ax caches them and refreshes them in the background every five minutes; when the endpoint is slow or down, tokens keep verifying with the cached keys. A token whose `kid` is not in the cache makes Ax fetch the keys once, so a rotated key works right away; such fetches happen at most once every 30 seconds. That token waits at most one second for the fetch, and a token that arrives while a fetch is running is refused at once, so a slow key endpoint never ties up the server for other callers. This implementation accepts **RS256 JWT access tokens with `kid`**; opaque tokens and other algorithms are not supported.
- `oauth.clock_skew_seconds` (optional, default `60`, at most `300`) is how far the identity provider's clock may differ from the Ax host's when a token's `exp`, `nbf` and `iat` are checked. A token issued by a provider whose clock runs a few seconds ahead is accepted right away. Set it to `0` to check those times exactly.
- `subjects` grants the named OAuth identities access to this project. No implicit access is granted to other signed-in users.
- `write_subjects` must be a subset of `subjects`. Memory/policy writes additionally require the `ax:write` token scope.
- `admin_subjects` (optional) must be a subset of `write_subjects`. See [Admin access](#admin-access).
- `client_address_header` (optional) names the header the proxy in front of Ax sets to the caller's address: `cf-connecting-ip` or `x-forwarded-for`. It only shares out the limits on the unauthenticated sign-in endpoints; leave it out when requests can reach Ax without passing that proxy.
- `allowed_origins` accepts explicit HTTPS browser origins (scheme, host and optional port; no path, query or fragment). Each entry is compared in its canonical form, so `https://App.Example.com:443` matches the `https://app.example.com` a browser sends. Requests without an Origin header can still come from authenticated native clients; present foreign Origin headers are rejected.
- Each root must already contain `.ax/ax.db`. Project IDs must be unique and URL-safe. Remote calls cannot override a root.

Start the dedicated remote listener:

```bash
ax mcp remote serve --config /absolute/path/remote-mcp.json --port 0
```

Port `0` asks the operating system for an available local port. Startup emits JSON with the **actual assigned port**, local URL, public base URL and each project MCP URL. It binds to `127.0.0.1`. An explicitly requested occupied port fails without terminating the other listener.

For the example, connect clients to:

```text
https://ax.example.com/projects/my-project/mcp
```

By default the listener binds `127.0.0.1` and serves plain HTTP. Put an HTTPS tunnel, proxy or ingress in front of it. Pass `--bind 0.0.0.0` only behind such an ingress, as the [hosted deployments](/guides/headless-linux/#deploy-to-the-cloud) do.

### API keys without an OAuth provider

The `oauth` block is required. A server that only hands out API keys names the built-in authorization server there and lists the keys:

```json
{
  "public_url": "https://ax.example.com",
  "oauth": {
    "built_in": true,
    "issuer": "https://ax.example.com/oauth",
    "jwks_uri": "https://ax.example.com/oauth/jwks"
  },
  "projects": [
    {
      "id": "my-project",
      "root": "/srv/my-project",
      "subjects": ["owner"]
    }
  ],
  "api_keys": [
    {
      "id": "other-laptop",
      "sha256": "REPLACE_WITH_64_HEX_CHARACTERS",
      "projects": ["my-project"],
      "write": false
    }
  ]
}
```

With `"built_in": true`, `issuer` must be `<public_url>/oauth` and `jwks_uri` must be `<public_url>/oauth/jwks`. `subjects` may not be empty. Browser sign-ins at the built-in authorization server are approved in the Command Center of the full ax. A headless Ax has no place to approve them, so with this configuration only the API keys get in. The `-server` container image writes a configuration of this shape.

`sha256` is the SHA-256 of the key in hexadecimal, never the key itself. For example:

```bash
KEY="$(openssl rand -hex 32)"
printf '%s' "$KEY" | sha256sum
```

Give the key to the client and keep only the hash in the configuration. See [Connect an IDE on another laptop](#connect-an-ide-on-another-laptop) for the client side and [Admin access](#admin-access) for a key that may manage the environment.

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

Ax forwards the socket's HTTP traffic to the actual assigned TCP listener. A socket connection stays open as long as bytes move in either direction; it is closed after ten and a half minutes without traffic, which is longer than the slowest admin tool call. Up to 32 socket connections are forwarded at once; further connections wait for a free slot. The socket is owner-only; run `cloudflared` under the same user or arrange an appropriate local permission setup. Ax refuses to overwrite an existing socket. After an unclean shutdown, verify that its owner is stopped before removing a stale socket.

Windows uses the TCP upstream path; Unix sockets in this feature are macOS/Linux only. Dynamic port allocation addresses listener collisions. It does not guarantee that every VPN, firewall, DNS or routing configuration will allow the tunnel's network connection.

The server does not rewrite your Cloudflare configuration or create another tunnel. OAuth discovery and authenticated MCP traffic must reach Ax through the route. If you also use Cloudflare Access, configure it so its separate browser login or custom service-token headers do not block the MCP OAuth client.

## External OAuth provider setup

With an external provider, the provider manages accounts and sign-in, while Ax validates its access tokens. This is the way to offer browser sign-in on a headless Ax. The following steps apply to a configuration whose `oauth` block names that provider's `issuer` and `jwks_uri`.

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

## Use Ax on this computer, or share the setup

When your app and the project are on the same computer, you do not need remote MCP. The app starts Ax itself over stdio. No remote server file, public hostname or OAuth ID is needed for this local connection. Add an `ax` server entry to the MCP configuration of Cursor, VS Code, Cline, Claude Code or another local MCP app, with `ax` as the command and `serve --mcp` as its arguments. Keep existing servers in that file, then reload the app. See [Manual setup](/reference/integrations/#manual-setup) and the [MCP Server reference](/reference/mcp-server/).

To share that local setup with a team, commit the entry to `.cursor/mcp.json` or `.vscode/mcp.json` in the project, with `ax` on PATH and `${workspaceFolder}` in place of this computer's paths. Each recipient installs Ax, opens their own project checkout and runs `ax init` there. This shares local setup; it does not give access to the project hosted on your computer. For hosted project access, use the remote flow below.

## Copy a project's remote configuration

A client needs one thing from the server: the project MCP URL, `<public_url>/projects/<project id>/mcp`. `ax mcp remote serve` prints each project URL in its startup JSON. The URL is built from `public_url` and the project ID in your server configuration, never from a folder name.

`ax mcp remote serve` registers its validated public export metadata automatically. You can register it without starting a server, indexing a project or signing in:

```bash
ax mcp remote register --config /absolute/path/remote-mcp.json --name "Team Ax"
```

To include public client IDs you have already registered with your OAuth provider:

```bash
ax mcp remote register --config /absolute/path/remote-mcp.json \
  --name "Team Ax" \
  --native-client-id YOUR_NATIVE_PUBLIC_CLIENT_ID \
  --cursor-client-id YOUR_CURSOR_PUBLIC_CLIENT_ID \
  --vscode-client-id YOUR_VSCODE_PUBLIC_CLIENT_ID
```

These IDs are separate OAuth registrations. No client secret belongs in the export. Adding client ID flags replaces the stored public registration map; omit all three flags to preserve it. Starting the server preserves your stored label and client IDs. Stop a running server before you register a different configuration for it. The CLI registers all explicitly configured projects and removes stale bindings omitted from a subsequent server snapshot. The catalogue retains canonical roots internally to match workspaces; exports exclude roots, subjects, grants, key hashes and tokens.

Write the client entry yourself. Where it goes and which transport it uses depends on the client:

| Client | Destination and transport |
|---|---|
| Cursor | `.cursor/mcp.json`, `mcpServers`; direct HTTP or Ax proxy |
| VS Code | `.vscode/mcp.json`, `servers`; `http` or `stdio` |
| Claude Code | `.mcp.json`, `mcpServers`, `http` |
| Cline | Cline MCP settings; Ax proxy by default, or verified direct `streamableHttp` |
| ChatGPT | Enter the MCP URL and follow the OAuth custom app setup; no JSON file |
| Other / Ax proxy | `mcpServers`, `ax` command with a JSON argument array |
| ax-admin | Enter the MCP URL and an API key or client ID on its connect screen; no JSON file |

Merge the server entry into your existing file. The Ax proxy entry is shown under [Connect an IDE on another laptop](#connect-an-ide-on-another-laptop). For the proxy, sign in on the receiving computer first with `ax mcp remote login`. In PowerShell, double single quotes inside quoted arguments.

A client entry does not issue credentials, authorize additional users or grant writes. Direct OAuth compatibility depends on the actual client surface/version and provider registration; valid JSON alone does not prove a successful login. To check that the public endpoint answers before anyone signs in, see [Verify your tunnel before signing in](#verify-your-tunnel-before-signing-in).

The full ax, which is being phased out, also builds these client entries for you, with copy and download actions and an endpoint check, under **Settings → Connections → Remote MCP** in its Command Center; see [Command Center](/guides/command-center/).

## Send and receive an Ax remote connection

To let someone connect, send them the project HTTPS address and, for browser sign-in, the public native Ax sign-in app ID (the native client ID registered with your OAuth provider). Neither is a secret. Do not send local paths, access or refresh tokens, project grants or your private server setup file. An API key is a secret: hand it over through a channel you trust, and only to the person who will use it.

On the receiving computer, use one of:

- **CLI:** `ax mcp remote login --url <project URL> --client-id <native client ID>`; see [Browser login from the CLI or ax-admin](#browser-login-from-the-cli-or-ax-admin).
- **ax-admin:** enter the address with the client ID or an API key on the connect screen; see [First start: connect to an environment](/guides/remote-admin/#first-start-connect-to-an-environment).
- **An IDE or ChatGPT:** enter the project URL in the client; see the sections below.

Receiving the details does not contact the server, sign in, store a token or grant project access. Each user must already be authorized for the remote project. Without a native Ax client ID, browser sign-in from the CLI or `ax-admin` is not possible; the server owner must supply it. An IDE's public client ID is not a substitute for the native Ax registration.

## Browser login from the CLI or ax-admin

The interaction follows a familiar desktop account-connection flow:

1. Have the project MCP URL and native client ID at hand.
2. Run the CLI login command, or choose **Browser sign-in** on the `ax-admin` connect screen.
3. Sign in and consent in the system browser.
4. Return to Ax. Ax exchanges the code, verifies access to the selected MCP project and stores credentials in the OS credential store. You can cancel a pending sign-in with Ctrl+C in the CLI.

```bash
ax mcp remote login \
  --url https://ax.example.com/projects/my-project/mcp \
  --client-id YOUR_NATIVE_CLIENT_ID

ax mcp remote status

ax mcp remote logout \
  --url https://ax.example.com/projects/my-project/mcp
```

Add `--write` to CLI login only when memory/policy writes are wanted and granted.

Invalid state or issuer is rejected; cancellation, timeout, missing project permission and unavailable credential storage produce errors rather than a connected confirmation.

Access and refresh tokens are kept in the OS credential store, with no plaintext fallback. A local metadata file records connection URLs/client IDs/expiry, without tokens. `ax mcp remote status` lists the saved connections; “Credentials saved” is stored state, not a continuous network health check. `ax mcp remote logout` removes local credentials and attempts provider revocation when the provider publishes that endpoint.

`ax-admin` uses the same sign-in and shares the stored tokens for that address with `ax mcp remote`; see [Browser sign-in](/guides/remote-admin/#browser-sign-in).

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

The bridge sends authenticated HTTPS requests, preserves JSON-RPC responses, forwards the transport session and refreshes a saved token before expiry when possible. Its logs go to stderr; stdout carries MCP responses only.

The bridge keeps running when one request fails. When the server answers with an HTTP error (for example 403 for a tool that is unavailable remotely, 429 when the server is busy, or 503 while the project is re-indexed), the IDE gets a JSON-RPC error for that request with code `-32000`, the server's message and `data.httpStatus`. A line that is not JSON gets a parse error (`-32700`), and a line over 128 KiB gets `-32600`. When the server rejects the token (401), the bridge refreshes it once and retries. When the server no longer knows the session (404), for example after a restart, the bridge starts a new session with the IDE's original `initialize` request and retries.

When a token refresh succeeds at the sign-in provider, the new tokens are saved right away, so a rotated refresh token is never lost. If the Ax server then answers 401 or 403, the refresh reports that the account lost access; if the server is briefly unavailable, the new token is used and the request reports the outage.

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

## Admin access

Management tools (re-index, sync, policy reload, file list, usage reports and the MCP call log) are not offered remotely unless a credential has the admin grant. The grant is explicit and per project:

- **API key:** add `"admin": true`. The key must also have `"write": true`.
- **External OAuth provider:** add the subject to the project's `admin_subjects` (it must also be in `write_subjects`) and issue the access token with the `ax:admin` scope next to `ax:context` and `ax:write`.

```json
{
  "id": "ops-laptop",
  "sha256": "REPLACE_WITH_64_HEX_CHARACTERS",
  "projects": ["my-project"],
  "write": true,
  "admin": true
}
```

A configuration that grants admin without write access is rejected at start-up. Both settings default to off, so an existing configuration file grants nothing new. Without the grant, the tool list and every response are the same as before, and a direct call to a management tool is refused.

The built-in authorization server issues `ax:context` and `ax:write` only; it cannot grant admin access. Use an API key for that.

The discovery document lists `ax:admin` under `scopes_supported` only when an external provider is configured and at least one project has an admin subject. Otherwise it is unchanged, so a client that asks for every advertised scope keeps working.

Remotely, `ax_policy_index` only re-imports the project's own policy files (`action: "index"`); its restore, history and audit actions are refused. An index run continues on the server when the request times out or the client disconnects; the 504 answer for that case carries `"running": true`. While it runs, other calls to that project get HTTP 503 with `Retry-After` after a two-second wait instead of queueing, so other projects keep their request slots. A server accepts four index-changing calls at once and answers 429 beyond that. On shutdown the server waits for a running index operation. The tools and their limits are in the [MCP Server reference](/reference/mcp-server/#management-tools-over-remote-mcp). [ax-admin](/guides/remote-admin/) is the application that uses them.

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

ChatGPT's documented MCP authentication flow uses Authorization Code + PKCE. It cannot be configured to send your arbitrary customer API key as a substitute for this login. With a headless Ax, ChatGPT therefore needs a server configured with an [external OAuth provider](#external-oauth-provider-setup).

## Tools, projects and conversations

The remote server exposes graph/context retrieval, preflight, rules/skills, scoped conversation context and memory retrieval. `ax_remember` and policy-capture save need both `ax:write` and an explicit write grant. A remote policy save writes only `.agents/rules/<id>.mdc` in the selected project: the rule id must be kebab-case, the scope must be `project`, and frontmatter that picks a location or a storage (`source`, `storage`, `rootId`) or that the rule schema does not know is refused with HTTP 403. Index, sync, policy reload, the file list, usage reports and the call log need the [admin grant](#admin-access). LSP, ship, global sharing and shell-running tools are unavailable remotely. Listing a smaller catalog does not authorize hidden tool calls: direct calls use the same permission checks. Protected tools advertise OAuth security schemes, and denied write calls return the MCP authentication challenge used by ChatGPT.

Each transport session is bound to the authenticated principal and selected project. Project A's session cannot be used on project B or by another principal. Sessions expire after 30 minutes of inactivity. One principal (an API key, a client token or a signed-in user) holds at most 8 sessions; a new one closes that principal's least recently used session. The server holds 128 sessions in all. It serves 16 requests at once, and one principal may have at most 4 of them in flight; a call that waits for its own busy session does not hold one of the 16 while it waits. A request takes those slots only after its token is accepted; token checks have their own limit of 32 at once, so requests without a valid token never make the server busy for signed-in callers. `axc_` client tokens are checked against the global database the server opened at startup.

Remote chat IDs are issued by the server. Pass the `session` from preflight on subsequent calls. Omit it for a new chat. Chat IDs and `ax_expand` cache IDs belong to the principal and project they were issued to, not to one transport session: after a reconnect (an expired session or a client restart) the same principal keeps using them. The server remembers the latest 4096 of each per principal and project, and forgets them when it restarts. A chat or cache ID that this principal was not issued, or that the server no longer knows, gets a tool error (`isError`) that says to omit `session` and start a new chat, or to run the query again. Local Cursor hooks are never used to identify a remote conversation. Keep the returned context hash only while the model still holds its snapshot.

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

Discovery returns the configured resource/issuer, never credentials. The unauthenticated POST must return HTTP 401 and a challenge pointing to your public discovery URL. Cloudflare sign-in HTML, a 502, or the page of another application indicates a routing/access issue to resolve before OAuth.

Run native browser login on the laptop that will use the connection: its callback is local to that laptop. Logging in on the server does not sign in another IDE automatically.

## Troubleshooting and verification

| Result | Meaning and next step |
|---|---|
| 401 | Missing/invalid/expired token or wrong signature/issuer/audience. Reauthorize against the advertised issuer/resource. |
| 403 | Account/project/scope or Origin denied. Inspect the explicit grant and requested scope. |
| 404 for session | Unknown/expired transport session. Reconnect; do not reuse a session from another project. |
| 400 protocol mismatch | Send the version negotiated by initialize with subsequent requests. |
| 429 | Session or request limit reached: the server is busy, or this principal already has 4 requests in flight. Retry after active requests finish. |
| Credential store unavailable | Unlock/configure the OS credential store. Ax does not write plaintext credentials as a workaround. |
| Tunnel cannot reach Ax after restart | Update a TCP upstream to the emitted actual port, or use the stable Unix socket option on macOS/Linux. |
| OAuth works but connection fails | Check Ax's subject/project grant and that the access token targets the configured Ax resource. |

Test a real deployment in order: local server readiness; unauthenticated metadata challenge; browser login; authenticated initialize/list/preflight; two-project isolation; then the actual IDE and ChatGPT connections. Repository interface tests and local builds cover implementation behavior, while real-provider, Cloudflare and ChatGPT checks need the deployment's actual configuration.

## Protocol references

- [OpenAI plugin authentication](https://developers.openai.com/plugins/build/auth)
- [MCP Streamable HTTP 2025-11-25](https://modelcontextprotocol.io/specification/2025-11-25/basic/transports)
- [MCP protocol changes in 2026-07-28](https://modelcontextprotocol.io/specification/2026-07-28/basic/transports/streamable-http)
- [Cloudflare Tunnel service protocols](https://developers.cloudflare.com/cloudflare-one/networks/connectors/cloudflare-tunnel/routing-to-tunnel/protocols/)
