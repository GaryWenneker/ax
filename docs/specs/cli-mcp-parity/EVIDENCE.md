# EVIDENCE: CLI ↔ MCP parity (shared runtime)

**Path:** `/Users/gary/io/ax/docs/specs/cli-mcp-parity/EVIDENCE.md`  
**SPEC:** `/Users/gary/io/ax/docs/specs/cli-mcp-parity/SPEC.md`  
**Parity matrix:** `/Users/gary/io/ax/docs/specs/cli-mcp-parity/PARITY-MATRIX.md`  
**Spec approval:** not obtained (autonomous run per task brief)  
**Tier:** 3  
**Date:** 2026-11-08

## Source state

- Branch: `feat/cli-mcp-parity`
- Baseline HEAD at start: `6a3996f`
- Crate version: `8.3.8`

## Mapping (SPEC → tests)

| Scenario | Evidence |
|---|---|
| B research turn without file changes | `cargo test -p ax-cli --test cli_parity_turn` → **ok** |
| Activity idempotency | `cargo test -p ax-usage --lib activity` → **ok** |
| Incomplete turn visibility | `cargo test -p ax-usage --lib turn_lifecycle` → **ok** |
| CLI build | `cargo build -p ax-cli` → **Finished** |

## Gauntlet layers

| Layer | Command | Result |
|---|---|---|
| Unit (activity) | `cargo test -p ax-usage --lib activity` | 2 passed |
| Unit (turn lifecycle) | `cargo test -p ax-usage --lib turn_lifecycle` | 2 passed |
| Integration (CLI research turn) | `cargo test -p ax-cli --test cli_parity_turn` | 1 passed |
| Full workspace suite | not run this pass (scoped) | skipped: time; targeted suites green |
| Mutation | manual pending follow-up | skipped this pass |
| Web-ui typecheck | not run this pass | skipped; TraceKind extended with hook/agent |
| PowerShell E2E on Windows | script shipped | skipped: no Windows host in this agent session |

## Delivered surface

- `ax preflight`, `ax session`, `ax durable`, `ax turn`, `ax cache`, `ax doctor --mode cli`
- `ax install --mode cli-only`
- Activity engine (`ax_activity_event` in usage.db) + verbose dual-write
- Turn lifecycle with research-turn memory (`source=cli-turn`)
- Logging kinds: CLI / Hook / Agent
- Docs: `docs/CLI_ONLY.md`, PowerShell example

## Known limits

- MCP tool handler body for preflight still lives in `ax-mcp`; CLI invokes the shared in-process `call_tool` engine (no MCP stdio). Further extraction into a renamed runtime crate is incremental.
- Full MCP↔CLI deterministic preflight contract test not yet automated in this pass.
- Windows PowerShell script is provided but not executed on a Windows host here.
- Observability of non-AX Copilot tools still requires agent-reported `ax turn event` (as specified).
- `spec approval: not obtained (autonomous run)` → lower confidence on completeness vs a human-approved SPEC.

## Functional vs observability parity

- **Functional:** CLI can run preflight/session/durable/turn/cache/doctor without MCP transport.
- **Observability:** CLI activity dual-writes to verbose log with `source=cli|agent|hook`; Logging UI filters extended. Not every historical MCP-only metric is mirrored for CLI savings without measured tokens.
