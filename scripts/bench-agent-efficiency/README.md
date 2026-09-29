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

## Savings gauntlet

Unattended comparison of the same tasks without ax, then with ax. Tasks cover the source graph, rules, skills, and memory, from a single symbol lookup through a multi-tool turn. Recipes are fixed in `tasks.yaml`.

```bash
./scripts/bench-agent-efficiency/run-savings-gauntlet.sh
```

The script writes `out/summary.md`, `out/report.md`, and `out/report.json`. With-arms call the ax MCP server over stdio and count `content[0].text`, which is what an agent receives. Deterministic token counts use o200k BPE on the tool output. The report uses a signed net (without minus with) and a previous-net column. The exit code is 1 when a task misses its anchors, a task or feature nets below 50%, the negative control saves more than 5%, or a `without:` block no longer matches `without_hashes.json`. The `savings-gauntlet` skill ("rerun the savings gauntlet") runs it, shows the tables, and proposes an improvement plan before any fix. Set `AX_GAUNTLET_LIVE=0` to skip the Claude Code arm. When `claude` is missing, that arm is recorded as skipped. `AX_GAUNTLET_LIVE_RUNS` defaults to 3 (median).

## Output

Publish a Markdown table on the site (tokens, tool calls, file reads, cost) — same shape as the competitive WITH/WITHOUT narrative. The gauntlet report is the local evidence; do not copy it to the site until the numbers are from a finished run.
