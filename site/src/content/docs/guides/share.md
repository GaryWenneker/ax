---
title: Share Command Center
description: LAN share with a token, PWA install, and live action stream.
banner:
  content: |
    This page describes part of the full ax, which is being phased out. <a href="/guides/phase-out/">ax-admin becomes the only visual interface</a>.
---

## `ax share`

Starts Command Center bound for LAN access with a random share token (**read-only**):

```bash
ax share --open
ax share --port 7070 --bind 0.0.0.0 --token mysecret
```

In read-only mode every route that writes answers 403, including importing agent logs into savings and syncing model prices. Reads keep working.

A browser that opens `?token=…` receives an `HttpOnly` `ax_share` cookie and continues at the same path without the token in the address. Scripts can send `Authorization: Bearer …` or the `ax_share` cookie.

The token is compared in constant time. A connection address that presents a wrong token (or a wrong `axc_` client credential) 20 times within a minute gets `429 Too Many Requests` for every credential it sends until that minute ends, the correct token included, and none of them is checked. A browser that keeps sending an old token can therefore lock itself out for up to a minute. Behind a tunnel or reverse proxy on the same computer every visitor arrives from `127.0.0.1`, so this limit is not applied to them. Failed client credentials are written to the security audit log. Node credentials (`axn_`) are only accepted by the replication server; the Command Center answers them with 401.

For remote collaborators, wrap localhost with a tunnel:

```bash
cloudflared tunnel --url http://127.0.0.1:7070
```

## Command Center UI

- **Settings → Sharing** — live session badge, port, copy base URL / CLI tip, How to share modal, Refresh/Retry
- Status-bar **Shared / Read-only** badge when a share token or read-only mode is active
- Themed HTML token gate (brand “ax”) when the token is missing
- **Install as app (PWA)** — Enable PWA, then Install / Add to Home Screen; dismissible with “Show hint again”

## PWA

Manifest + icons (`/icon-192.png`, `/icon-512.png`). Service worker is **opt-in** via Settings → Sharing → **Enable PWA**, `?pwa=1`, `localStorage ax-pwa-optin=1`, or an installed standalone window — so normal `ax web` always uses live APIs until you opt in.

`ax web --open` uses `http://127.0.0.1:PORT`. If `localhost` still shows empty pages from an old cached service worker, open `http://localhost:PORT/api/reset-client-cache` once, then reload.

### Mobile / phone

Narrow viewports: hamburger drawer, safe-area insets, status bar keeps **Project**, **Logging**, and **Activity**. After UI changes, run:

```powershell
.\scripts\web-ui-mobile-smoke.ps1
```

Screenshots: `crates/ax-web/web-ui/test-results/mobile-shots/`.

## Live action stream

| Endpoint | Purpose |
|----------|---------|
| `GET /api/actions/events` | SSE feed of workspace / agent events |
| `POST /api/actions/publish` | `{ "kind", "message", "meta?" }` |

Events appear in the StatusBar **Activity** chip (not a floating strip). With Verbose MCP on, the same events dual-write to Logging as `action` lines.

## Further reading

- [Command Center](/guides/command-center/)
- [MCP Logging & Quality](/guides/mcp-quality/)
- Repo docs: [`docs/SHARE.md`](https://github.com/GaryWenneker/ax/blob/main/docs/SHARE.md)
