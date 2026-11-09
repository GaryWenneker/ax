# SPEC: CLI↔MCP parity rules/skills for every IDE

**Tier:** 2 (normal)  
**Isolation:** branch `feat/cli-ide-parity` (no worktree — installer/policy templates + tests only)  
**Spec approval:** not obtained (autonomous MUST run) — see EVIDENCE.md

## Goal

Agents must follow a working ax workflow on **every IDE ax supports**, whether MCP is connected or the project is **CLI-only**. Today CLI-only install covers only Cursor + VS Code/Copilot, and IDE bootstrap text is MCP-first without a concrete CLI tool map.

## In scope

1. **IDE bootstrap templates** (`crates/ax-policy/templates/ide/**` + `AGENTS_INSTRUCTIONS_BLOCK` / Claude / Cline blocks in `ide_seed.rs`): add an explicit **CLI fallback** section — when MCP is unavailable or the project is CLI-only, use the CLI equivalents (`ax preflight`, `ax explore`, `ax session` / `ax turn`, `ax sync`, …) and the session/turn loop. Keep “prefer MCP when connected”.
2. **Seeded team rule templates** that gate on MCP (`prefer-mcp-ops`, `explore-before-grep`, `subagents`, `startup` skill if needed): add CLI-only / DEGRADED rows that name the matching CLI commands (not “skip graph entirely”).
3. **`ax install --mode cli-only`**: expand beyond `vscode`+`cursor` so every agent-instruction surface gets CLI rules/skills **without** writing MCP server configs:
   - cursor → `.cursor/rules/ax-cli.mdc`
   - vscode/copilot → `.github/copilot-instructions.md` + `.github/skills/ax-cli/SKILL.md`
   - windsurf → `.windsurfrules` (CLI block upsert)
   - continue → `.continue/rules/ax-cli.md` (**no** `.continue/mcpServers/ax.json`)
   - claude → `.claude/rules/ax-cli.md` + CLAUDE.md marker upsert
   - gemini → `GEMINI.md` marker upsert
   - cline / antigravity → `.clinerules` / clinerules CLI body
   - zed, jetbrains, kiro, codex, hermes, opencode → shared `AGENTS.md` + `.agents/skills/ax-cli/SKILL.md` (same shared skill)
4. **Default `cli-only` targets** when `--target` omitted: all of the above that have a distinct path, not only vscode+cursor.
5. **Tests** proving each target gets the expected files and that MCP config files are **absent** after CLI-only install.
6. **Docs:** `docs/CLI_ONLY.md` + site CLI reference note listing per-IDE files.

## Out of scope

- Rewriting every review skill (rust-review, etc.) — they are domain reviews, not transport.
- Removing MCP install paths for normal `ax install`.
- Netlify/site marketing copy.
- Renaming `McpEngine`.

## Acceptance scenarios

### A — CLI-only covers all IDE instruction surfaces
Given a temp project  
When `install_cli_only(root, &[])` (empty = defaults)  
Then these paths exist with non-empty CLI workflow content containing `ax turn` or `ax preflight` or `ax explore`:
- `.cursor/rules/ax-cli.mdc`
- `.github/copilot-instructions.md`
- `.github/skills/ax-cli/SKILL.md`
- `.agents/skills/ax-cli/SKILL.md`
- `AGENTS.md` (AX marker with CLI fallback)
- `.windsurfrules` (or windsurf path used by seed)
- `.continue/rules/ax-cli.md`
- `.claude/rules/ax-cli.md`
- `GEMINI.md` (AX marker)
- `.clinerules` (or clinerules path used by seed)  
And these MCP configs do **not** exist:
- `.continue/mcpServers/ax.json`
- Cursor/Windsurf/Zed MCP json files written by full install

### B — Explicit target still works
When `install_cli_only(root, &["windsurf".into()])`  
Then windsurf + shared skill/AGENTS are installed; cursor-specific file may be absent unless shared.

### C — Unknown target still gets shared
When `install_cli_only(root, &["zed".into()])`  
Then shared `.agents/skills/ax-cli` + `AGENTS.md` exist; report notes zed uses shared AGENTS.

### D — Bootstrap templates dual-path
Given each of: cursor `ax.mdc`, continue `ax.md`, claude `ax.md`, cline rules, AGENTS block  
Then body contains both:
- prefer MCP / `ax_preflight` when MCP available
- CLI fallback naming at least `ax preflight` and `ax explore` (or `ax turn begin`) when MCP unavailable / CLI-only

### E — prefer-mcp-ops / explore-before-grep templates
When MCP unavailable or CLI-only project  
Then those seeded templates instruct using CLI graph/ops commands (`ax explore`, `ax sync`, …), not “stop” or “Grep only”.

### F — Full MCP seed unchanged for MCP configs
When `seed_ide_agent_workflow` runs (normal init)  
Then Continue MCP json is still written (regression: MCP path intact).

### G — Real CLI smoke
When `ax install --mode cli-only --target cursor --path <temp>` (or library call)  
Then exit 0 and doctor/`ax preflight --help` still work on the machine.

## Invariants (must not break)

- Existing `sync_ide_bootstrap_fix_creates_all_targets` and MCP seed tests stay green (update only if template assertions need dual-path phrases).
- Normal `ax install` still installs MCP where it did before.
- No new Cargo dependencies.

## Setup plan

- Branch: `feat/cli-ide-parity`
- Files likely touched:
  - `crates/ax-installer/src/cli_only.rs` + `templates/cli_only_*`
  - `crates/ax-policy/src/ide_seed.rs` + `templates/ide/**` + `templates/rules/{prefer-mcp-ops,explore-before-grep}.mdc` (+ subagents/startup if required by tests)
  - `crates/ax-installer` and/or `ax-policy` unit tests
  - `docs/CLI_ONLY.md`, site CLI note if needed
- Checkpoints: commit SPEC on approval; commit GREEN; no force-push
- Gauntlet: `cargo test -p ax-installer …`, `cargo test -p ax-policy ide_seed …`, clippy on touched crates, real `install_cli_only` in tempdir, 3–5 manual mutants on target mapping

## Mapping IDE → files (source of truth for tests)

| Target id | CLI-only files | Must not write |
|---|---|---|
| cursor | `.cursor/rules/ax-cli.mdc` | MCP json |
| vscode / copilot | `.github/copilot-instructions.md`, `.github/skills/ax-cli/SKILL.md` | MCP json |
| windsurf | `.windsurfrules` (CLI AX block) | MCP json |
| continue | `.continue/rules/ax-cli.md` | `.continue/mcpServers/ax.json` |
| claude | `.claude/rules/ax-cli.md`, CLAUDE.md AX block | MCP json |
| gemini | `GEMINI.md` AX block | — |
| cline / antigravity | `.clinerules` CLI body | MCP json |
| zed / jetbrains / kiro / codex / hermes / opencode | `AGENTS.md` + `.agents/skills/ax-cli/SKILL.md` | MCP json |
| shared (always) | `.agents/skills/ax-cli/SKILL.md`, `AGENTS.md` | — |
