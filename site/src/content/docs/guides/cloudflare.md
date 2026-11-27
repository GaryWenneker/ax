---
title: Expose Command Center with Cloudflare
description: Publish the local Ax Command Center on a public HTTPS name without opening the process to the network.
---

Command Center listens on `127.0.0.1:7070`. A Cloudflare Tunnel can give that loopback server a public HTTPS name such as `https://admin.example.com`. The browser talks to Cloudflare. Cloudflare talks to Ax on this computer. Ax does not bind a public interface.

## Tunnel

Install `cloudflared`, then point a named tunnel at the local server:

```bash
cloudflared tunnel --url http://127.0.0.1:7070
```

For a stable hostname, create a named tunnel in the Cloudflare dashboard and set the public hostname service to `http://127.0.0.1:7070`. Leave Ax on its loopback address.

Cloudflare should send:

- `Host` set to the public hostname
- `X-Forwarded-Proto: https`
- the browser `Origin`, for example `https://admin.example.com`

Ax answers only for host names it knows. Add the public hostname to `~/.ax/config.json`, or start Ax with `AX_WEB_AUTH_ORIGIN=https://admin.example.com`:

```json
{ "web": { "allowed_hosts": ["admin.example.com"] } }
```

Restart Command Center after changing the list. A request for any other host gets `403`. A cross-site request is rejected.

## Sign-in

Sign-in is off until you turn it on in **Settings → Security**. With sign-in off, the public address works without a password. Turn **Require sign-in** on when the hostname is reachable beyond this computer.

Passkeys are tied to the address you register them on. Open the public `https://` origin in the browser and register the passkey there. A passkey created on `http://localhost:7070` does not work on the public name.

## What stays local

The graph, memory, and logs stay on this computer. The tunnel only forwards HTTP to Command Center. Replication and Remote MCP use their own configured URLs. They are not implied by the tunnel.

After changing the hostname, reload Command Center on that hostname. A page still opened on localhost keeps talking to localhost.