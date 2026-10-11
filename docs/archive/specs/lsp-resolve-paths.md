# LSP resolve working binaries (not rustup shims)

Tier 2. Isolation: none. Spec approval: not obtained (autonomous run).

## Behaviors

### L1 — rustup proxy is not “available”
A binary whose `--version` output contains `Unknown binary` or `is not installed` is not available, even if it is on PATH.

### L2 — Prefer a working binary over a shim
When PATH has a rustup shim and `rustup which rust-analyzer` (or another candidate) is a real server, status and enrich use the working path.

### L3 — Project `node_modules/.bin`
`typescript-language-server` (and other commands) are resolved from `{project}/node_modules/.bin` and `{project}/crates/ax-web/web-ui/node_modules/.bin` when those files exist and pass `server_binary_works`.

### L4 — Command Center status uses project root
`GET /api/lsp/status` searches extra dirs relative to the open workspace, not PATH alone.

## Invariants
- Servers that are truly absent stay `missing`.
- English UI/docs only.
- No new crates; `which` already in ax-lsp.

## Files
- `crates/ax-lsp/src/servers.rs`, `client.rs`, `enrich.rs`, `lib.rs`
- `crates/ax-web/src/lsp_api.rs`
- `crates/ax-mcp/src/tools.rs`
- `site/src/content/docs/guides/lsp.md`

## Deps
- Environment: `rustup component add rust-analyzer`
- Optional: `typescript-language-server` in web-ui node_modules
