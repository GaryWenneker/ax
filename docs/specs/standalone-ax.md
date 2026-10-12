# Standalone Ax: server, discovery and primary/replica synchronization

Status: implemented on main: initial server in PR #13, preserve-and-exchange in PR #25, and complete protocol 3 datasets and autonomous hosting in PR #42. Settings setup/pairing is tracked separately in [the current pairing spec](settings-primary-pairing.md). Release and production deployment are paused by the owner; they are not claimed complete.

## Goal and deployment modes

Ax must run without an IDE, locally or as a long-running service. Keep the existing local stdio MCP and Command Center. Add an explicit `ax server` lifecycle for enrolled, authenticated project-state replication, with project registration, health, discovery and synchronization. External MCP clients use the existing OAuth-protected remote MCP service. Deliver native and Docker examples and a user walkthrough with real browser screenshots. Releases and production website deployment remain restricted to validated main.

## Authority and storage

Use **primary** (hoofdserver) and **replica** throughout code, commands and documentation. One configured Primary coordinates authenticated contributions from every enrolled node. Replication includes the enrolled project's complete graph and cached source, memory, rules/skills, MCP logs, structured call history, working context, context caches and durable conversations/documents/tasks/hooks. Native and received records occupy separate namespaces. Credentials, account sessions, provider keys and unrelated projects stay local. Graph/history are exported from consistent SQLite reads into a validated whitelist, never by copying a live project database.

Pairing authorizes private, unshared and inactive project knowledge as well as shared records. Replicas contribute their own native records; colliding IDs retain distinct origins. Received memory and graph/history become locally usable after verified application. Rules and skills enter pending review and require approval before activation. Omitted knowledge never deletes accepted records; locally edited memory is preserved as a conflict. Current per-origin graph heads can replace only that origin's imported graph. See [complete dataset behavior](autonomous-host-and-replication/SPEC.md) and [preserve-and-exchange](node-data-preservation.md).

## CLI and configuration

`ax server init --config FILE --role primary|replica --cluster ID --project ID --path DIR [--primary URL]` writes a new JSON config without overwriting an existing file. A stable node ID is generated once. Project roots must exist and be initialized; init does not scan unrelated directories. `ax server run --config FILE` runs the listener. Replicas automatically poll their configured primary every 60 seconds unless overridden by `--sync-interval-seconds N` (5–86400), even when UDP push is enabled. `--push-port N` (default UDP 9072; use 0 to disable) enables authenticated LAN notification; it is separate from the opt-in UDP discovery port. The primary starts the UDP registration receiver on that port and replicas connect from ephemeral UDP ports. `ax server sync --config FILE` performs one bounded replica pull. `ax server status --config FILE` reports local role, project registration and persisted last synchronization results. `ax server discover --config FILE --seconds N` searches the configured LAN discovery port.

Per-project authentication uses separate environment variables containing random tokens (at least 32 characters). Configuration stores variable names, never token values. Static configuration of the primary URL is required before a replica trusts or contacts a publisher. HTTP is accepted only for loopback or when a private-network transport exception is explicitly configured; hosted deployments use a TLS reverse proxy. Refuse redirects when sending credentials. Bound payloads, file counts, timeouts and concurrent snapshot exports. Expose no shell, arbitrary-file or remote configuration mutation endpoint.

## Discovery

Opt-in UDP announcements carry protocol version, cluster ID, node ID, role and an advertised URL, but no credentials, project paths or project content. Send announcements periodically and expire them after a bounded observation period. Discovery helps locate candidates; an announcement never grants access, changes the primary or triggers automatic failover. Static URLs remain the Docker/cloud fallback because broadcast is generally limited to a LAN. Distinct configured primaries must be visible rather than resolved by last-response-wins.

## Near-realtime update delivery (LAN)

Replicas explicitly configure and trust the primary's HTTPS origin and a per-project random token. After startup, each replica sends an **authenticated UDP registration** to the primary's UDP push port every 10 seconds. The primary stores an in-memory table of registered node IDs, source socket addresses, project IDs and last-seen times. Registrations expire after 45 seconds without a heartbeat; at most 256 registrations are held.

The primary checks each enrolled project's current snapshot revision every three seconds. When a revision changes, it sends a small **signed UDP change notification** to registered replicas of that project. The datagram contains metadata and the new revision, never the actual data. The replica verifies cluster, project, node identity, protocol version, short timestamp validity and a keyed message signature before initiating a regular **authenticated HTTP(S) snapshot pull**. Neither unauthenticated discovery advertisements nor spoofed UDP packets grant database access.

The LAN UDP path is a latency optimization, not a guarantee of delivery. On packet loss, host sleep or after the primary restarts, replicas still poll over HTTP(S) every 60 seconds by default. No primary election, bidirectional conflict resolution or automatic activation of mirrored policies is implied. Hosted/WAN nodes use HTTPS polling when private-LAN UDP is unavailable; discovery alone must never rewrite the configured primary.

## Snapshot protocol

Authenticated GET `/v1/projects/{id}/snapshot` is available on a primary only. Enforce exact project registration and that project's token. Serialize policy rules, policy skills and project memory records directly, with their enabled, status, tags, scope and share attributes intact. Never invoke share/export or use its filters to decide what nodes receive. Include schema version, cluster ID, primary node ID, project ID, a content revision and a sorted file map. Verify the revision and every path before writing any files. Protocol version 3 retains the bounded knowledge envelope and adds authenticated manifests and content-addressed chunks for graph, source, history and logs. Every chunk, assembled database, schema, reference and claimed record count is validated before publication. Validate JSON records, duplicate IDs, content revision and bounded payloads before staging; never follow symlinks or trust received paths. Clear machine-local source paths and root IDs, and do not send credentials or entire SQLite databases.

Replicas stage each validated revision in a fresh directory, persist the selected mirror and retain the previous revision until the new one is durable. Repeated revisions skip import after a successful import. Policy changes remain pending review. Applied records use namespaced IDs and per-origin projections to preserve native records and enabled/share flags. Persist successful revision/time and failures per project. Offline failure keeps the last mirror and locally usable Ax. Replica polling retries with a bounded interval; no election or implicit role promotion.

## Docker and hosted service

Build the CLI and embedded Command Center from source in a multi-stage image. Run as a non-root user, keep configuration read-only and project/state volumes persistent. A Compose example binds the service to host loopback by default and accepts secrets through environment variables. Include initial indexing, server health, logs, backup, upgrade and shutdown commands. A container bind address is configurable; remote MCP must retain loopback as the native default and require an explicit non-loopback bind. Never expose the unauthenticated Command Center to the Internet.

## Documentation and delivery gates

Add a discoverable guide covering local use, primary/replica setup, OAuth MCP clients, Docker, TLS hosting, LAN discovery limitations, policy review, offline recovery and troubleshooting. Capture actual rendered guide/Command Center screens with Puppeteer; label documentation captures as documentation, without presenting mock state as a running cluster.

Verify config rejection, token/project isolation, replica publication denial, unsafe snapshot paths, revision integrity, deterministic snapshots, inactive/unshared state parity, idempotent staging, preserved local memory, same-cluster discovery filtering and multiple-primary visibility. Run affected local Rust/browser checks and strict Clippy. Automatic pipelines, releases and production website builds/deployment remain suspended by the owner. Platform-wide CI and physical LAN discovery are unverified here; merging source does not imply released binaries or a live website.
