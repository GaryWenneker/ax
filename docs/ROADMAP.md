# ax v4 — Missing Features Roadmap (2026–2027)

This roadmap covers **net-new** capabilities only. It does not rebuild shipped features (Leiden, god-nodes, incremental sync, `ax affected` / `ax test-impact`, HTML export, FTS5, in-tree extractors).

Branch: **`ax-v4`**.

## Timeline

| Window | Deliverables |
|--------|----------------|
| **Q3 2026** | Monorepo workspace federation + `ax policy pull`; multi-format graph export |
| **Q4 2026** | WASM plugin host; ONNX dense vectors; `ax ship --ci` + test runners |
| **Q1 2027** | LSP bridge (`ax-lsp`); Command Center share / PWA / live stream |

## Q3 status (shipped on `ax-v4`)

### 1. Monorepo workspace federation

- [x] `members` in `ax.json` / `.ax.json`
- [x] `ax init --workspace` discovery (Cargo workspace + nested `.ax/`)
- [x] `ax sync --all` / `ax index --all`
- [x] `ax policy pull <git-url>`
- [x] Cross-service contract edges (OpenAPI / Protobuf / GraphQL → `contract:…` routes)
- [x] Hierarchical policy merge (`~/.ax/global_policy` → workspace → member)
- [x] Shared memory git sync (`ax memory export|import`, opt-in `memorySync` hooks)

### 2. Multi-format graph export

- [x] `ax export graph --format json|dot|graphml|gexf|cypher|mermaid|plantuml|html`
- [x] Leiden community + god-node degree fields in payloads
- [ ] PDF report export (deferred)

## Q4 status (in progress on `ax-v4`)

- [x] `ax ship --ci` — JSON report on stdout, exit 1 if gate failed
- [x] Multi-runner TIA execution (cargo / pytest / jest / vitest / go)
- [x] Process plugin host (`.ax/plugins/*/plugin.toml`) — see [PLUGINS.md](./PLUGINS.md)
- [x] Optional WASM host (`--features plugins-wasm`, wasmtime)
- [x] Optional ONNX dense embeddings (`--features onnx`) — see [ONNX.md](./ONNX.md)
- [x] Weighted hybrid recall (vector 0.5 / FTS 0.3 / graph reserved 0.2)
- [x] CI example workflow — [examples/github-actions-ship.yml](./examples/github-actions-ship.yml)

## Q1 status (shipped on `ax-v4`)

- [x] LSP bridge (`ax-lsp`) — `ax lsp status|enrich`, Exact/`lsp` edges — [LSP.md](./LSP.md)
- [x] `ax share` token gate + LAN bind (read-only) — [SHARE.md](./SHARE.md)
- [x] Command Center PWA manifest + service worker
- [x] Live action stream (`/api/actions/events`)
- [x] Site guides + CI snippets (GitHub / GitLab / Azure Pipelines)

## Reality notes

| Topic | Already shipped | Net-new |
|-------|-----------------|---------|
| Edge confidence | `extracted` / `inferred` / `ambiguous` | LSP `exact`, LLM `synthesized` |
| Memory hybrid search | FTS5 + feature-hash vectors | Neural ONNX embeddings |
| Test impact | Graph selection | CI mode + runner execution |
| Config file | `ax.json` | `.ax.json` alias + `members` |

## Extension points

See [EXTENSION_POINTS.md](./EXTENSION_POINTS.md).
