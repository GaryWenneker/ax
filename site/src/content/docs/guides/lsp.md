---
title: LSP enrichment
description: Optional Language Server bridge for Exact graph edges.
---

Tree-sitter remains the default extractor. The LSP bridge is **opt-in** and best-effort: missing servers, timeouts, and empty definitions are skipped.

## Commands

```bash
ax lsp status              # which servers are on PATH
ax lsp enrich              # resolve up to 200 unresolved refs
ax lsp enrich --limit 50 --json
```

## Servers

| Id | Command | Languages |
|----|---------|-----------|
| rust-analyzer | `rust-analyzer` | Rust |
| typescript-language-server | `typescript-language-server --stdio` | TS/JS |
| pyright | `pyright-langserver --stdio` | Python |
| gopls | `gopls` | Go |

Successful definitions become edges with provenance `lsp` and confidence `exact`.

## Further reading

- Repo docs: [`docs/LSP.md`](https://github.com/GaryWenneker/ax/blob/ax-v4/docs/LSP.md)
