# LSP bridge

Optional Language Server enrichment for unresolved references.

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

Tree-sitter remains the default extractor. LSP is best-effort: missing servers,
timeouts, and empty definitions are skipped. Successful definitions become edges
with provenance `lsp` and confidence `exact`.
