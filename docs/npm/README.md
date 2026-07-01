# @garywenneker/ax

Native code-intelligence CLI for AI agents — MCP server, call graphs, structural search, and **policy engine** (rules/skills).

**Current ax release:** [v2.0.0](https://github.com/GaryWenneker/ax/releases/tag/v2.0.0) — the npm launcher downloads the matching native binary automatically.

This npm package is a **thin launcher**: it downloads the matching prebuilt `ax` binary from [getax.wenneker.io/releases](https://getax.wenneker.io/releases/) (GitHub Releases as fallback) and runs it. No bundled Node runtime; no JavaScript API.

## Install

```bash
npm install -g @garywenneker/ax
# or one-shot:
npx @garywenneker/ax install
```

## Requirements

- Node.js 18+ (launcher only — used to download and exec the native binary)
- macOS (Intel + Apple Silicon), Linux (x64 + arm64), Windows (x64 + arm64)
- WSL2: use the Linux binary via `install.sh` inside WSL

## What you get

```bash
ax init          # index a project (+ .ax/policy/ scaffold on v2.0.0+)
ax install       # wire MCP into Cursor, Claude Code, etc.
ax explore "…"   # source + call paths for agents
ax policy match "deploy"  # test policy rules/skills (v2.0.0+)
ax web --open    # graph + policy editor in browser
ax serve --mcp   # MCP stdio server (started by your agent)
ax version       # print installed version (e.g. 2.0.0)
```

Full docs: [getax.wenneker.io](https://getax.wenneker.io)

## Alternatives (no npm)

```bash
# macOS / Linux
curl -fsSL https://getax.wenneker.io/install.sh | sh

# Windows
irm https://getax.wenneker.io/install.ps1 | iex

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
