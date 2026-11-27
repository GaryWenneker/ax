# Extractor plugins

Out-of-tree extractors live under `.ax/plugins/<name>/` with a `plugin.toml`.
During `ax index` / `ax sync`, matching extensions are handled by the plugin
before the built-in tree-sitter pool.

A plugin runs a command with the user's rights, so a project plugin only runs
after `ax plugins trust` (all plugins, or the named ones). Trust is stored in
`~/.ax/plugin-trust.json` per plugin folder and is bound to the exact
`plugin.toml`; after editing it, trust it again. `ax plugins list` shows what
each plugin runs and whether it is trusted.

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
