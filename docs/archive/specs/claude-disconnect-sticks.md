# SPEC — Disconnecting Claude Code sticks

Tier 2 bug fix. The user disconnects Claude Code, and it shows up connected again right away.

## Root causes (verified on this machine)
- R1: connecting Claude writes ax into two files: `~/.claude.json` and the project's `.mcp.json`. Disconnecting (`uninstall_claude_mcp`) only cleans `~/.claude.json` and `~/.claude/settings.json`; it never touches the project's `.mcp.json`. The status check counts `.mcp.json`, so Claude still shows **Connected**, and Claude Code keeps loading ax from that file. Here, `~/.claude.json` has no ax, but `/Users/gary/io/ax/.mcp.json` still does.
- R2: `ax policy pack import` connects every found IDE (`install_detected`) without looking at your choice, so it reconnects Claude whenever the Claude CLI is installed.

## Behaviors
- D1: Disconnect Claude Code (from Settings, `ax init` unchecking, or `ax uninstall --target=claude`) also removes the `ax` entry from the project's `.mcp.json`. Other servers in that file stay. If `ax` was the only server, the file is left as `{"mcpServers":{}}`, not deleted. The removed files are listed.
- D2: after a Disconnect, the status shows Claude as **Found**, not Connected.
- D3: `ax policy pack import` only refreshes IDEs that already have ax configured. It never connects a new one. When `ax.json` has a saved `agents.ides`, it only refreshes those.
- D4: the uninstall of other targets is unchanged.

- D5 (added at the user's request: "ik wil overal takumi eruit hebben"): Takumi is removed everywhere.
  - Installer: the `takumi` target, its install and uninstall code, the CLI catalog entry, detection, and the `ax init` menu. There are 12 targets after this. `ax install --target=takumi` fails with "unknown target", like any unknown id.
  - Command Center: it no longer shows in Settings → IDEs & agents. The `?takumi=1` embed parameter is dropped (`?embed=1` stays), and the `bonzaicoder` project is no longer shown as "takumi" (StatusBar, Stats, `ax-web` lib.rs).
  - Text: help text, CLI flag examples, hints and comments that name Takumi (main.rs, help_text.rs, installer/mod.rs, mcp_ops.rs, proxy.rs, ax-db lib.rs, PolicyShareSettingsSection).
  - The Takumi section of the ship skill (template and `.agents/skills/ship`).
  - Docs: the `guides/takumi.md` page and its sidebar entry are deleted. Every mention in README, installation, cli, mcp-server, policy-sharing, command-center, troubleshooting and integrations is removed.
  - Kept: old specs and audit files in `docs/`, which are history.
  - Not cleaned up: ax config that an earlier ax already wrote into Takumi's files stays on disk, because ax no longer knows Takumi.

## Must not
- N1: delete a `.mcp.json` or remove other servers from it.
- N2: change what Connect writes.

## Tests
- installer unit test: install Claude into a temp home and project, then uninstall. The project's `.mcp.json` keeps a second server `other` and has no `ax`, and the status says configured=false.
- `init_ides`: after `AX_INIT_IDES="cursor claude"` then `AX_INIT_IDES=cursor`, the project's `.mcp.json` has no `ax`.
- pack import: the target list becomes a pure function `pack_refresh_targets(configured, saved)`, with a unit test: nothing configured means nothing; a saved list filters.
- D5: `TARGETS` has 12 ids and no `takumi`; `parse_ide_choice("takumi")` is an error; after the change, a grep gate (`rg -i takumi` outside `docs/specs`, `docs/audits`, `dist`, and lockfiles) finds nothing. The gate is run once against the current tree as a negative control, and it must fail there.
- Mutants added to `scripts/init-ides-mutants.sh`, full tests, clippy, reinstall, and a real Disconnect in the Command Center, then check `.mcp.json`. Update docs and EVIDENCE.

## Setup
- `uninstall_targets` gets a `project_root` argument (callers: init, web `/api/agent/uninstall`, `ax uninstall`). No new dependencies. No commits.
