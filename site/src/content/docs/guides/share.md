---
title: Share Command Center
description: LAN share with a token, PWA install, and live action stream.
---

## `ax share`

Starts Command Center bound for LAN access with a random share token (**read-only**):

```bash
ax share --open
ax share --port 7070 --bind 0.0.0.0 --token mysecret
```

Clients must pass `?token=…`, `Authorization: Bearer …`, or the `ax_share` cookie set after the first successful request.

For remote collaborators, wrap localhost with a tunnel:

```bash
cloudflared tunnel --url http://127.0.0.1:7070
```

## PWA

The web UI ships a web app manifest and service worker so mobile browsers can install Command Center as a standalone app against your local or `ax share` URL.

## Live action stream

| Endpoint | Purpose |
|----------|---------|
| `GET /api/actions/events` | SSE feed of workspace / agent events |
| `POST /api/actions/publish` | `{ "kind", "message", "meta?" }` |

The UI shows a small live strip for recent events.

## Further reading

- [Command Center](/guides/command-center/)
- Repo docs: [`docs/SHARE.md`](https://github.com/GaryWenneker/ax/blob/ax-v4/docs/SHARE.md)
