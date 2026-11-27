# Extractor plugins

Out-of-tree extractors live under `.ax/plugins/<name>/` with a `plugin.toml`.
During `ax index` / `ax sync`, matching extensions are handled by the plugin
before the built-in tree-sitter pool.

A plugin runs a command with the user's rights, so a project plugin only runs
after `ax plugins trust` (all plugins, or the named ones). Trust is stored in
`~/.ax/plugin-trust.json` per plugin folder and is bound to a hash of every
file in that folder: `plugin.toml`, the scripts and wasm modules it runs, and
anything else there. After any file in the folder changes (an edit, a
`git pull`, a new or removed file), trust it again. A plugin should therefore
not write files into its own folder; ax sets `PYTHONDONTWRITEBYTECODE=1` so
Python plugins do not. `ax plugins list` shows what each plugin runs and
whether it is trusted.

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

ax writes stdin and reads stdout and stderr concurrently. Each file may take
at most 60 seconds (`timeout_secs` in `plugin.toml` changes this); then the
plugin is killed and the file is reported as failed. Stdout or stderr above
64 MiB is refused.

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

Same JSON contract as process plugins. The module is compiled once per index
run; each file runs in a fresh store with the `timeout_secs` limit (default 60
seconds), at most 512 MiB of guest memory and at most 64 MiB of output.
