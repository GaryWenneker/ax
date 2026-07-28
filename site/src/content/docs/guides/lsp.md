---
title: LSP enrichment
description: Optional Language Server bridge for Exact graph edges.
---

Tree-sitter remains the default extractor. The LSP bridge is **opt-in** and best-effort: missing servers, timeouts, and empty definitions are skipped.

## Commands

```bash
ax lsp status              # which servers are on PATH *and runnable*
ax lsp enrich              # resolve up to 200 unresolved refs
ax lsp enrich --limit 50 --json
```

`ax lsp status` marks a server `--` when the binary is a rustup shim that is not installed yet. On Windows/Rust that usually means:

```bash
rustup component add rust-analyzer
```

Install the other servers (examples):

```bash
npm install -g typescript typescript-language-server pyright
go install golang.org/x/tools/gopls@latest
```

A shim on PATH alone is not enough — enrich would otherwise spam `missing Content-Length` (empty stdout) and Quality would false-flag `LspAvailableUnused`. Note: `pyright-langserver` does not support `--version`; ax still detects it when the binary is on PATH and runnable.

## Servers

| Id | Command | Languages |
|----|---------|-----------|
| rust-analyzer | `rust-analyzer` | Rust |
| typescript-language-server | `typescript-language-server --stdio` | TS/JS |
| pyright | `pyright-langserver --stdio` | Python |
| gopls | `gopls` | Go |

Successful definitions become edges with provenance `lsp` and confidence `exact`. CLI and Command Center both write `lsp enrich …` lines to the daily verbose log when Verbose MCP logging is on.

Enrich keeps **one language-server process per server** for the whole batch and waits until `rust-analyzer` reports `quiescent` before definition queries. On large Cargo workspaces the first Enrich can take 1–3 minutes; that warmup is required — a cold server often returns empty definitions with no error.

### Reconcile vs Enrich

| Action | What it does |
|--------|----------------|
| **Reconcile** | Prune stale rows, then name-match against symbols already in the graph. Leftover stdlib/method noise is normal. |
| **Enrich with LSP** | Ask installed language servers for definitions and write Exact edges. |

If `ax lsp status` shows all servers `[ok]` but Enrich still resolves 0, wait for the longer first run (or raise `--limit` after a successful warm run) rather than reinstalling servers.

## Command Center

On **Unresolved**, use **Enrich with LSP** (ModalShell) — calls `POST /api/lsp/enrich`.
**Refresh servers** re-reads `GET /api/lsp/status`. Badges:

| Badge | Meaning |
|-------|---------|
| `available` | Binary on PATH and `--version` succeeds |
| `shim` | On PATH but not runnable (common: rustup shim without the component) |
| `missing` | Not found on PATH |

With Verbose MCP logging on, Logging shows `lsp` domain lines; Activity chip deep-links filter to `/logging?kind=lsp`.

## Further reading

- Repo docs: [`docs/LSP.md`](https://github.com/GaryWenneker/ax/blob/main/docs/LSP.md)
