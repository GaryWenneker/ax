---
title: Extractor plugins
description: Out-of-tree process and WASM extractors under .ax/plugins.
banner:
  content: |
    The Command Center is being phased out. <a href="/guides/phase-out/">ax-admin becomes the only visual interface</a>.
---

ax loads custom extractors from `.ax/plugins/<name>/plugin.toml` during `ax index` / `ax sync`. Matching extensions are handled by the plugin **before** the built-in tree-sitter pool.

## Trust

A plugin runs a command with your user rights, and `.ax/plugins` can arrive with any cloned repository. So ax does not run a project plugin until you trust it:

```bash
ax plugins list            # what each plugin runs, and whether it is trusted
ax plugins trust           # trust every plugin in this project
ax plugins trust demo      # or only the named ones
ax plugins untrust demo
```

Until then indexing skips the plugin (and logs a warning), and its extensions go to the built-in extractors. Trust is kept in `~/.ax/plugin-trust.json` per plugin folder and is bound to a hash of every file in that folder: `plugin.toml`, the script or wasm module it runs, and anything else there. When any of them changes, for example a new `command`, an edited `extract.py` after a `git pull`, or a new file, the plugin stops running until you trust it again. So a plugin should not write files into its own folder; ax sets `PYTHONDONTWRITEBYTECODE=1` so Python plugins do not.

## Process plugin

```toml
# .ax/plugins/demo/plugin.toml
name = "demo"
extensions = [".demo"]
command = "python"
args = ["extract.py"]
```

JSON on stdin → `ExtractionResult` JSON on stdout. ax writes the input and reads stdout and stderr at the same time, so a plugin may log or answer before it has read all of its input. Each file may take at most 60 seconds; after that the plugin is stopped and the file is reported as failed. Set `timeout_secs = 300` in `plugin.toml` to allow longer. Output above 64 MiB on stdout or stderr is refused.

```bash
# Copy the example plugin into a project
cp -r examples/plugins/echo .ax/plugins/echo
```

```bash
ax plugins trust echo
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

Guest exports: `memory`, `alloc(i32) -> i32`, `extract(i32, i32) -> i64` (hi = out ptr, lo = out len). Same JSON contract as process plugins. The module is compiled once per index run. Each file runs with the same `timeout_secs` limit (default 60 seconds), at most 512 MiB of guest memory, and at most 64 MiB of output.

## Visibility

- `GET /api/plugins/` lists every plugin found (name, extensions, mode, command/wasm, `trusted`)
- Command Center **Settings → Plugins** shows the live table (and empty-state hint)
- With Verbose MCP logging, index/sync writes `plugin extract name=… ok|fail` lines (filter Logging → plugin)

## Further reading

- Repo docs: [`docs/PLUGINS.md`](https://github.com/GaryWenneker/ax/blob/main/docs/PLUGINS.md)
- [MCP Logging & Quality](/guides/mcp-quality/)
- [CLI reference](/reference/cli/)
