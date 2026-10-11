# EVIDENCE: Restore package — per-change (hunk) Accept

**Spec:** `/Users/gary/io/ax/docs/specs/policy-zip-hunk-restore.md`  
**Spec approval:** obtained 2026-09-03 (user: "approve")  
**Tier:** 2  
**Entry point:** `bash tools/gauntlet-policy-zip-package.sh`  
**Source state:** working tree (uncommitted hunk-restore changes on `main`; identify with `git status` / `git diff`)

## Behavior mapping

| Spec | Test | Result |
|------|------|--------|
| H1 distant hunks | `split_lcs_into_hunks_separates_distant_edits` | pass |
| H3 merge first hunk | `merge_accepts_first_hunk_keeps_second_local` | pass |
| H3 restore merge | `restore_merge_accepts_first_hunk_keeps_second_local` | pass |
| H4 string overwrite | `restore_merge_string_overwrite_still_full_file` | pass |
| H4 JSON merge | `restore_merge_json_roundtrip` | pass |
| H4 invalid index | `restore_merge_invalid_hunk_index_errors` | pass |
| H2 Partial / API coerce | `restoreFileActionLabel is partial when some hunks are accepted` | pass |
| H2 Change N of M | `changeNavLabel is 1-based Change N of M` | pass |

## Gauntlet (fresh run after last code edit)

Command: `bash tools/gauntlet-policy-zip-package.sh`

| Layer | Result |
|-------|--------|
| Full zip_package tests | **24 passed**, 0 failed (`cargo test -p ax-policy zip_package`) |
| Web helpers | **14 passed**, 0 failed (`node --experimental-strip-types --test src/policyPackage.test.ts`) |
| Wiring greps | pass (including `acceptHunks`, hunk UI, Partial, Change N of M) |
| Negative control | pass (`ax-policy-package-does-not-exist` absent) |
| tsc | `npx tsc --noEmit` exit 0 |
| Manual mutation | killed `wrong-kind`, `default-overwrite`, `hash-mismatch-inverted`, `never-accept-hunk` (4/4) |

## API gates (old-coder-api)

Scope: **internal** · **existing** `POST /api/policy/package/restore` and `POST /api/policy/package/diff`

| Gate | Status |
|------|--------|
| Boring | ✓ same routes; additive `hunks` on diff; additive merge object on decisions |
| Compatibility | ✓ `"overwrite"` / `"skip"` strings unchanged |
| Authentication | N/A same local Command Center server |
| Authorization | N/A same as other `/api/policy/*` |
| Idempotency | ✓ same bytes + same decisions rewrite the same files; no key (low-stakes local write) |
| Blast radius | ✓ existing 8 MiB zip cap |
| Pagination | N/A |
| Expensive fields | ✓ `hunks` computed from files already in the zip |
| No implementation leakage | ✓ hunk indexes are the public LCS split, documented |

Invalid hunk index → `ZipPkgError::BadZip` → HTTP **400**.

## Layers skipped

- cargo-tarpaulin / diff-cover: not in this crate’s zip gauntlet; changed merge paths covered by named tests
- Property-based tests: not added (LCS merge covered by explicit two-hunk `.mdc` files)
- Browser e2e: not in gauntlet; UI helpers + Modal hunk markup grepped
- Independent verification: not performed (Tier 2)

**Follow-on (H6/H7, 2026-09-03):** line numbers + Local/Package checkboxes including both. Gauntlet re-run: **24** zip_package tests, **14** UI helpers, **4/4** mutants, `tsc` clean (`bash tools/gauntlet-policy-zip-package.sh`).

- Skill extra files are still whole-file; merge applies to `SKILL.md` only
- Compact two-column hunk strips, not a full Kaleidoscope window
