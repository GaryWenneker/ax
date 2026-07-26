# Command Center share, PWA, live actions

## `ax share`

Starts Command Center bound for LAN access with a random share token (read-only):

```bash
ax share --open
ax share --port 7070 --bind 0.0.0.0 --token mysecret
```

Clients must pass `?token=…`, `Authorization: Bearer …`, or the `ax_share` cookie
set after the first successful request.

For remote collaborators, wrap localhost with a tunnel:

```bash
cloudflared tunnel --url http://127.0.0.1:7070
```

## Command Center UI

Settings → Sharing shows share status, plugin count, embed backend, and a dismissible
PWA install hint. The StatusBar shows a Shared/Read-only badge. Activity events live
in the StatusBar Activity chip.

## PWA

Manifest + icons ship with Command Center. The service worker is **opt-in**
(`?pwa=1` or an already-installed standalone window) so local `ax web` never keeps
a stale offline shell that can block live `/api/*` data.

`ax web --open` prefers `http://127.0.0.1:PORT` (browsers treat `localhost` as a
separate origin). If an old service worker left `localhost` empty, open
`http://localhost:PORT/api/reset-client-cache` once to clear that origin’s cache
and service workers, then reload.

### Mobile / phone

On narrow viewports Command Center uses a hamburger drawer, safe-area padding, and
a status bar that keeps **Project**, **Logging**, and **Activity** (plus Share when
active). Graph and Agent terminal remain desktop-oriented.

After UI changes, agents run the Playwright mobile smoke (Pixel 5) without a
physical phone:

```powershell
.\scripts\web-ui-mobile-smoke.ps1
```

Screenshots land in `crates/ax-web/web-ui/test-results/mobile-shots/`.

## Live action stream

SSE feed: `GET /api/actions/events`  
Publish: `POST /api/actions/publish` `{ "kind", "message", "meta?" }`

With Verbose MCP logging, action events also append to `.ax/mcp-verbose-*.log`.
