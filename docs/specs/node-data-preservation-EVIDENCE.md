# Node data preservation — verification

Verified on 2026-10-11 with the preservation changes applied to main `20c0c6e0c6aa0eefc371435f759c49e7a7ef1561`, including its menu and Command Center styling changes. Main merge is pending; downloadable releases and the public website have not been published.

## Results

| Check | Result |
|---|---|
| `cargo build -p ax-cli` with the freshly built web assets | Passed |
| `cargo test -p ax-cli --bin ax server_` | 23 passed |
| `cargo test -p ax-web --lib` | 89 passed |
| Frontend TypeScript and Vite production build | Passed |
| Frontend TypeScript unit tests | 296 passed |
| Playwright replication setup, access management and Azure shell | 18 passed |
| Website tests and local Astro build | 27 passed; build passed |
| Actual Command Center/CLI exchange across three synthetic projects | Passed |

The eight exchange regression cases cover an empty Primary with existing Replica knowledge; distinct origin nodes with identical IDs; durable/idempotent history; returning to a previously accepted revision; memory propagation to a third node without feedback copies; empty snapshots and locally edited imports; pending policy approval and edited policy contributions; and atomic capacity rejection without pruning retained state. HTTP tests reject forged contributor identity, another project and revoked node credentials before creating contribution history. Existing tests cover tampering, cross-cluster state, path validation, disabled/private memory and signed notifications.

## Actual-process check

Three independent disposable project databases were created from an existing synthetic schema fixture. No CLI initialization, provider call, user database or inherited provider credential was used. The children used a fresh Ax home/global database and only loopback HTTP; every managed service and Command Center was stopped afterwards.

The test started three `ax web` processes, then created their supervised Primary/Replica services through the real local Owner API, including session CSRF and Origin headers. A populated Replica paired with an empty Primary. Its native memory ID, body, enabled flag and timestamps were unchanged; the Primary received a separately identified copy. A third node paired and received that knowledge. Invalid credentials left native data unchanged and did not save node settings or contribute data.

The third node then contributed a different body using the same native record ID. Both values survived on the Primary and peers. Three additional rounds of `ax server apply` produced no extra copies. Editing an imported copy locally preserved the edit, contributed it as a local variant and made it available to peers while keeping the original contributor's native memory unchanged. The real Settings status reported review attention. The Primary retained the original and edited source snapshots. Unauthenticated and forged-sender HTTP submissions did not add retained history; persisted node settings contained no replication credential.

## UI and documentation

Settings shows existing memory/rule/skill counts before pairing, explains empty-Primary preservation and trusted full-project contribution, and displays retained-conflict or failed-sync attention. Received policies use the existing review screen. Discovery still requires an explicit Primary selection and connection. Read-only permissions, phone sizing, light/dark shell behavior and connection controls passed browser checks.

The guide, CLI reference and README describe contribution, separate imported IDs, retained source revisions, review, omission and the required two-node upgrade. The refreshed pairing capture uses synthetic candidates and states that explicitly. Light and dark access captures select their actual theme explicitly.

## Scope and limits

This synchronizes enrolled memories, rules and skills. Code, graph data, provider keys, client credentials and live database files stay local. Existing native records are not replaced by incoming state. Received policies are pending review; locally edited imports are preserved for review. Ordinary local deletion is not a cluster deletion instruction.

Source revisions remain in `ax_node_contributions`. The Primary rejects new contributions transactionally if retained source payloads exceed 64 MiB, contributing nodes exceed 64, or the merged state exceeds 8 MiB / 10,000 records per kind. The HTTP wire cap remains 9 MiB. It never prunes accepted history or clears local records to fit. Both Primary and Replica must run the current build; an older endpoint produces an explicit upgrade error.

GitHub Actions remain manual-only. No workflow was dispatched, release published or Netlify build/deployment run. Public-site and released-binary behavior therefore does not yet include this change.
