# Autonomous remote hosting and complete project replication

Status: implementation and local verification complete. Ready to merge; releases and deployments remain paused.

## Goal

The host retains its setup form. Ax generates its configuration and native client identity, persists the selected service configuration, starts configured services automatically, and displays their actual state. Built-in OAuth provides public client registration, PKCE authorization, explicit project-scoped host approval, token refresh and revocation. Existing externally managed OAuth configuration remains available under advanced options. Public HTTPS routing remains an explicit host prerequisite.

Replication transfers the registered project's memories, rules, skills, complete graph (nodes, relationships, references, communities and metadata), indexed source contents, project agent history, structured usage records, working context, context caches, durable transcripts/documents/tasks/hooks and MCP logs. Every authorized node contributes its own data. Primary and replicas can query received graph/source and read received logs after restart. Authentication secrets, machine accounts and unrelated projects stay local.

## Data flow

Protocol version 3 adds a manifest to the existing knowledge exchange. Graph and history are exported from a consistent SQLite read into a clean, whitelisted database; imported namespaces are excluded to prevent feedback. Files use content-addressed BLAKE3 chunks of at most 512 KiB, so graph size is independent of the existing 8 MiB knowledge envelope. Each chunk is authenticated per project and checked before publication. The assembled database must pass integrity, schema, reference and claimed-count checks before its contribution is accepted.

The Primary retains knowledge revisions and current per-origin dataset heads separately. Graph changes therefore do not duplicate all unchanged knowledge history. Received graphs are projected under `ax-replica/<node>/` IDs and paths. This preserves native records and collisions between nodes, keeps imported source available to normal graph queries, and protects imported graph data from local re-indexing. A changed graph replaces only that origin's graph; it cannot overwrite another node's native graph. Accepted conversation rows remain in per-origin history heads after native cache cleanup; imported working context is protected from local expiry/cap eviction. MCP logs retain origin labels, remain separate from native log files and are visible in history and live trace. Rotation, truncation and deleted native log files must preserve already synchronized history.

Sync success means verified chunks, graph/history projection and memory/policy application completed. Transfer errors or partial application must remain visible and must not display a successful synchronization. Status includes dataset counts and the last completed revision. Offline/restarted nodes reuse persisted settings, credentials, manifests and chunks.

## Settings and interfaces

Use centered ModalShell dialogs for setup. A status indicator shows gray for stopped/unconfigured, orange for connecting/review/warning, red for an error, and green for a running service. Replication activity animates only when actual peers or completed replica exchanges exist; reduced-motion preferences are respected. Removed configuration removes its stale state card.

Primary setup selects an advertised private interface address; use the selected address for binding and discovery. Replica setup requires the Primary URL and shared key. The URL already contains the Primary's port. Authenticated enrollment returns cluster and project IDs; discovery hints never grant access. A local service port is allocated automatically, with an advanced override. Persist intent and runtime configuration atomically.

Only Azure Portal Light and Dark themes remain, named Light and Dark. Migrate old theme preferences. Buttons must have clear spacing and text must meet existing contrast rules.

## Verification gates

- Actual Primary and at least two replicas exchange graph, source, memory and logs in both directions.
- Large source/index transfers exceed the old envelope limit using bounded chunks.
- Colliding IDs and names remain distinct; foreign data is not fed back as native contributions.
- Restart, local re-index, log append/rotation/truncation and repeated sync preserve usable received data.
- Corrupt/incomplete chunks, invalid SQLite, mismatched node/project, old protocol and unauthorized requests fail before acceptance.
- Built-in OAuth registration, approval, PKCE, refresh, replay rejection and revocation work through the actual HTTP interfaces.
- Desktop/mobile setup, status, spacing, theme migration and advanced help are validated.
- Build the embedded UI and verify the rebuilt Ax binary serves it; update README, guides and screenshots.
- No automatic workflows, releases or deployments. Commit and PR checkpoints are allowed; merge only after all gates pass.

## Implementation checkpoint

- Autonomous host, built-in OAuth, selected-interface setup and settings UI are reconstructed and saved on the feature branch.
- Protocol 3 includes graph/source, logs, memory/policy and explicitly project-scoped usage/conversation data.
- Complete three-node HTTP exchange and 603 backend unit tests passed, including restart reads, source availability, collision protection, no feedback, history retention and OAuth refresh replay/revocation.
- All 52 desktop/mobile browser checks, 35 website checks, frontend production build, theme/log parsing checks and strict Clippy passed. Current screenshots include a real installed-host capture.
- The installed release binary exchanged graph, cached source, memory, logs and durable history through separate Primary/two-replica CLI processes, including a Primary restart. A late-joining replica also received accepted history after native cleanup in the HTTP integration test.
- The installed host passed owner CSRF/stale-project guards, generated configuration/native identity, discovery, public registration, PKCE/host approval, token exchange, MCP initialization, refresh, revocation and persistent stop/start. The freshly embedded UI serves the generated bundle and displays one local configuration block with actual host state.
- Tests use loopback services and controlled interface fixtures. Physical multi-machine LAN broadcast, OS credential-store availability and an externally routed HTTPS hostname require their deployment environment; no release, workflow dispatch or deployment was performed.
