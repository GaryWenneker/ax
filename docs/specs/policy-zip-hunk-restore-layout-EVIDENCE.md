# EVIDENCE: Restore inspect modal size + Old/New labels

**Spec:** `/Users/gary/io/ax/docs/specs/policy-zip-hunk-restore.md` H8  
**Spec approval:** not obtained (autonomous run — user screenshot of crushed Id column)  
**Tier:** 1 (layout; merge semantics unchanged)  
**Entry point:** `bash tools/gauntlet-policy-zip-package.sh`  
**Source state:** working tree on `main` after H8 crush fix; parent commit `4c7a89d24edd3f1d0d60eb4c39ca77459331b68c`

## Behavior mapping

| Spec | Check | Result |
|------|-------|--------|
| H8 restore `full` after preview | grep `size={items.length > 0 ? 'full' : 'md'}` | pass |
| H8 CSS | grep `.ax-modal--full` | pass |
| H8 Old/New above code | grep `policy-pack-hunk-age`, `>Old<`, `>New<` | pass |
| H8 file list min width | grep `minmax(22rem, 26rem)` | pass |
| H8 Compare hidden while inspecting | grep `policy-pack-split--with-diff .policy-pack-preview th:nth-child(3)` | pass |
| H8 Id not letter-wrap | fail-closed: `overflow-wrap: anywhere` on Id column is a gauntlet FAIL | pass |
| Must not shrink compose | grep `size="xl"` still present | pass |

## Gauntlet (fresh run after last code edit)

Command: `bash tools/gauntlet-policy-zip-package.sh`

| Layer | Result |
|-------|--------|
| Full zip_package tests | **24 passed**, 0 failed |
| Web helpers | **14 passed**, 0 failed |
| Wiring greps | pass (including H8 crush greps) |
| Negative control | pass (`ax-policy-package-does-not-exist` absent) |
| tsc | `npx tsc --noEmit` exit 0 |
| Manual mutation | 4/4 killed (engine mutants; layout has no merge mutants) |

Skipped: changed-line coverage tool, property tests, cargo-audit (no new deps). Browser E2E not in this gauntlet — verify locally: restart `ax web`, restore demo zip, click a changed row.

## Known limits

Modal max width is 1760px. Compare is hidden in the table only while a row is selected (status remains in the inspect title).
