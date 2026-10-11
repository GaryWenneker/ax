---
title: Standalone server and replicas
description: Run Ax as a long-lived primary or replica, with authenticated UDP change notifications and resilient HTTPS synchronization.
---

Ax can run independently of an IDE. A **primary** maintains an enrolled Ax project; one or more **replicas** contribute their existing knowledge and receive the combined cluster state. Native records remain intact, and received policy stays in review. The optional LAN notification channel makes synchronization responsive, while HTTPS polling catches missed notifications.

> **Availability:** Standalone server mode is available in the current `main` source. [Build the CLI from main](/getting-started/installation/#build-from-main) until downloadable releases include it. CLI `sync` stages a mirror; Settings pairing and CLI `apply` safely import received memories. Policies require separate review.

## Choose your setup

| Setup | Primary | Replica | Notification |
|---|---|---|---|
| All on one computer | Runs locally | Separate project directory | UDP on loopback, HTTPS/HTTP pull |
| Computers on the same LAN | One machine or Docker host | Another trusted machine | Signed UDP + authenticated HTTPS |
| Remote/cloud or separate networks | HTTPS reverse proxy | External desktop/server | HTTPS polling; UDP is optional and LAN-only |
| Existing ChatGPT / IDE client | Existing [Remote MCP](/guides/remote-mcp/) endpoint | Not required | OAuth MCP is separate from node replication |

This guide covers **node replication**, not sharing an individual memory or exporting a policy pack. An authorized replica receives all enrolled project's memory and policy records, including inactive and not-shared items. **Only join machines that you trust with the full project state.** Export and sharing flags remain unchanged and still govern their separate workflows.

## 1. Use an existing project or initialize a new one

Keep an existing initialized Ax project, including its `.ax/ax.db`. For a new project only, run this on the primary and separately on the replica, replacing the directories with your own:

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

Set the same `AX_SERVER_KNOWLEDGE_TOKEN` as on the primary. Make an initial authenticated exchange (local knowledge is contributed first, then the combined snapshot is staged):

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
5. The replica verifies the message and exchanges local knowledge and receives the combined state using the project-specific secret over HTTP(S).
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

![Ax Command Center Memory Vault interface](/screenshots/cc-memory-vault-dark.png)

Policy changes received by a replica are staged separately and are **not active by default**. Use the existing Policy Review workflow when you decide to import and approve them. The image below is an actual capture of Ax's review UI, not a claim that a live LAN synchronization was captured.

![Ax Command Center policy review interface](/screenshots/cc-policy-review.png)

## 5. Check status and recover from outages

```sh
ax server status --config ./ax-replica.json
ax server sync --config ./ax-replica.json
```

The status output reports the last attempt, last successful revision and failures for every enrolled project. If the primary goes offline, the replica keeps its last verified staged mirror. Once network connectivity returns, the next signed notification or HTTPS polling cycle can update it.

A successful **mirror** does not overwrite the replica's local database. When you explicitly enable memory application, the replica imports copies using stable, namespaced IDs and a ledger recording the last applied version. If a user modifies an imported memory locally, subsequent syncs detect the conflict instead of replacing it. Missing remote records never delete local records, including unchanged imports. Accepted source revisions are retained on the Primary; collisions from different nodes keep separate IDs. Policies are not activated automatically. The original revisioned mirror remains under `.ax/server/replicas/` for review.

## Empty Primary, existing Replica and conflicts

Pairing an Ax environment that contains months of work to an empty Primary does not reset that environment. Its native memories, rules and skills are contributed through authenticated project-scoped exchange. The Primary retains the contribution and makes the combined knowledge available to other replicas. Original IDs and records in the contributor's database are unchanged.

Different nodes that use the same record ID retain separate copies. A locally edited imported memory is preserved and contributed as a local variant; an unchanged imported copy is not uploaded again. Received rules and skills remain pending until you approve them. An unchanged approved import is also excluded from uploads, preventing feedback loops.

Absence is never a deletion instruction. Current copies and previous accepted source revisions remain available; ordinary local deletion does not broadcast deletion to the cluster. Source snapshots are retained as JSON in the project's `ax_node_contributions` table. Staged received revisions remain under `.ax/server/replicas/`. This exchanges enrolled knowledge, not source code, the graph, provider keys, client tokens or a live database file.

Update **both Primary and Replica** to the current build. A Primary without the preserve-and-exchange endpoint is rejected; the client keeps its local data. Enrollment credentials now authorize contributing the enrolled project's full knowledge, including private records, so choose a Primary you trust with that data.

Synchronization retains the existing 8 MiB state and 10,000-record-per-kind limits. The retention archive is capped at 64 MiB and 64 contributing nodes per project/cluster. Reaching a limit rejects the new contribution with an error; it never prunes accepted history to make room. Keep normal backups of your project state. Release and deployment remain paused.

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


## Team members, clients and tokens

Open **Settings → Team** in the local Command Center, then choose **Add member**. Enter a name, an optional email address, and a role. An existing standalone installation gets an initial local Owner automatically; its project data and current local access remain available. Change a member's role with the role selector or use Disable/Enable. The last active Owner's controls are locked; only an Owner can assign or change ownership. Disabling a member requires confirmation because their clients lose access, and a lower role takes effect on the next authenticated request. The role guide below the member list explains each role.

Open **Settings → Clients & tokens** and choose **Add client**. Enter the application name, select its type and an active team member, then issue a token. Use **Copy token** before dismissing the one-time notice: Ax stores a verifier, never its plaintext, and cannot show it again. Switching to another settings section clears that notice. Tokens use the `axc_` namespace.

Use the labeled permission checkboxes and **Save scopes** to choose permissions. Scopes unavailable to the member's role are disabled; a Read-only member cannot receive write or administration permissions. **Discard changes** restores the saved permissions. A failed save keeps your draft so you can correct or retry it. Each client lists token prefixes, creation time and last use. **Revoke token** disables a single credential; **Revoke client** disables that client's credentials without affecting the member's other clients. **Rotate tokens** issues a replacement and immediately invalidates all older credentials for that client. Confirm these actions and keep the replacement before closing its one-time notice.

The selected settings section survives a reload and can be bookmarked: `/settings?section=team`, `/settings?section=clients`, or `/settings?section=nodes`. Filter settings by **Access** to show all three entries. Each access section refreshes while open; **Refresh** reloads its current data. Read-only sessions display the records with mutation controls disabled.

Client credentials can call explicitly scoped HTTP APIs using `Authorization: Bearer <token>`. Management endpoints require `admin:team`, `admin:clients` or `admin:nodes`. A client credential cannot invoke the terminal, run arbitrary Command Center actions or act as a replica. Local browser management requires the local owner session and a CSRF token; shared read-only Command Center cannot change access records.

For [Remote MCP](/guides/remote-mcp/), add the member's stable ID to the desired project's `subjects` in the remote config and to `write_subjects` when writes are allowed. Both the project's explicit grant and the client's effective tool scope are required. Revoked tokens and role/scope changes are checked on every request, including existing MCP sessions. OAuth and configured API-key access remain supported.

![Team roles and member status in Azure Portal Light](/screenshots/cc-access-team-dark.png)

![Clients, effective scopes and credential metadata](/screenshots/cc-access-clients-dark.png)

## Configure nodes from Settings

Open **Settings → Replication** in the local Owner Command Center. Without a saved configuration, Ax shows **Configure this Ax environment**. With one, it shows the saved configuration and status; the setup form stays hidden even when replication is stopped.

1. For a **Primary**, select **Start Primary**. Empty cluster ID, replication project ID and credential fields are generated automatically; you can also supply values. Only the project currently open in Command Center is enrolled.
2. Copy the cluster/project IDs and replication credential from the saved overview. The credential is masked by default, with eye and copy controls on the right. Ax keeps it in the OS credential store for automatic startup, never in the settings JSON.
3. On another Ax environment, choose **Replica**. Available Primary services appear automatically through UDP discovery; there is no search button. Select a candidate or enter its URL directly, then use the same cluster/project IDs and credential. Selection does not connect automatically.
4. Select **Connect to Primary**. Ax authenticates the Primary, contributes existing knowledge and verifies the combined state before saving configuration. An empty Primary cannot clear an existing Replica. Received memories use separate IDs; rules and skills require review. Failed synchronization preserves local data and prior settings.
5. **Stop managed node** temporarily stops replication and preserves its configuration. **Remove local configuration** stops it, removes settings and the stored credential, and returns the setup form. Existing memories, rules and skills remain. Use removal to configure a different role or Primary. Both actions require confirmation; cancellation changes nothing.

A configured Primary or Replica starts automatically when `ax web` or `ax serve --mcp` starts, including an IDE refresh and MCP daemon mode. These processes reuse the same service. Replication continues when Command Center closes or the IDE refreshes; explicit Stop ends it, and the next web/MCP startup starts it again. Remove configuration prevents automatic restart. Restart controls are available while stopped and normally reuse the stored credential. Unlock an unavailable OS credential store or reconnect with the credential. Headless installations can supply both `AX_COMMAND_CENTER_REPLICATION_TOKEN` and `AX_COMMAND_CENTER_NODE_CONTROL` securely in each starting process's environment; these external values are not deleted by the UI.

The service listens on `127.0.0.1`, port 9070 by default, and announces automatically on UDP 9071. Its default advertised URL works on the same machine. For other machines, configure a TLS reverse proxy and enter its origin as **Advertised URL**. Announcements never replace authentication. Hosted connections can use the URL field directly. An occupied port is reported rather than terminating another service. The managed path uses authenticated polling; separate CLI setup also supports signed UDP notifications.

These controls require the local writable Owner session with CSRF protection. Remote client tokens cannot configure or start local node processes. Team/client credentials and project replication enrollment credentials remain separate.

The latest published `v8.0.0` predates standalone/access UI merges. Build current `main` for these controls until a subsequent validated release is published. Automatic build/release pipelines are currently paused at the owner's request; merging code is not publication.

## Replication access and connection status

Open **Settings → Replication** for the local node ID, Primary/Replica role, the Replica's configured Primary URL, and its last sync state. Use the setup controls above, or start/synchronize a separately configured CLI server, to populate this status. Primary lists authenticated replicas with endpoint, last seen and last successful snapshot. Online means seen within two minutes; older records are marked not seen recently.

A Replica must explicitly configure its Primary and project credential. Successful authenticated registration adds it automatically; no manual approval is needed. **Block node** asks for confirmation and blocks future snapshots and registration for that node ID. **Allow re-enrollment** explicitly clears that block, but keeps previous node credentials revoked; the Replica must authenticate again with its project enrollment credential. UDP presence never chooses or changes the configured Primary. Client tokens (`axc_`) and node credentials (`axn_`) cannot be used interchangeably. Shared project credentials remain the automatic enrollment mechanism for existing standalone deployments; treat them as enrollment secrets.

Disabling or revoking access does not delete a team member's existing data. These controls govern authorization; export/import and shared/non-shared or active/inactive data selection retain their existing rules.

![Primary/Replica management in Azure Portal Light](/screenshots/cc-access-nodes.png)

![Primary/Replica management in Azure Portal Dark](/screenshots/cc-access-nodes-dark.png)

These screenshots use synthetic public example identities and contain no working credentials.

### Saved configuration and automatic discovery

![Saved Primary configuration with a masked credential and eye/copy controls](/screenshots/cc-replication-configured.png)

The saved overview replaces setup. The credential remains masked until the Owner reveals it.

![Settings with two automatically discovered Primary candidates and an explicit selection](/screenshots/cc-primary-pairing.png)

Documentation capture with synthetic candidates; this screenshot illustrates selection before enrollment. A selected card requires the Primary’s project credential and **Connect to Primary**.
