---
title: Standalone server and replicas
description: Run Ax as a long-lived primary or replica, with authenticated UDP change notifications and resilient HTTPS synchronization.
---

Ax can run independently of an IDE. A **primary** maintains an enrolled Ax project; one or more **replicas** keep an isolated, read-only mirror of its knowledge. The optional LAN notification channel makes synchronization responsive, while HTTPS polling catches missed notifications.

> **Availability:** Standalone server mode is under development. Build the CLI from the `feature/standalone-ax` branch until the feature is released. A staged mirror is not automatically activated in the replica's local Ax database.

## Choose your setup

| Setup | Primary | Replica | Notification |
|---|---|---|---|
| All on one computer | Runs locally | Separate project directory | UDP on loopback, HTTPS/HTTP pull |
| Computers on the same LAN | One machine or Docker host | Another trusted machine | Signed UDP + authenticated HTTPS |
| Remote/cloud or separate networks | HTTPS reverse proxy | External desktop/server | HTTPS polling; UDP is optional and LAN-only |
| Existing ChatGPT / IDE client | Existing [Remote MCP](/guides/remote-mcp/) endpoint | Not required | OAuth MCP is separate from node replication |

This guide covers **node replication**, not sharing an individual memory or exporting a policy pack. An authorized replica receives all enrolled project's memory and policy records, including inactive and not-shared items. **Only join machines that you trust with the full project state.** Export and sharing flags remain unchanged and still govern their separate workflows.

## 1. Initialize a project on both machines

Run these on the primary and separately on the replica, replacing the directories with your own:

```sh
ax init /absolute/path/to/project
```

You do not need to share a working tree or copy `.ax/ax.db` from one machine to another. The two registered project directories must exist and be independently initialized.

## 2. Create the primary configuration

On the primary:

```sh
ax server init \
  --config ./ax-primary.json \
  --role primary \
  --cluster studio \
  --project knowledge \
  --path /absolute/path/to/project
```

The command generates a stable node ID and refuses to overwrite an existing file. The configuration contains only the **name** of the authentication-token environment variable. Create one secret with at least 32 random, non-whitespace ASCII characters:

```sh
export AX_SERVER_KNOWLEDGE_TOKEN="$(openssl rand -hex 32)"
```

Store this secret securely. Provide the same token to the trusted replica through its environment or a secrets manager. Never commit the token to Git or place it inside the configuration JSON.

A Replica does not require manual approval on the Primary. When its `primary_url`, cluster/project IDs and project credential are valid, it registers itself automatically with that Primary and synchronization begins. The Primary records the Replica in Settings so operators can see its endpoint, last-seen/sync state and revoke or block it later.

To run on a single computer:

```sh
ax server run --config ./ax-primary.json --bind 127.0.0.1:9070
```

For a LAN or Docker-hosted primary, use a private network and put HTTPS in front of the Ax HTTP port:

```sh
ax server run \
  --config ./ax-primary.json \
  --bind 0.0.0.0:9070 \
  --allow-private-http \
  --push-port 9072
```

**Do not expose plain HTTP or the Ax Command Center to the public Internet.** Put a properly configured HTTPS reverse proxy in front of the authenticated HTTP endpoint.

## 3. Join a replica

On a second trusted machine, use the **same cluster and project IDs**:

```sh
ax server init \
  --config ./ax-replica.json \
  --role replica \
  --cluster studio \
  --project knowledge \
  --path /absolute/path/to/replica-project \
  --primary https://primary.example.com
```

The URL must resolve to the actual authenticated Ax primary and be an HTTPS **origin** without a path, query or user credentials. For a same-machine trial, `http://127.0.0.1:9070` is allowed instead.

Set the same `AX_SERVER_KNOWLEDGE_TOKEN` as on the primary. Make an initial one-time pull:

```sh
ax server sync --config ./ax-replica.json
ax server status --config ./ax-replica.json

# After reviewing the mirror, optionally apply replica memories to the
# local vault under deterministic non-conflicting IDs:
ax server apply --config ./ax-replica.json
```

Then start the replica's long-running service:

```sh
ax server run \
  --config ./ax-replica.json \
  --bind 127.0.0.1:9073 \
  --push-port 9072 \
  --sync-interval-seconds 60 \
  --auto-apply-memories
```

The replica uses its own transient UDP socket, so it can run on the same host as the primary. The 60-second timer is a **fallback**, not the normal expected LAN-update latency. **With `--auto-apply-memories`**, newly arrived memories appear in the replica's own Ax vault and can be recalled, while preserving their enabled flags. Without it, they remain staged until you run `ax server apply`. Policy rules and skills remain staged for review; never activate new executable instructions automatically.

## 4. How near-realtime updates work

1. The replica sends an authenticated registration over UDP to the configured primary every 10 seconds.
2. The primary tracks connected project/node IDs and their real UDP source addresses. Missing heartbeats expire after 45 seconds.
3. Every three seconds the primary compares snapshot revisions of enrolled projects.
4. On a new revision, it sends a **signed UDP change notification** to registered replicas.
5. The replica verifies the message and pulls the complete changed state using the project-specific secret over HTTP(S).
6. If UDP is dropped or blocked, a scheduled HTTPS pull still converges the mirror.

UDP sends **only change metadata**, not memories, rules, skills or project files. It does not replace HTTPS authentication and does not elect a new primary.

```text
Primary Ax project     Replica Ax project
        |                       |
        |<-- UDP register ------|  signed, repeated
        |                       |
   [state changes]              |
        |-- UDP changed ------->|  signed hint, not data
        |<-- HTTPS GET ---------|  authenticated
        |--- snapshot --------->|  validated, staged
        |                       |
```

## What you will see in Ax

The replicas keep their own Command Center. With opt-in memory application, imported notes are available in the existing Memory Vault, with independent namespaced IDs. The screenshot below shows the **actual Ax Memory Vault interface**, not simulated replica network activity.

![Ax Command Center Memory Vault interface](/screenshots/cc-memory-vault.png)

Policy changes received by a replica are staged separately and are **not active by default**. Use the existing Policy Review workflow when you decide to import and approve them. The image below is an actual capture of Ax's review UI, not a claim that a live LAN synchronization was captured.

![Ax Command Center policy review interface](/screenshots/cc-policy-review.png)

## 5. Check status and recover from outages

```sh
ax server status --config ./ax-replica.json
ax server sync --config ./ax-replica.json
```

The status output reports the last attempt, last successful revision and failures for every enrolled project. If the primary goes offline, the replica keeps its last verified staged mirror. Once network connectivity returns, the next signed notification or HTTPS polling cycle can update it.

A successful **mirror** does not overwrite the replica's local database. When you explicitly enable memory application, the replica imports copies using stable, namespaced IDs and a ledger recording the last applied version. If a user modifies an imported memory locally, subsequent syncs detect the conflict instead of replacing it. Remote deletions only remove records that still match the last imported copy. Policies are not activated automatically. The original revisioned mirror remains under `.ax/server/replicas/` for review.

## 6. LAN discovery versus registration

`ax server discover --config ./ax-replica.json --seconds 5` lists candidate peers when a primary was started with `--discovery-port 9071 --advertise https://primary.example.com`. Announcements are public hints, never authentication. The replica continues to trust only its configured `primary_url`.

**UDP registration is different**: it uses `--push-port 9072` and signed messages with the per-project token. Registration never automatically trusts a new node simply because it answered an announcement.

## 7. Docker, hosting and remote clients

For Docker, mount project data and the server JSON as persistent read-only inputs where appropriate, pass token environment variables through secret injection, and expose UDP 9072 only on trusted networks. On cloud platforms where UDP is unavailable, set `--push-port 0`; HTTPS polling continues automatically.

For browser login and AI clients such as ChatGPT, use the existing [Remote MCP and browser login](/guides/remote-mcp/) instructions. OAuth client connections, policy sharing, and standalone node replication are three **separate** concerns.

## 8. Run the primary in Docker Compose

A production-oriented starting point is available at [deploy/standalone/compose.yaml](https://github.com/GaryWenneker/ax/blob/feature/standalone-ax/deploy/standalone/compose.yaml). The image is built from the Rust workspace and contains the bundled Command Center, but it runs only the standalone authenticated server by default.

1. Create `deploy/standalone/primary-project`, make the directory writable by Linux UID/GID **10001**, and initialize it with `ax init` (from the host if Ax is installed, or by using the container: `docker compose run --rm primary init /data/project`). It needs a writable `.ax` directory.
2. Create a `primary.json` configuration using `ax server init`. When storing it for the container, set the registered project `path` to **`/data/project`**, not the host's absolute path. Do not change the stable generated node ID or the token environment-variable name.
3. Provide `AX_SERVER_KNOWLEDGE_TOKEN` through a **private**, untracked environment file or secrets manager. The primary and trusted replicas use the same value for this particular project.
4. Start the service:

```sh
cd deploy/standalone
docker compose build
docker compose up -d
docker compose ps
docker compose logs -f primary
curl -fsS http://127.0.0.1:9070/health
```

The default Compose port binding is **host-loopback only**. Access from other machines requires a trusted HTTPS reverse proxy; do not expose the plaintext Ax listener directly. If the replicas are on a private LAN, explicitly enable the commented **UDP 9072** host port and start Ax with `--push-port 9072`. If they are on different networks or your host firewall blocks UDP, keep `--push-port 0`: periodic authenticated HTTPS polling is still supported.

For backups, stop the service and back up the complete `primary-project/.ax` state and separately back up the server configuration. Keep credentials in your secret store, never in the backup of a public repository. Upgrades: rebuild the image from a tested main release, run `docker compose up -d --build`, and verify the health endpoint.

## Troubleshooting

- **401 Unauthorized:** check that the project ID and token environment variable exactly match on both nodes.
- **Replica cannot contact the primary:** check the configured HTTPS origin, TLS termination and TCP port; no automatic discovery-based promotion occurs.
- **No immediate LAN update:** verify UDP 9072 is allowed through both firewalls and the primary is still running. Manual sync and periodic polling remain available.
- **Status shows mirrored, not applied:** default behavior stages snapshots without database modifications. Run `ax server apply --config ...` once or enable `--auto-apply-memories` for continuous memory projection. Policy still needs review.
- **Memory projection reports conflicts:** an imported memory was edited locally or its ledger does not match. The local value is preserved; investigate the conflict before reapplying.
- **Different projects seem to cross over:** stop using the service and check project IDs, cluster IDs and tokens. Each project must have its own independent secret.

See the implementation [in the public repository](https://github.com/GaryWenneker/ax/tree/feature/standalone-ax). This is a development branch, not an announcement that CI, releases or production deployment have passed.
