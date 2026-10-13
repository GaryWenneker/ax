# Command Center authentication, users, multi-role RBAC and passkeys

**Status:** Proposed, awaiting specification approval. No production authentication behavior is changed by this document.
**Target:** Ax standalone Command Center (`crates/ax-web` + `crates/ax-web/web-ui`), including remote installations.
**Existing implementation:** `docs/specs/team-client-access-management.md` remains authoritative for machine clients and replication.

## Objective

An Ax Owner can optionally require interactive sign-in to the Command Center, manage human users, assign **one or more** roles to each, and enroll and use passkeys (WebAuthn). Users may have multiple passkeys. Disabling the local interactive login must be possible without deleting users, roles, tokens, or passkeys.

The existing Team members are the human user identities; **do not create a parallel users table**. The existing Owner/Admin/Member/Read-only role vocabulary remains. Machine client tokens, Remote MCP OAuth and replica credentials remain separate principals and must not be mistaken for human browser sessions.

## Confirmed baseline / blast radius

- `crates/ax-global-db/src/access.rs`: `team_members.role` is currently a single `MemberRole`. Client effective scopes filter against it, and last-Owner protection checks this column.
- `database/schema/global.db.schema.sql`: durable members and clients already exist; an additive migration is required.
- `crates/ax-web/src/access_api.rs`: local host + CSRF currently acts as implicit Owner for Access API requests. A valid hostname is **not** an authenticated person.
- `crates/ax-web/src/workspace_state.rs`: the majority of Command Center APIs are on other routers. Securing only `/api/access`, or showing a React login screen without API middleware, leaves data exposed.
- `crates/ax-web/src/local_owner.rs`: another local-owner-only guard exists for publication operations and must be reconciled.
- `crates/ax-web/web-ui/src/components/AccessSettingsSection.tsx`: Team UI edits a single role.
- `crates/ax-web/web-ui/src/lib/settingsNavigation.ts`: Access settings has Team, Clients & tokens, Replication.
- `crates/ax-web/web-ui/src/App.tsx`: no application-wide authenticated-session gate.
- `crates/ax-web/web-ui/e2e/access-settings.spec.ts`: existing CSRF + one-time token visibility fixtures must remain valid in legacy local mode.

## Decisions

1. **Local legacy by default:** keep login disabled for existing loopback-only local installations, preserving current standalone behavior.
2. **Remote fail-closed:** a publicly reachable Command Center must never become open merely because login is disabled. If login is off, server-managed Command Center access is loopback-only or protected by an explicitly configured, trusted upstream authentication system with a validated principal. A proxy forwarding untrusted `Host`, `Origin`, `X-Forwarded-*`, or identity headers is not an authentication mechanism.
3. **Human login:** email/username + password (Argon2id) is a supported recovery-capable baseline; passkey is first-class passwordless login. An Owner can prefer passkey-first. Never allow unauthenticated self-registration.
4. **Passkey enrollment:** browser **registers** a passkey with its platform authenticator/password manager/security key; there is no software to install. One user may register many passkeys. Enrollment requires a recent authenticated session, except for the tightly constrained initial Owner bootstrap ceremony.
5. **Role semantics:** roles are additive (union of granted scopes), while explicit denials and resource ACLs still apply; `readonly` does not override `admin` when both are assigned. Only an Owner can assign or revoke Owner. No user may remain role-less if active. Role changes immediately affect all browser and client permissions.
6. **No parallel permission engine:** reuse and extend existing `MemberRole`, `VALID_SCOPES`, client-scope narrowing and effective-principal evaluation. UI controls are only hints; every API enforces permissions.
7. **Switching login off:** Owner-only, requires fresh passkey/password reauthentication and local trusted management access; immediately invalidates web sessions. Do not delete identities or authenticators. Re-enabling requires confirming a usable active Owner sign-in method.
8. **Safe access recovery:** single-use hashed recovery codes issued at bootstrap; support multiple passkeys; protect the final Owner and the final viable method. No bypass through a magic URL, environment variable, `localhost` Host header, share cookie or client bearer token once login is enabled.

## Persistence (additive, backward compatible)

- `member_roles(member_id TEXT, role TEXT, assigned_by, assigned_at, PRIMARY KEY(member_id, role))`: copy existing `team_members.role` values exactly once using idempotent migration, in a transaction. Make this the authoritative role set; retain legacy `role` column only as a compatibility projection while all readers are migrated. Do not silently continue reading only the old field.
- `web_auth_settings(singleton=1, enabled, rp_id, canonical_origin, updated_by, updated_at)`; explicit canonical origin(s) and RP ID with validation, never auto-trust arbitrary request headers. Store in global installation DB, not a project index.
- `web_password_credentials(member_id, password_hash, updated_at)`: Argon2id encoded verifier, never plaintext. Use safe reset/recovery policy and rate limiting.
- `web_passkeys(id, member_id, credential_id UNIQUE, webauthn_credential_json, label, created_at, last_used_at, revoked_at)`: public-key credential metadata/counters, not private keys.
- `web_auth_challenges(id_hash, member_id, kind, state, expires_at, consumed_at)`: server-side ceremony state, single use, short TTL, bound to user and RP/origin.
- `web_sessions(id_hash, member_id, created_at, last_seen_at, expires_at, revoked_at, authenticated_at)`: opaque random cookies; hash stored, idle and absolute expiration, rotate on authentication and privilege changes.
- `web_recovery_codes(member_id, verifier UNIQUE, consumed_at, created_at)`: generated cryptographically, hashed, displayed once.
- Add indexes/foreign keys; migration and `database/schema/global.db.schema.sql` must agree, with backfill and rollback strategy. All sensitive changes go into `security_audit_log` without secret values.

## Backend design

- Add a single browser authentication/authorization boundary around Command Center HTML/static application entry and **all** Command Center data/mutation APIs in `workspace_state::WebHub::nest_routers` (and any routes registered outside it). Explicitly separate machine endpoints (Remote MCP OAuth, replication, access-token clients, health checks, WebDAV where relevant) and document their own principals/scopes. Do not inadvertently block MCP or replica flows, or let their tokens authenticate as human sessions.
- Unauthenticated `GET /api/web-auth/status`: return minimal `enabled` and setup-ready state, never user enumeration; `GET /api/web-auth/session` returns current identity, roles/scopes if authenticated or 401. Browser API failures are JSON 401/403, not SPA HTML with status 200.
- `POST /api/web-auth/login`, `POST /api/web-auth/logout`, `POST /api/web-auth/passkeys/authentication/start`, `POST /api/web-auth/passkeys/authentication/finish` for interactive sign-in. Pair each start/finish with one stored, expiring, consumed ceremony state. Login rate limit per IP and account with non-enumerating error messages.
- Authenticated personal profile endpoints: `GET /api/web-auth/passkeys`, registration start/finish, rename/revoke, `GET /api/web-auth/sessions`, revoke session(s), password change, recovery-code rotation. Passkey add/remove requires recent step-up authentication.
- Owner-only management endpoints: `GET/PATCH /api/web-auth/config`, users list/create/update/status, assign/remove roles atomically, reset credentials/revoke sessions, with CSRF and strong reauthentication for sensitive changes.
- Prefer `webauthn-rs` (verified server-side verification of challenge, signed origin/RP ID, UV, credential ownership and counter handling); never implement COSE, CBOR or signature verification ad hoc. For browser serialization use supported native WebAuthn APIs with tested compatibility, or a small maintained browser library if native compatibility cannot be assured.
- Passkey RP ID needs a **stable canonical hostname**; use `https://admin.wenneker.dev` only when that is the actual configured external origin. HTTPS in deployments; local `http://localhost:<port>` may use the localhost WebAuthn exception. Changing hostname requires an explicit migration/re-enrollment plan. Do not derive RP ID from inbound `Host`.
- Session cookie: `HttpOnly`, `Secure` for HTTPS, `SameSite=Lax` or stricter, host-only path, no credential in localStorage. Rotate session identifier after login and step-up; protect mutations with CSRF token + exact Origin checks. Set `Cache-Control: no-store` on sensitive APIs. Enforce CSP and adequate XSS protection.
- Separate readonly mode from authentication and roles; authenticated Owners may still be constrained by readonly.
- Consider realtime connections/WebSockets, file downloads, static assets and error routes during middleware coverage review. Disabling a user or revoking a role must make authorization fail on the next request.
- Initial bootstrap: only on a trusted loopback/local management session before web login is enabled, claim the existing `local-owner` member, verify a password and/or register a passkey, issue recovery codes, and only then atomically activate login. Never allow a remote caller to claim the initial Owner.

## Frontend design (desktop and mobile)

- A dedicated responsive full-screen Sign in page, returning to the originally requested route after successful auth; password and **Sign in with passkey** actions; relevant support/error/recovery states.
- AppShell uses server-confirmed session state and a loading state; no protected application data is fetched or displayed before authentication. A 401 revocation returns to Sign in.
- Settings navigation: **Security**, **Users & roles** (reuse Team, migration-friendly), **Clients & tokens**, **Replication**. Avoid duplicating Team users as a separate identity type.
- Security: authentication toggle, canonical site origin/RP ID, passkey status, sessions and recovery controls; show security implications clearly. Enabling requires completed Owner enrollment.
- Users & roles: searchable user list and detail slideout on mobile; multi-select role chips/checkboxes, effective scope preview, status, last activity, passkeys count, session management, create/invite administrative onboarding, disable, audit. The last active Owner cannot lose Owner or be disabled.
- My account: register/rename/remove multiple passkeys with browser UI, last-used timestamps, password update and sign out.
- Accessible input errors, focus management, WCAG contrast, touch-safe buttons, no horizontal overflow. Cover desktop and mobile Playwright flows.
- Password/secret/recovery values never leak into screenshots, logs, analytics, HTML attributes or client-side persistent storage.

## Executable acceptance scenarios

| ID | Given / When | Then (automated evidence) |
|---|---|---|
| AUTH-01 | Legacy local installation, login not enabled | Existing local owner workflows still work; remote Command Center is not anonymously exposed |
| AUTH-02 | Owner attempts enablement without a usable login method | API rejects change, state stays disabled, no lockout |
| AUTH-03 | Enrolled Owner enables authentication | Unauthenticated UI and every protected API reject/redirect appropriately; authenticated session works |
| AUTH-04 | HTTP caller forges `Host: localhost` from remote, or sends share/client token as cookie | No Owner impersonation |
| AUTH-05 | Valid password or valid passkey assertion | New opaque rotated session cookie, correct human identity/roles; no credentials in API response |
| AUTH-06 | Wrong password, expired/duplicate/mismatched challenge, wrong RP/origin, missing UV, unknown/revoked credential | No session created, failures audited/rate-limited |
| AUTH-07 | User registers two passkeys, then revokes one | Remaining key still signs in, revoked key cannot authenticate; secret material never persisted |
| AUTH-08 | Administrator edits user from one role to multiple | Effective permissions are union of roles; client scopes only narrow, never elevate |
| AUTH-09 | Role revoked or user disabled while session/client active | Next protected request fails for permissions no longer held |
| AUTH-10 | Attempt to disable/demote final active Owner | Atomic rejection even under concurrent requests |
| AUTH-11 | Other account attempts to remove someone's passkey or roles | 403, no mutation |
| AUTH-12 | Owner disables login after step-up on authorized local management surface | Existing browser sessions invalidated, local legacy restored, remote requests still denied or upstream-authenticated |
| AUTH-13 | Owner loses a device | Another registered passkey or one-time recovery code restores access; code cannot be reused |
| AUTH-14 | Browser session expired/revoked | SPA returns to Sign in; stale data cannot be fetched; other active sessions unchanged |
| AUTH-15 | Machine client or authenticated Replica is used | Existing scope/trust boundaries, token rotation and replication behavior remain unchanged |
| AUTH-16 | Mobile browser opens User detail/Settings/Passkeys | Slideout focus/scroll/touch behavior works and no horizontal overflow |
| AUTH-17 | Upgrade from single-role populated global DB | Roles backfill exactly once; no clients, node identities or data lost |

## Implementation sequence and source mapping

1. **RED tests first**: `crates/ax-global-db/src/access.rs` tests for migration, unions, last-Owner concurrency and client narrowing; Axum integration tests for route and session security; `crates/ax-web/web-ui/e2e/command-center-auth.spec.ts` for browser/role/passkey flows, including mobile.
2. Add schema/migrations at `database/schema/global.db.schema.sql` and the actual global DB init migration path; update `crates/ax-global-db/src/access.rs`, preserving existing APIs until consumers migrate.
3. Implement authentication module under `crates/ax-web/src/web_auth/`, and shared middleware in `crates/ax-web/src/workspace_state.rs` plus route-specific integration tests; reconcile `access_api.rs`, `local_owner.rs`, Remote MCP and WebDAV policies.
4. Add frontend authentication API/client module, `Login` and account/security components, `App.tsx` gate, `AccessSettingsSection.tsx` multi-role UI, and `settingsNavigation.ts` grouping.
5. Update user-facing documentation in `site/src/content/docs/guides/command-center-walkthrough.md`, `site/src/content/docs/reference/web-api.md`, and `README.md` in the same implementation PR; no per-version changelog.
6. Run full Cargo format/lint/tests, frontend `tsc && vite build`, Playwright desktop/mobile, negative controls for 401/403, dependency/security audit, and report precise evidence in `docs/specs/command-center-auth-passkeys/EVIDENCE.md`. No merge if gates fail.

### Dependencies proposed (to validate/pin during implementation)

- Rust `webauthn-rs` (0.5.x): maintained WebAuthn verification rather than hand-rolled cryptography.
- Rust `argon2` + `password-hash`: password verifiers.
- Existing `uuid`, `sqlx`, `axum`, `serde` and browser WebAuthn API for session/DB/HTTP logic. Add a browser helper only if tested compatibility requires it. Update `Cargo.lock` and npm lockfiles together with manifests.

**Repository workflow:** use a feature branch from current `main`; commit this SPEC first, obtain review/approval before modifying implementation files; then test-first implementation and a recorded EVIDENCE report. Do not deploy to Netlify from the agent. Merge only after applicable CI checks are green and authorization scope is verified.

## Out of scope

- Replacing the existing Remote MCP OAuth authorization server.
- Replacing per-client token issuance with browser cookies.
- Changing replication node trust semantics or export/import visibility.
- Auto-inviting users by email until mail delivery and verified enrollment are explicitly designed.
- Multi-tenant SSO/SCIM, external social login or passwordless email magic links.
