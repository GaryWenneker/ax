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

A shim on PATH alone is not enough — enrich would otherwise spam `missing Content-Length` (empty stdout) and Quality would false-flag `LspAvailableUnused`.

## Servers

| Id | Command | Languages |
|----|---------|-----------|
| rust-analyzer | `rust-analyzer` | Rust |
| typescript-language-server | `typescript-language-server --stdio` | TS/JS |
| pyright | `pyright-langserver --stdio` | Python |
| gopls | `gopls` | Go |

Successful definitions become edges with provenance `lsp` and confidence `exact`. CLI and Command Center both write `lsp enrich …` lines to the daily verbose log when Verbose MCP logging is on.

## Command Center

On **Unresolved**, use **Enrich with LSP** (ModalShell) — calls `POST /api/lsp/enrich`.
With Verbose MCP logging on, Logging shows `lsp` domain lines; Activity chip deep-links filter to `/logging?kind=lsp`.

## Further reading

- Repo docs: [`docs/LSP.md`](https://github.com/GaryWenneker/ax/blob/main/docs/LSP.md)
