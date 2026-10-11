# Team, Client Access & Node Management

Status: implemented — verification and merge pending
Branch: `feature/team-client-access-management`

## Goal
Make a standalone Ax environment safely usable by teams, external AI clients, and replicated Ax nodes without conflating human identity, client credentials, and replication identity.

## Domain model

### Team member
A human identity belonging to an Ax environment/team.

Roles:
- Owner: full control, including team, clients, tokens and nodes.
- Admin: manage members, clients and nodes, except ownership transfer.
- Member: use Ax through authorized clients.
- Read-only: read/query access only.

### Client
An application acting on behalf of a team member, for example ChatGPT, Cursor, Cline, CLI or a custom integration.

A client has:
- stable client ID
- display name
- client type
- owner/team-member ID
- scopes
- created/last-used timestamps
- status: active/revoked
- one or more credentials with rotation/revocation metadata

Client credentials are shown once at creation and stored only as a secure hash. Plaintext tokens must never be persisted or logged.

### Node
An Ax installation participating in replication. Nodes are not users and are not clients.

Roles:
- Primary
- Replica

A node has a stable node ID, display name, endpoint/address metadata, trust state, credential identity, last-seen timestamp, last-sync timestamp and replication health.

## Authorization

Authentication answers who/what is calling. Authorization answers what that identity may do.

Initial client scopes:
- `memory:read`
- `memory:write`
- `preflight:read`
- `context:read`
- `share:read`
- `share:write`
- `admin:clients`
- `admin:team`
- `admin:nodes`

All protected HTTP/MCP operations must resolve an authenticated principal and enforce the required scope before executing. Denials return a non-secret-bearing authorization error.

Role defaults are translated into scopes, but per-client scopes may only narrow a member's effective permissions, never elevate them.

## Client token lifecycle
1. Authorized member creates a client.
2. Ax creates a cryptographically random token.
3. Token is displayed once.
4. Ax stores only a verifier/hash plus token metadata.
5. Every use updates last-used metadata without logging the token.
6. Tokens can be revoked immediately.
7. Rotation creates a new credential and invalidates the old credential after the configured transition policy.

## Replication trust lifecycle
The Replica configuration is the source of intent. A Replica must explicitly configure its Primary URL and valid project replication credentials. There is no manual approval step when that configuration authenticates successfully.

1. Replica is configured with an explicit `primary_url`, stable node ID and project credential.
2. Replica contacts only that configured Primary.
3. Primary validates the project credential / signed registration.
4. On successful authentication, Primary automatically records or refreshes the Replica as `trusted` for that project.
5. Synchronization starts immediately.
6. Settings shows the connected Replica, endpoint, last seen, last sync and health.
7. Owner/Admin may revoke/block the Replica at any time.
8. A revoked Replica cannot synchronize again until it is explicitly unblocked or re-enrolled according to policy.

UDP discovery/registration never selects or changes the Primary. It only accelerates presence/change notification for a Primary already explicitly configured by the Replica.

Node/replication credentials and client tokens use separate namespaces, storage and authorization paths.

## Settings UX

### Settings > Team
Show members, role, status, joined/created time and last activity. Owner/Admin can add, change role, disable/remove subject to ownership safety rules.

### Settings > Clients & tokens
Show:
- client name/type
- team member
- effective scopes
- active/revoked
- created
- last used
- token/credential status

Actions:
- create client
- issue token
- rotate token
- revoke token/client
- edit scopes

Never redisplay an existing plaintext token.

### Settings > Replication
Show local role (Primary/Replica) and node identity.

For Primary, show:
- Trusted/connected replicas
- Online/offline state
- address/endpoint
- last seen
- last successful sync
- sync/health state
- revoke/block actions

For Replica, show the paired Primary and connection/sync state.

## API requirements
Provide authenticated management endpoints for team members, clients/credentials and nodes. Mutations require role/scope checks. List/detail responses never expose credential material.

Existing replication endpoints must be migrated to node-principal authorization without changing replication semantics.

## Persistence
Add durable records for members, clients, client credentials, nodes and node credentials/trust. Migrations must be backward-safe for existing standalone installations. Existing single-user installations bootstrap an Owner identity and retain working local access.

## Audit
Record security-relevant events:
- member created/role changed/disabled
- client created/revoked
- token issued/rotated/revoked
- node discovered/approved/revoked
- authorization denied

Audit records contain actor, target, action and timestamp, never secret material.

## Compatibility
- Preserve current standalone behavior after migration.
- Preserve existing Primary/Replica terminology.
- Do not use master/slave terminology in product UI, docs or new code.
- UDP remains optional discovery; replication transport and export/import remain separate concepts.
- Shared/non-shared and active/inactive data rules remain governed by the existing replication policy.

## Acceptance criteria
- Multiple team members can use one Ax environment with distinct roles.
- A member can own multiple clients.
- Each client can have independently revocable credentials and narrowed scopes.
- Revoking one client does not revoke the member's other clients.
- A correctly configured and authenticated Replica registers automatically with its configured Primary.
- Primary Settings lists connected/trusted replicas and useful live/sync metadata.
- Revoking a node blocks subsequent replication.
- Client and node credentials cannot be used interchangeably.
- Secrets are never returned after initial issuance or written to logs.
- Existing standalone installations migrate without losing data or access.
- Settings exposes Team, Clients & tokens, and Replication as separate concepts.
- Build, formatting/linting and existing CI gates pass before merge.
- Merge to `main` occurs only after required checks are green.

## Implementation slices
1. Persistence/domain types and migration/bootstrap.
2. Principal resolution, roles/scopes and authorization middleware.
3. Client credential issuance, hashing, rotation and revocation.
4. Automatic Replica registration/trust integration with existing replication.
5. Management API.
6. Settings UI: Team, Clients & tokens, Replication.
7. Audit events.
8. Documentation and upgrade notes.
9. Build/CI, PR, then merge after green checks.

## Completion and verification record

The initial draft was missing effective client-token integration, member edits, scope edits, per-token controls and local node status. These have now been implemented, including last-owner protection, immediate atomic rotation, authenticated/CSRF-protected management, per-request HTTP/MCP scope enforcement, explicit Remote MCP project grants and replica revocation that automatic registration cannot undo. Settings integrates three separate Access entries with the Azure Portal navigation. Client token issuance and credential lists are non-cacheable.

Local verification: targeted database lifecycle/security tests, MCP/web tests, workspace Clippy with warnings denied, production UI build and browser controls. Final CI and merge remain required. No secrets are used in documentation screenshots.
