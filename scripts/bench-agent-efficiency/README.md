# Agent efficiency benchmark (WITH vs WITHOUT ax)

Reproduce CodeGraph-style token / tool-call savings for marketing and regression.

## Method

1. Pick OSS repos (clone shallow).
2. Index each with `ax init` / `ax index`.
3. Run the same architecture question headlessly with Claude Code (or Cursor agent) twice:
   - **WITH**: MCP config pointing at `ax serve --mcp`
   - **WITHOUT**: empty MCP config
4. Record median of ≥4 runs: wall-clock, tool calls, file reads, total tokens, cost.

## Suggested repos / queries

| Repo | Query |
|------|-------|
| tokio-rs/tokio | How does tokio schedule and run async tasks on its runtime? |
| excalidraw/excalidraw | How does Excalidraw render and update canvas elements? |
| gin-gonic/gin | How does gin route requests through its middleware chain? |

## Local harness (no API key)

Time the WITH-graph arm on the current project:

```powershell
.\scripts\bench-agent-efficiency\Run-LocalExploreBench.ps1 -Runs 3 -Out results.md
```

## Collecting numbers (full WITH/WITHOUT)

Prefer Claude Code headless:

```bash
claude -p --strict-mcp-config "$QUERY"
```

Parse transcript / usage for tool-call count and tokens. Optionally correlate with `ax savings` after WITH runs (MCP calls are logged in `~/.ax/usage.db`).

## Output

Publish a Markdown table on the site (tokens, tool calls, file reads, cost) — same shape as the competitive WITH/WITHOUT narrative.
