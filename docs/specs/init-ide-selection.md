# SPEC — Choose IDEs during `ax init`, and remember the choice

Tier 2: a CLI prompt plus a saved setting. The installer itself doesn't change.

Approval: approved by the user with revised A6 ("Yes, build it"), after asking that deselected IDEs be removed from disk.

## Behaviors

- A1: after the stack question, an interactive `ax init` asks "Which IDEs and agents should ax connect?" in the same menu as stacks (arrow keys or j/k, space toggles, enter confirms, esc keeps the defaults). The list is every installer target in two groups:
  - **Editors**: Cursor, VS Code, Windsurf, Zed, Kiro, Antigravity, Takumi, Continue;
  - **CLI agents**: Claude Code, Codex CLI, opencode, Hermes, Gemini CLI.
  Each row shows its name, plus "found" when it is installed on this machine.
- A2: the choice is saved in the project's `ax.json` as `agents.ides` (a list of ids, for example `["cursor", "vscode"]`). Other keys in `ax.json` are kept.
- A3: pre-checked rows:
  - when `agents.ides` exists, exactly the saved list, even if it is empty;
  - when it doesn't exist yet, every IDE that was found.
- A4: after the choice, init connects exactly the chosen IDEs, like `ax install --target=<ids> --yes`. With nothing chosen, it connects nothing and says so.
- A5: rerunning `ax init` shows the saved choice pre-checked, so enter keeps it.
- A6 (superseded by F1 in docs/specs/ide-detection-and-removal.md: every configured, unchosen IDE is removed; A7's "found" is redefined by F2) (revised at the user's request): deselecting an IDE that was saved before removes ax's config for it from disk, like `ax uninstall --target=<id>`, and prints the removed files. Only IDEs that were in the saved list are removed. An IDE that was never chosen is left alone, even if ax is configured there. Note: most of these configs live in your home folder, so this disconnects that IDE for every project on this machine.
- A7: without a terminal (CI or piped stdin), init doesn't ask. It connects the saved list, or the found IDEs when nothing is saved (as today), and saves nothing.
- A8: when the menu can't draw, init falls back to typed input: ids separated by spaces or commas, `none`, or empty for the defaults. An unknown id is an error that names the valid ids.
- A9: saved ids that are no longer known are ignored with a warning.

## Must not
- N1: change what the installer writes per IDE.
- N2: change the stack question or its saved format. Moving the menu code into a shared helper must keep the stack menu output identical.
- N3: add dependencies.

## Setup plan
- Isolation: none, in the current tree (uncommitted work). No commits.
- Code:
  - `crates/ax-policy/src/config.rs`: `read_project_ides` (returns `None` when unset) and `write_project_ides`;
  - `crates/ax-installer/src/targets.rs`: pure `ide_defaults`, `parse_ide_choice`, `ide_groups`;
  - `crates/ax-cli/src/commands/init.rs`: a shared menu and `choose_ides_for_init`.
- Tests: unit tests next to each function, plus `crates/ax-cli/tests/init_ides.rs` running `ax init` without a terminal in a temp project with a temp `HOME`.
- Gauntlet: tests, clippy, a mutation script (`scripts/init-ides-mutants.sh`), reinstall of the CLI, one real `ax init` in a temp project, docs (installation guide, CLI reference, README), and EVIDENCE.

## Revision 1 (during implementation)
- A7 correction: before this change, `ax init` did not connect only the found IDEs. It connected every one of the 13 targets (`install_detected` with `install_all`). A7 now connects the found IDEs when nothing is saved, as written, so a first `ax init` in CI connects fewer IDEs than before.
- N2 finding: the stack menu's `{:<12}` never aligned the ids, because the padding applies to an already colored string. The output is kept identical, as N2 requires, and the IDE menu has the same look.
- A10 (added so A2, A5 and A6 can be tested end to end): `AX_INIT_IDES="cursor vscode"` (or `none`) answers the IDE question without a terminal. It is handled exactly like a typed answer: saved, and dropped IDEs are removed. An empty value means the defaults.
