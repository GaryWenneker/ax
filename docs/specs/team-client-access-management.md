# Team, Client Access & Node Management

Status: implementation specification
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
UDP discovery is discovery only and never grants trust.

1. Replica broadcasts/discovers Primary.
2. Primary records the unknown node as `pending`.
3. Settings shows the pending node for explicit approval.
4. Owner/Admin approves it.
5. Primary issues node-specific credentials.
6. Node becomes `trusted` and may replicate.
7. Primary can revoke the node at any time.
8. Revoked nodes cannot synchronize even if they remain visible through UDP discovery.

Node credentials and client tokens use separate namespaces, storage and authorization paths.

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
- Pending nodes requiring approval
- Trusted replicas
- Online/offline state
- address/endpoint
- last seen
- last successful sync
- sync/health state
- approve/revoke actions

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
- A discovered replica cannot synchronize before explicit Primary approval.
- Primary Settings lists pending/trusted replicas and useful live/sync metadata.
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
4. Node trust/credential integration with existing replication.
5. Management API.
6. Settings UI: Team, Clients & tokens, Replication.
7. Audit events.
8. Documentation and upgrade notes.
9. Build/CI, PR, then merge after green checks.
