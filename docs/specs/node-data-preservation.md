# Node synchronization without losing existing knowledge

Status: implemented and locally verified; main merge pending.
Owner request: 2026-10-11 — coupling an existing Ax environment to an empty Primary must retain its months of work and synchronize knowledge across nodes.

## Contract

- Native memories, rules and skills remain unchanged when pairing, including with an empty Primary.
- Enrolled, authenticated replicas contribute their native state to the Primary and receive the combined state. Enrollment authorizes the full project's knowledge, including private and disabled records.
- Every accepted source revision is retained. Returning to an earlier revision publishes that current choice while retaining later history. Missing records are not deletions. No automatic pruning or replacement with an empty peer is allowed.
- Different origin nodes using the same record ID preserve distinct values. Imported copies use separate IDs. Locally edited copies are preserved and contributed as local variants; unchanged imports do not loop around the cluster.
- Settings applies received memories safely and shows existing counts, conflict/failed-sync attention states and a policy-review link. CLI `sync` stages; `apply`/`--auto-apply-memories` project memory. Received rules and skills require approval.
- Authentication binds the contributor to its node identity and enrolled project before accepting data. Client credentials, blocked nodes, invalid identities, cross-cluster/project payloads and malformed/tampered revisions cannot change retained state.
- Retention and aggregate bounds reject new contributions atomically without pruning history or local records. Existing protocol state/wire limits remain in force.
- Unsupported older primaries cause an explicit upgrade error. Both nodes must use current main. The graph, secrets, caches, credentials and database files are not transported.

## Verification required

An empty Primary plus a populated Replica must share existing memories with a third node without changing the contributor. Test identical record IDs with different values, imported/local edit conflicts, empty snapshots, policy review and feedback prevention, repeated exchange, immutable history, capacity rejection and unauthorized submissions. Exercise actual CLI/Command Center processes using isolated synthetic databases, without project initialization or provider credentials. Update README, CLI reference, guide and current UI capture in the same change. Actions/releases/deployments remain paused.

Completed local results and environment limits are recorded in [the verification evidence](node-data-preservation-EVIDENCE.md).
