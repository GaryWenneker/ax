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

See the repo example at [`examples/plugins/echo`](https://github.com/GaryWenneker/ax/tree/main/examples/plugins/echo).

## WASM plugin (optional build)

Build ax with `--features plugins-wasm` (pulls `wasmtime`):

```toml
name = "hcl"
extensions = [".hcl"]
wasm = "extractor.wasm"
```

Guest exports: `memory`, `alloc(i32) -> i32`, `extract(i32, i32) -> i64` (hi = out ptr, lo = out len). Same JSON contract as process plugins.

## Visibility

- `GET /api/plugins/` lists loaded manifests (name, extensions, mode, command/wasm)
- Command Center **Settings → Plugins** shows the live table (and empty-state hint)
- With Verbose MCP logging, index/sync writes `plugin extract name=… ok|fail` lines (filter Logging → plugin)

## Further reading

- Repo docs: [`docs/PLUGINS.md`](https://github.com/GaryWenneker/ax/blob/main/docs/PLUGINS.md)
- [MCP Logging & Quality](/guides/mcp-quality/)
- [CLI reference](/reference/cli/)
