# Evidence — withdraw STE-inspired preflight

User decision on 2026-10-13: “I suggest keeping it out.” The optional experiment is removed from runtime and current documentation. Original research records remain explicitly historical. This report supersedes the opt-in rollout decision, while retaining its measurements and source history.

- Baseline main: `7cd03a58f250988555fb8c2f88b92e1f6967a893`.
- Tested local implementation: `a95fb76a79347205959a7880e14c8f3a8a7ae114`, tree `4a9255719f6732bc672e409533c5c0deccb568ea`.
- Published implementation: `e278a087b04be71771d33759223d5483eaf08b8b`, with the exact same Git tree. GitHub returned the expected tree SHA, establishing byte-for-byte identity of the tracked tested and published source.
- SPEC: `docs/specs/remove-ste-inspired-output.md`. Separate SPEC approval: not obtained (autonomous targeted reversal). The latest removal instruction and earlier implementation/test/merge authorization define the task scope; no independent approval is implied.
- Isolation: a new feature worktree. No user `.ax` data was moved, deleted or changed. Later main changes, including OpenSSL vendoring, are preserved.
- Rust/Cargo 1.99.0, Python 3.12.14, cargo-llvm-cov 0.9.1, Node 24.19.0. No production manifest or dependency-lock change is published.

## Final results

| Layer | Actual check | Result |
| --- | --- | --- |
| RED | Four new removal contracts on feature-enabled main | 4 failed, 0 passed; behavioral failures in schema and all three adapters |
| Complete affected runtime suites | `cargo test -p ax-mcp -p ax-policy --locked --no-fail-fast -- --test-threads=1` | 471 passed, 0 failed, 0 ignored |
| Workspace types/compile | `cargo check --workspace --locked` | exit 0 |
| Workspace lint | `cargo clippy --workspace --all-targets --locked -- -D warnings` | exit 0; existing asset-embedding build-script notice is not a Rust lint failure |
| Scoped formatting | rustfmt on `preflight_standard_contract.rs` | exit 0 |
| Coverage checker controls | unittest on `test_mcp_removal_coverage.py` | 4 passed; includes missing source, zero-count region, unreadable report and positive CLI controls |
| Changed executable line coverage | LLVM JSON plus `check_mcp_removal_coverage.py --base 7cd03a5...` | 33/33, 100%; line coverage, not branch coverage |
| Three partial-removal mutations | `python3 tools/ste_removal_mutants.py` | 3/3 killed by named behavioral failures; changed-source hashes recorded; original bytes restored |
| Post-mutation contracts | Normal projection contract suite | 4 passed, 0 failed |
| Full structured projection | `AX_MCP_FULL=1` contract suite | 4 passed, 0 failed |
| Website tests | `npm test` in `site/` | 55 passed, 0 failed |
| Local documentation build | `npm exec -- astro build` in `site/` | exit 0; 46 pages |
| Source restoration | Compare six affected runtime Rust files against pre-experiment commit `b8d832b...` | byte-identical; withdrawn module absent |
| Current docs and unrelated changes | Current documentation marker checks; manifest/lockfile diff; `git diff --check` | pass; OpenSSL fix unchanged |

The difference from the prior 487-test count is intentional: the withdrawn feature's 17 integration checks and 3 module tests are removed; 4 removal contracts replace them. Unrelated existing assertions are unchanged.

## Contract mapping

| SPEC condition | Evidence |
| --- | --- |
| R01 selector absent | `preflight_schema_has_no_writing_style_selector`; schema mutant killed |
| R02 standard output in each adapter | Raw, wire and embedded contract checks; guidance mutant killed |
| R03 obsolete arguments do not enable style or validation | Strings, unknown value, null and number exercised; embedded-validation mutant killed |
| R04 exact source and cold/warm/reset delivery | `wire_preflight_preserves_cold_warm_reset_delivery`; required Unicode/conditional source assertions; complete existing context/project-isolation suites |
| R05 focused reversal | Six runtime files exactly match the pre-experiment state; manifests and OpenSSL change match baseline main |
| R06 no active implementation or documentation | Module/example/experiment-helper deletion; current README and site marker checks |
| R07 historical evidence preserved | Original metrics unchanged; original SPEC/evidence marked withdrawn; standalone token research report and bundle retained |

## Review rounds

| Round | Usable skills | Findings | Resolution |
| --- | --- | --- | --- |
| 1 | old-coder, old-coder-api, review-loop, rust-review, python-review | 2 minor verification-helper findings | Annotate the mutation entry point and add a bounded subprocess timeout; fixed before final tested state |
| 2 | same | 0 | No finding remains open |

The API is the existing public MCP/JSON surface. Removal of its newly introduced optional selector and presentation field is intentional under the user's decision. Restoring permissive handling of unknown arguments lets older calls with `textStyle` receive standard output without an experimental validation failure. Tool names, baseline field types, authentication, project ownership, policy delivery, transport and default output retain their pre-experiment implementation. This is a focused reversal, not a general security certification.

## Reproduce

Use a clean checkout at the published implementation above, with the installed tools and matching dependencies:

```bash
bash tools/gauntlet-ste-removal.sh
```

The executor used an isolated Rust toolchain, a shared ignored Cargo target directory, 4 build jobs, disabled dev/test debug symbols, and existing Linux OpenSSL locations. Website dependencies were copied from the existing local installation into the isolated worktree. No Netlify build or deployment, GitHub workflow dispatch, release or provider evaluation was performed.

The final checks all ran on the exact implementation state above. The gauntlet was resumed at the coverage layer after an instrumentation-cache failure, then mutation, post-mutation, full-projection and documentation layers were completed on the same unchanged source. On an executor that reused instrumented artifacts from another worktree, `cargo llvm-cov clean --workspace` was required before rerunning the persisted coverage command; this removed the `colliding StableCrateId` build failure without changing source or assertions.

Machine-readable line coverage and executed mutation records are in `remove-ste-inspired-output-metrics.json` beside this report. Every verification command and mutation/checker source is persisted in the repository.

## Honest limits and setup corrections

- The initial broader run completed the affected suites but failed because the verification script was edited while Bash was executing it. It was not credited as a completed gauntlet. The script was committed, a fresh run started, and all final numbers above came after the final code edit.
- Shared instrumentation artifacts caused a coverage-only crate-ID collision. Workspace coverage artifacts were cleaned, instrumentation recompiled, and coverage passed; no coverage threshold was weakened.
- The web build step adjusted only two root version metadata entries in its package lockfile. Those unrelated generated changes were restored; dependency resolution was unchanged and no lockfile diff is published.
- Full workspace runtime/provider testing remains unverified. Its earlier unrelated CLI startup path contacted OpenRouter and was rejected by automatic approval review. This reversal runs complete affected MCP/policy suites and workspace compile/lint instead; that is not a full runtime/CI pass.
- Repository-wide formatting has inherited differences documented in the original evidence. Scoped new-file formatting passes; no global formatter pass is claimed.
- Randomized suite order, Windows/macOS runtime, exhaustive mutation and independent fresh-context verification are unverified/not performed. The three manual mutations cover specific incomplete-removal bugs rather than every possible mutation. No new parser/state-machine algorithm was added; existing suites and explicit argument/state cases supply the relevant property coverage.
- No configured Python type checker or linter is claimed. Standard-library helper controls and real mutation execution pass. Production dependencies are unchanged, so no new dependency-set audit is claimed.
- GitHub automation remains manual-only under the repository instructions. Local success does not claim a deployed release or green hosted CI run.
