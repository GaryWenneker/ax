---
title: Standalone server and replicas
description: Run Ax as a long-lived primary or replica, with authenticated UDP change notifications and resilient HTTPS synchronization.
---

Ax can run independently of an IDE. A **primary** maintains an enrolled Ax project; one or more **replicas** contribute their existing knowledge and receive the combined cluster state. Native records remain intact, and received policy stays in review. The optional LAN notification channel makes synchronization responsive, while HTTPS polling catches missed notifications.

> **Availability:** Standalone server mode is available in the current `main` source. [Build the CLI from main](/getting-started/installation/#build-from-main) until downloadable releases include it. CLI `sync` stages a mirror; Settings pairing and CLI `apply` apply the received graph, indexed source, logs, conversation history and memories. Policies require separate review.

## Choose your setup

| Setup | Primary | Replica | Notification |
|---|---|---|---|
| All on one computer | Runs locally | Separate project directory | UDP on loopback, HTTPS/HTTP pull |
| Computers on the same LAN | One machine or Docker host | Another trusted machine | Signed UDP + authenticated HTTPS |
| Remote/cloud or separate networks | HTTPS reverse proxy | External desktop/server | HTTPS polling; UDP is optional and LAN-only |
| Existing ChatGPT / IDE client | Existing [Remote MCP](/guides/remote-mcp/) endpoint | Not required | OAuth MCP is separate from node replication |

This guide covers **node replication**, not sharing an individual memory or exporting a policy pack. An authorized replica receives the enrolled project's graph, indexed source, logs, working context, durable conversations and memory/policy records, including inactive and not-shared items. **Only join machines that you trust with the full project state.** Export and sharing flags remain unchanged and still govern their separate workflows.

## Complete project synchronization

Protocol 3 transfers memories, rules and skills together with the complete graph: nodes, relationships, unresolved references, communities, metadata and stored source contents. It also transfers project agent metrics, daily MCP log files, structured MCP/agent usage records, working context, cached context, durable transcripts, documents, tasks and hooks. Global machine accounts, credentials and unrelated projects remain local.

Each node contributes its native records. Received graph IDs and source paths use `ax-replica/<node>/`, so colliding IDs and filenames remain distinct and graph/source queries work without copying the other machine's working tree. Local re-indexing preserves received namespaces. Memory projection rebuilds its full-text and vector indexes. Received rules and skills still require review before activation.

Graph/history databases and log files use verified 512 KiB content-addressed chunks. This allows indexes larger than the existing 8 MiB knowledge envelope. The Primary validates assembled databases, schemas, relationships and counts before accepting a contribution. Knowledge revisions and dataset heads are retained separately, so log/index changes do not repeatedly archive unchanged knowledge. Transfers reuse chunks. Received log history survives source rotation, truncation and removal. Accepted conversation history is retained when a native cache/context cleanup omits older rows, so later replicas can still retrieve it. Logging history and live trace label remote lines by node.

Settings shows per-origin counts, last completed synchronization and actual service state: green running, orange waiting/review, red error and gray stopped/unconfigured. A complete sync is reported only after graph/history and memory/policy application finish. Graph replacement affects only that origin; missing knowledge never deletes an accepted memory, and local edits remain protected.

Both nodes need protocol 3; update and rebuild older installations together. The knowledge envelope remains bounded to 8 MiB and 10,000 records per kind; dataset manifests allow at most 64 origins, 3,660 log files per origin and 8 GiB of graph/history/log data. Conversation history is bounded to one million rows per origin. Capacity failures are explicit and cannot report a completed sync. Unreferenced chunks are reclaimed after a 24-hour grace period; current dataset heads remain pinned.

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

The URL must resolve to the authenticated Ax Primary and be an **origin** without a path, query or user credentials. Use HTTPS for remote access. Explicit private or loopback IP addresses also support HTTP, for example `http://192.168.1.20:9070` on a trusted LAN or `http://127.0.0.1:9070` on the same computer.

Set the same `AX_SERVER_KNOWLEDGE_TOKEN` as on the primary. Make an initial authenticated exchange (local knowledge is contributed first, then the combined snapshot is staged):

```sh
ax server sync --config ./ax-replica.json
ax server status --config ./ax-replica.json

# Apply verified graph, source, logs, history and memory to this project
# using separate identities for each contributing node:
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

The replica uses its own transient UDP socket, so it can run on the same host as the primary. The 60-second timer is a **fallback**, not the normal expected LAN-update latency. **With `--auto-apply-memories`**, verified graph, cached source, logs, conversations and memory are applied to the replica's project. Received memories can be recalled and keep their enabled flags. Without it, received data remains staged until you run `ax server apply`. Policy rules and skills remain staged for review; never activate new executable instructions automatically.

## 4. How near-realtime updates work

1. The replica sends an authenticated registration over UDP to the configured primary every 10 seconds.
2. The primary tracks connected project/node IDs and their real UDP source addresses. Missing heartbeats expire after 45 seconds.
3. Every three seconds the primary compares snapshot revisions of enrolled projects.
4. On a new revision, it sends a **signed UDP change notification** to registered replicas.
5. The replica verifies the message, contributes its native project data, and receives the combined knowledge and chunked datasets using the project-specific secret over HTTP(S).
6. If UDP is dropped or blocked, a scheduled HTTPS pull still converges the mirror.

UDP sends **only change metadata**, not memories, rules, skills or project files. It does not replace HTTPS authentication and does not elect a new primary.



## What you will see in Ax

The replicas keep their own Command Center. After complete application, received graph and cached source are available to graph queries and file previews, conversations remain in the project history, and imported notes are available in the Memory Vault with independent namespaced IDs. The screenshot below shows the **actual Ax Memory Vault interface**, not simulated replica network activity.

![Ax Command Center Memory Vault interface](/screenshots/cc-memory-vault-dark.png)

Policy changes received by a replica are staged separately and are **not active by default**. Use the existing Policy Review workflow when you decide to import and approve them. The image below is an actual capture of Ax's review UI, not a claim that a live LAN synchronization was captured.

![Ax Command Center policy review interface](/screenshots/cc-policy-review.png)

## 5. Check status and recover from outages

```sh
ax server status --config ./ax-replica.json
ax server sync --config ./ax-replica.json
```

The status output reports the last attempt, last successful revision and failures for every enrolled project. If the primary goes offline, the replica keeps its last verified staged mirror. Once network connectivity returns, the next signed notification or HTTPS polling cycle can update it.

A successful **mirror** does not overwrite the replica's local database. When you enable complete application, received graph/source and conversation history are projected separately from native records. Memories use stable, namespaced IDs and a ledger recording the last applied version. If a user modifies an imported memory locally, subsequent syncs detect the conflict instead of replacing it. Missing remote records never delete local records, including unchanged imports. Accepted source revisions are retained on the Primary; collisions from different nodes keep separate IDs. Policies are not activated automatically. The original revisioned mirror remains under `.ax/server/replicas/` for review.

## Empty Primary, existing Replica and conflicts

Pairing an Ax environment that contains months of work to an empty Primary does not reset that environment. Its native memories, rules and skills are contributed through authenticated project-scoped exchange. The Primary retains the contribution and makes the combined knowledge available to other replicas. Original IDs and records in the contributor's database are unchanged.

Different nodes that use the same record ID retain separate copies. A locally edited imported memory is preserved and contributed as a local variant; an unchanged imported copy is not uploaded again. Received rules and skills remain pending until you approve them. An unchanged approved import is also excluded from uploads, preventing feedback loops.

Missing knowledge records never delete accepted memories, rules or skills. Knowledge revisions remain in `ax_node_contributions`; the current graph/source dataset for an origin replaces only that origin's imported graph. Accepted conversation rows and log contents are retained even after native cleanup or log truncation, including for newly joined replicas. Staged revisions remain under `.ax/server/replicas/`. Source contents and graph records are included through verified, whitelisted database exports. Provider keys, client tokens and unrelated project data remain local.

Update **both Primary and Replica** to the current build. A Primary without the preserve-and-exchange endpoint is rejected; the client keeps its local data. Enrollment credentials now authorize contributing the enrolled project's full knowledge, including private records, so choose a Primary you trust with that data.

The knowledge envelope retains its 8 MiB and 10,000-record-per-kind limits; its revision archive is capped at 64 MiB. Graph, source, history and logs use separate 512 KiB chunks, with an 8 GiB total dataset limit, up to 64 origins and 3,660 log files per origin. History exports and retained heads allow up to one million rows. Reaching a limit rejects the new contribution with an error; it never prunes accepted history to make room. Keep normal backups of your project state. Release and deployment remain paused.

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
- **Status shows mirrored, not applied:** default behavior stages snapshots without database modifications. Run `ax server apply --config ...` once or enable `--auto-apply-memories` for continuous graph/source, log, history and memory application. Policy still needs review.
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

![Team roles and member status in Light](/screenshots/cc-access-team-dark.png)

![Clients, effective scopes and credential metadata](/screenshots/cc-access-clients-dark.png)

## Configure nodes from Settings

Open **Settings → Replication → Set up replication** in the local Owner Command Center. Setup opens in a centered modal. With a saved configuration, the overview displays actual service and sync state.

1. Choose **Primary** on the hosting computer. Select the private interface IP other machines can reach. IDs, a local port and a shared credential are generated automatically. Advanced options provide overrides or a public HTTPS proxy origin.
2. Copy the shared credential and advertised Primary URL from the overview. The credential is masked with eye/copy controls and kept in the OS credential store.
3. On another computer, choose **Replica** in the setup modal. Select a discovered Primary or enter its URL, then enter the shared credential. Authenticated enrollment returns cluster/project IDs. An advanced project override is needed only for a Primary serving multiple projects. The Primary port is already in its URL; the Replica's local port is allocated automatically.
4. Choose **Connect to Primary**. Ax exchanges and validates the full project datasets before saving settings. Local records remain protected and received policy requires review. The overview shows real service state, active replicas, dataset counts and completed sync time. Activity respects reduced-motion preferences.
5. **Stop managed node** stops replication and disables automatic startup until restarted. **Remove local configuration** stops it, removes settings and its credential, and removes the stale local-node card. Synchronized project data is retained. Both actions require confirmation.

Configured services start automatically when `ax web`, stdio MCP or Remote MCP starts, including IDE refresh and daemon mode. They reuse the saved node identity and credential and survive the parent process closing. Ax Sync watches native edits; replication retains received graph data separately. Restart normally reuses the stored credential. Headless installations can provide both `AX_COMMAND_CENTER_REPLICATION_TOKEN` and `AX_COMMAND_CENTER_NODE_CONTROL` in the process environment.

The selected advertised IP is used for the listener and UDP 9071 broadcast. A LAN IP is preferred over loopback when available. Authenticated local management stays on loopback. HTTPS proxy origins route to the local listener. Discovery supplies hints; authenticated enrollment grants project access. Occupied ports are reported without stopping another service.

These controls require the local writable Owner session with CSRF protection. Remote client tokens cannot configure or start local node processes. Team/client credentials and project replication enrollment credentials remain separate.

These controls ship in v8.1.0.

## Replication access and connection status

Open **Settings → Replication** for the local node ID, Primary/Replica role, the Replica's configured Primary URL, and its last sync state. Use the setup controls above, or start/synchronize a separately configured CLI server, to populate this status. Primary lists authenticated replicas with endpoint, last seen and last successful snapshot. Online means seen within two minutes; older records are marked not seen recently.

A Replica must explicitly configure its Primary and project credential. Successful authenticated registration adds it automatically; no manual approval is needed. **Block node** asks for confirmation and blocks future snapshots and registration for that node ID. **Remove revoked node** removes a blocked entry from the visible list after confirmation, while preserving its revocation marker and synchronized project data. Discovery and shared-key registration cannot restore it. **Allow re-enrollment** explicitly clears the block, but keeps previous node credentials revoked; the Replica must authenticate again with its project enrollment credential. UDP presence never chooses or changes the configured Primary. Client tokens (`axc_`) and node credentials (`axn_`) cannot be used interchangeably. Shared project credentials remain the automatic enrollment mechanism for existing standalone deployments; treat them as enrollment secrets.

Disabling or revoking access does not delete a team member's existing data. These controls govern authorization; export/import and shared/non-shared or active/inactive data selection retain their existing rules.

![Primary/Replica management in Light](/screenshots/cc-access-nodes.png)

![Primary/Replica management in Dark](/screenshots/cc-access-nodes-dark.png)

These screenshots use synthetic public example identities and contain no working credentials.

### Saved configuration and automatic discovery

![Saved Primary configuration with a masked credential and eye/copy controls](/screenshots/cc-replication-configured.png)

The saved overview replaces setup. The credential remains masked until the Owner reveals it.

![Settings with two automatically discovered Primary candidates and an explicit selection](/screenshots/cc-primary-pairing.png)

Documentation capture with synthetic candidates; this screenshot illustrates selection before enrollment. A selected card requires the Primary’s project credential and **Connect to Primary**.

## Review incoming policy sources

Policy Review shows whether an incoming rule or skill is new, duplicates current
content, or replaces it. Expand an item to compare the complete incoming and
current text and see changed fields. Filter by type and effect, select several
items, and confirm a bulk accept or reject operation.

New pack and remote-share imports record their source and export time. Replication
records the contributing node, project, policy identity and snapshot revision.
Source records persist with the queue and apply only to the exact incoming text;
editing that text invalidates old attribution. Older items without source records
show an explicit unknown origin. Accepting or rejecting clears the source record
for that item. A source label is context for review, never permission to activate
policy automatically.

### Different node versions

Replication exchanges app-version and protocol metadata without changing snapshot fingerprints. Settings shows a persistent warning when peer versions differ or an older peer does not report its version. Complete protocol 3 peers can still synchronize; a matching successful sync clears the warning. Explicitly incompatible protocols stop before applying incoming data, with an update/reconnect message. Existing local records remain intact. Ax does not automatically replace or restart another node: update the older node through your normal deployment process.
