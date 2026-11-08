# SPEC: CLI ↔ MCP functional parity (shared AX runtime)

**Path:** `/Users/gary/io/ax/docs/specs/cli-mcp-parity/SPEC.md`  
**Tier:** 3 (public CLI/MCP contract, persistence, Command Center visibility)  
**Source state baseline:** `main` @ `6a3996f` (local HEAD at task start); Cargo version `8.3.8`  
**Spec approval:** not obtained as a separate human review turn — the task brief explicitly requires end-to-end implementation (“Voer deze opdracht van begin tot eind uit”). EVIDENCE will record `spec approval: not obtained (autonomous run)` and claim correspondingly lower confidence on spec completeness.

## Goal

Make AX fully usable via CLI only (no MCP transport, plugin, daemon, IDE hooks, or network listener required), with the same shared engine MCP uses for preflight, sessions, durable context, memory, history, activity, and cache. MCP keeps working unchanged. Command Center shows both sources with real provenance.

## Non-goals / hard constraints

- Do not fake CLI traffic as MCP traffic.
- Do not remove or relocate `.ax` / `ax.db`.
- Do not require hooks for the happy path.
- Do not invent token or cost numbers.
- Do not install MCP config in `--mode cli-only`.
- Existing MCP verbose logs and tools remain available.

## Architecture

```text
AI agents → Rules/Skills → AX CLI ─┐
                                   ├→ SHARED RUNTIME (preflight, session, durable,
AX MCP (optional) ─────────────────┘   turn, activity, memory hooks, cache)
                                         → usage.db / ax.db → Command Center
```

- New modules live primarily in `ax-usage` (persistence + activity + turn orchestration) and thin CLI adapters in `ax-cli`.
- Preflight body is extracted from `ax-mcp` tool handlers into a callable shared function used by MCP and CLI.
- Activity events use a versioned schema in `usage.db` (`ax_activity_event`), dual-written to the verbose log with `source=cli|mcp|hook|agent`.

## Isolation / setup plan

- Work on branch `feat/cli-mcp-parity` in the existing tree (worktree avoided: ignored build artifacts and `.ax` are required for local gauntlet).
- Checkpoint commits at SPEC, then per P0 slice.
- No new third-party crates unless justified below.
- New deps: none planned (reuse `sqlx`, `serde_json`, `uuid`, `chrono` already in workspace).

## Parity matrix

See `/Users/gary/io/ax/docs/specs/cli-mcp-parity/PARITY-MATRIX.md` (authoritative inventory).

## Acceptance scenarios (executable)

### A — Code-change turn (CLI-only)

```gherkin
Given a temp project with ax init and no MCP server
When the agent runs: ax session start --json
And ax turn begin --session <id> --prompt-stdin --json
And ax explore "auth" (or ax query)
And a file is modified
And ax turn finish --session <id> --outcome-stdin --json
Then usage.db has activity events with source=cli
And ax.db memories include a turn memory for that session
And history lists the turn with files/commits when present
And Command Center Logging can classify the events as CLI
```

### B — Research turn without file changes (required)

```gherkin
Given a CLI session and turn begin
When the agent explores architecture and records a decision via turn event
And finishes the turn with an outcome and no file diffs
Then a turn memory / history entry exists
And no false file-change list is invented
And the conclusion is recallable
```

### C — Memory reuse

```gherkin
Given a prior turn that stored a semantic decision
When a new session runs ax preflight --prompt-stdin
Then the inject / JSON includes the relevant memory title or recall hint
```

### D — Parallel sessions

```gherkin
Given two session ids in the same project
When both append working-context facts
Then each session's ax session get returns only its own facts
```

### E — Crash recovery

```gherkin
Given ax turn begin without finish
When the process ends
Then ax turn status / doctor reports an incomplete turn
And working context remains
And no successful completion memory is written
```

### F — MCP/CLI preflight parity

```gherkin
Given identical prompt, files, session, known_context in a fixture project
When preflight runs via MCP ToolHandler and via ax preflight --json
Then matched rule ids, skill ids, and memory titles match
And transport metadata may differ
```

### G — Restricted Windows (PowerShell script)

```gherkin
Given only the ax binary and agent instructions
When the PowerShell sample script runs session→turn→explore→finish
Then DB rows exist and no listener/daemon is required
```

### H — Command Center

```gherkin
Given CLI activity events in the verbose log / activity store
When Logging is opened with source filter CLI
Then CLI events appear and MCP events remain filterable separately
```

## CLI surface (contract)

| Command | Behavior |
|---|---|
| `ax preflight` | Shared preflight; `--prompt-stdin`, `--session`, `--file`, `--path`, `--context-mode`, `--known-context`, `--context-reset`, `--json` |
| `ax session start\|get\|add\|update\|compact\|fork\|handoff\|resume\|end` | Wraps `working_context_*` / durable scope; JSON |
| `ax durable …` | Wraps `durable_project_apply` actions |
| `ax turn begin\|event\|checkpoint\|finish\|fail\|status` | Explicit turn lifecycle; works without file changes |
| `ax cache stash\|expand\|status` | Context cache / reuse |
| `ax doctor --mode cli --json` | CLI-only health |
| `ax install --mode cli-only --target …` | Rules/skills/instructions; no MCP/plugin/hooks by default |

## Invariants (must not break)

1. Existing MCP tools and schemas keep working.
2. `TURN_RETENTION_DAYS` remains 90 unless settings override is added (configurable, default 90).
3. Verbose MCP log format remains parseable; CLI lines use domain `cli` / activity tags.
4. `ax turn-hook` remains for backward compatibility.
5. No fabricated savings/cost figures (`provenance: unknown|measured|estimated`).

## Failure model (Tier 3)

| Failure | Mitigation |
|---|---|
| Double `turn finish` | Idempotency key on turn id; second finish returns prior receipt |
| Concurrent CLI writers | SQLite WAL + idempotency unique constraint |
| Activity write fails | Command still returns error if primary op fails; activity failure is logged and surfaced in receipt when secondary |
| Secret leakage in logs | Redact known secret patterns; metadata-only privacy mode |
| Cross-project bleed | `project_scope(root)` on all session/durable/activity rows |
| Agent skips finish | Doctor/incomplete-turn visibility; no fake success |

## Gauntlet plan

- Unit/integration tests in `ax-usage`, `ax-cli`, `ax-mcp` for parity and turn lifecycle.
- Manual mutation of finish idempotency and session isolation (3–5 mutants).
- `cargo test -p ax-usage -p ax-cli -p ax-mcp` focused suites; full workspace where feasible.
- PowerShell sample under `docs/specs/cli-mcp-parity/examples/`.
- Web-ui unit tests for Logging source filters if UI changes.
- Changed-line coverage on new modules via targeted tests (fail closed).

## Files expected (initial)

- `docs/specs/cli-mcp-parity/SPEC.md` (this file)
- `docs/specs/cli-mcp-parity/PARITY-MATRIX.md`
- `crates/ax-usage/src/activity.rs`
- `crates/ax-usage/src/turn_lifecycle.rs`
- `crates/ax-mcp/src/preflight.rs` (extracted)
- `crates/ax-cli/src/commands/{preflight,session_cmd,durable_cmd,turn_cmd,cache_cmd,doctor}.rs`
- Installer + `.agents` / `.github` / `.cursor` CLI-only assets
- Integration tests + EVIDENCE.md

## Out of scope for v1 completeness claim

- Observing arbitrary non-AX Copilot tool calls without agent-reported events.
- Guaranteeing agents always obey Rules/Skills (instructions ≠ runtime hooks).
