# EVIDENCE: CLI↔MCP parity rules/skills for every IDE

**Tier:** 2  
**Spec approval:** not obtained (autonomous run — user MUST directive)  
**Source state:** branch `feat/cli-ide-parity` (uncommitted at report time)  
**Date:** 2026-11-09

## Spec → test map

| Scenario | Test / check |
|---|---|
| A — all IDE surfaces + no MCP configs | `cli_only::tests::default_cli_only_covers_all_ide_instruction_surfaces` |
| B — windsurf without cursor | `cli_only::tests::windsurf_target_installs_without_cursor_rule` |
| C — zed shared AGENTS | `cli_only::tests::zed_uses_shared_agents_only` |
| D — dual-path bootstrap templates | `ide_seed::tests::every_bootstrap_names_cli_fallback` |
| E — prefer-mcp-ops / explore CLI rows | same + templates include CLI tables |
| F — MCP seed still writes Continue MCP | `ide_seed::tests::seeds_continue_mcp_server` + `sync_ide_bootstrap_fix_creates_all_targets` |
| G — install smoke | covered by A (library `install_cli_only`) |
| antigravity → cline | `cli_only::tests::antigravity_aliases_to_cline_rules` |
| continue never MCP | `cli_only::tests::continue_cli_only_never_writes_mcp_server` |
| AGENTS upsert idempotent | `cli_only::tests::upsert_agents_md_is_idempotent` |

## Gauntlet

| Layer | Command | Result |
|---|---|---|
| Tests (installer) | `cargo test -p ax-installer --lib` | **77 passed** |
| Tests (ide_seed) | `cargo test -p ax-policy --lib ide_seed` | **18 passed** |
| Dual-path filter | `cargo test -p ax-policy --lib every_bootstrap` | **2 passed** |
| Clippy | `cargo clippy -p ax-installer -p ax-policy --tests -- -D warnings` | **clean** |
| Changed-line coverage | not instrumented for Rust in this env | skipped: no diff-cover toolchain wired |
| Mutation | manual: drop `continue` from `DEFAULT_CLI_ONLY_TARGETS` | **killed** by `default_cli_only_covers_all…` (`missing .continue/rules/ax-cli.md`); restored |
| Real execution | library install in tempdir via scenario A | **pass** (all expected paths + MCP absences) |
| Types | rustc via cargo test | **pass** |
| Supply chain | no new deps | n/a |

## What changed

- Expanded `install_cli_only` defaults to vscode, cursor, windsurf, continue, claude, gemini, cline + shared; shared-only path for zed/jetbrains/kiro/codex/hermes/opencode; antigravity → cline.
- New CLI-only templates for windsurf/continue/claude/gemini/cline.
- Dual-path CLI fallback in IDE bootstrap templates + `AGENTS`/`CLAUDE` instruction blocks.
- `prefer-mcp-ops`, `explore-before-grep`, `degraded-continue` teach CLI graph/ops when MCP is down (seedVersion bumped where present).
- Docs: `docs/CLI_ONLY.md` per-IDE matrix.
- Durable rule saved: `cli-mcp-parity-ide-rules`.

## Known limits

- Live project `AGENTS.md` / `.cursor/rules/ax.mdc` are not auto-rewritten until `ax sync` / IDE bootstrap fix runs in a project.
- Review skills (rust-review, etc.) unchanged — out of scope.
- Spec human approval was not obtained; confidence is lower than a reviewed SPEC run.

## Recheck — all rules/skills (2026-11-09)

### CLI command matrix (20/20 help OK)

`preflight`, `explore`, `query`, `node`, `session`, `turn`, `sync`, `status`, `remember`, `recall`, `ship`, `lsp`, `policy index`, `policy guard`, `policy capture`, `policy skill`, `cache expand`, `callers`, `impact`, `insights`

### Workflow policy dual-path (fixed this pass)

| Surface | Status |
|---|---|
| startup skill | CLI SS-00 + SS-02c + SS-04 |
| subagents rule + skill | CLI turn order + Task prompt wording |
| mcp-callmcp-shape | CLI-only skips CallMcpTool; uses CLI cmds |
| prefer-mcp-ops | guard/capture/skill/cache rows |
| old-coder-mandatory | `ax policy skill` load path |
| explore / degraded / IDE bootstraps | already dual-path |

### Intentional non-gaps

| File | Why not dual-path |
|---|---|
| codegraph-parity | TS→Rust implementation map, not agent workflow |
| docs-with-features | docs shipping rule; MCP is a product surface |
| savings-gauntlet | measures MCP token savings by design |
| language review skills | stack reviews; transport-agnostic |

### Tests after recheck

`cargo test -p ax-policy --lib every_bootstrap` → **2 passed** (startup/subagents included in CLI fallback assertions)
