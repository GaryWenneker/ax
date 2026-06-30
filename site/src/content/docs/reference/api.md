---
title: Rust API
description: Embed ax in Rust applications via the ax-core crate.
---

ax is built in Rust. The primary interfaces are the **CLI** and **MCP server**. For programmatic use from Rust, depend on the `ax-core` crate in this repository.

## Cargo dependency

```toml
[dependencies]
ax-core = { git = "https://github.com/GaryWenneker/ax", package = "ax-core" }
tokio = { version = "1", features = ["full"] }
```

When `ax-core` is published to [crates.io](https://crates.io), you can switch to a version pin.

## Basic usage

```rust
use ax_core::Ax;
use ax_extraction::orchestrator::IndexOptions;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut ax = Ax::open(std::path::Path::new("/path/to/project")).await?;

    ax.index_all(IndexOptions::default(), None).await?;

    let results = ax.search("UserService", Default::default()).await?;
    let explore = ax
        .explore("who calls login?", Default::default())
        .await?;

    ax.close().await?;
    Ok(())
}
```

## Key types

| Type / method | Purpose |
|---|---|
| `Ax::init(path)` / `Ax::open(path)` | Create or open a project index |
| `index_all(opts, progress)` | Full index |
| `sync(opts)` | Incremental update |
| `search(query, opts)` | Hybrid symbol search |
| `explore(prompt, opts)` | Source + call paths for agents |
| `get_callers` / `get_callees` | Graph traversal |
| `build_context(task, opts)` | Markdown context for LLMs |

## npm package

`@garywenneker/ax` on npm is a **CLI launcher only** — it downloads the native binary from GitHub Releases. It does not expose a JavaScript/TypeScript library. Use MCP or the Rust crate for programmatic access.

See [MCP Server](/reference/mcp-server/) for agent integration.
