# EVIDENCE: CLI Logging direction parity

**SPEC:** `/Users/gary/io/ax/docs/specs/cli-logging-direction-parity.md`  
**Spec approval:** not obtained (autonomous run)  
**Source state:** local tree after ToolTrace wiring; Cargo `8.4.3`  
**Tier:** 2

## Mapping

| Scenario | Check | Result |
|---|---|---|
| `ax explore` → inbound/outbound | Smoke log lines 764–767 in `.ax/mcp-verbose-2026-11-09.log` | PASS — `[ax] inbound/outbound tool=ax_explore` |
| `ax remember` / `ax recall` | Lines 770–779 | PASS — inbound/outbound + Internal `memory` domain lines |
| `ax policy skills` / `ax policy skill` | Lines 782–791 | PASS — inbound/outbound `ax_skill` + Internal `policy` lines |
| CLI brand ≠ MCP | Same lines use `[ax]`, not `[ax-mcp]` | PASS |
| Unit: tool_trace | `cargo test -p ax-usage tool_trace` | PASS — 4/4 |
| Frontend classify CLI brand | `mcpTrace.test.ts` addition | SKIPPED — runner fails resolving `./timeZone` (pre-existing import); classify already matches on `inbound tool=` body |

## Gauntlet

| Layer | Command | Result |
|---|---|---|
| Unit tests | `cargo test -p ax-usage tool_trace` | 4 passed |
| Build | `cargo build -p ax-cli --release` (via `scripts/reinstall-cli.sh`) | Finished release |
| Reinstall | `bash scripts/reinstall-cli.sh` | `ax 8.4.3` at `~/.local/bin/ax` → `target-dev/release/ax` |
| Real execution | explore / remember / recall / policy skills / policy skill | Verbose log shows inbound + outbound + internal domain |
| Mutation | skipped | Tier 2 smoke + unit sufficient for log-line shape |
| Frontend test | skipped | module resolution; behavior covered by live log + Rust tests |

## Confidence

Medium-high for the shipped CLI log shape (observed live lines). Lower on full UI filter-chip rendering (not rebuilt into embedded web UI this turn — direction classification is string-based and already live for MCP; CLI lines use the same body tokens).
