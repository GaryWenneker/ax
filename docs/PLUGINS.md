# Extractor plugins

Out-of-tree extractors live under `.ax/plugins/<name>/` with a `plugin.toml`.
During `ax index` / `ax sync`, matching extensions are handled by the plugin
before the built-in tree-sitter pool.

## Process plugin

```toml
# .ax/plugins/demo/plugin.toml
name = "demo"
extensions = [".demo"]
command = "python"
args = ["extract.py"]
```

JSON on stdin:

```json
{ "path": "rel/path.demo", "content": "..." }
```

Stdout must be an `ExtractionResult` JSON object (`nodes`, `edges`,
`unresolved_references`, …).

See [examples/plugins/echo](../examples/plugins/echo/) for a minimal Python plugin.

## WASM plugin (optional build)

Build ax with `--features plugins-wasm` (pulls `wasmtime`). Manifest:

```toml
name = "hcl"
extensions = [".hcl"]
wasm = "extractor.wasm"
```

Guest exports:

| Export | Signature |
|--------|-----------|
| `memory` | linear memory |
| `alloc` | `(i32) -> i32` |
| `extract` | `(i32, i32) -> i64` (hi=out_ptr, lo=out_len) |

Same JSON contract as process plugins.
