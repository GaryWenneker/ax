# EVIDENCE: Policy hash-on-change revisions

**SPEC:** `/Users/gary/io/ax/docs/specs/policy-revisions.md`  
**Approval:** user “accept” (2026-09-02)  
**Tier:** 2  
**Entry:** `bash tools/gauntlet-policy-revisions.sh`  
**Source:** git `c03181ef9821e7fb8216d025b39f2b8d4cd6d8b6` plus uncommitted tree (this feature + prior zip work)  
**Gauntlet run:** after last implementation edit (EVIDENCE file only after that run)

## Behavior map

| Spec | Test |
|---|---|
| B1 first write | `revisions::tests::first_write_identical_then_changed` |
| B2 identical no-op | same |
| B3 changed body | same |
| B4 cap 20 | `revisions::tests::cap_keeps_twenty_newest` |
| B5 save_rule hook | `revisions::tests::save_rule_skips_noop_and_records_change` |
| B6 zip restore source | `revisions::tests::restore_source_is_recorded` + wiring grep `record_restore_writes` in web + CLI |
| B7/B8 HTTP | wiring greps for `/revisions` routes (no HTTP integration harness) |
| B9 History UI | `PolicyRevisionHistory` in rule/skill editors; `policyRevisions.test.ts` |
| Schema v20 | `cargo test -p ax-db --test migration_v20` |

## Gauntlet (fresh `bash tools/gauntlet-policy-revisions.sh`)

| Layer | Result |
|---|---|
| migration_v20 | `1 passed` |
| `cargo test -p ax-policy revisions` | `5 passed` (125 other lib tests filtered) |
| web helpers | `2 passed` (`policyRevisions.test.ts`) |
| wiring greps | pass |
| negative control | pass (`policy-revisions-table-does-not-exist` absent) |
| `npx tsc --noEmit` | pass |
| manual mutants | **3/3 killed** (skip-identical, cap-21, restore-as-save) |

## Skipped

- Changed-line coverage gate: no `--cov-fail-under` for these crates; unit tests execute the new `record_if_changed` / prune / save hook paths.
- Property-based tests: N/A (small enum of sources + cap).
- Browser e2e: no browser MCP in this session. Restart `ax web` and hard-refresh to exercise **History**.
- Supply chain: no new dependencies.
- Independent verification: not performed (Tier 2).

## Known limits

- Disk edits that never call `save_rule` / `save_skill` / zip restore are not logged.
- Rename starts a new `(kind, id)` log; old rows stay under the previous id.
- History is local SQLite, not git.
