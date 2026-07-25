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

## PWA

The web UI ships `manifest.webmanifest` + `sw.js` so mobile browsers can install
Command Center as a standalone app against your local/`ax share` URL.

## Live action stream

SSE feed: `GET /api/actions/events`  
Publish: `POST /api/actions/publish` `{ "kind", "message", "meta?" }`

The UI shows a small live strip for recent workspace/agent events.
