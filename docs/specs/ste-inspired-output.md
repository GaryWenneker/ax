# SPEC — STE-inspired output for AX

AX — Agent Xpansion · The context layer for AI agents

Date: 2026-10-13

Status: implementation plan. No implementation or gauntlet results are claimed.

## Decision

Add an optional STE-inspired writing style to AX's existing output pipeline. Keep language style separate from serialization format and context selection. Start with preflight and AX-generated instructions; extend to other generated explanations only after evaluation.

Use JSON metadata for machine-readable facts and explicit English for instructions. Reuse the existing compact output, expansion and token-budget components. Apply the style through authored templates and deterministic rendering. Runtime model calls for rewriting are outside this plan.

Describe the feature as **STE-inspired**. This plan does not claim formal ASD-STE100 conformity or a measurable percentage of conformity. Short sentences are useful only when their conditions, exceptions, actors and uncertainty remain correct.

## Old Coder workflow and scope

Use `SPEC → RED → GREEN → REFACTOR → GAUNTLET → REVIEW LOOP → EVIDENCE`.

Tier: **3**, because the change touches a public MCP response contract, mandatory policy delivery and project-bound context. This tier reflects the consequences of a missing instruction or incompatible response, even though the first implementation should be small.

The requested deliverable is this plan. Implementation begins under a separately approved specification, following the loaded Old Coder workflow. Record approval against this revision; do not treat approval of a different task as approval of this specification.

`old-coder-api` contributes HTTP/JSON compatibility and operational checks. MCP-specific result, error and tool-schema checks remain necessary; REST status-code guidance is not a substitute for MCP protocol tests.

## Observed baseline

These observations come from the connected AX project and indexed source inspected during planning. The remote index was not pinned to a Git commit. Refresh and verify them against current `main` before implementation.

| Observation | Evidence and implication |
| --- | --- |
| Output formatting already exists | `crates/ax-usage/src/output_format.rs`, `render_mcp_output`, lines 95–102: XML, plaintext, rich text and Markdown branches. Add a language style independently of these formats. |
| Policy compaction already exists | `crates/ax-policy/src/format.rs`, `format_inject_block_with`, lines 127–274, and `compact_rule_body`, lines 337–363. Reuse the current delivery pipeline. |
| Structured output already avoids some duplication | `crates/ax-mcp/src/server.rs`, `lean_structured`, lines 718–791: preflight keeps actionable metadata while text carries bodies. Do not introduce a second full body by adding structured fields. |
| Budget accounting includes multiple response surfaces | `crates/ax-mcp/src/context_budget.rs`, `projection_tokens_with_mode`, lines 9–17: counts text and serialized metadata. Measure both. |
| One live preflight exceeded its requested budget | Returned `finalTokens=12924`, `limit=12000`, `requiredMinimum=12924`, `overBudget=true`, using `o200k_base`. This is one sample, 924 tokens or 7.7% over budget, not a typical-turn estimate or billed-token measurement. |
| Session-bound follow-up failed in this connector run | Both skill calls carrying the issued session ID returned `Chat belongs to another connection`. Skill and graph retrieval without that reused session succeeded. Investigate session binding separately if reproduced; do not weaken connection ownership. Warm-session savings remain unverified. |

The policy formatter passes source text to its delivery tracker before choosing full or compact rendering. Inspect the tracker end to end before treating a source hash as proof that a complete body reached the client. The inspected snippet alone does not prove an existing defect.

## Proposed behavior

### 1. Pilot contract

Add an optional `textStyle` parameter to `ax_preflight`:

- Omitted or `standard`: reproduce existing presentation and defaults.
- `ste-inspired`: use the new templates for text AX authors itself.
- Any other value: use the existing MCP invalid-argument error contract. Validate before changing delivery state.

Leave `contextMode`, output-format settings, tool names and existing response fields unchanged. Add compact presentation metadata only for an explicit style request, using a schema reviewed against actual MCP clients. Proposed shape:

```json
{
  "presentation": {
    "textStyle": "ste-inspired",
    "profile": "ax-ste-inspired-1"
  }
}
```

The profile identifies the template revision for cache invalidation; it is not a conformity score. Request and response examples here describe planned behavior, not an existing AX feature.

### 2. Writing rules

Use one instruction per sentence. Name the actor and object when they could be unclear. Use active verbs and consistent project terms. Put conditions before their actions. Preserve prohibitions, permission levels, exceptions, quantities and uncertainty. Keep code identifiers exact.

Example of an AX-authored instruction pair:

```text
If replication configuration exists, hide the setup form.
If replication configuration does not exist, show the setup form.
```

Avoid universal word substitution. Replacing a technical term or dropping “may”, “only”, “unless” or a scope restriction can change the instruction.

### 3. Source preservation

Apply templates to summaries, retrieval guidance, warnings and other text generated by AX. Preserve stored policy bodies, memories, source code, error messages, paths, identifiers and quotations exactly.

Long policies can become shorter through explicitly authored and reviewed agent instructions. Treat that as a separate editorial change: compare each requirement and exception against the original, keep its source reference, and record approval. A heading extractor or sentence-length check cannot prove semantic equivalence. Arbitrary user prose remains unchanged when no approved concise representation exists.

### 4. Delivery and budget handling

Reuse existing scope checks and expansion tools. Mark delivered representations as full, approved concise, or reference-only; do not equate those states. Cache keys must distinguish source content, rendering profile, scope and context epoch where relevant.

A summary or retrieval pointer must not cause a subsequent request for the complete skill to be suppressed. After context reset, restore the required representations. Expansion returns the exact canonical source.

Preserve mandatory instructions even when they exceed the budget. Report `overBudget` and the required minimum honestly. The renderer must not silently truncate a prohibition, hide overflow or mark an incomplete response as complete.

## Failure model

| Failure | Required check |
| --- | --- |
| A rewritten rule changes permission or loses an exception | Exact fixtures, review of authored templates and protected-source equality tests. |
| A default or field change breaks an existing client | Legacy golden responses and MCP contract tests with existing request schemas. |
| A concise representation suppresses full delivery | Cold, warm, expansion, style-switch and reset tests; mutation of delivery-state handling. |
| Expansion exposes another project's content | Two-project authorization tests with valid and foreign references. |
| A budget trim removes critical policy | Oversized mandatory-rule fixtures and truthful overflow assertions. |
| Output savings are an accounting artifact | Measure final text plus metadata, expansion calls and actual model usage separately. |
| A custom checker passes after a crash or skipped fixture | Negative controls, malformed inputs and nonzero failure checks. |
| Output quality degrades despite passing renderer tests | Paired agent-task evaluation with executable task acceptance checks. |

## Acceptance scenarios

Each scenario becomes a named automated test. Scenario identifiers must appear in the final SPEC-to-test mapping.

| ID / proposed test | Concrete input | Expected result |
| --- | --- | --- |
| S01 `legacy_default_is_unchanged` | Fixed preflight fixture; omit `textStyle`. | Same stable response content, field types and selection as the baseline; exclude only documented volatile values. |
| S02 `explicit_standard_matches_legacy` | Same fixture; `textStyle=standard`. | Same legacy presentation and selection. |
| S03 `style_is_independent_of_format` | Same selected content under each supported output format; select `ste-inspired`. | Existing format behavior remains valid; the selected language templates change without changing facts or policy selection. |
| S04 `invalid_style_has_no_delivery_effect` | `textStyle=unknown`. | Existing invalid-argument error category; delivery state unchanged. |
| S05 `protected_source_is_exact` | A UTF-8 policy body containing “unless”, a code fence, `must not`, a URL and a path. | Canonical body and identifiers compare byte-for-byte; style templates add no source edits. |
| S06 `conditions_and_uncertainty_survive` | Approved template fixture with “If role is Reader, deny writes unless an explicit grant exists.” and a finding marked “unverified”. | Condition, actor, exception and uncertainty remain explicit; no success claim is added. |
| S07 `required_overflow_is_honest` | Required bodies larger than a fixed token budget. | All mandatory instruction content remains recoverable according to the delivery contract; overflow is reported; optional omission is explicit. |
| S08 `expansion_returns_full_source` | Reference-only or concise skill representation, followed by expansion. | Exact canonical skill body is returned and is not suppressed as already delivered. |
| S09 `profile_change_invalidates_presentation` | Same canonical source and session; switch between standard and STE-inspired. | Correct requested presentation; no stale response from the other profile. |
| S10 `reset_restores_required_context` | Warm session with delivery state, then `context_reset=true`. | Required content is restored; previous cache state does not suppress it. |
| S11 `project_boundary_is_preserved` | A caller authorized for project A requests a reference belonging only to project B. | Existing authorization denial; no B text or metadata is exposed. |
| S12 `final_projection_is_measured` | Fixed fixture with independently tokenized text and JSON metadata. | Reported projection follows the existing accounting contract, including added presentation metadata. |
| S13 `metadata_does_not_duplicate_bodies` | Explicit style request with a long protected policy. | New presentation metadata contains no copied policy or skill body. |
| S14 `rendering_is_deterministic` | Repeat the same inputs, profile, scope and source state. | Identical stable presentation; zero model calls from the renderer. |
| S15 `checks_fail_on_known_bad_input` | Remove a mandatory rule, corrupt a fixture or make a check input unreadable. | Relevant check exits nonzero; a crash or skipped input never becomes a pass. |

Use property-based checks for UTF-8 preservation, deterministic rendering, profile-aware cache behavior and metadata/body separation. Pair preservation checks with positive delivery checks so an implementation that returns nothing cannot pass.

## Setup and planned files

Implementation isolation: create a feature worktree from current `main`, proposed branch `feature/ste-inspired-output`. Keep the user's working tree and `.ax` data untouched. Rebuild ignored dependencies inside the isolated worktree and record differences relevant to execution.

No new production dependencies or subscriptions are planned. Reuse existing Rust, serialization and tokenization dependencies. Verify existing coverage, mutation and property-test tooling first. Any additional tool, dependency or paid evaluation must be named in a visible SPEC revision before it is installed or run.

At implementation approval, commit the SPEC under the repository's conventions. Keep checkpoint commits at completed GREEN/REFACTOR boundaries if accepted by those conventions. Publish and merge through the normal feature-branch PR flow; releases originate from `main`.

Proposed file inventory, to verify against the pinned tree before editing:

| Path | Planned purpose |
| --- | --- |
| `docs/specs/ste-inspired-output.md` | Commit this approved SPEC. |
| `crates/ax-mcp/src/server.rs` and the existing preflight handler/schema files | Optional parameter, presentation metadata and shared pipeline integration. Find exact handler paths through AX before editing. |
| `crates/ax-mcp/src/smart_output.rs` | Authored concise templates and reuse of current rendering helpers. |
| `crates/ax-mcp/src/context_budget.rs` | Reuse measurement; change only if new output reveals a verified accounting mismatch. |
| `crates/ax-policy/src/format.rs` | Profile-aware representation and delivery tests where required by the pilot. |
| `crates/ax-mcp/tests/ste_output_contract.rs` | Proposed MCP compatibility and source-preservation tests. |
| `crates/ax-mcp/tests/fixtures/ste-output/` | Fixed legacy, protected-source, overflow and project-boundary fixtures. |
| `tools/gauntlet-ste-output.sh` | Single reproducible entry point; reuse existing layer commands where possible. |
| `tools/evaluate-ste-output.py` | Paired evaluation runner; standard library where possible; real usage and toolchain records. |
| `docs/evidence/ste-inspired-output.md` | Final fresh-run EVIDENCE with review rounds and limits. |
| `site/src/content/docs/guides/smart-output.md` | Describe current behavior, style limits and examples. |
| `site/src/content/docs/reference/mcp-server.md` and `README.md` | Document the optional preflight style in the same feature change. |

Additional fixture or harness files require a recorded SPEC inventory update. The first release adds no settings page or persistent configuration. A validated follow-up can expose a project setting through the existing settings system.

## Implementation sequence

1. **Baseline and SPEC:** pin current `main`, verify the relevant graph against source, inspect first-party clients and record existing test failures. Finalize the handler paths and fixture inventory. Confirm session binding before warm-session evaluation.
2. **RED:** establish legacy regression armor, then demonstrate failing tests for the optional style, protected sources, representation delivery and truthful token accounting. If a new test already passes, prove it is meaningful with a temporary mutant and restore the tree.
3. **GREEN:** implement the smallest opt-in renderer change. Keep the current default. Do not introduce a second output framework or a runtime rewrite service.
4. **REFACTOR:** simplify template helpers while assertions remain fixed. Rerun the suite after each implementation refactor.
5. **GAUNTLET:** execute the layers below against the isolated source state, including real MCP execution and paired agent tasks.
6. **REVIEW LOOP:** load usable Rust and API review skills; fix findings, rerun the gauntlet and review until a round has zero findings. Record missing review skills honestly.
7. **EVIDENCE and rollout:** produce one report from a final fresh run after the last edit. Ship an opt-in pilot with its docs. Extend to other AX-generated text only if evaluation supports the change.

## Gauntlet and evidence requirements

Use the project's actual commands and supported feature/platform matrix. Candidate Rust commands are `cargo test --workspace`, `cargo check --workspace`, `cargo fmt --all -- --check` and `cargo clippy --workspace --all-targets -- -D warnings`; verify repository configuration before making them the runnable entry point.

Gate changed-line coverage with a failing threshold. Use `cargo-mutants` if available; otherwise persist and execute 3–5 meaningful manual mutants, verify each was applied and restore the original source. Report property-suite-only mutant results separately from whole-suite results.

Run real preflight, full expansion, reset and style-switch requests. Run MCP contract tests and project-boundary tests. Scan the diff for secrets and unintended capabilities. Run suite-order checks with supported tooling; disclose unavailable or substituted layers. Build touched documentation packages; frontend checks become applicable if a later revision adds UI.

The entry point must remove stale report artifacts, fail closed and record source SHA, commands and tool versions. A failed layer blocks completion. Existing unrelated baseline failures remain named rather than silently fixed or ignored.

EVIDENCE must map every scenario and invariant to a test or an explicit `unverified`/`n-a` reason. Include actual test totals, coverage, mutants, real request results, token/latency measurements, review rounds and the final clean review. Independent fresh-context verification is optional and has not been performed during planning; do not imply that a self-review provides it.

## Agent evaluation and cost comparison

Separate presentation effects from editorial effects:

- A: current standard output.
- B: STE-inspired AX templates with the same canonical rules and memories.
- C: B plus a small, explicitly reviewed concise policy fixture set. C tests a future editorial improvement, not a runtime rewrite capability in the pilot.

Use three tasks: a simple scoped change, a role/permission task and a replication task with conditions and exceptions. Run at least five paired repetitions per task and arm, with identical model version, task input, tool permissions and token limits. Counterbalance arm order. Use disposable project fixtures; no production code or user data is changed by the benchmark.

Record completed acceptance checks, policy violations, clarification/recovery rounds, all tool output and expansion tokens, actual model input/output/cached-input usage, latency and total cost. Internal AX tokenizer counts and provider-billed usage are separate columns. Do not substitute a readability score or an LLM's self-grade for task correctness.

Pilot release requires all deterministic safety and compatibility gates to pass. Broad default rollout is a later decision. Its proposed criteria are zero new policy violations, no lower paired task pass count, and either at least 15% lower median total task tokens or at least 20% fewer recovery rounds without higher total task cost. These are acceptance targets, not forecasts; the small pilot cannot establish universal superiority.

Report monthly cost from measured per-cycle usage for 22 workdays and the user's eight-hour workday scenarios:

| Cycles per day | Cycles per month |
| ---: | ---: |
| 50 | 1,100 |
| 100 | 2,200 |
| 200 | 4,400 |
| 500 | 11,000 |

Monthly estimate = measured mean cycle cost × monthly cycles, plus applicable fixed charges. Use provider prices verified on the evaluation date, including cache pricing and a dated currency conversion where needed. Eight hours describes workload; it is not itself a token-billing multiplier. Compare the estimates with €60/month and report unsupported combinations as such. No savings or prices have been measured for this proposed feature yet.

## Must NOT / invariants

- Change existing defaults, tool names, response-field types or authentication boundaries.
- Automatically rewrite stored policy, memories, source code or quotations.
- Omit an applicable critical instruction to satisfy a token target.
- Turn a reference-only response into a false claim of full delivery.
- Remove uncertainty or manufacture facts during simplification.
- Add duplicate full bodies to structured metadata.
- Move, rename, hide or delete the user's `.ax` folder or database.
- Install unlisted dependencies or run undisclosed paid evaluations.
- Claim formal STE compliance, billed-token savings or passed checks without evidence.

## References and limits

Loaded workflows: AX `old-coder`, `old-coder-api` and `review-loop`. SPEC and gauntlet templates were read from the available AX template checkout. Reconfirm reference versions against the implementation source state.

Official STE background: https://www.asd-ste100.org/STE_faq.html

Official standard/AI guidance: https://asd-ste100.org/STE_downloads.html

Karpathy's referenced post: https://x.com/karpathy/status/2105819303471976479

Current evidence consists of source inspection and one live preflight response. Implementation, automated contract tests, gauntlet checks and comparative model evaluations remain unverified.

## Revisions

- R1, 2026-10-13: initial plan. Reuse existing AX output infrastructure; preserve the default; limit the pilot to optional authored presentation; add source-preservation, delivery and compatibility scenarios and paired evaluation.

- R2, 2026-10-13: implementation approved by the user with “Do it” after R1 was displayed. Baseline pinned to `94975f9cfe39f164f14db254973fc860d848eb96`. The current handler and schema are in `crates/ax-mcp/src/tools.rs`; a separate `crates/ax-mcp/src/text_style.rs` module and `crates/ax-mcp/src/lib.rs` module declaration may be added for small rendering helpers and unit tests. This executor lacks Rust: install a minimal stable Rust toolchain in the task scratch directory, including rustfmt and Clippy, without changing the user's environment or production dependencies. Record exact versions in EVIDENCE. Coverage/mutation tools remain subject to availability; substitutions must be disclosed.

- R3, 2026-10-13: the fresh workspace baseline build requires executor system packages `pkg-config` and `libssl-dev` for the existing `openssl-sys` dependency. Install these build prerequisites in the executor; no repository dependency changes. Also register `crates/ax-mcp/src/invoke.rs` for early argument validation, `crates/ax-mcp/examples/evaluate_ste_output.rs` for model-free MCP measurement, and `tools/test_evaluate_ste_output.py` for fail-closed evaluation checks. S15 is covered by optional-schema assertions plus gauntlet negative controls; it remains a checker-failure requirement, not merely a schema check.

- R4, 2026-10-13: current Smart Output no longer exports or exposes the legacy format selector. S03 verifies the supported embedded and MCP-envelope presentations instead of testing dead legacy format code. System package installation is blocked by executor user/group permissions; existing OpenSSL headers and libraries are available, so build with explicit `OPENSSL_INCLUDE_DIR=/usr/include` and `OPENSSL_LIB_DIR=/usr/lib/x86_64-linux-gnu` rather than modifying system packages.

- R5, 2026-10-13: install the executor-only `cargo-llvm-cov` tool and the Rust `llvm-tools-preview` component for real changed-line coverage. Use existing dependencies for deterministic property generators; no new production or test-library dependency. Register `tools/ste_output_mutants.py` for a persisted manual mutation substitute when no project mutation tool is present, and `tools/check_ste_output_coverage.py` for fail-closed diff coverage. Record tool versions and negative controls. Model-free task fixtures measure MCP output only; agent correctness and billed usage stay unverified until real model runs are supplied.

- R6, 2026-10-13: automatic approval review blocked the full workspace runtime suite after an unrelated CLI init test contacted OpenRouter. The persisted gauntlet uses the complete affected MCP and policy suites as a declared substitute and keeps workspace compilation/Clippy. The full-workspace runtime layer and other-platform execution remain blocked/unverified; no external-provider test is retried as a workaround. Register `tools/test_ste_output_coverage.py` for the coverage gate negative controls. Baseline workspace Clippy passes; workspace formatting has pre-existing differences, so enforce formatting on new Rust files and disclose the inherited workspace-format failure.

- R7, 2026-10-13: the user explicitly authorized implementation, testing and merge. Review fixes require a clean committed source state before the evidence gauntlet, distinguish declaration-only module lines from executable coverage, and use asynchronous fixture IO in the measurement example. The coverage gate has positive and negative CLI controls, including rejection of missing executable source; declarations are listed explicitly. The final gauntlet runs workspace compilation and Clippy, complete affected MCP/policy runtime suites, and the existing local Astro documentation build. Broad runtime tests with external provider egress are not retried.

- R8, 2026-10-13: final evidence records tested implementation `3de29cd22e99450f128fe528f2522f8b829dae31` and adds `docs/evidence/ste-inspired-output.md` plus `docs/evidence/ste-output-metrics.json`. The opt-in release criteria passed for the affected surface; broad runtime/provider, randomized-order and cross-platform limits are disclosed rather than promoted to passes. No default rollout or billed model-quality/savings claim is made.

- R9, 2026-10-13: source published through the connected GitHub API because terminal Git lacks push credentials. Public implementation commit `2294a8ab6fcce44226ff2910c54242d9b75c3a81` has tree `cb36150bc96b5f18ddaa4d27b9980c7d307091a5`, exactly matching the tested local source tree. Evidence references retain the local SHA and add the reachable public SHA; executable code is unchanged.
