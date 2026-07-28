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

Enrich reuses **one process per language server** for the whole batch and waits for
`rust-analyzer/serverStatus` (`quiescent`) before querying definitions. Large Cargo
workspaces can take 1–3 minutes on the first enrich; later refs in the same run are
fast. Spawning a fresh rust-analyzer per file was the main reason Enrich used to
report `resolved: 0` with no errors.

### Reconcile vs Enrich

| Action | What it does |
|--------|----------------|
| **Reconcile** | Prune stale unresolved rows, then name-match against symbols already in the graph. Stdlib/method noise (`into`, `map`, `to_string`, …) usually stays unresolved — that is expected. |
| **Enrich with LSP** | Ask language servers for `textDocument/definition` and write Exact edges. Needs installed servers (`ax lsp status`) and enough warmup time on big projects. |

If Enrich shows `examined > 0`, `resolved: 0`, `errors: []`, the servers ran but returned
no definitions (still indexing, or the symbol is unresolved even for the LSP). Check
`ax lsp status` first; do not reinstall blindly when all servers show `[ok]`.

A rustup shim on PATH alone is not enough — install the component first:

```bash
rustup component add rust-analyzer
```

Other servers (examples):

```bash
npm install -g typescript typescript-language-server pyright
go install golang.org/x/tools/gopls@latest
```

Command Center **Refresh servers** shows `shim` when PATH has a binary that fails
as a rustup-style stub, and `missing` when the command is absent. `pyright-langserver`
rejects `--version`; ax still marks it available when the binary runs.

On Windows, workspace paths may be stored with a `\\?\` prefix; ax strips that before
building `file://` URIs so language servers do not reject documents with
`url is not a file`.
