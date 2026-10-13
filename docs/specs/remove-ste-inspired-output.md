# Remove the STE-inspired preflight experiment

Date: 2026-10-13. User instruction: “I suggest keeping it out.” This supersedes the earlier decision to implement the optional writing style. Implementation and merge were authorized earlier in this task. Separate SPEC review was not obtained for this focused reversal (autonomous run); confidence must not imply independent approval.

Baseline: main `7cd03a58f250988555fb8c2f88b92e1f6967a893`. Isolation: feature worktree `revert/ste-inspired-output`, so other worktrees and user project data are preserved. Tier 2 for a targeted reversal, with explicit public MCP compatibility checks.

## Contract and failure model

- R01: `tools/list` no longer advertises the optional `textStyle` property on `ax_preflight`.
- R02: a normal preflight contains standard policy instructions and has no STE guidance or presentation metadata. This must hold for raw handler, MCP server and embedded invoke adapter.
- R03: an obsolete `textStyle` argument is treated as an unknown argument under the pre-experiment permissive contract. It cannot enable a style or cause the experimental validation error. Exercise standard, ste-inspired, invalid string, null and numeric values.
- R04: cold and reset requests deliver the required fixture policy exactly; a warm request deduplicates it. Removal does not disable policy delivery, context reset or existing project isolation.
- R05: runtime Rust files touched by the original experiment return byte-for-byte to their pre-merge state. Preserve the subsequent OpenSSL change and all unrelated current-main changes.
- R06: README and current site documentation no longer advertise the removed feature. Delete its runtime module, dedicated experiment tests/example and now-inapplicable benchmark/gauntlet helpers. Existing unrelated tests are not weakened.
- R07: keep the original SPEC, evidence and metrics as historical records, explicitly marked as withdrawn. Retain the standalone detailed research report and evidence bundle already delivered.

Failure risks: partial removal leaves an enabling adapter or schema; an obsolete argument produces an experimental validation error; a blanket revert loses unrelated changes; stale site docs instruct users to select the removed mode. Address these with removal regression tests, exact source comparisons, a targeted merge revert and documentation checks.

## Execution and setup

Use the installed Rust/Cargo and Node toolchains, existing cached dependencies and a shared ignored build directory. No production dependencies, lockfiles, workflows, releases, deployments or user `.ax` data are changed. Keep the current OpenSSL vendoring configuration. New source/tooling paths: `crates/ax-mcp/tests/preflight_standard_contract.rs`, `tools/gauntlet-ste-removal.sh`, and `docs/evidence/remove-ste-inspired-output.md`.

Write the new removal checks and observe RED on the feature-enabled baseline. Reverse only PR #118 with `git revert --no-commit -m 1 238ce27c00550f9ac37a05fc4ab0ef62e9122496`, then restore historical SPEC/evidence/metrics. Run GREEN and the complete affected MCP/policy runtime suites. Run workspace compile and Clippy, scoped formatting, site tests and local documentation build. Record coverage/mutation and suite-health limits honestly. Use the original implementation commit as a persisted real mutation: feature-enabled source must fail the removal suite. No provider evaluation or full CLI runtime suite is run because the earlier broad suite contacted OpenRouter.

Review with the usable Old Coder, API, review-loop and Rust review skills until no finding remains. Publish a PR whose tree matches the locally verified source, then merge using the expected head SHA. GitHub CI remains manual-only and is not dispatched.


## R1 — preserve applicable coverage tooling

The original generic changed-line coverage checker and its unchanged behavioral controls remain useful for the restored MCP code. Retain them under neutral names `tools/check_mcp_removal_coverage.py` and `tools/test_mcp_removal_coverage.py`. Add `tools/ste_removal_mutants.py` for three partial-removal mutations. No dependencies are added. Four removal tests were observed failing on the feature-enabled source, then passing after reversal.
