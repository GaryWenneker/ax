# Command Center hardening

Approved by the user on 2026-10-21 ("do it") from the remediation plan in this chat.

Spec approval: obtained (user: "do it").

Isolation: the current working tree. The change is the product fix the user asked to land here.

## API gates

Scope: existing local Command Center HTTP surface.

- Boring: no new resources. Share links still start with `?token=`.
- Compatibility: same-origin Command Center requests stay the same. A browser that follows redirects still opens a share link. The wildcard cross-origin grant is removed on purpose.
- Authentication: login stays opt-in. When it is on, the vault requires that session. A share token still opens the vault.
- Authorization: the gate checks the session or the share token on the server.
- Idempotency: the share redirect is safe to repeat.
- Blast radius, pagination, expensive fields: unchanged. N/A.
- No leakage: the redirect location does not contain the token.

## Behaviors

1. A request whose `Origin` is `http://evil.example` does not receive `Access-Control-Allow-Origin` for that origin from `command_center_cors`. A request whose `Origin` is `http://127.0.0.1:{port}` does.
2. `GET /api/agent/pty/ws` with a foreign `Origin` is rejected before a CLI is spawned. The same socket with `Origin: http://127.0.0.1:{port}` is accepted.
3. With login enabled and no share token, `GET /dav/` and `GET /ax/` without a session are 401. `/api/share/status` stays available. A request that presents the share token is not rejected as unsigned-in.
4. `family_command("pytest", &["ok; touch /tmp/ax-ship-pwned"], &config)` uses program `pytest` and passes that string as the single `-k` argument. The program is not `sh` or `cmd`.
5. `GET /notes?token=sekret&stay=1` when the share token is `sekret` responds 303 to `/notes?stay=1` with an `HttpOnly` `ax_share` cookie. The location does not contain `sekret`. The follow-up with that cookie and no token is 200.
6. `crates/ax-mcp/tests/fixtures/remote-test-key.pem` is not in the tree. Remote MCP token tests sign with a key generated in the test.

## Must keep

- Argon2 passwords and the `HttpOnly` `SameSite=Strict` session cookie.
- Login off: non-vault behavior stays open on localhost.
- Placeholder SQL and the static replication table list.
