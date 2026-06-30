# @garywenneker/ax

Native code-intelligence CLI for AI agents — MCP server, call graphs, and structural search.

This npm package is a **thin launcher**: it downloads the matching prebuilt `ax` binary from [GitHub Releases](https://github.com/GaryWenneker/ax/releases) and runs it. No bundled Node runtime; no JavaScript API.

## Install

```bash
npm install -g @garywenneker/ax
# or one-shot:
npx @garywenneker/ax install
```

## Requirements

- Node.js 18+ (launcher only — used to download and exec the native binary)
- macOS, Linux, or Windows (x64 or arm64)

## What you get

```bash
ax init          # index a project
ax install       # wire MCP into Cursor, Claude Code, etc.
ax explore "…"   # source + call paths for agents
ax serve --mcp   # MCP stdio server (started by your agent)
```

Full docs: [getax.wenneker.io](https://getax.wenneker.io)

## Alternatives (no npm)

```bash
# macOS / Linux
curl -fsSL https://raw.githubusercontent.com/GaryWenneker/ax/main/install.sh | sh

# Windows
irm https://raw.githubusercontent.com/GaryWenneker/ax/main/install.ps1 | iex

# From source
cargo install --git https://github.com/GaryWenneker/ax ax-cli --force
```

## Environment

| Variable | Purpose |
|---|---|
| `AX_INSTALL_DIR` | Cache dir for downloaded binaries (default: `~/.ax` on Unix, `%LOCALAPPDATA%\ax` on Windows) |
| `AX_GITHUB_REPO` | Override release repo (default: `GaryWenneker/ax`) |
| `AX_VERSION` | Pin release tag (default: latest) |
| `AX_NO_DOWNLOAD` | Disable GitHub download fallback |

## License

MIT — see [GaryWenneker/ax](https://github.com/GaryWenneker/ax).
