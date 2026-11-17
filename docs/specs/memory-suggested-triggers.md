# Suggested triggers

Spec approval: not obtained (autonomous run). The approved plan plus "continue" authorized this slice. Isolation: the working tree.

Tier 2. This does not open the project database.

## Scenarios

1. `suggested_triggers("Keep sqlite-vec because the archive stays local", ["crates/ax-memory/src/brain.rs"], "the user asked to remember the vector index", ["sqlite-vec"])` returns `["archive", "local", "ax-memory", "brain", "vector", "index"]`.
2. Title order comes first, then file-name anchors, then prompt words. Generic path segments `src`, `lib`, `tests`, `test`, `dist`, `node_modules`, `target`, `target-dev`, and `crates` are skipped. The extension is skipped.
3. A suggestion is a normalized cue of 4 to 40 characters. Stop cues, noise cues, and the glue words in the implementation are skipped. A trigger the caller already stored is skipped. Duplicates are skipped. The list stops at 8.
4. `suggested_triggers("Update the function", [], "the", [])` returns an empty list.
5. `ax_remember` and `ax remember --json` include `suggested_triggers` on a saved memory. `action=confirm` does not. The suggestions are not stored as triggers by this function.

## Evidence

Recorded after the last code edit. Spec approval: not obtained (autonomous run).

- `cargo test -p ax-memory --offline --test brain`: 41 passed, 0 failed.
- `cargo clippy -p ax-memory --offline --all-targets -- -D warnings`: exit 0.
- `cargo check -p ax-cli -p ax-mcp --offline`: exit 0. `ax-cli` still warns about unused `windows-sys`, which was already there.
- Manual mutants, each restored before the suite above: skipping the title left `["ax-memory", "brain", "vector", "index"]`; dropping the glue list left `["keep", "because", "archive", "stays", "local", "ax-memory", "brain", "user"]`; a cap of 2 left `["archive", "local"]`.
- The function takes no database handle, so it does not store the suggestions. `ax_remember` and `ax remember --json` both put them in `suggested_triggers`. Confirm does not.

## Must not

- Open or rewrite the live vault.
- Register the suggestions as trigger cues.
