# Standalone Ax: server, discovery and primary/replica sharing

Status: implementation in progress on `feature/standalone-ax`. No merge or production deployment is claimed by this document.

## Goal and deployment modes

Ax must run without an IDE, locally or as a long-running service. Keep the existing local stdio MCP and Command Center. Add an explicit `ax server` lifecycle for shared knowledge, with project registration, health, discovery and synchronization. External MCP clients use the existing OAuth-protected remote MCP service. Deliver native and Docker examples and a user walkthrough with real browser screenshots. Releases and production website deployment remain restricted to validated main.

## Authority and storage

Use **primary** (hoofdserver) and **replica** throughout code, commands and documentation. One configured primary publishes a project's shareable policy and memories. Replicas pull bounded snapshots and retain their local source index and private knowledge. Never copy a live SQLite database, private policy, credentials, sessions, provider keys or working-context caches between nodes. The graph remains local to each registered checkout; a server can index its own checkout with existing `ax init` / `ax index` commands.

Replication is knowledge sharing through the existing policy-pack and shared-memory contracts. It is not high-availability database replication. Downloaded snapshots are complete mirrors in a separate namespace; policy activation uses the existing review flow and memories have deterministic replica IDs. Local edits cannot become primary content automatically. A withdrawn primary item disappears from the next staged mirror; previously activated policy needs explicit local review/removal. Do not silently delete private or locally edited records.

## CLI and configuration

`ax server init --config FILE --role primary|replica --cluster ID --project ID --path DIR [--primary URL]` writes a new JSON config without overwriting an existing file. A stable node ID is generated once. Project roots must exist and be initialized; init does not scan unrelated directories. `ax server run --config FILE` runs the listener and optional replica polling. `ax server sync --config FILE` performs one bounded replica pull. `ax server status --config FILE` reports local role, project registration and persisted last synchronization results. `ax server discover --config FILE --seconds N` searches the configured LAN discovery port.

Per-project authentication uses separate environment variables containing random tokens (at least 32 characters). Configuration stores variable names, never token values. Static configuration of the primary URL is required before a replica trusts or contacts a publisher. HTTP is accepted only for loopback or when a private-network transport exception is explicitly configured; hosted deployments use a TLS reverse proxy. Refuse redirects when sending credentials. Bound payloads, file counts, timeouts and concurrent snapshot exports. Expose no shell, arbitrary-file or remote configuration mutation endpoint.

## Discovery

Opt-in UDP announcements carry protocol version, cluster ID, node ID, role and an advertised URL, but no credentials, project paths or project content. Send announcements periodically and expire them after a bounded observation period. Discovery helps locate candidates; an announcement never grants access, changes the primary or triggers automatic failover. Static URLs remain the Docker/cloud fallback because broadcast is generally limited to a LAN. Distinct configured primaries must be visible rather than resolved by last-response-wins.

## Snapshot protocol

Authenticated GET `/v1/projects/{id}/snapshot` is available on a primary only. Enforce exact project registration and that project's token. Serialize only a fresh policy pack and enabled memories explicitly tagged `shared`, excluding `local`, `noshare`, private and recall-only content. Include schema version, cluster ID, primary node ID, project ID, a content revision and a sorted file map. Verify the revision and every path before writing any files. Only `policy/manifest.json`, `policy/rules/*.mdc`, `policy/skills/*/SKILL.md` and `memory/shared.jsonl` are allowed. Validate manifest identifiers and content hashes before importing; never follow symlinks or trust a received absolute path.

Replicas stage each validated revision in a fresh directory, persist the selected mirror and retain the previous revision until the new one is durable. Repeated revisions skip import after a successful import. Policy changes remain pending review. Shared memory IDs are namespaced by cluster, project and publishing node to preserve private/local IDs. Persist successful revision/time and failures per project. Offline failure keeps the last mirror and locally usable Ax. Replica polling retries with a bounded interval; no election or implicit role promotion.

## Docker and hosted service

Build the CLI and embedded Command Center from source in a multi-stage image. Run as a non-root user, keep configuration read-only and project/state volumes persistent. A Compose example binds the service to host loopback by default and accepts secrets through environment variables. Include initial indexing, server health, logs, backup, upgrade and shutdown commands. A container bind address is configurable; remote MCP must retain loopback as the native default and require an explicit non-loopback bind. Never expose the unauthenticated Command Center to the Internet.

## Documentation and delivery gates

Add a discoverable guide covering local use, primary/replica setup, OAuth MCP clients, Docker, TLS hosting, LAN discovery limitations, policy review, offline recovery and troubleshooting. Capture actual rendered guide/Command Center screens with Puppeteer; label documentation captures as documentation, without presenting mock state as a running cluster.

Verify config rejection, token/project isolation, replica publication denial, unsafe snapshot paths/manifest IDs, revision integrity, deterministic snapshots, idempotent import, preserved local memory, same-cluster discovery filtering and multiple-primary visibility. Build the workspace and docs and require Linux/Windows/macOS CI and strict Clippy before merge. Publish new binaries only from main with an appropriate version bump. Confirm website deployment independently from successful build/release; report any missing Netlify credentials explicitly.
