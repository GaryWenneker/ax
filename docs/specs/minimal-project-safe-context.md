# SPEC: Minimal, project-safe agent context

Status: proposed implementation plan; source audit completed, runtime isolation validation pending.
Date: 2026-10-10.
Repository: GaryWenneker/ax.
Audited source: main at 730699d82965d629fcd6530d29b211e34f8ae22e and the CI correction branch at 9547bd4426158c7c1336d4b565d1314659df039a.
Scope: preflight, project identity, session lifecycle, policy provenance, memory, graph retrieval and context measurement.
This document authorizes no implementation by itself. It records the requested plan.

## 1. Goal and evidence boundary

Keep each agent turn as small as practical without losing applicable constraints, task continuity or current source evidence. Context from project B must never enter project A automatically. Deliberately shared global policy must be distinguished from unrelated project content.

The remote source audit can inspect committed files, configuration and GitHub history. It cannot inspect the user's local SQLite databases, Cursor's active workspace, unpushed changes, actual MCP traffic or model request construction. The reviewed recent main history contains the preflight budget work dated October 3 and Smart Output/project isolation dated October 10. No October 9 commit was identified in the inspected recent main history. Match the user's yesterday/Cursor change to its actual local or remote SHA before claiming that exact change was validated.

No token savings, live project isolation result or provider cache benefit is claimed as measured here.

## 2. Current capabilities to preserve

| Capability | Source evidence | Action |
|---|---|---|
| Configurable context budget | ax-usage/src/budget.rs reads context.budgetTokens, with project override of global settings | Extend rather than replace |
| Prioritized context selection | ax-usage/src/context_plan.rs::select_context keeps HardRequired blocks and drops optional blocks whole | Preserve required semantics |
| Explicit project mismatch rejection | ax-mcp/src/tools.rs::resolve_preflight_cwd checks projectPath against MCP project root | Move validation to shared request boundary where needed |
| Project-scoped expansion and graph reuse | context_cache.rs uses project in IDs and reads; reuse_cache.rs scopes conversation with canonical project root | Preserve and test against hostile cross-project IDs |
| Small session notes | working_context.rs supports bounded snapshots, content hashes, stale detection, fork and handoff | Add reliable context-reset semantics |
| Session identity | chat_session.rs and server.rs resolve explicit session, recent hook marker and connection fallback | Remove ambiguous hook attribution |
| Policy delta delivery | policy_session.rs and ax-policy/src/format.rs track delivered hashes | Add agent-context epoch and tested rehydration |
| Graph-first instructions | .agents/rules/explore-before-grep.mdc and startup skill describe explore/node/expand workflow | Verify adapters actually follow it |
| Lean MCP output | smart_output.rs and server.rs project text and structured output separately | Measure complete returned payload |

## 3. Findings and risks

### F1 — Foreign project instructions are already committed

.agents/skills/feature-information/SKILL.md contains project-specific Hoornaarpreventie/Brevo instructions and paths. This is confirmed content contamination in the repository, not proof of a database retrieval leak. A project-local file can still be irrelevant because it was imported from another project.

Inventory all policy and memory sources with origin, scope, import path and content hash. Produce a review list of generic reusable items and foreign project items. Do not silently delete, rewrite or promote them to global policy. Unrelated skills must not match a generic Ax feature request.

### F2 — Project checks are not a unified request contract

The explicit projectPath check is present in preflight. Cache expansion is separately project-scoped, and project-local memory queries use the local database pool. These protections are valuable but do not establish end-to-end isolation across every tool, daemon, hook, policy hierarchy and import route.

Use one validated request context before session resolution and any stateful read/write. Audit graph, remember/recall/history, rules/skills, durable state, session operations, stash/expand and logging. Never rely solely on an ID that happens to contain a project prefix.

### F3 — A recent hook marker is not tied to a project or window

cursor_state.rs writes a session ID into the shared ~/.ax/active-cursor-session marker. resolve_chat reads that marker, and resolve_session accepts the hook before its connection fallback. Recency alone does not prove that the marker belongs to this window, project or conversation.

Treat this as an attribution risk requiring multi-window tests, not a confirmed cross-project memory leak: downstream project scoping may contain the damage while session continuity still becomes incorrect.

### F4 — Delivery is not proof that content survived compaction

PolicySessions tracks connection, chat ID, hashes and TTL. No explicit client context epoch is present in the inspected delivery state. A client can compact away rule bodies while preserving the session ID; the server may continue returning unchanged references.

The same distinction applies to graph reuse stubs. Correct cached data on the server does not establish that the model still has the earlier answer. Rehydrate essential policy after reset; expand missing graph answers on demand.

### F5 — The budget applies after a large required block is assembled

preflight groups policy, project/session instructions, working context, budget warnings and directive proposals into a single HardRequired block, then adds selected optional blocks. Consequently a tiny configured budget cannot shrink the required block. The omission explanation and over-budget warning are appended after selection and consume additional tokens.

Split blocks by semantic necessity. Count separators, mandatory footer and final model-visible metadata before final selection. Keep critical requirements complete and report an explicit required minimum when they alone exceed the budget.

### F6 — Snapshot comparison can ignore project differences and removals

context_plan.rs::compare_snapshots compares session and content hash without a project identity check. Its overlap logic can classify a subset of previous parts as Same when parts disappeared. This is a confirmed behavior of the comparison code, but its production effect depends on callers and must be traced before fixing.

Test project changes, part deletion, policy/config changes and graph changes explicitly. Never report unchanged across projects.

### F7 — Repeated cheap-looking work can still be expensive internally

Preflight recalls memories, reads related turns and may enumerate up to 100,000 memory rows for policy link expansion before final budget selection. This does not prove poor latency on the user's machine. Measure database reads, serialization work and elapsed time separately from output tokens, then replace full enumeration with targeted linked-ID retrieval where justified.

## 4. Required design

### 4.1 Project identity and provenance

Build a RequestContext with canonical project root/ID, connection identity, validated session ID and context epoch. Resolve it once at request entry. Reject an explicit mismatching project before reading or modifying state.

Canonicalization must work on Windows, macOS and Linux, including symlinks and path separators. Define behavior for moved repositories and separate worktrees; do not silently merge worktrees that have different source state.

Project memories, session notes, cache entries and transcripts remain local. Global rules/skills require explicit global scope and origin. Project-specific content cannot become global merely because it lives in a shared database.

For unknown legacy cache ownership, return a miss with a retrieval hint. For ambiguous imported policy, report an inventory issue and request a reviewed classification; do not guess ownership from text.

### 4.2 Session and compaction lifecycle

Keep the existing optional session argument compatible. Validate malformed explicit IDs instead of silently treating them as absent.

Replace the unscoped hook marker with a versioned record containing project identity, window/connection identity, conversation ID and timestamp. Accept it only when the bindings match. A legacy marker may be used for diagnostics, not authoritative session reuse. Missing identity starts cold rather than borrowing another chat.

Add an explicit context-reset/epoch contract, through preflight and adapters, separate from durable session identity. On compaction, reconnect, handoff or lost acknowledgement:
- resend the applicable required policy or its complete normative representation;
- restore the bounded working snapshot;
- retain durable transcript and documents on disk/database;
- present graph-answer references as retrievable, not necessarily already visible.

The existing known_context hash remains an acknowledgement of working notes only. It must not implicitly acknowledge policy or graph answers.

### 4.3 One budgeted context builder

Keep context.budgetTokens as the primary setting. Proposed optional settings are context.profile = minimal|balanced|full and per-section ceilings. These are proposed API additions, not currently available settings.

Create independently budgeted blocks:
1. project/session identity and freshness errors;
2. applicable critical/always-required policy;
3. current objective and essential working facts;
4. task-matched policy/skills and directly relevant memory pointers;
5. optional history, catalogs and graph detail pointers.

Use stable ordering and content hashes. A first response delivers necessary content; subsequent responses send changed blocks and compact acknowledgements. Whole optional blocks can be omitted, with a precise expansion/retrieval route.

Do not use heuristic heading extraction as the only guarantee that a critical rule is complete. Prefer an explicitly authored normative section whose semantics are testable; otherwise keep the complete critical rule. Session continuity must survive small budgets.

Report requestedBudget, requiredMinimum, finalTokens, included/omitted block IDs, measurement method and overBudget. Count all model-visible text and structured content according to the client projection being measured. Do not double-count metadata that the client does not send, or omit metadata it does send.

### 4.4 Memory and policy selection

Keep short titles/IDs in preflight; full bodies come from ax_recall, ax_rules or ax_skill on demand. Deduplicate by scoped content identity, not title alone.

Rank task relevance using prompt, open/changed files and graph symbols. Preserve provenance: project/global, source location, revision/hash and why matched. Bound linked expansion by depth, count and tokens. Fetch linked targets by ID rather than loading all memory rows if benchmarks confirm that route is material.

Add a disk-vs-SQLite audit for .agents/rules and .agents/skills: ID, body hash, metadata hash, enabled/approved state, scope, source path and revision. Respect configured authoritative storage mode. Do not overwrite a newer database rule with an older export or re-import stale copies automatically.

### 4.5 Graph-first retrieval

Use existing tools before inventing another graph API:
- ax_explore / ax_search to locate a concept or symbol;
- ax_node for current numbered symbol source and direct neighbors;
- ax_callers / ax_callees / ax_impact for relationships;
- ax_expand for a cached response that is absent from model context;
- ax_sync when source/index freshness requires it.

A source read is justified for files the graph does not index, missing source coverage, a stale/unavailable graph, or the exact current text immediately before an edit. Record the fallback reason; do not penalize legitimate configuration/document reads.

Test stale source edits, new/deleted symbols and inferred/ambiguous edges. A graph response must carry enough source location/version evidence to avoid treating inference as an exact call relationship. Avoid whole-file payloads where one symbol suffices.

## 5. Implementation order and file map

1. Capture baseline traffic and disk/database inventory before changes.
2. Fix shared project binding and hook attribution.
3. Add context epoch, acknowledgement and compaction recovery.
4. Correct snapshot comparisons and version invalidation.
5. Refactor preflight blocks and final payload accounting.
6. Tighten policy/memory provenance and targeted link retrieval.
7. Verify graph-first behavior in real adapters.
8. Benchmark, document and release only after acceptance gates pass.

Primary files: crates/ax-mcp/src/{server,tools,chat_session,policy_session,smart_output,links}.rs; crates/ax-usage/src/{cursor_state,context_plan,context_cache,reuse_cache,working_context,budget}.rs; crates/ax-policy/src/format.rs and policy import/index/store paths; crates/ax-memory/src/store.rs.

Expected test additions: crates/ax-mcp/tests/project_context_isolation.rs, crates/ax-mcp/tests/context_epoch.rs, crates/ax-usage/tests/context_budget_contract.rs, and scripts/bench-agent-efficiency/context_isolation.py. Confirm existing integration harness conventions before creating paths; retain equivalent tests in current modules when that is the established convention.

No new production dependency is planned. Use existing SQLx, serialization, hashing and tokenizer facilities. A migration, if needed for provenance/epoch storage, must have fixture upgrade and rollback checks before acceptance.

Implement on an isolated feature branch after review of this spec. The current Windows/Clippy/Actions repair stays separate.

## 6. Executable acceptance scenarios

| ID | Given / action | Required result |
|---|---|---|
| ISO-01 | Two projects have the same symbol, rule ID and memory title but different bodies | Each response contains only the active project's body |
| ISO-02 | Submit project B's cache ID, session or explicit projectPath to project A | No B data; scoped error or cache miss; no cross-project write |
| ISO-03 | Two Cursor windows alternate preflight/tool calls while sharing a daemon | Correct window/chat state, no borrowed hook session |
| ISO-04 | Explicitly shared generic global rule and unrelated B-only skill exist | Global rule may apply with provenance; B-only skill does not |
| ISO-05 | Imported Hoornaarpreventie feature-information skill is present during an Ax context task | It does not match merely because the prompt says feature |
| SES-01 | Same chat, unchanged content and acknowledged epoch | Delta reply; applicable required constraints remain recoverable |
| SES-02 | Compact away policy/graph bodies but retain session ID | Epoch reset rehydrates required policy and notes; missing graph body is expandable |
| SES-03 | Malformed explicit session ID or mismatched hook record | Clear validation outcome; no silent borrowing of another chat |
| SES-04 | Fork/handoff within project, then repeat in another project | Intended working notes only; caches are not blindly inherited |
| BUD-01 | Budget is below required policy minimum | Full required semantics; overBudget=true and exact reported minimum |
| BUD-02 | Optional context fills the budget exactly, including footer/metadata | Final measured payload fits or reports a genuine required overflow |
| BUD-03 | Omit an optional block, then increase budget next turn | Previously omitted content remains eligible; not marked delivered |
| SNAP-01 | Same session/text in a different project | Changed, never Same |
| SNAP-02 | Remove a previous part without changing the remaining parts | Snapshot invalidation records the removal |
| POL-01 | Edit disk rule while database/export revision differs | Audit shows difference and authoritative source; no silent overwrite |
| MEM-01 | Large memory collection with a small linked subset | Correct matches and bounded payload; targeted query behavior measured |
| GR-01 | Ask for one known symbol's source | Graph answers with location/source; no redundant source read |
| GR-02 | Source changes after indexing or edge is ambiguous | Explicit freshness/confidence handling; no stale exact claim |
| GR-03 | Required file is not indexed or graph unavailable | Controlled source fallback with reason |
| COST-01 | Provider usage is unavailable | Report model-visible estimates as estimates; provider cache savings remain unknown |

## 7. Benchmark and release gates

Use three task levels: one-symbol fix, cross-module behavior change, and a multi-project/session/compaction scenario. Compare the current audited implementation with the proposed implementation under identical source fixtures, prompts, tool sequences, tokenizer and warm/cold states. Fix model/client versions and record run IDs when live-agent trials are available.

Report per turn and per completed task: preflight tokens, all tool response tokens, expanded tokens, actual provider input/output tokens when observable, total round trips, graph versus source-read calls, cache hit/miss reasons, latency and behavioral correctness. Include expanded content so a shorter preflight cannot hide a larger overall cost.

Tune token targets after collecting baseline. Acceptance requires zero cross-project disclosures in the defined isolation suite, preserved critical constraints, correct compaction recovery and no deterioration in task completion quality. Treat latency/token improvements as measured distributions, not invented percentages. ax reduces supplied context; provider prefix caching is a separate optimization and does not remove tokens from the context window.

Run cargo build --workspace --locked, cargo test --workspace --locked on Ubuntu and Windows, strict workspace Clippy, docs build and docs tests. Add macOS path/identity checks for the user's development environment. Do not hide failures with skipped tests or continue-on-error.

Create docs/specs/minimal-project-safe-context-EVIDENCE.md during implementation, mapping every scenario to a real test and the final source SHA. Update README and site guides/references in the same implementation change; publish documentation through Netlify, never GitHub Pages.

## 8. Completion definition

This planning task is complete when this spec is saved with the confirmed findings and runtime unknowns clearly separated. The proposed feature is complete only after reviewed implementation, disk/database parity checks, multi-project live validation, reproducible benchmarks and green gates. Source inspection alone is not a runtime isolation certificate.
