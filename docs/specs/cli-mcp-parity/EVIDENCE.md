# EVIDENCE: CLI ↔ MCP parity (shared runtime)

**Path:** `/Users/gary/io/ax/docs/specs/cli-mcp-parity/EVIDENCE.md`  
**SPEC:** `/Users/gary/io/ax/docs/specs/cli-mcp-parity/SPEC.md`  
**Parity matrix:** `/Users/gary/io/ax/docs/specs/cli-mcp-parity/PARITY-MATRIX.md`  
**Gap plan:** `/Users/gary/io/ax/docs/specs/cli-mcp-parity/GAP-PLAN.md`  
**Spec approval:** not obtained (autonomous run per task brief)  
**Tier:** 3  
**Date:** 2026-11-08 (gap pass refresh)

## Source state

- Branch: `feat/cli-mcp-parity`
- Baseline HEAD at feature start: `6a3996f`
- P0 commit: `8e90e5b9`
- Crate version: `8.3.8`

## Mapping (SPEC → tests)

| Scenario | Evidence |
|---|---|
| B research turn without file changes | `cargo test -p ax-cli --test cli_parity_turn` → **ok** (1 passed) |
| Activity idempotency | `cargo test -p ax-usage --lib activity` → **ok** (2 passed) |
| Incomplete turn visibility (unit) | `cargo test -p ax-usage --lib turn_lifecycle` → **ok** (2 passed) |
| D parallel sessions isolation | `cargo test -p ax-cli --test cli_parity_gaps` → **ok** |
| E unfinished turn after restart | same suite → **ok** |
| F CLI/MCP preflight policy match | same suite → **ok** |
| CLI build | `cargo build -p ax-cli` → **Finished** |

## Gauntlet layers (fresh after gap pass)

| Layer | Command | Result |
|---|---|---|
| Unit (activity) | `cargo test -p ax-usage --lib activity` | 2 passed |
| Unit (turn lifecycle) | `cargo test -p ax-usage --lib turn_lifecycle` | 2 passed |
| Integration (CLI research turn) | `cargo test -p ax-cli --test cli_parity_turn` | 1 passed |
| Integration (gap scenarios D/E/F) | `cargo test -p ax-cli --test cli_parity_gaps -- --test-threads=1` | 3 passed |
| Full workspace suite | not run this pass (scoped) | skipped: time; targeted suites green |
| Mutation | manual pending follow-up | skipped this pass |
| Web-ui typecheck | `npx tsc -p tsconfig.json --noEmit` in `crates/ax-web/web-ui` | exit 0 (Memory `cli-turn` badge) |
| Web-ui production build | `npm run build` in `crates/ax-web/web-ui` | exit 0 (`tsc && vite build`) |
| PowerShell E2E on Windows | script shipped | skipped: no Windows host in this agent session |
| MCP smoke | `cargo test -p ax-mcp --test new_tools_smoke` | 2 passed |
| MCP preflight contract | `cargo test -p ax-mcp --test preflight_standard_contract` | 4 passed |

## Review rounds

| Round | Skills (status) | Findings | Fixed |
|---|---|---|---|
| 1 | `old-coder` usable, `review-loop` usable, `rust-review` usable, `react-review` usable | 1 minor (R1-1: discarded activity `Result` without explicit best-effort note) | R1-1 comment on `record_cli_command` discard in `main.rs` |
| 2 | same | 0 | — |
| 3 (polish) | `review-loop` usable; docs/README/Memory stats only | 0 blockers (Tier-1 docs + presentational Memory count) | — |
| 4 (Logging→Memory) | `react-review` usable; routes + loggingPresentation tests | 0 open | — |

### Logging → Memory (follow-up)

| Check | Result |
|---|---|
| `npx tsx --test src/lib/loggingPresentation.test.ts` | 15 passed |
| `node --experimental-strip-types --test src/lib/routes.test.ts` | 3 passed (incl. memory deep-link) |
| `cargo test -p ax-usage --lib turn_lifecycle` | 2 passed |
| `npm run build` (web-ui) | exit 0 |

## Delivered surface

- `ax preflight`, `ax session`, `ax durable` (incl. fork/handoff/doc-put/doc-get), `ax turn`, `ax cache`, `ax doctor --mode cli`
- `ax install --mode cli-only`
- Activity engine (`ax_activity_event` in usage.db) + central `record_cli_command` + verbose dual-write
- Turn lifecycle with research-turn memory (`source=cli-turn`)
- Logging kinds: CLI / Hook / Agent
- Docs: `docs/CLI_ONLY.md`, PowerShell example, site CLI reference (gap pass)

## Known limits

- MCP tool handler body for preflight still lives in `ax-mcp`; CLI invokes the shared in-process `call_tool` engine (no MCP stdio). Further extraction into a renamed runtime crate is incremental.
- Windows PowerShell script is provided but not executed on a Windows host here.
- Observability of non-AX Copilot tools still requires agent-reported `ax turn event` (as specified).
- Deep History UI navigation from CLI activity rows is deferred (GAP-PLAN).
- `spec approval: not obtained (autonomous run)` → lower confidence on completeness vs a human-approved SPEC.

## Functional vs observability parity

- **Functional:** CLI can run preflight/session/durable/turn/cache/doctor without MCP transport.
- **Observability:** CLI activity dual-writes to verbose log with `source=cli|agent|hook`; Logging UI filters extended; Memory shows a CLI badge for `cli-turn` sources. Not every historical MCP-only metric is mirrored for CLI savings without measured tokens.
