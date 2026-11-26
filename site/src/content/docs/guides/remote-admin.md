---
title: Remote admin (ax-admin)
description: A standalone application with the Command Center look that manages any Ax, including the headless build, through its remote MCP endpoint.
---

`ax-admin` is a separate application for looking after an Ax that runs somewhere else: in a container, in a sandbox or on another computer. It has the look of the Command Center, but it is not part of `ax` and does not need `ax` installed.

The headless Linux build has no `ax web`. `ax-admin` is the interface for it, and it works the same against a full Ax.

![The Operations page of ax-admin connected to a headless Ax](/screenshots/ax-admin-operations.png)

## How it connects

```text
browser (this computer only)  ->  ax-admin  ->  HTTPS  ->  ax mcp remote serve
                                                           /projects/<id>/mcp
```

- The only thing `ax-admin` talks to is the environment's [remote MCP](/guides/remote-mcp/) address. It uses no `ax web` API, no replication API and no local `.ax` database.
- The `ax-admin` process makes the calls, not the browser page. The server therefore needs no CORS setting and no `allowed_origins` entry.
- The interface is served on `127.0.0.1` only. There is no option to bind another address.
- Pages follow the tools the connected server offers. An Ax without a tool simply shows fewer pages, so one `ax-admin` works with different Ax versions.

## Install

macOS and Linux:

```bash
curl -fsSL https://getax.wenneker.io/install-admin.sh | sh
```

Windows (PowerShell):

```powershell
irm https://getax.wenneker.io/install-admin.ps1 | iex
```

The installer puts one binary, `ax-admin`, in `/usr/local/bin` (as root) or `~/.local/bin`, and on Windows in `%LOCALAPPDATA%\ax-admin\bin`. It verifies the download against the release's `SHA256SUMS`. It never installs, replaces or removes `ax`.

Options for `install-admin.sh`: `--version vX.Y.Z`, `--bin-dir DIR` and `--uninstall`. For `install-admin.ps1`: `-Version vX.Y.Z` and `-Uninstall`. Run the installer again to upgrade. Uninstalling removes the binary and leaves saved connections and keys in place.

To build from source instead (Node.js and Rust required):

```bash
cargo build --release -p ax-admin
```

## Start

```bash
ax-admin
```

This serves the interface on `http://127.0.0.1:7080/` and opens your browser. Use another port, or do not open the browser:

```bash
ax-admin --port 0 --no-open
```

Port `0` picks a free port; the address is printed. Stop the application with Ctrl+C.

## First start: connect to an environment

With no saved connection, `ax-admin` shows only the connect screen. Nothing else is reachable until one environment is connected. You need at least:

- **Address**: the project MCP address, `https://your-host/projects/<project>/mcp`. Plain `http://` is accepted only for an address on this computer.
- **Credentials**: an API key, or a browser sign-in.

**Test** checks the address and credentials without saving. **Connect** checks them and saves the connection only when the server accepts them. A wrong key, an unreachable host, an address that is not an Ax server and an address that redirects each give their own message. A redirect is never followed, so a key is never sent to another address.

![The connect screen shown on first start](/screenshots/ax-admin-connect.png)

### API key

Paste a key whose SHA-256 hash is listed under `api_keys` in the server's remote MCP configuration. See [Give a key the admin grant](#give-a-key-the-admin-grant) for a key that also shows the management pages.

### Browser sign-in

Choose **Browser sign-in**, enter the public client ID of the environment and select **Sign in with browser**. Finish the sign-in in the tab that opens; `ax-admin` continues when access is granted, and saves the connection only then. **Paste connection details** accepts the text that **Copy connection details** produces in a Command Center and fills in the address and client ID.

Tick **Also ask for admin access** when your account has the admin grant and you want the management pages; it asks for write access as well. An environment whose authorization server does not issue the admin scope refuses such a sign-in, so leave it off there.

Browser sign-in needs an `https://` address. It uses the same sign-in as `ax mcp remote login` and shares its stored tokens for that address: removing the connection in `ax-admin` also signs out `ax mcp remote` for that address on this computer. Closing or cancelling the sign-in before it finishes stores nothing.

## Several environments

The **Connections** button in the title bar is always visible. It lists the saved environments and lets you:

- **Add connection**: another address and credentials.
- **Switch to this**: make another environment the one you are looking at. The pages reload from that server.
- **Test**: check a saved connection without retyping its key.
- **Edit**: change the name, the address or the key. A changed key is checked before it replaces the saved one. A saved key is only ever sent to the address it was saved for, so another address needs its key entered again.
- **Remove**: delete the connection and its stored credential from this computer. The environment itself is not changed.

![The Connections panel with saved environments](/screenshots/ax-admin-connections.png)

After a restart, `ax-admin` opens the environment that was active last.

List the saved connections from a terminal (names and addresses only):

```bash
ax-admin connections
```

## Where credentials are kept

| What | Where |
|---|---|
| Names, addresses, which one is active | `~/.ax-admin/connections.json` (readable by you only). Set `AX_ADMIN_HOME` to use another folder |
| API keys | The operating system's credential store: macOS Keychain, Windows Credential Manager or the Linux Secret Service |
| Browser sign-in tokens | The same credential store, shared with `ax mcp remote login` |

A key is never written to a file, a log or the browser, and the interface never shows a saved key. There is no plaintext fallback: when the credential store is unavailable, saving a connection fails with a message.

## Pages

| Page | Shown when the server offers | What it does |
|---|---|---|
| Status | `ax_status`, `ax_cache_status` | Index statistics, freshness and cache counts |
| Search | `ax_search` | Full-text symbol search |
| Nodes | `ax_node`, `ax_callers`, `ax_callees`, `ax_impact` | Source, callers, callees and impact of one symbol |
| Explore | `ax_explore` | Ask how something works |
| Insights | `ax_insights`, `ax_report` | Subsystems, most connected symbols, architecture report |
| Rules | `ax_rules` | Policy rules, and which rules match a prompt |
| Skills | `ax_skill` | Open a skill by name |
| Memory | `ax_recall`, `ax_history`, `ax_remember` | Search memories and history. **Add memory** appears only with write access |
| Sessions | `ax_session`, `ax_durable` | Working context and durable transcripts of this connection |
| Operations | `ax_sync`, `ax_index`, `ax_policy_index` | Sync, rebuild the index, reload policy. Each one asks first and names the environment |
| Files | `ax_files` | Indexed files |
| Usage | `ax_cost`, `ax_tool_economics`, `ax_budget`, `ax_optimization_advice` | Cost, tool economics, budget and advice |
| Logging | `ax_logs` | MCP calls handled for this project: time, tool, result, duration, size |
| Tools | always | Every tool the server offers, with a form built from its input schema |

Operations, Files, Usage and Logging are the management pages. They appear only when the connection has the admin grant. The status bar shows the access level of the active connection: read, write or admin.

The **Tools** page runs any tool the server lists, including tools this version of `ax-admin` has no page for. It asks first for the three tools that change the index.

Every page belongs to the environment it was opened for. If another window switches `ax-admin` to a different environment, a page that still shows the old one refuses to run anything and reloads.

## Give a key the admin grant

Management tools are off unless the server's remote MCP configuration grants them. Add `"admin": true` to an API key; it also needs `"write": true`:

```json
{
  "id": "ops-laptop",
  "sha256": "REPLACE_WITH_64_HEX_CHARACTERS",
  "projects": ["my-project"],
  "write": true,
  "admin": true
}
```

For an external OAuth provider, list the subject under `admin_subjects` of the project (it must also be in `write_subjects`) and issue the token with the `ax:admin` scope. Restart `ax mcp remote serve` after changing the configuration. See [Admin access](/guides/remote-mcp/#admin-access) for the rules.

## Limits

- `ax-admin` cannot change server settings, edit rules or skills in an editor, edit specifications, show the graph canvas, or run ship, LSP or a shell command on the server. The remote MCP server offers no tools for those.
- The built-in authorization server of a host set up from a Command Center issues context and write access only. Use an API key, or an external OAuth provider, for admin access.
- `ax mcp remote serve` listens on `127.0.0.1`. Making it reachable over HTTPS (an ingress sidecar or a tunnel) stays part of deploying the environment. A static host such as Netlify cannot run Ax.
- Management calls wait up to ten minutes for the server; other calls just over two minutes. An index run that takes longer keeps running on the server: `ax-admin` then says the server is still working on it, and Status and Logging show when it is done.
- While an index operation runs, other calls to the same project wait for it. One runs at a time per project, and a server accepts a few of them at once across all projects.
- Reload policy re-imports the project's own policy files. Restoring removed rules and the policy history are not available remotely.

## Troubleshooting

| Message | Meaning and next step |
|---|---|
| The server rejected these credentials. | The key or token is wrong or expired. Check the key against the server's `api_keys`, or sign in again |
| Access denied by the server | The credential is valid but has no grant for this project |
| The server does not know this project. | The project in the address is not in the server's configuration |
| The address did not answer as an Ax remote MCP server | Something else answers there: a login page, a proxy error. Check the route to `ax mcp remote serve` |
| The address redirects somewhere else. | Enter the final address. Credentials are not sent to a redirect |
| The server could not be reached. | DNS, network or TLS. Check the address from a terminal with `curl` |
| The key for this connection is no longer in the OS credential store. | Edit the connection and enter the key again |
| The server is still working on this. | An index run outlasted the wait. It continues on the server; look at Status or Logging in a moment |
| Enter the key again to use another address. | A saved key is not sent to a different address. Type the key for the new address |
| … is the active environment now. Reload to continue. | Another window switched environments. The page reloads for the active one |
