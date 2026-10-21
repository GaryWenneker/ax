# Evidence — Command Center hardening

Spec: `/Users/gary/io/ax/specs/2026-10-21-command-center-hardening/SPEC.md`

Spec approval: obtained. User message: "do it".

Tier: 2. Isolation: current working tree. No commit.

## Behavior map

| Behavior | Test |
|---|---|
| Foreign origin gets no CORS allow header; loopback origin does | `browser_origin::tests::foreign_origin_cannot_read_the_api`, `command_center_origin_can_read_the_api` |
| Terminal socket rejects a foreign origin before upgrade | `foreign_origin_cannot_open_the_terminal_socket` |
| Login on: `/dav/` and `/ax/` are 401; `/api/share/status` stays 200; a share token is not treated as signed-out | `login_covers_the_vault_and_a_share_token_still_opens_it` |
| Existing share-token WebDAV check still passes | `dav_requires_share_token_when_sharing` |
| Pytest filter is one argument, program is `pytest` | `pytest_filter_stays_one_argument` |
| `?token=` sets an HttpOnly cookie and redirects without the token | `share_link_sets_a_cookie_and_drops_the_token_from_the_url` |
| Wrong token and a protocol-relative path do not redirect | `wrong_query_token_is_not_a_redirect`, `share_redirect_stays_on_this_host` |
| Test signing key is not in the tree | `signing_key_is_not_committed` |
| Remote MCP tokens still verify | `remote_mcp` suite, 14 passed |

## Fresh run

After the last source edit:

- `cargo test -p ax-web --lib browser_origin -- --test-threads=1` — 3 passed
- `cargo test -p ax-web --lib share_auth -- --test-threads=1` — 4 passed
- `cargo test -p ax-web --test command_center_hardening --test dav_share -- --test-threads=1` — 2 passed, then 1 passed
- `cargo test -p ax-ship --lib pytest_filter_stays_one_argument -- --test-threads=1` — 1 passed
- `cargo test -p ax-mcp --test remote_mcp -- --test-threads=1` — 14 passed, 0 failed, 18.80s

The combined command was aborted while `remote_mcp` was still running. Those four earlier results had already finished. `remote_mcp` was run again on the same sources and passed 14.

## Mutation controls

- Pytest runner swapped to `sh -c`. `pytest_filter_stays_one_argument` failed: left `"sh"`, right `"pytest"`. Restored.
- `browser_origin_allowed` forced to return true. `loopback_origin_matches_the_command_center_port` failed on `http://evil.example`. Restored.

No project mutation tool was run. These two hand mutants are the mutation layer.

## Skipped

- Full `ax-web` suite, coverage, clippy `-D warnings`, and `cargo audit`: not run. The change adds dev-dependencies `rsa` 0.9 (feature `pem`) and `rand` 0.8, recorded in `Cargo.lock`.
- CodeGraph compare matrix: this is Command Center HTTP hardening. It does not change graph extraction or resolution.
- Web UI rebuild: no web-ui source change.
- Websocket 101 in-process: `oneshot` has no upgrade extension, so an allowed origin stops at 426. The foreign origin is 403 with body `Untrusted origin` before that.

## Review

Rust review after the fixes: approved with the limits above. Dead `fixed_share` warning removed by keeping that helper inside the test module. Redirect paths that start with `//` or contain `\` are not used as `Location`.
