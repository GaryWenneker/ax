# Evidence Report — Graph-only robustness: closing C5 and C6 (Tier 3)

Follow-up to [../2026-08-19-preflight-graph-only/EVIDENCE.md](../2026-08-19-preflight-graph-only/EVIDENCE.md), which found C1–C4 passing and **C5** (query-time snippets read the working tree) and **C6** (the MCP catalog hides the graph tools the CRITICAL rules mandate) failing.

## Provenance

| | |
|---|---|
| Spec | `c:\Users\gary.wenneker\.cursor\plans\graph-only_robustness_25b9e776.plan.md` |
| Spec approval | **Obtained** — the user approved the plan and its two locked decisions (C5 hybrid store, C6 graph-core catalog), then instructed "Implement the plan as specified". Guard level/scope/globs were answered separately before the policy rule was saved. |
| Source state | working tree, uncommitted, 33 changed paths on top of `96ddab4cc85bfcaf9aba0da8739ada0761f6f5d7`. Fingerprint: `sha256(git diff HEAD ‖ git status --porcelain)` = `41229fa38b0f3bd9cf5a85b38ccb74e9998833e24e5eb02fc0bed99663e298b7` |
| Tier | 3 — public MCP surface plus a schema migration |
| Isolation | **none: worked directly in the user's tree.** The plan did not specify a worktree or branch and I did not propose one. This is a deviation from the skill's isolation rule and it is the weakest part of this report's provenance. Every file the gauntlet mutates is restored from an in-memory byte copy and verified byte-for-byte (layers 7–9); `git status` was checked clean of mutation residue after each run. |
| Independent verification | not performed — declared downgrade |
| Tooling | `cargo 1.97.0 (c980f4866 2026-06-30)`, `rustc 1.97.0 (2d8144b78 2026-07-07)`, `clippy 0.1.97 (2d8144b788 2026-07-07)`, ax `4.4.0`, schema `v17`. `ax.db` figures below were taken with an out-of-tree `sqlite3 3.22.0` CLI reading copies of the live database (page counts, row counts, and file sizes after an explicit `VACUUM` where stated) — not a claim about the sqlx-linked engine ax itself uses. |
| One-command rerun | `.\scripts\gauntlet-graph-only.ps1` (9 layers; calls `.\scripts\mutate-graph-only.ps1`) |

All numbers below come from **one final full run after the last code edit** (26m 00s, exit 0). Its transcript is committed beside this report as [gauntlet-final.txt](gauntlet-final.txt) — earlier drafts cited a path under `.ax/`, and then a `.log` name; both are gitignored, so neither was reachable from a clean checkout.

## Verdict

Both failures are closed, and both are now non-regressable by an executable check rather than by intent.

| Claim | Result | Evidence |
|---|---|---|
| C5 — snippets come from the graph, never a query-time disk read | **pass** | Layers 2, 4, 7, 8; live verification below |
| C6 — agents can discover the graph tools the rules mandate | **pass** | Layers 3, 5, 8 |
| Neither can silently regress | **pass** | Fail-closed source gate + CRITICAL policy rule + catalog coherence test, each proven able to fail |

One honest qualification on scope, unchanged from the earlier audit: *index-time* scanning still walks the tree (that is how the graph is built), and Command Center's source viewer still reads files, because it shows a human the file's current content rather than answering a graph query.

## Gauntlet

| # | Layer | Command | Result |
|---|---|---|---|
| 1 | Workspace suite | `cargo test --workspace` | PASS — 37 test binaries, **0 new failures**, 1 baseline failure (below) |
| 2 | Graph-only gate + guard scoping | `cargo test -p ax-context --test no_query_time_disk_reads`, `cargo test -p ax-policy --lib guard` | PASS — 3 passed, 0 failed; 14 passed, 0 failed |
| 3 | MCP catalog coherence | `cargo test -p ax-mcp --lib tool_filter` + `--test new_tools_smoke` | PASS — 6 passed, 0 failed; 2 passed, 0 failed |
| 4 | Source store | migration_v17, source_store_coverage, source_store, source_store_write_path, stats_format | PASS — 4 + 1 + 21 + 2 + 7 passed, 0 failed |
| 5 | Catalog payload size | `cargo test -p ax-mcp --test catalog_payload_size -- --nocapture` | PASS — figures below |
| 6 | Clippy | `cargo clippy` on the six touched crates, `--all-targets --no-deps` | PASS — **0 new findings** (16 pre-existing, all baselined) |
| 7 | Negative control | inject a real disk read, require the gate to go red | PASS — gate failed (exit 101), file restored byte-for-byte |
| 8 | Mutation (full) | `.\scripts\mutate-graph-only.ps1` | PASS — **12/12 killed** |
| 9 | Mutation vs properties alone | `.\scripts\mutate-graph-only.ps1 -PropertiesOnly` | PASS — **4/4 killed** (8 mutants report N/A: outside what the properties claim) |

### Baselines (zero NEW, not zero)

This repo carries pre-existing failures and lints that predate the work. Both baselines are recorded literally in `scripts/gauntlet-graph-only.ps1`, and the layers compare **counts**, so a new finding of the same kind in the same file still fails.

- **Suite:** 1 pre-existing failure, `ax-usage` `pricing_sync::tests::upsert_and_history`. It is order-dependent — it shares the `AX_USAGE_DB` env var and cached pool with its neighbours, so it fails in a parallel `--workspace` run and passes run alone (verified both ways). `ax-usage` is untouched by this change (`git status` clean for that crate). Not fixed: unrelated, and fixing it is scope creep.
- **Clippy:** 16 pre-existing findings across 10 file/lint groups, e.g. `ax-extraction/src/languages/refs.rs [too_many_arguments] ×7`, `ax-db/src/queries.rs [len_zero] ×1` (in `build_fts_prefix_query`, outside every hunk of this diff). `-D warnings` has never been a passing gate here, so the layer parses clippy's JSON and diffs against the baseline instead.

Two findings **were** mine and were fixed rather than baselined: `ptr_arg` in `migration_v17.rs` and `unnecessary_min_or_max` in a property test.

## C6 — the catalog

`is_core_tool` now advertises the turn contract plus the whole graph read surface. Measured cost of that decision, from layer 5:

| Catalog | Tools | `tools/list` chars | ≈ tokens |
|---|---|---|---|
| Default (`AX_MCP_TOOLS` unset) | 22 | 7,331 | ~1,832 |
| `AX_MCP_TOOLS=all` | 28 | 9,725 | ~2,431 |

Still gated by default: `ax_index`, `ax_lsp`, `ax_ship`, `ax_policy_index`, `ax_files`, `ax_diagnostics`. The default payload ships every turn; the test asserts a 24,000-char ceiling so growth has to be a decision, and asserts the full catalog is strictly larger so the lean filter is proven to still gate something.

**Drift guard.** `POLICY_REFERENCED_TOOLS` lists every `ax_*` tool named in the shipped rules and IDE bootstrap text; `every_policy_referenced_tool_is_classified` requires each to be either advertised in `CORE_TOOLS` or knowingly listed in `GATED_BY_DESIGN`. Naming a tool in a rule without deciding its catalog status now fails a test — that is the check that would have caught C6.

**Known tension, not resolved:** `prefer-mcp-ops` tells agents to call `ax_ship` / `ax_lsp` / `ax_index` / `ax_diagnostics` / `ax_policy_index` over the shell, and those five stay off the default catalog per the locked decision. They remain callable — the allowlist only controls discovery — but an agent that trusts `tools/list` will not see them. Documented in the MCP reference; the fix for such an agent is `AX_MCP_TOOLS=all`.

**Payload test was fixed mid-run.** The first version measured the two catalogs in two `#[tokio::test]`s that both wrote the process-global `AX_MCP_TOOLS`; they raced inside the shared test binary and reported each other's catalog (one run printed 22/22, another 28/28). Its sibling assertion also claimed "full is larger than default" while only asserting `> 0`. Both are now one sequential test with the real comparison. **The pre-fix numbers were not trustworthy and are not reported.**

## C5 — the source store

Snippets are served from `file_contents` (schema v17) and the query path can no longer reach the filesystem.

### Migration

`crates/ax-db/tests/migration_v17.rs`, 4 tests: a v16-shaped database upgrades without losing rows; repeated opens are idempotent; an **interrupted** upgrade (DDL committed, version row missing) recovers on the next open; `path` is unique.

One test was rewritten during the work, and it is worth saying why: `migration_is_idempotent_across_reopens` originally asserted a guarantee the code does not make (re-running a migration list against an already-migrated DB), which is API misuse rather than a failure mode. It now asserts what the plan's failure model actually cares about — recovery from a crash mid-upgrade.

### Cost in `ax.db`, measured on this repo

Measured on two `VACUUM`ed copies of the live database, one with `file_contents` deleted:

| | Files | Stored text | `ax.db` (vacuumed) | Delta |
|---|---|---|---|---|
| Without the store | — | — | 38,203,392 B (36.4 MB) | — |
| With the store | 500 | 3,970,158 B (max row 72,150 B) | 42,573,824 B (40.6 MB) | **+4,370,432 B (+11.4%)** |

The live file is larger than either figure — 222,531,584 B (212 MB), of which 41,870 of its 54,329 pages (171 MB at a 4 KB page size) are free list. That is the bloat described below, already deleted: SQLite returns freed pages to its own free list and reuses them, so the file does not shrink without an explicit `VACUUM`, which ax does not run for you. The database is not still growing; it is carrying reusable space. Orphaned stored rows in the live database: **0**.

### The bug this measurement found

The first measurement came back **+95,608,832 B (+402.6%)** on 1,659 stored rows — five times the whole database, far outside what the plan's failure model anticipated for ~500 code files. Rather than report it as an accepted cost, I looked: **1,160 of those rows (90.4 MB) were build artifacts under `target-dev/`** — `.o`, `.rlib`, `.rmeta`, linker argument files — none of which has a graph node, so no snippet could ever be served from them. Their `updated_at` timestamps were interleaved with the good rows, i.e. current code was writing them.

Cause: `index_files` takes paths straight from its caller and, unlike `scan_files`, applied no admission filter before storing text. The daemon's file watcher hands it every path a build touches, so a `cargo build` streamed object-file text into `ax.db`.

Fix, test-first:

1. RED — `crates/ax-extraction/tests/source_store_write_path.rs` asserts a `.o` and a `.rmeta` get no stored row while a `.rs` does. Observed failing on behaviour: *"blob.o has no language … (got 21 bytes)"*.
2. GREEN — `is_extractable` extracts the admission test `scan_files` already used, and `index_files` applies it before storing.
3. A second RED/GREEN pair for recovery: `prune_orphan_file_contents` drops stored text no indexed file claims, reported as `SyncResult::source_pruned`, so a database written by an earlier binary heals on the next `ax sync` instead of carrying the bloat forever.

Live confirmation on this repo: after one `ax sync`, stored rows went 1,659 → 499, orphans → 0, and `ax status` reported complete coverage. On the shipped binary it now reads `Source store: 500/500 files — snippets served from the graph`.

### The second defect measurement found: a coverage warning that could never be satisfied

`ax status` and the preflight `<ax_index>` block warn when the store does not cover the index. That warning compared stored rows against `COUNT(*) FROM files` — and at the time of the final run `files` holds 3,915 rows against 500 storable code files: 3,414 have no language at all (SVG assets, and the `.o`/`.rmeta`/`.rlib` rows the watcher still records during a build) and 1 is over the cap. Complete coverage therefore printed `Source store: 500/3915 files … run ax_index (or ax_sync) to backfill`: a permanent nag naming a command that can never clear it, on a store that was in fact complete. The denominator also drifts upward during any `cargo build` (it read 3,539 when the fix was written, 3,915 at the final run), so the nag got worse the more you used the repo.

This is a defect in my own migration-UX work, and the unit tests missed it because they asserted the formatter against synthetic stats where the two numbers happened to agree. Measurement on the real database is what surfaced it. Fixed test-first:

1. RED — `rows_no_parser_claims_are_not_a_store_gap` (formatter) and `crates/ax-db/tests/source_store_coverage.rs` (the SQL). Observed failing on behaviour, not compilation: *"complete coverage must be silent even when most file rows are unparseable"* and `Source store: 0/3539 files …` (3,539 was the row count when the test was written — the drift to 3,915 by the final run is the same symptom).
2. GREEN — `GraphStats` gained `source_expected_files`: files a parser claimed **and** under the store cap. `source_store_coverage` computes both numbers in one place; the warning and the status line measure against the expected set.
3. Both new tests were green on their first run, which proves nothing, so both are pinned by mutants (layer 8): restoring the old `COUNT(*) FROM files` denominator, and restoring the old `file_count` comparison. Each is killed.

The cap is part of the definition on purpose: a file over the cap stores no text by design, so counting it as a gap would be the same false alarm. The test covers that both ways, including that raising `AX_SOURCE_STORE_MAX_BYTES` brings an over-cap file back into scope.

### The third defect: `guard: forbid-content` ignored its rule's globs

The CRITICAL rule this change captured bans disk-read spellings, scoped to the query-path modules the user chose. Saving it exposed a pre-existing flaw in the directive itself: `forbid-content` matched **every** write in the project and ignored the rule's `globs` entirely, while its sibling `require-content` honoured them. So the rule as scoped would also have blocked `orchestrator.rs` — the indexer whose whole job is reading files to build the graph. The only ways out would have been to weaken the rule until it enforced nothing, or to switch the guard off; either one silently costs the enforcement this phase exists to provide.

Fixed test-first in `crates/ax-policy/src/guard.rs`: `forbid_content_directive_is_scoped_by_rule_globs` (observed red first), then the match arm gained the same `rule.globs.is_empty() || any_glob_matches(…)` test `require-content` already used. Empty globs still mean project-wide, deliberately — the shipped secrets rule declares none and must keep firing everywhere. Both directions are pinned by mutants (layer 8): removing the glob check, and requiring globs before the directive can fire at all. Each is killed. Layer 2 now runs the `ax-policy` guard tests alongside the source gate, since the two are one enforcement story.

### Read path, verified live with the shipped binary

Run on a scratch project (`ax init` + `ax index`, one Rust file) rather than the working database, so the states could be produced without mutating real data:

| State | How it was produced | Observed |
|---|---|---|
| Fresh | `ax explore greet` | correct numbered source, `2 pub fn greet(name: &str) -> String {` … lines 2–4 |
| Store diverged from the graph | `UPDATE file_contents SET content_hash='deadbeef', content='STALE TEXT — must never be shown as current'` | mismatch detected, that one file auto-synced, **correct** source served; the planted text never appeared. Stored row afterwards: real hash `535a5cf9…`, real text. |
| Nothing stored | `DELETE FROM file_contents WHERE path=…` | `(source not stored: src/greeter.rs — run ax index to backfill the source store)` and `ax status` → `Source store: 0/1 files … run ax_index (or ax_sync) to backfill`. **No disk fallback.** |
| Recovery | `ax sync` (reported "Already up to date" — no file changed) | `Source store: 1/1 files — snippets served from the graph`, i.e. the backfill path restores stored text without re-parsing |
| Over the cap | this repo's 2.5 MB `web-ui/dist` bundle | the one parseable file with no stored row, excluded by the 1 MB cap and excluded from the coverage denominator |

### A behaviour change worth stating plainly

I edited a file on disk without syncing and explored a symbol in it: the snippet showed the **last indexed** text, with no stale marker. That is correct for this design and worth being explicit about, because it differs from before.

The hash comparison detects *store vs graph index* divergence, not *graph vs disk*. An unsynced edit leaves both database values consistent, so there is nothing for the check to flag — the whole graph is stale until sync, and `ax status`'s pending-files report is what covers that. The upside is that a snippet's line numbers and its text now always come from the same index; the previous behaviour read the *new* text while reporting the *old* line numbers, silently misaligning the two. Documented in the MCP reference and README.

## Enforcement, and proof it can fail

**Fail-closed source gate** — `crates/ax-context/tests/no_query_time_disk_reads.rs` asserts no `fs::read_to_string`, `File::open`, `fs::read`, `read_dir` or `WalkBuilder` appears in the query-path modules, ignoring comments. An unreadable or missing file is a hard failure, never a pass. A companion test requires every module in the crate to be either checked or explicitly exempt, so adding a module cannot quietly escape the gate.

**Negative control (layer 7)** — a real `std::fs::read_to_string` was appended to `explore.rs`; the gate failed with exit 101, naming `src/explore.rs:237`, and the file was restored byte-for-byte. Precisely what this buys: **one known-bad spelling reaches the failure path.** It does not prove every possible disk read is caught — the gate matches patterns, so a novel spelling (an aliased import, a helper in another crate) would pass it. The policy rule below is the second, independent layer for that reason.

**CRITICAL policy rule** — `graph-only-query-path`, project scope, `guard: forbid-content` on the query-path globs (`crates/ax-context/src/{explore,builder,source_store,formatter,explore_format,markers}.rs`), so `ax_guard` blocks a write reintroducing a disk read. Level, scope and globs were confirmed with the user before saving. Saving it is what exposed the glob-scoping defect above — before that fix, the rule as scoped would have blocked the indexer too.

**Mutation (layers 8–9)** — no mutation tool is installed (no `cargo-mutants`, no `proptest`), so this is the manual procedure, persisted as `scripts/mutate-graph-only.ps1` so the report is reproducible from the repo alone. 12 plausible bugs, **12/12 killed**:

| Mutant | Models |
|---|---|
| blank graph hash counts as fresh | unparseable file looking verified |
| every stored row is fresh | the core failure: stale source served unlabelled |
| end line not clamped to EOF | snippet silently empty |
| line numbers off by one | agent sent to the wrong line |
| store scope: keep build output | the 90 MB regression above |
| prune deletes claimed source instead of orphans | inverted cleanup wiping live snippets |
| coverage counts every file row | the false-alarm nag above, in the SQL |
| coverage warning measures against `file_count` | the same defect, in the formatter |
| `forbid-content` ignores the rule globs | the unscoped ban that blocks the indexer, so the guard gets switched off |
| `forbid-content` needs globs to fire | the opposite drift: the globless secrets rule silenced everywhere |
| graph reads hidden again | C6 itself |
| heavy ops advertised by default | the opposite drift, every turn paying for the full menu |

The runner holds itself to two rules, because a hand-rolled mutation runner can otherwise credit kills it never performed: it verifies the bug is really on disk before running tests (a missing anchor is a **hard error**, not a skipped mutant), and it verifies the byte-for-byte restore afterwards. Both fired for real during this work: an anchor that no longer matched was reported `ERROR — mutant not applied` rather than passing, and an additive mutant that my application check mishandled was reported `ERROR — refusing to report a kill` rather than counted.

### The property tests earned their claim the hard way

`source_store` has 5 exhaustive property tests (no generator dependency — the state space is small enough to enumerate, which is stronger than sampling): `classify` over every combination of graph hash × stored row × size either side of the cap, and the slicers over 6 file shapes × 100 line ranges including `i32::MIN`/`MAX`, negatives, zero, inverted and past-EOF.

They passed first try, which proves nothing on its own. Kills are credited to whichever test fails first, so layer 9 re-runs the mutants against **the properties alone**. First attempt: **3/4 — the "end line not clamped to EOF" mutant survived.**

That is exactly the one-sided-invariant trap: every property was a *never-exceeds* bound ("never shows a line that isn't real", "never over the cap"), and dropping the clamp makes the output **empty** — which violates no upper bound. Paired them with the opposite bound (`a_range_overlapping_the_file_shows_exactly_those_lines` and the `slice_lines` equivalent: a range that overlaps the file must show exactly those lines). Layer 9 now kills **4/4**.

## Docs (required by `docs-with-features`, same change)

- `site/src/content/docs/reference/mcp-server.md` — new default catalog and what `AX_MCP_TOOLS` still gates; the known tension above; the source store, its cap, what it stores and why, the prune, the indexed-state behaviour, and what the coverage number counts
- `README.md` — tool table split into default/opt-in, graph-only snippet guarantee with the measured size
- `crates/ax-installer/src/targets.rs` — an explicit note that we chose *not* to write `AX_MCP_TOOLS` into IDE configs, since the code default is now correct
- `guard: forbid-content` scoping, everywhere it is described: `site/src/content/docs/guides/policy-engine.md`, `site/src/content/docs/reference/cli.md`, `site/src/content/docs/reference/mcp-server.md`, the `ax_guard` tool description in `crates/ax-mcp/src/tools.rs`, and `crates/ax-policy/templates/skills/startup/SKILL.md`. The behaviour changed, so every place that stated the old semantics had to change with it — a doc that still promised project-wide matching would send the next author to write an unscoped rule.

## Release

`.\scripts\release-local.ps1` → `cargo clean -p ax-cli` + `cargo build --release -p ax-cli` (8m 45s), then all three Windows install paths verified to hash-match `target-dev\release\ax.exe`:

```text
OK: C:\Users\gary.wenneker\.cargo\bin\ax.exe
OK: C:\Users\gary.wenneker\AppData\Local\ax\current\bin\ax.exe
OK: C:\Users\gary.wenneker\AppData\Local\ax\current\ax.exe
All 3 ax.exe copy/copies match release build (SHA256 E871083418CA8F94)
```

Rebuilt after the last edit, because the `ax_guard` tool description that documents the new glob scoping is compiled into the binary; an unrebuilt binary would have advertised the old semantics.

`ax --version` → `ax 4.4.0`. Every live check in this report was run against that binary.

`ax MCP` must be restarted in Cursor to pick it up. No version bump: that goes through `.\scripts\release-tag.ps1` only.

A caught mistake worth recording: the first attempt used `reinstall-cli.ps1`, which by design **copies without rebuilding**. It happily synced a binary built 80 minutes before the orphan-store fix and reported all three copies matching — a green check on a stale binary. The timestamp comparison against the newest source file is what caught it.

## Known limits

- **No isolation.** Implemented in the user's working tree (see Provenance). Restores are byte-verified, but a green run here is not evidence about a clean checkout.
- **Uncommitted source state.** The tree fingerprint below identifies it; there is no commit SHA to cite yet.
- **No independent verification.** Tier 3 permits this only as a declared downgrade, so: declared, and the confidence claimed here is correspondingly lower. A fresh-context verifier was not run.
- **The disk-read gate is a pattern matcher.** It proves one known-bad spelling fails. A novel spelling of a read would pass it; the CRITICAL policy rule is the independent second layer.
- **Suite baseline is a tolerated failure, not a fixed one.** If `ax-usage`'s order dependence ever hides a real regression in that crate, this gauntlet would not see it.
- **Clippy layer is scoped** to six crates and drops findings from workspace dependencies, because rustc replays a dependency's warnings only for units cargo rebuilds — counting them would make the layer depend on the build cache. Findings in other crates are not this layer's business and are not reported.
- **`ax_files` stays out of the default catalog**, superseded by graph queries. Stated rather than left silently hidden.
- **Size cap:** exercised for real (the 2.5 MB `web-ui/dist` bundle is excluded from both the store and the coverage denominator), but the *marker* an over-cap file produces in explore output is covered by unit tests only — I did not query a symbol inside that bundle to see it in situ.
- **The freed 171 MB is not reclaimed automatically.** The orphan prune deletes rows; SQLite reuses the pages but never shrinks the file. Anyone who ran a pre-fix build keeps a large-but-mostly-empty `ax.db` until they `VACUUM` or re-index from scratch. No ax command does this today, and I did not add one — out of scope, but worth knowing.
- **`GraphStats.file_count` is still labelled "code files"** in status and preflight output while counting every indexed row: `Code files: 3915` against 500 storable ones. This change stopped *coverage* from using it; the label itself, and the fact that the watcher records `files` rows for build output at all, are pre-existing and untouched.
- **The guard glob fix widened one directive and narrowed another.** A pre-existing rule that declared globs *and* used `forbid-content` expecting project-wide matching now enforces less than it did. None ships with ax (the secrets rule declares no globs, so it is unaffected), but a user's own rule could rely on the old behaviour; the changed semantics are documented in all five places listed above rather than only in the code.
- **`handle_source` in `ax-web` still reads disk** by design (human viewing current file content), and index-time `scan_files` still walks. Both are out of scope per the plan.

## Revisions

- 2026-08-19: implementation evidence. C5 and C6 closed. Three defects were found and fixed test-first, each pinned by a mutant: build output stored in `file_contents` and a coverage warning that could never be satisfied (both mine, both surfaced by measuring the real database rather than trusting the suite), and `guard: forbid-content` ignoring its rule's globs (pre-existing, surfaced by trying to save the CRITICAL rule this phase requires). Property suite strengthened after a survivor in layer 9. Final numbers from a 26m 00s full run, transcript committed beside this report.
