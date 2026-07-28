# ax-desktop-client

Native **Command Center** for ax — an `egui` / `eframe` (wgpu) desktop window that embeds the same `ax-web` HTTP API used by the browser UI.

## What it does

1. Starts `ax_web::serve_with` **in-process** (no separate `ax web` required).
2. Talks to `/api/*` over HTTP JSON and SSE (graph stream, MCP Logging, Ship events).
3. Renders Stats, Nodes, Graph, Files, Search, Memory, Unresolved, Savings, Prices, Ship, Settings, Logging, Policy rules/skills, and a stub Agent sessions view.

## Requirements

- A project with an indexed graph: `<root>/.ax/ax.db` (run `ax index` first).
- GPU / wgpu-capable display (Windows, macOS, or Linux with a working Vulkan/Metal/DX12 stack).

## Run

Via the ax CLI (preferred):

```bash
ax desktop
ax desktop --port 17070
ax desktop . --port 17070 --bind 127.0.0.1
```

Standalone binary from the repo:

```bash
cargo run -p ax-desktop-client -- .
```

Custom port / bind (use a free port if `ax web` already owns 7070):

```bash
cargo run -p ax-desktop-client -- . --port 17070 --bind 127.0.0.1
```

Release binary:

```bash
cargo build --release -p ax-desktop-client
./target-dev/release/ax-desktop . --port 17070
```

On this repo, Cargo’s target directory is `target-dev/` (see `.cargo/config.toml`).

## CLI flags

| Flag | Default | Description |
|------|---------|-------------|
| `[path]` | cwd | Project root (must contain `.ax/ax.db`) |
| `--port` | `7070` | Embedded `ax-web` listen port |
| `--bind` | `127.0.0.1` | Bind address |

## Architecture

```text
ax-desktop (egui/eframe + wgpu)
    │  HTTP JSON + SSE
    ▼
ax-web (axum)  ← embedded in the same process
    │
    ▼
.ax/ax.db · policy · usage · mcp-verbose logs · ship.toml
```

Closing the window stops the embedded server (Drop + `App::on_exit`).

## Parity notes

- **Graph** — force simulation + pan/zoom/drag + `/api/graph/stream`.
- **Logging** — SSE live tail, filters, history chunks, call inspector overlay.
- **Ship** — status/config, SSE pipeline events, evaluate/draft commands.
- **Agent** — session metadata only; interactive PTY remains in the browser Agent page for now.
- Styling aims for readable dark Command Center parity, not pixel-perfect CSS clone.

## Docs

See the [Desktop Client guide](https://getax.wenneker.io/guides/desktop-client/) on the site.
