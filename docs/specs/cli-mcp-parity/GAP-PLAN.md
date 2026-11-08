# Gap plan revision (after P0 delivery)

**Date:** 2026-11-08  
**Branch:** `feat/cli-mcp-parity`  
**Parent SPEC:** `SPEC.md` (append-only revision — does not replace acceptance criteria)

## Verdict

The P0 CLI surface is **real and usable**: preflight, session, durable (including fork/handoff/doc put/get), turn, cache, doctor, cli-only install, activity store, research-turn memory, Logging kinds.

**Must-do gap pass: complete.** Follow-up polish (MCP contract, README, guard docs, Memory CLI stats): complete. Remaining items below are explicitly deferred.

## Status vs original acceptance table

| Criterion | Status | Notes |
|---|---|---|
| CLI-only (no MCP/plugin/hook/daemon) | **Met** for happy path | Install `--mode cli-only`; turn lifecycle without hooks |
| Preflight same engine | **Met (interim)** | CLI uses in-process `call_tool`; extract to `preflight.rs` still polish |
| Sessions persist/resume | **Met** | `ax session *`; parallel isolation covered by `cli_parity_gaps` |
| Turns without file changes | **Met** | Integration test green |
| Activity auto for AX CLI | **Met** | Central `record_cli_command` after non-hook commands + command-local records |
| Memory auto on finish | **Met** | `source=cli-turn`; Memory UI shows **cli** badge + cli-turns count |
| History linked | **Met** | Turn memories in Memory vault; Logging inspector **Open memory** → `/memory?memory=` |
| Durable via CLI | **Met** | append/read/search/compact/fork/handoff/doc-put/doc-get/tasks |
| Cache CLI | **Met** | stash/expand/status |
| Logging CC | **Met** | CLI/Hook/Agent kinds + Open memory deep-link for `turn.completed` |
| Savings | **Partial** | No fabricated numbers; CLI savings parity not expanded |
| Security / no forbidden integrations | **Met** for cli-only install |
| Windows PowerShell | **Script only** | Not executed on Windows host |
| MCP regression | **Met** | `new_tools_smoke` 2 + `preflight_standard_contract` 4 |
| Docs | **Met** | `docs/CLI_ONLY.md`, site CLI reference, README CLI-only row, `ax_guard` mapping |

## Must-do next (gap pass) — DONE

1. ~~Wire `ax durable fork|handoff|doc-put|doc-get`~~
2. ~~Central CLI activity instrumentation~~
3. ~~Scenario D~~ parallel sessions
4. ~~Scenario E~~ incomplete turn
5. ~~Scenario F~~ MCP vs CLI preflight
6. ~~Refresh PARITY-MATRIX + EVIDENCE~~

## Follow-up polish — DONE

1. ~~`preflight_standard_contract`~~ — 4 passed
2. ~~README CLI-only row + install comment~~
3. ~~`ax_guard` mapping documented under CLI sessions section~~
4. ~~Memory vault cli-turns count~~

## Follow-up — Logging → Memory (this pass)

1. `turn.completed` activity summary embeds `memory=<id>` (parsed by Logging).
2. Route `/memory?memory=<id>` selects the vault row.
3. Logging inspector shows **Open memory** when `memoryId` is present.

## Defer (explicit)

- Full rename of `McpEngine` → shared runtime crate.
- Windows host execution of `.ps1`.
- Full workspace cargo test / mutation gauntlet.
- Optional native IDE hooks mode (must stay off by default).
- CLI savings counterfactual expansion (no invented numbers).

## Non-goals unchanged

Do not fake CLI as MCP. Do not move `.ax`. Do not invent token/cost numbers.
