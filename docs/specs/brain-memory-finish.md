# SPEC: finish the brain-scale memory plan

Source: the approved plan "Finish brain memory gaps" (user: "Implement the plan as specified"). It closes the gaps between the brain-scale memory plan and the code. No new memory features.

Tier 2. Isolation: none (branch-free, on `main`), because the change is tests, templates, one verify check, and one bug fix found while landing the work. Commits happen at each green step, as the release flow already does on `main`.

## Behaviors

### Parity (library, MCP, CLI on one temp project)

1. A turn saved 91 days ago and pruned is absent from the `ax_preflight` memory inject, and `ax_recall id`, `ax recall --id`, the library, `ax_recall cue`, and `ax recall --cue` all return it with the same id and tier `t3`.
2. `ax_recall query depth=archive` and `ax recall QUERY --depth archive` return the same ids, in the same order, with the same scores as the library `search` with `Depth::Archive`.
3. `ax_history depth=archive` and `ax history --depth archive` return the same graph-change ids.
4. `ax_remember action=confirm id=X` and `ax remember --confirm X` move a candidate from `candidate` to `active`, and the library `confirm_memory` reports the same status.
5. `ax preflight --prompt "... <cue> ..."` and `ax_preflight` with the same prompt list the cue memory first in the memory block.

### Bootstrap teaches the procedure everywhere

6. One `MEMORY_PROCEDURE_SENTENCE` (MCP form) and one `MEMORY_PROCEDURE_CLI_SENTENCE` (CLI form) live in `crates/ax-policy/src/ide_seed.rs`.
7. The AGENTS, Claude, Gemini, Copilot, Windsurf, and Cline managed blocks, the Cursor, Claude, and Continue rule templates, and the startup skill contain the MCP sentence verbatim.
8. All nine `crates/ax-installer/src/templates/cli_only_*.md` files contain the CLI sentence verbatim, and the CLI-only seed writes no MCP server config.
9. `verify_ide_bootstrap` reports `missing memory procedure` for a target whose managed section lacks the sentence. A seeded project passes.
10. A file with user text outside the AX markers keeps that text after `ax policy sync`.

### Bug found while landing the policy work

11. A symbol that changes and then changes back to an earlier fingerprint records a second graph-change entry. `capture_graph_episodes` does not fail with `UNIQUE constraint failed: memories.id`, and later syncs keep working.

### Revision: bugs the parity tests exposed

The parity tests in behaviors 2, 3, and 5 failed on real defects, not on test mistakes. They are added here instead of being worked around:

14. `index_archive_vector` maps each archive memory to its own vector row. `last_insert_rowid()` was read from a different pooled connection, so rows pointed at the wrong memory and archive search returned unrelated rows. Schema 29 drops `memory_vec` and `memory_vec_map`; sync rebuilds them.
15. Archive vector hits at or below cosine 0.1 are dropped, the same floor hot recall already uses (`MIN_VECTOR_SIMILARITY`).
16. Archive search ranks by relevance and stored confidence, without age decay. A 91-day-old exact match outranks fresh lookalikes.
17. In a prompt, a memory matched by a named trigger comes before one matched only by a title, tag, file, or symbol word. A trigger that equals a title word is stored with source `trigger`.

Found while writing the tests, recorded and not changed: a fresh project's required policy (about 13k tokens) exceeds the default 12k preflight budget, so the memory block is omitted there until the budget is raised. The recall limit caps at 25 on MCP and 50 on the CLI.

### Performance (measured, ignored by default)

12. With 100,000 archive rows, an exact cue lookup takes under 5 ms and archive vector top-20 under 50 ms (median of 5 runs, release build). The measured numbers go into EVIDENCE. The 1e6 target is claimed only if measured.

### Command Center

13. The memory vault screenshots (light, dark, mobile) show the tier badge, and one shows an archive search labeled "recalled from archive".

## Must not change

- Hot recall, cue lookup, and preflight still skip the archive.
- Demotion still does not delete.
- No new MCP or CLI flag. Field names stay the same on both surfaces.
- Text outside the AX markers is never rewritten (`preserve-user-policy`).
- Existing tests in ax-memory, ax-policy, ax-installer, and ax-cli stay green.

## Setup plan

- No new dependencies.
- Files added: `docs/specs/brain-memory-finish.md`, `docs/specs/brain-memory-finish-EVIDENCE.md`, `scripts/brain-memory-gauntlet.sh` (one entry point for every layer and the manual mutants).
- Files changed: `crates/ax-cli/tests/memory_parity.rs`, `crates/ax-memory/tests/brain.rs`, `crates/ax-memory/src/brain.rs`, `crates/ax-policy/src/ide_seed.rs`, the IDE and startup templates, the nine CLI-only templates, the CLI-only verify in ax-installer, and the memory vault screenshots.
- Release: 9.0.3 (patch) after the gauntlet is green.

## Revision 2 (found while running the bootstrap and performance work)

18. Archive vector hits resolve to memories in one query driven from `memory_vec_map` by an index on its `rowid` column (`INDEXED BY idx_memory_vec_map_rowid`). Before, each hit walked the tier index across the whole archive: 100,000 rows took 4.1 s for 80 hits.
19. Hot and archive recall run the FTS ranking and the vector ranking concurrently. Ranking and fusion are unchanged.
20. The Gemini and Windsurf CLI-only snippets keep their lifecycle line inside the `AX_CLI` markers, so a reinstall does not append another copy outside them.
21. `ax doctor --mode cli` reports `cli_memory_procedure` as degraded, naming each CLI-only file that lacks the memory procedure, with repair `ax install --mode cli-only`. Files without CLI-only content are not checked.
22. There are ten CLI-only templates (the Cursor rule is `.mdc`), not nine; all ten carry the CLI memory procedure.

Found and not changed: a fresh `ax init` fails `ax policy sync` on `.agents/rules/prefer-mcp-ops.mdc` ("missing ax_preflight; missing once-per-turn dedup"). That check and template are outside this change.
