# AX-SPEC-PLAN-CONTEXT-OVERHAUL

Status: accepted for implementation on 2026-11-22.
Owner words: "Deze moeten gezien worden als een final versie. Dus begin met implementeren."

The base contract below is that accepted text. Amendment 1 is the owner's answer "A en B" to the approver question asked in the same exchange. It is part of the contract being implemented. It is not a later silent edit.

## Amendment 1 — who may approve

Recorded 2026-11-22.

- An agent cannot approve a specification, a plan, or a decision.
- The local human is the default approver. That person is the global-database bootstrap `owner` in `team_members` and `member_roles` (`crates/ax-global-db/src/access.rs`).
- Plan A reuses that membership store. It adds a named `approver` role beside `owner`. `owner` and `approver` may approve. No second ACL database is introduced.
- In SPEC-14, tenant means the project database plus global-database membership. This overhaul does not add a new account product.

## Amendment 2 — admin may approve

Recorded 2026-11-22 when the owner said "store and approved, continue, use my role as admin".

- Membership-checked approval also accepts an active `admin` role from `member_roles`, beside `owner` and `approver`.
- The role is read from the membership database for the actor member id. A claimed role string in the request is not trusted.
- The live bootstrap member used for this approval (`a4c2d177-d0ca-4d7d-8789-cba7ebcae5c7`) is `owner` and has no `admin` row. Approval therefore used the verified `owner` identity.

## Base contract

Metagegevens

- id: AX-SPEC-PLAN-CONTEXT-OVERHAUL
- kind: specification
- status: accepted for implementation (was draft-for-human-review)
- version: 1.0.0
- scope: full-overhaul
- owner: AX
- updated: 2026-11-22

### 1. Purpose and decision

Replace AX's existing flat Specs/Plans experience and ad hoc context assembly with one coherent, hierarchical, versioned development-intent system. This is a full product and architectural overhaul, not a cosmetic redesign or another parallel registry. Preserve historical records and established AX capabilities through explicit, tested migration. The project database remains the canonical authority for versioned records; Markdown is a Git-readable projection of approved material, not a competing source of truth.

AX must let a new AI agent start without prior chat history, identify a historical task using AX session, turn, and agent identifiers, reconstruct the relevant authoritative intent, and independently produce the intended functional result. AX must expose how that context was selected, not merely assert that it was correct.

Acceptance ambition: >=95% independently measured functional reproduction on a pre-registered benchmark and 100% pass rate on defined critical security and architectural invariants. These are future release gates, not current product claims, and cannot establish absolute correctness of a specification.

### 2. Verified baseline and replacement scope

Target project: GaryWenneker/ax (main branch inspected 2026-11-22; a targeted review, not a complete code audit).

Observed implementation:

- `crates/ax-usage/src/spec_registry.rs`: specs, spec_revisions, spec_bindings, spec_deliveries, and file-seal storage. A finished plan can currently be captured as a spec_revisions entry; kind is not a durable first-class Plan identity.
- `crates/ax-web/web-ui/src/pages/Specs.tsx`: one list with All / Plans / Specs filters and revision viewer, not an explicit spec to plan hierarchy.
- `crates/ax-web/src/specs.rs`: list, revision save, and approval endpoints for specs.
- `crates/ax-usage/src/continuity.rs`: a snapshot/checkpoint test harness and session-identity structures. Its own documentation explicitly does not claim that a live AI model reproduces a prior result.
- `docs/specs/spec-driven/SPEC.md`: existing approved contract for versioned specs, human approvals, immutable history, source/hash drift, and Markdown projection.
- `docs/specs/context-continuity/CONTEXT-EXPORT-COVERAGE.md`: fixture-based export checks; cross-chat equivalence is not established.

The overhaul supersedes the product model and UI of the previous spec-driven feature, while retaining its valid invariants and creating a traceable migration from existing tables. The implementation agent must inventory the current schema, APIs, integrations, tests and project policies before modifying them; do not assume the above list is exhaustive.

### 3. Definitions and relationships

Work Item: optional external task identity, e.g. Azure DevOps PBI. Can link to multiple specifications; is not the source of truth for AX's internal identity unless explicitly synchronized and reconciled.

Specification: stable identity plus immutable revisions defining what and why, outcomes, constraints and testable acceptance criteria.

Plan: stable identity plus immutable revisions defining how and in what order, options, decisions, steps, dependencies, and risks. A plan has exactly one owning specification. It can reference other specs without becoming owned by them.

Plan version: an immutable change to one plan. Alternative plans are distinct identities, never disguised as successive versions.

Decision: an explicit proposed/accepted/rejected/superseded choice linked to spec, plan, revision, agent, session, turn, reason and evidence.

Rule: normative instruction or constraint from global, project or approved task scope, with version, authority and applicability.

Skill: reusable procedure or capability, selected for relevant tasks, not itself proof of correctness.

Context Manifest: a sealed, ordered record of the exact bytes or immutable references supplied, versions and hashes, selection decisions, omissions, environment and cutoff.

Replay: a fresh, isolated agent execution using only allowed task identifiers and AX access to reconstruct permitted historical context. It is distinct from replaying a transcript.

Required structure:

```text
Project / Work Item (optional source link)
  -> Specification [stable ID; immutable revisions; approval]
       -> Plan A [stable ID; immutable revisions; optional active selection]
       -> Plan B [alternative implementation path]
       -> Plan C [independent workstream]
  -> Rules and Skills [independent registries, linked by applicable version]
  -> Sessions -> Turns -> Agent identity -> Decisions / Context Manifests
  -> Replay Runs -> Scorecards -> Evidence
```

Lifecycle rule: Creating a new specification via UI/API must create or attach at least one real initial plan within the same transaction; an empty placeholder is not acceptable. Legacy imported records without a defensible plan remain visible as needs_plan and cannot progress to execution-ready until repaired by an explicit user/agent proposal. Multiple plans can coexist; selection of one or more plans is recorded per execution/workstream. Never infer that the newest revision is the approved one.

### 4. Mandatory domain invariants

SPEC-01 Hierarchy. Specifications and plans are separate first-class entities; one spec owns >=1 real plan when execution-ready; every plan has exactly one owning spec. A session can work with multiple spec/plan bindings without losing provenance.

SPEC-02 Immutable revision history. Spec, plan, decision and context revisions are append-only, content-hashed, durable and retrievable in full. Store parent/based-on revision and exact author-agent-session-turn provenance. Never silently overwrite approved content.

SPEC-03 Review lifecycle. Both specs and plans support draft, in_review, change_requested, approved, rejected, superseded, with explicit authorization for approval. Agents can propose or save drafts; cannot self-approve through capture. Approval uses optimistic concurrency and records actor, time, reason and revision hash.

SPEC-04 Plan dependencies. Every approved/active plan revision declares the exact compatible spec revision/hash. A changed approved spec causes affected plan selections to become compatibility_review_required until checked; unrelated plan histories remain unchanged.

SPEC-05 Turn capture. At turn completion, capture finished specs/plans and material modifications as drafts with source type, origin event or file, session, turn, stable AX agent identity, provider/model metadata, relevant code references and before/after hashes. Ignore unfinished streams and deduplicate idempotently. A word such as "plan" in a prompt must not create spurious artifacts.

SPEC-06 Concurrent changes. Detect revisions based on a stale parent; do not auto-merge conflicting changes. Give a three-way diff and review flow; use transactions to prevent half-written hierarchy or cross-links.

SPEC-07 Policy separation. Rules and skills remain independent from specs/plans; their applicable versions are linked, not copied into every spec. Mandatory rules are enforced, relevant skills made available. Policy versions and their exact payload hashes must be preserved for historical replay.

SPEC-08 Deterministic authority. The validator determines whether a selected item is permitted to act as authority using explicit scope, trust, approval, temporal validity, policy precedence, permission and revision criteria. Semantic relevance alone cannot confer authority. Conflicting mandatory sources block execution and show an actionable conflict; retrieved instructions embedded in untrusted materials never override policy.

SPEC-09 Context selection. Given project, source task, phase, code graph, spec/plan selections, agent identity and historical cutoff, the resolver returns an ordered, budget-aware set. It must include all required valid constraints, rank other evidence transparently, report omitted candidates and reasons, and fail closed when required evidence is unavailable. A token budget may truncate advisory material but cannot silently drop mandatory constraints.

SPEC-10 Context manifests. Each delivery is sealed with manifest ID, source identifiers and cutoff, selected artifact+revision+hash, ordered delivery hash, authority result, omitted/candidate reasons, token counts, project/tree/commit identity, tools, model/provider/config, rule/skill revisions, and redaction/availability status. No secret raw values in manifests; references observe access policy. Manifest content and implementation input must match verifiably.

SPEC-11 Historical fidelity. as_of_turn must retrieve only information available at or before that historical cutoff. No later decisions, code, approved specs, answer/benchmark fixtures or hidden tests can leak into historical replay. Distinguish continue using current state from reproduce from historical state in API and UI.

SPEC-12 Agent identity. Model AX agent ID separately from provider-issued chat/turn IDs. Persist both when issued, never invent an external identifier. Accept replay input containing an AX session ID, a selected turn ID or bounded range, and AX agent ID; resolve project identity and authorization via AX. Invalid/ambiguous IDs fail explicitly.

SPEC-13 Integration parity. MCP, CLI and API/IDE integration paths must uphold the same contracts and authorization checks. Replay and manifest features cannot require MCP for users restricted to CLI-only operation.

SPEC-14 Privacy and isolation. Enforce project/tenant scopes, ACLs, least privilege, secret redaction, secure hash/reference handling, retention policies and audit logs. No access across projects, accounts or replicas unless explicitly authorized. Protect local /.ax/ data and migration backups.

SPEC-15 Replica synchronization. If a synced AX replica participates, preserve stable IDs, immutability and content hashes; reconcile immutable revision records and detect conflicting approval heads instead of last-write-wins for authority. Expose sync freshness in context manifests. No replay success claim from stale or inconsistent replica state.

SPEC-16 Navigation. Replace the current flat Specs screen with a hierarchical workspace showing spec tree, child plan alternatives/workstreams, status, active version, approval, history, current bindings, rules/skills, manifest, drift, replay and coverage. Each spec and plan has distinct detail and revision comparison screens. Show provenance at session/turn/agent resolution and make selected authority clear.

SPEC-17 Compatibility and migration. Non-destructively migrate existing specs, spec_revisions, bindings, deliveries, Markdown pointers and file seals. Detect and label legacy "plans stored as specs"; classify automatically only when unambiguous, otherwise queue mapping for review. Preserve old IDs and all revision bytes/hashes; maintain API compatibility during cutover or deliver explicit versioned breaking changes and consumers. Provide validation reports, rollback path and feature flag cutover. Do not create .ax/plans/ or a second authoritative registry.

SPEC-18 Observability. Record provenance, resolver decisions, authority conflicts, manifest mismatches, missing references, replay costs, runtime, score, critical gate failures and migration reconciliation. Logs must be useful without exposing secrets or prompt payloads to unauthorized users.

SPEC-19 Reproduction fidelity. Implement independent golden/replay benchmark protocol in 04-VALIDATION-SPEC-AND-PLAN.md. Functional reproduction target >=95% in eligible evaluated runs and 100% for predeclared critical invariants. Never score text similarity as a proxy for functional correctness. not_tested, blocked or partial cannot count as passed.

SPEC-20 Documentation synchronization. A material spec change prompts a linked plan compatibility review and impacted test mapping; implementation drift opens a change request but never silently edits the approved spec. Document what applies now; avoid release-by-release documentation clutter.

### 5. Storage and conceptual API contract

The agent must first inspect existing schema and migration practices. The design below is conceptual: refine identifiers, names, constraints and indexes in the implementation plan after inventory rather than blindly introducing duplicate tables.

Candidate authoritative entities: specs, spec_revisions, plans, plan_revisions, plan_selections, spec_plan_links (only if cross-reference semantics require it), development_decisions, artifact_policy_bindings, context_manifests, context_manifest_items, context_deliveries, replay_runs, replay_checks, migration_mapping, approval_events and/or existing compatible equivalents.

Every immutable revision must carry id, stable parent ID, monotonically increasing version, based-on pointer, status, content hash, original author/agent, originating session/turn, creation timestamp, approval metadata when applicable. Every foreign key must be checked; use unique constraints to prevent duplicate capture from hooks/retries. A foreign key referencing historical source content must not be overwritten by later current state.

Required service capabilities (exact route naming may follow repo conventions):

- List spec tree with child plans; filter by project, work item, session, agent, status, technology and search text.
- Create spec with first plan atomically; create separate alternative plan; revise spec or plan; compare revisions; propose/approve/reject; select plan revision(s) per workstream.
- Link and inspect decisions, rules and skills, sessions/turns, related code/tests and historical manifest deliveries.
- resolve_context(project, task, session, turn, agent, mode, cutoff) returning authority validation and a sealed manifest plus materialized payload pointers.
- replay_reference(session, turn(s), agent, mode) resolving authorized project and freezing historical cutoff/snapshot.
- Start/cancel/status/evidence for replay runs; inspect immutable scorecard and export evidence.
- Expose the same semantics through CLI, MCP and server API, with HTTP status/error contracts, authorization and idempotency. Preserve preflight's compact default and explicit expansion of full bodies.

### 6. Authority and selection policy

Authority and relevance are separate computations:

1. Determine project/tenant identity and permissions; reject wrong scope.
2. Resolve a consistent as-of snapshot and matching approved spec/plan revisions.
3. Load mandatory global/project/task rules and check explicit precedence and applicability. A security or authorization restriction cannot be overridden by a plan/skill.
4. Discover candidate skills, code, decisions and evidence using graph/lexical/semantic signals. Those signals rank relevance only, never override authority.
5. Verify hashes, status, temporal validity, scope and compatibility. Distinguish untrusted source text from executable policy.
6. Detect conflicts; either block (mandatory conflict/missing required artifact) or declare a reviewed advisory conflict. No model auto-approval.
7. Assemble bounded ordered content, record exact delivery/omissions, seal manifest and persist turn binding.

The priority model must be explicit, scope-aware and reviewable. If two valid authorities cannot be ordered safely, return authority_conflict with both sources rather than inventing a winner. The system guarantees consistency of selection relative to approved records, not that an approved human spec is factually true.

### 7. UX requirements

Primary route: Specs becomes a true Specifications & Plans workspace (route compatibility may keep /specs). Suggested layout:

- Left: project/work-item-filtered specifications, expand to named plans. Search and filters; legacy unmatched items clearly labeled.
- Center: selected spec or plan; requirements/steps and current approved version; draft and review state; version history with compare/diff; alternative plans and active workstreams.
- Right contextual panel or responsive drawer: source work item, dependent rules and skills, linked code/tests, decisions, session/turn/agent history, authority status, manifest and replay actions.

Desktop and mobile must support keyboard navigation, accessible focus, clear approval actions, async loading, error/empty states and project-local performance budgets. Show Current, Historical, Draft, Conflict and Missing evidence explicitly. Never conflate an approved revision with the latest. Creation workflow must require a real initial plan, but drafts may remain incomplete.

### 8. Proof and release gates

Acceptance must trace each SPEC-xx invariant to a test ID and actual command. Required gates:

- Schema upgrade preserves all existing revision counts, bytes/hashes, bindings and links; zero orphaned foreign keys and zero silent loss; automatic classifications audited.
- At least one real plan for each execution-ready spec; multiple independent plans and separate version timelines demonstrably work.
- Agent turn can propose edits to spec and plan with correct provenance; neither approved revision mutates without approval.
- Authority evaluator blocks stale, superseded, poisoned, unauthorized and conflicting mandatory sources; manifest audit confirms exact delivered versions.
- Isolated new-agent historical replay returns only as-of sources and produces a scorecard from independent tests.
- CLI/MCP/HTTP parity with consistent unauthorized/missing reference behavior.
- Full existing regression suite, new property and mutation tests, migration rollback rehearsal, accessibility checks, real execution smoke tests and security threat tests pass.
- Formal quality gate described in the validation document passes on a held-out multi-domain corpus; if not, report failure, never relabel it success.

### 9. Explicit non-goals and design boundaries

- Do not require exact source-code equality, deterministic model text, or identical commit hashes.
- Do not pretend a recorded fixture agent proves live-model reproducibility.
- Do not grant the agent human approval privileges.
- Do not rewrite old approved specs/plans or drop source artifacts during migration.
- Do not store raw secrets or expose hidden benchmark answers to replay agents.
- Do not assume one static rule/skill set is sufficient throughout a task; re-resolve after a relevant scope change and record a new manifest.

### 10. Source snapshot / affected areas

Initial source pointers (to be revalidated before implementation):

- https://github.com/GaryWenneker/ax/blob/main/docs/specs/spec-driven/SPEC.md
- https://github.com/GaryWenneker/ax/blob/main/crates/ax-usage/src/spec_registry.rs
- https://github.com/GaryWenneker/ax/blob/main/crates/ax-usage/src/continuity.rs
- https://github.com/GaryWenneker/ax/blob/main/crates/ax-web/web-ui/src/pages/Specs.tsx
- https://github.com/GaryWenneker/ax/blob/main/crates/ax-web/src/specs.rs
- https://github.com/GaryWenneker/ax/blob/main/docs/specs/context-continuity/CONTEXT-EXPORT-COVERAGE.md

### 11. Contract change control

This document was a draft proposed contract. The owner accepted it for implementation on 2026-11-22, together with Amendment 1. Plans 02 and 03 implement this same spec; 04 defines independent verification. If implementation reveals a conflict, make a spec change request with impact assessment, never quietly make code the authority. Preserve prior approved contracts as historical references with a superseding link.
