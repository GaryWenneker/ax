---
title: Headless ax and ax-admin
description: The full ax and its built-in Command Center are being phased out. The headless build becomes ax on every platform, and ax-admin becomes the only visual interface.
---

Ax is moving to two programs:

- **ax**, the headless build: the CLI and the MCP server, without a built-in interface.
- **ax-admin**, a separate application and the only visual interface.

The full ax is being phased out. That is the installation from `install.sh` and `install.ps1`, with the built-in Command Center (`ax web`), the desktop client (`ax desktop`) and `ax share`.

No date is set. This page describes the direction and what is released today.

## What changes

| Today | Where it is going |
|---|---|
| Full ax for Windows, macOS and Linux, with the Command Center built in | Phased out |
| Headless build for Linux only (`install-slim.sh`, `ax-slim` images) | The one ax for every platform: Linux, macOS and Windows, on x64 and arm64, and in containers, sandboxes and CI |
| Command Center (`ax web`), desktop client (`ax desktop`), `ax share` | Phased out |
| `ax-admin` as a separate application for a remote Ax | The only visual interface |

## What is released today

- Every release still ships the full ax for six platforms. It keeps working, and its documentation stays on this site while releases ship it.
- From v10.1.1, releases ship the headless build for Linux x86_64 and arm64, as static binaries and as container images. See [Headless Linux build](/guides/headless-linux/). There is no released headless build for macOS or Windows yet.
- From v10.1.2, releases ship `ax-admin` for Windows, macOS and Linux, on x64 and arm64. See [ax-admin](/guides/remote-admin/).

On Linux you can use the new combination today: the headless build as the server and `ax-admin` on your own computer.

## What stays the same

The headless build keeps the CLI, MCP over stdio (`ax serve --mcp`), `ax mcp remote serve`, indexing, graph queries, policy, specs, sessions and the memory vault. Agents keep using the same MCP tools. Project data stays in each project's `.ax/` directory.

## From the Command Center to ax-admin

`ax-admin` connects to an Ax through its [remote MCP](/guides/remote-mcp/) address. A page appears when the connected server offers the tools for it.

| Command Center page | ax-admin page |
|---|---|
| Stats | Status |
| Search | Search |
| Nodes | Nodes |
| Graph: communities and most connected symbols | Insights |
| Files | Files |
| Memory | Memory |
| Logging | Logging |
| Savings | Usage |
| Rules: browse, and test which rules match a prompt | Rules |
| Skills: browse | Skills |
| Sync and re-index | Operations |

`ax-admin` also has Explore, Sessions and a Tools page that runs every tool the server offers.

Not in `ax-admin` today:

- the graph canvas
- the rule and skill editors
- reading and editing specs
- settings of the server
- policy review and policy sync
- model prices, the unresolved references list and the SonarQube page
- running ship, LSP or a shell command on the server

See [Limits](/guides/remote-admin/#limits) in the `ax-admin` guide for the current list.

## Documentation

This documentation is written for headless ax and `ax-admin`: each guide leads with the CLI command or MCP tool and names the `ax-admin` page where there is one.

The pages under **Full ax (phasing out)** in the sidebar describe what only the full ax has: the Command Center, the desktop client, `ax share`, the Cloudflare tunnel for the Command Center, the Obsidian vault and the standalone server with replicas. They carry a notice at the top and stay available while releases ship the full ax.
