# AX-OVERHAUL-PLAN-RESOLVER

- id: AX-OVERHAUL-PLAN-RESOLVER
- kind: implementation-plan
- parent_spec: AX-SPEC-PLAN-CONTEXT-OVERHAUL
- status: accepted for implementation on 2026-11-22
- version: 1.0.0
- updated: 2026-11-22

The owner said "all approved" on 2026-11-22. That accepts this plan for implementation. It does not approve the drift notes on `link-plans-to-sessions-b38a9318-plan`; version 3 stays the approved body. This plan does not grade its own replay accuracy. `04-VALIDATION-SPEC-AND-PLAN.md` is not in the repo, so benchmark scoring stays blocked. Correction (recheck, 2026-11-22): document 04 was added later the same day and accepted as `AX-REPLAY-VALIDATION-CONTRACT`. Benchmark scoring follows that protocol. No measured 95% result exists yet.

## Implementation Plan B: Authority, Context Resolver, Manifests and Fresh-Agent Replay

Scope: Implement the reproducible context layer, not merely a UI label. Prerequisites: schema/API/provenance contracts from Plan A, approved master spec, agreed independent validation definitions in 04-VALIDATION-SPEC-AND-PLAN.md.

## Phase B0: Contract freeze and threat model

- Inventory existing ax_preflight, ax_context, policy matching, code graph, memory exports, snapshot continuity and CLI equivalents. Identify exact reuse candidates and missing functionality.
- Define immutable provenance graph: work item -> spec revision -> selected plan revisions -> decisions -> rule/skill revisions -> session -> turn -> agent -> source/commit -> manifest.
- Define replay modes explicitly: continue/current retrieves currently authoritative versions; reconstruct/as-of retrieves historical versions that were valid at cutoff; benchmark/reproduce uses a frozen pre-implementation snapshot and isolated environment.
- Threat-model prompt injection from retrieved files, cross-project leakage, tampered manifest, stale replica, wrong agent identity, hidden-test leakage, dropped mandatory rules and time-travel data.
- Write exact typed DTO schemas, failure outcomes and contract tests for resolver inputs, selection receipts and manifest verification.

Exit gate: published schemas and a threat-to-test matrix; source timestamp semantics and project/agent identity are unambiguous.

## Phase B1: Authority Validator

- Enumerate authority types, scope and precedence, respecting higher-order security constraints and repo policy. Keep approval/authority distinct from semantic relevance.
- Check project ACL, artifact integrity, approval validity, plan/spec compatibility, cutoff, rule applicability, policy priority, replica freshness and required dependency completeness.
- Report explicit states: authoritative, advisory, superseded, unapproved, conflict, stale, unauthorized, missing, integrity_failed.
- Make conflict, unauthorized, integrity_failed and missing critical dependencies fail closed. Never promote a lower-priority skill or a retrieved instruction into binding policy.
- Support human resolution with an audit decision/revision. No agent-driven silent override.

Proof: tests for conflicting rule/spec, old plan/new spec, malicious skill text, revoked approval, missing hash and cross-tenant access. Fail a known-negative control before trusting the gate.

## Phase B2: Context Resolver

- Input: task, project, mode, session, turn range, agent, current phase, spec/plan bindings, codebase revision and token budget.
- Build candidate pool from approved specs, bound and required plans, mandatory rules, relevant skills, decisions, graph symbols, memory and source references. Only sources available at cutoff are eligible in historic modes.
- Use graph, lexical and semantic ranking to rank eligible candidates; separate pinned mandatory items from ranked advisory ones.
- Implement deterministic mandatory selection, explicit exclusion reasons, deduplication by immutable hash and bounded packaging. Permit re-resolution when a turn expands scope to another framework or module.
- When budget cannot include required constraints, return a blocking/incomplete status and expandable pointers. Never silently omit critical policy.
- Persist why each candidate was selected or rejected; emit an independently inspectable selection receipt.

Proof: .NET-only, TypeScript-only, mixed React/.NET and repo-agnostic scenarios, with deliberate poisoned and obsolete candidates. Verify mandatory constraints always selected and irrelevant skills not flooded into delivery.

## Phase B3: Sealed Context Manifest and as-of reconstruction

Each manifest records:

- Manifest/run identity, schema version, AX project, task, session, turn range, stable agent ID and provider IDs if actually issued.
- Mode (current, as_of, benchmark), historical cutoff and immutable checkpoint/snapshot ID.
- Every source: kind, stable ID, revision/version, hash, role (mandatory/advisory/evidence), authority status, source position, selected/included/omitted reason.
- Exact delivery ordering and canonical payload/segment hash, bytes/tokens, truncation and missing references.
- Model/provider/version/parameters, available tool versions, relevant environment/config, source commit/tree hash, dependency lockfile hash, AX index/replica freshness, redaction and ACL decision.
- Integrity seal, created_at, audit actor, and immutable association to the receiving turn.

Canonicalize and hash payload bytes and manifest structure with domain separation. Verify on fetch and replay; do not use nondeterministic serialization for sealing.
Archive immutable historical revisions and policy payloads referenced by manifests, or durable pointers with verifiable access guarantees. Where unavailable, return partial or blocked, never recreate missing history from a newer version.
Record delivered manifest at each turn; rescope on new plan/spec versions. Exact retrieved version need not be latest.
Provide human-readable explanation: what was included, why, which valid candidates were omitted, which conflicts blocked the request.

Proof: tampering, reordered payload, deleted snapshot, omitted rule, changed file at same path, stale replica and two manifests across a scope change.

## Phase B4: Fresh-agent resume and historical replay interfaces

- Define replay_reference resolving an AX session ID, explicit selected turn ID(s)/range, and stable AX agent ID. Authenticate and infer project from AX records; reject ambiguous or unauthorized combinations.
- Support CLI and MCP/API equivalent operations: resolve reference, verify snapshot, inspect manifest, fetch/expand context, launch fresh agent through supported provider adapter, obtain run status/evidence.
- Fresh session must not inherit conversation, provider memory, hidden scratchpad, parent worktree edits or copied output. It receives only replay reference + AX access + benchmark task contract as allowed by the independent validation protocol.
- Historical benchmark replays begin from a common pre-implementation code snapshot, not the completed reference implementation. This explicitly prevents accidental answer leakage from Git history or local files.
- If no automated provider session adapter is available, provide a manual-but-auditable fresh-agent protocol and mark automated replay not_implemented; do not claim a mocked fixture is a live replay.
- Record exact model settings and tool provenance so a later failure can distinguish AX retrieval error, agent variability and environment drift.

Proof: two fresh agents from the same sealed checkpoint and fixed task; no transcript; as-of cutoff prevents future leakage; improper reference fails explicitly; CLI-only workflow reaches equivalent context.

## Phase B5: Resolver/authority/replay UI overhaul integration

- Embed authority/conflict state beside current spec and selected plans; expose linked rules/skills and their versions without pretending the entire policy is owned by the spec.
- Add preview of next-turn context bundle and exactly why each element was selected or omitted.
- Display manifest timeline and allow an operator to compare context versions and delivery hashes between turns/agents.
- Add Replay action selecting historical source, cutoff and fresh agent adapter; provide status, isolated source reference and scorecard navigation.
- Make uncertainty explicit: historical snapshot incomplete, critical authority conflict, not tested, replay failed; never show a confident green indicator without evidence.

Proof: UX+API integration and access tests; correctly displayed critical blockers; exports usable from both desktop and compact/mobile UI.

## Phase B6: Hardening and performance

- Bench under realistic 100+ turn history and repository sizes; retain compact preflight and lazy expansion; report latency and token cost versus existing AX.
- Test sync/replica conflict handling, permission changes, event idempotency and race conditions between approvals and context assembly.
- Fuzz identifier and manifest parsers; test malformed tool payloads and injected Markdown instructions as data.
- Run full regression, security, mutation, property, integration and live smoke suites. Produce reproducible command-based evidence.
- Integrate independent validation from Plan C; do not mark the release complete until the 95%/100% gates actually pass.

## Interfaces to deliver

Illustrative names, subject to repo convention. These are proposed contract examples, not assertions that the commands exist.

- ax specs tree / show / revise / approve
- ax plans list / create / revise / select / compare
- ax context resolve --session <ax-session> --turn <ax-turn> --agent <ax-agent> --mode as-of
- ax context manifest show / verify <manifest-id>
- ax replay reference / run / status / evidence

All functionality must also be represented through compatible programmatic MCP/API operations.

## Handoff contract

End with: stable service API, reproducible sealed manifest sample, test fixtures, negative-control proofs, known limits, model-adapter support matrix, performance measurements and acceptance-to-test matrix. Plan C owns the independent quality verdict; Plan B may not grade its own replay accuracy.
