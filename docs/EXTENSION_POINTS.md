# ax Extension Points

Where future plugins and federation layers attach. Prefer extending these hooks over forking pipelines.

## Policy

| Hook | Location | Notes |
|------|----------|-------|
| Load / index | `ax-policy` `import_policy_from_files`, `ensure_policy_ready` | Files or DB storage |
| Match / inject | `match_policy`, MCP `ax_preflight` | Agents must not Read `.ax/policy/` when MCP is up |
| Guard | `guard_operation` + `guard:` directives in CRITICAL rules | |
| Remote pull | CLI `ax policy pull` | Clones git registry into `.ax/policy/vendored/` |
| Hierarchy | `policy_layer_dirs` / `import_policy_from_files` | `~/.ax/global_policy/` → workspace → member (later wins) |

## Extraction

| Hook | Location | Notes |
|------|----------|-------|
| Language extractors | `LanguageExtractor` in `ax-extraction` | In-tree tree-sitter extractors |
| Process / WASM plugins | `ax-plugins` via `ExtractionOrchestrator::parse_with_plugins` | `.ax/plugins/*/plugin.toml` — see [PLUGINS.md](./PLUGINS.md) |
| Framework passes | `ax-resolution::frameworks` | Post-extract route/pattern edges |
| Contract federation | `ax_extraction::contracts::index_contracts` | OpenAPI / Protobuf / GraphQL → `contract:…` routes + inferred links |

## Memory

| Hook | Location | Notes |
|------|----------|-------|
| Embed | `ax-memory::embed::embed_text` | Feature-hash default; ONNX when `--features onnx` + model |
| Hybrid recall | `ax-memory::store::recall` | Weighted RRF (vector / FTS / graph reserved) |

## LSP

| Hook | Location | Notes |
|------|----------|-------|
| Enrich | `ax-lsp::enrich_project` / `ax lsp enrich` | Best-effort definition → `Provenance::Lsp` + `EdgeConfidence::Exact` |

## Command Center

| Hook | Location | Notes |
|------|----------|-------|
| Share gate | `ax-web::share_auth` / `ax share` | Token via query / Bearer / cookie; read-only |
| Live actions | `GET /api/actions/events` | SSE stream; `POST /api/actions/publish` |
| PWA | `web-ui/public/manifest.webmanifest` + `sw.js` | Installable shell |

## Resolution / synthesizers

| Hook | Location | Notes |
|------|----------|-------|
| Callback synthesizer | `callback_synthesizer.rs` | Heuristic edges |
| C fn-ptr synthesizer | `c_fnptr_synthesizer.rs` | |
| Dynamic synthesizers | WASM/process plugins can emit edges | Prefer plugin extractors for project-specific patterns |

## Offload / enrichment

| Hook | Location | Notes |
|------|----------|-------|
| Offload config | `ax-reasoning` `OffloadConfig` | BYO LLM for explore |
| Enrich (planned) | Isolated graph layer + confidence `synthesized` | Local models first |

## Workspace

| Hook | Location | Notes |
|------|----------|-------|
| Config | `ax-core::workspace` — `ax.json` `members` | Alias `.ax.json` |
| Discovery | `discover_members` | Cargo map + nested `.ax/` |
| Sync all | CLI `ax sync --all` | Per-member DB |

## Graph export / analysis

| Hook | Location | Notes |
|------|----------|-------|
| Insights | `ax-graph` Leiden + god-nodes | Already persisted |
| Export | `ax export graph --format …` · `GET /api/graph/export` (Command Center Graph Download) | html (CLI), json, dot, graphml, gexf, cypher, mermaid, plantuml |
