---
title: Extractor plugins
description: Out-of-tree process and WASM extractors under .ax/plugins.
---

ax loads custom extractors from `.ax/plugins/<name>/plugin.toml` during `ax index` / `ax sync`. Matching extensions are handled by the plugin **before** the built-in tree-sitter pool.

## Process plugin

```toml
# .ax/plugins/demo/plugin.toml
name = "demo"
extensions = [".demo"]
command = "python"
args = ["extract.py"]
```

JSON on stdin → `ExtractionResult` JSON on stdout:

```bash
# Copy the example plugin into a project
cp -r examples/plugins/echo .ax/plugins/echo
```

```bash
ax sync
```

See the repo example at [`examples/plugins/echo`](https://github.com/GaryWenneker/ax/tree/ax-v4/examples/plugins/echo).

## WASM plugin (optional build)

Build ax with `--features plugins-wasm` (pulls `wasmtime`):

```toml
name = "hcl"
extensions = [".hcl"]
wasm = "extractor.wasm"
```

Guest exports: `memory`, `alloc(i32) -> i32`, `extract(i32, i32) -> i64` (hi = out ptr, lo = out len). Same JSON contract as process plugins.

## Further reading

- Repo docs: [`docs/PLUGINS.md`](https://github.com/GaryWenneker/ax/blob/ax-v4/docs/PLUGINS.md)
- [CLI reference](/reference/cli/)
