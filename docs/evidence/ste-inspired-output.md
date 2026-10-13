# Evidence — STE-inspired preflight output

Historical experiment: withdrawn at the owner's request on 2026-10-13. The current product no longer supports this optional style. This report records the original implementation, not current functionality. To reproduce its original checks, use implementation commit `2294a8ab6fcce44226ff2910c54242d9b75c3a81`. The subsequent token investigation isolated 67–68 extra o200k projection tokens after removing a three-token session-label bias and incidental epoch noise; whole-agent savings remained unverified. See `remove-ste-inspired-output.md` beside this report for removal verification.

Date: 2026-10-13. Tier 3. Implementation is an opt-in presentation change.

- SPEC approval: obtained. The user approved the displayed plan with “Do it” and then explicitly authorized “Implement, test, merge”.
- Published implementation source: `2294a8ab6fcce44226ff2910c54242d9b75c3a81`, tree `cb36150bc96b5f18ddaa4d27b9980c7d307091a5`. This tree is byte-identical to tested local commit `3de29cd22e99450f128fe528f2522f8b829dae31`; GitHub returned the exact expected tree SHA. The evidence commit adds reports and an append-only SPEC record, with no executable code changes.
- Baseline: `94975f9cfe39f164f14db254973fc860d848eb96`.
- Isolation: a separate feature worktree; the user's working tree and `.ax` data were not moved or modified.
- Toolchain: Rust/Cargo 1.99.0, Python 3.12.14, cargo-llvm-cov 0.9.1, Node 24.19.0. Exact Rust build identifiers are retained in `ste-output-metrics.json`.
- Independent fresh-context verification: not performed. The recorded review is the implementation agent's review; it does not provide independent assurance.

## Reproduce

Run from a clean checkout with Cargo, rustfmt, Clippy, cargo-llvm-cov and its matching `llvm-tools-preview` component installed:

```bash
bash tools/gauntlet-ste-output.sh
```

The executor used an isolated Rust toolchain and explicit existing OpenSSL locations (`OPENSSL_INCLUDE_DIR=/usr/include`, `OPENSSL_LIB_DIR=/usr/lib/x86_64-linux-gnu`). It also set `CARGO_BUILD_JOBS=4`, `CARGO_PROFILE_DEV_DEBUG=0`, and `CARGO_PROFILE_TEST_DEBUG=0`. These are executor build settings, not new production dependencies.

The entry point requires a clean committed tree, records the commit and tool versions, removes stale report files, and exits nonzero for a failed layer. Reports default to ignored `target-dev/ste-output-evidence/`. The metrics JSON beside this report contains the actual final-run coverage, executed mutation hashes and paired measurements.

## Final verification

| Layer | Actual command / method | Result |
| --- | --- | --- |
| Affected runtime suites | `cargo test -p ax-mcp -p ax-policy --locked --no-fail-fast -- --test-threads=1` | 487 passed, 0 failed, 0 ignored; includes 17 new contract checks |
| Workspace compile/types | `cargo check --workspace --locked` | exit 0 |
| Workspace lint | `cargo clippy --workspace --all-targets --locked -- -D warnings` | exit 0; no Clippy errors |
| New Rust file formatting | rustfmt check on the style module, contract tests and measurement example | exit 0 |
| Evaluation and coverage checkers | `python3 -m unittest discover -s tools -p 'test_*ste_output*.py' -v` | 8 passed; includes negative and positive CLI controls |
| Changed executable line coverage | `cargo llvm-cov -p ax-mcp --locked --json ...` plus `tools/check_ste_output_coverage.py` | 156/156, 100%; `lib.rs` module declaration is listed separately, not counted as executed code |
| Manual mutation substitute | `python3 tools/ste_output_mutants.py --output ...` | 5/5 selected real bugs killed; every source modification hash recorded; source restored |
| Property checks | deterministic UTF-8/body/envelope cases with 16 repetitions and both profiles | 192 body/profile combinations pass; exception-rewrite mutant also fails the property suite alone |
| Post-mutation regression | contract suite rerun on restored source | 17 passed, 0 failed |
| Real MCP execution | `cargo run -p ax-mcp --example evaluate_ste_output --locked` | 90 real dispatcher requests; required-source delivery checks pass; no renderer model calls |
| Full structured escape hatch | `AX_MCP_FULL=1 cargo test -p ax-mcp --locked --test ste_output_contract s03_style_is_independent_of_transport -- --exact` | 1 passed, 0 failed |
| Website tests | `npm test` in `site/` | 55 passed, 0 failed |
| Local documentation build | `npm exec -- astro build` in `site/` | exit 0; 46 pages built |
| Diff / capability review | source diff, dependency manifests and write paths reviewed; `git diff --check` | exit 0; no new production dependencies, provider calls or credentials |

The 5 mutations accept invalid style, remove validation before context reset, enable metadata for the wrong profile, duplicate canonical policy bodies into metadata, or rewrite a source exception. A compile failure is not credited as a killed mutant: each must reach a named failing behavioral assertion. The mutation runner restores each original source file byte-for-byte.

The checker controls reject incomplete pairs, duplicates, failed source preservation, nonfinite/negative values, missing executable coverage and unreadable reports. A synthetic zero-count region exits nonzero; a covered region exits zero. Declaration-only exemption is tested against executable text to prove it cannot silently exempt a missing function.

## SPEC → test mapping

Contract tests are in `crates/ax-mcp/tests/ste_output_contract.rs`.

| Scenario | Test | Status |
| --- | --- | --- |
| S01 | `s01_legacy_default_is_unchanged` | pass |
| S02 | `s02_explicit_standard_matches_legacy` | pass |
| S03 | `s03_style_is_independent_of_transport` | pass |
| S04 | `s04_invalid_style_has_no_delivery_effect` | pass |
| S05 | `s05_protected_source_is_exact` | pass |
| S06 | `s06_conditions_and_uncertainty_survive` | pass |
| S07 | `s07_required_overflow_is_honest` | pass |
| S08 | `s08_expansion_returns_full_source` | pass |
| S09 | `s09_profile_change_invalidates_presentation` | pass |
| S10 | `s10_reset_restores_required_context` | pass |
| S11 | `s11_project_boundary_is_preserved` | pass |
| S12 | `s12_final_projection_is_measured` | pass |
| S13 | `s13_metadata_does_not_duplicate_bodies` | pass |
| S14 | `s14_rendering_is_deterministic` | pass |
| S15 | `s15_schema_exposes_only_optional_supported_styles + Python coverage/evaluation negative controls` | pass |

Additional regression checks cover history fields and the requirement to follow matched skill workflows **before** editing. Project isolation includes a positive owner retrieval of the valid foreign handle before testing denial/absence in the other project.

| Invariant | Evidence | Status |
| --- | --- | --- |
| Existing default and public fields | S01, S02, S15 and source/API review | pass |
| Canonical policies, source, exceptions and uncertainty | S05, S06, S08 and UTF-8 properties | pass |
| Required policy survives budget pressure | S07 | pass |
| Reference/omission does not suppress full retrieval | S08, S10 and existing budget/session suites | pass |
| Profile switches use the requested presentation | S09 and content-hash-based helper delivery | pass |
| Project ownership remains enforced | S11 and existing project isolation suites | pass |
| Metadata stays compact and measured | S12, S13 and duplication mutant | pass |
| No runtime rewrite model or dependency additions | source and manifest diff review | pass |
| User `.ax` data remains intact | isolated feature worktree; test data in owned temporary projects | pass within the local task scope |
| No conformity, billed savings or task-quality claim | docs and evaluation report labels | pass |

## Review rounds

Usable workflows: `old-coder`, `old-coder-api`, `review-loop`, `rust-review` and `python-review`.

| Round | Findings | Resolution |
| --- | --- | --- |
| 1 | 1 major: simplified wrapper did not explicitly retain skill workflow timing | RED test observed failing; wording fixed; targeted GREEN and complete gauntlet rerun |
| 2 | 2 test-assurance findings: ownership lacked a positive retrieval control; a multiline body absence check was ineffective after JSON escaping | Positive owner control added; metadata field/marker checks strengthened; duplication mutant added; complete gauntlet rerun |
| 3 | 0 | Review completed against the final implementation source |

Earlier harness checks also corrected declaration-only coverage handling, source-state recording and asynchronous IO in the measurement example. No review finding is open. No finding was dismissed.

API scope is an existing public MCP/JSON surface. The optional argument preserves old defaults, tool names, transport and field types. Existing authentication is unchanged; project-bound authorization is covered by the actual contract suite. This does not certify every existing HTTP authentication, rate-limit or security implementation. No new endpoint, destructive action, pagination or expensive default field was introduced.

## Measured output overhead

Three fixtures × five paired repetitions × two styles × cold/warm/reset = 90 requests. Rows below are median **text plus serialized wire metadata** tokens. These are not provider-billed input counts.

| Fixture | Turn | Standard | STE-inspired | Change |
| --- | --- | ---: | ---: | ---: |
| simple | cold | 5175 | 5245 | +1.353% |
| simple | warm | 445 | 516 | +15.955% |
| simple | reset | 5176 | 5245 | +1.333% |
| access | cold | 5173 | 5244 | +1.373% |
| access | warm | 442 | 514 | +16.29% |
| access | reset | 5174 | 5244 | +1.353% |
| replication | cold | 5201 | 5270 | +1.327% |
| replication | warm | 452 | 522 | +15.487% |
| replication | reset | 5199 | 5271 | +1.385% |

The guidance and presentation metadata increase this projection. The feature therefore stays opt-in. Agent task correctness, fewer recovery rounds, model output tokens, provider-billed costs and monthly savings remain **unverified**. The summarizer can accept separately supplied real agent runs and dated EUR prices for the 50/100/200/500-cycle, 22-workday comparison; this task ran no such paid evaluations.

## Limits and blocked layers

- **BLOCKED / substituted:** automatic approval review rejected the full workspace runtime run because an unrelated CLI startup path contacted OpenRouter with outbound contents not established in the transcript. It was not retried as a workaround. Complete affected MCP/policy suites, workspace compilation and workspace Clippy ran instead. This is narrower than a full workspace runtime pass.
- **Inherited formatting failure:** baseline `cargo fmt --all -- --check` exits 1 across 181 existing files. New Rust files pass formatting; this task does not claim the inherited workspace formatter gate is green.
- **SUBSTITUTED property tooling:** deterministic property generators reuse existing dependencies; no general random/fuzz engine was added. The recorded cases cannot prove every possible string or state sequence.
- **SUBSTITUTED mutation tooling:** no project mutation tool was present; 5 persisted manual bugs are narrower than exhaustive syntax-tree mutation. Only the exception mutant was separately checked against the property suite; that result is not attributed to unrelated properties.
- **UNVERIFIED suite order / platform matrix:** stable serial execution does not establish randomized-order independence. Runtime checks were Linux x86-64; Windows/macOS runtime execution was not performed here.
- **UNAVAILABLE Python static/lint configuration:** these scripts use the existing standard-library/unittest approach. No configured Python type checker or linter was added; their type annotations are not a claim of a strict checker pass.
- **N-A dependency-set audit:** production/test-library dependency sets and lockfiles are unchanged. Executor coverage tooling was installed separately and its version is recorded.
- **Not dispatched:** GitHub CI is manual-only under the repository instructions. No workflow trigger, release, Netlify build or deployment was restored or started.
- **Structural blind spot:** renderer/transport correctness and preserved source do not prove a model follows the writing guidance or makes better coding decisions.

## Honest notes

The initial RED run showed 10 failing new behavior checks and 5 already supported behaviors. Fresh workspace setup first failed for missing OpenSSL discovery; existing headers/libraries were used after executor package installation was unavailable. A history fixture initially used an unsupported detection phrase; it was changed to an existing recognized input without weakening its assertions. Review timing regression was separately observed RED before its wording fix. All final numbers above come from the last implementation state; prior trial measurements were not reused as the final evidence.

Publication used the connected GitHub API after terminal Git reported missing credentials. Ownership, public visibility and admin/push permissions were verified against the connected `GaryWenneker` account. The publicly reachable implementation commit above reproduces the exact tested source tree.
