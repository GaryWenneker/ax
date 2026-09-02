# EVIDENCE: Sitecore-style policy zip packages

**SPEC:** `/Users/gary/io/ax/docs/specs/policy-zip-package.md`
**spec approval:** B1–B5 approved 2026-09-02 (user: "approve"). B6–B7: earlier turn. **B8 approved 2026-09-02 (user: "approve")**.
**tier:** 2
**command:** `bash /Users/gary/io/ax/tools/gauntlet-policy-zip-package.sh`
**source:** working tree after B8 (engine + restore UI + docs)

Fresh gauntlet run after last code edit:

```
== rust unit tests ==
  14 passed (zip_package)
== web helpers ==
  7 passed (policyPackage.test.ts)
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
| B6 | `size="xl"`, Select all / Select none, skill + generated rule descriptions |
| B7 | preview `compare`; `unified_diff` + `diff_policy_zip_item`; POST `/package/diff` |
| B8 new action | `restore_new_honors_skip`; UI Skip/Install; `restore_new_default_installs` |
| B8 newer | `preview_new_has_newer_none`; `preview_changed_local_newer`; `preview_changed_package_newer`; `preview_legacy_zip_without_mtime_is_unknown`; `restore_explicit_overwrite_when_local_newer`; `newerLabel` |
| Zip-slip | preview rejects `../` paths |

**API gates (internal `/api/policy/package*`):** Boring ✓. Compatibility ✓ additive `newer` on preview items, optional `mtime` on manifest paths; restore `decisions` map unchanged (`overwrite` now honored for `new`). Auth N/A localhost CC. Idempotency N/A. Blast radius ✓ 8 MiB cap. Pagination N/A. No leakage ✓.

Skipped: browser E2E (restart `ax web` after web-ui rebuild). Independent verification: not performed (Tier 2). Mutation: 2/2 hand mutants killed via gauntlet script (file restored after).
