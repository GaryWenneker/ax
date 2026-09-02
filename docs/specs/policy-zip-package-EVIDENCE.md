# EVIDENCE: Sitecore-style policy zip packages

**SPEC:** `/Users/gary/io/ax/docs/specs/policy-zip-package.md`
**spec approval:** B1–B5 approved 2026-09-02 (user: "approve"). B6–B7: user requested larger modal, select-all, skill descriptions, visual local vs package compare, and git-style row diffs (this turn).
**command:** `bash /Users/gary/io/ax/tools/gauntlet-policy-zip-package.sh`
**CLI note:** `ax policy pack zip`; restore is `ax policy restore`.
**source:** working tree (uncommitted B6/B7 UI + `/api/policy/package/diff`)

```
== rust unit tests ==
  6 passed (zip_package)
== web helpers ==
  5 passed (policyPackage.test.ts)
== wiring ==
== negative control ==
== tsc ==
== manual mutation ==
killed wrong-kind
killed default-overwrite
gauntlet-policy-zip-package: ok
```

| Behavior | Check |
|----------|--------|
| B1 | `PolicyZipPackageButtons` + `ModalShell` on Rules and Skills |
| B2 | pack test includes selected files; private/disabled → Unknown |
| B3/B4 | preview new/conflict; skip default; overwrite writes |
| B5 | `ax policy pack zip`, `ax policy restore` in cli.md |
| B6 | `size="xl"`, Select all / Select none, skill + generated rule descriptions (`policyItemDescription` / `summarize_item_description`) |
| B7 | preview `compare` new/identical/changed; `unified_diff` + `diff_policy_zip_item`; POST `/package/diff`; restore badges + unified diff pane |
| Zip-slip | preview rejects `../` paths |

**API gates (internal `/api/policy/package*`):** Boring ✓ (multipart zip, not JSON REST — same as preview/restore). Compatibility ✓ (additive `compare`/`summary` on preview; new `/diff`). Auth N/A localhost CC. Idempotency N/A (read). Blast radius ✓ 8 MiB cap. Pagination N/A. Expensive fields ✓ diff on click not in preview. No leakage ✓ `local`/`package` labels.

Skipped: browser E2E (restart `ax web` + hard refresh). Independent verification: not performed (Tier 3 downgrade). Relink Mach-O after embed (`scripts/reinstall-cli.sh`).
