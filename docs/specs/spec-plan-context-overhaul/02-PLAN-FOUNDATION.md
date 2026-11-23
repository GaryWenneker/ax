# AX-OVERHAUL-PLAN-FOUNDATION

- id: AX-OVERHAUL-PLAN-FOUNDATION
- kind: implementation-plan
- parent_spec: AX-SPEC-PLAN-CONTEXT-OVERHAUL
- status: accepted for implementation on 2026-11-22
- version: 1.0.0
- updated: 2026-11-22

Accepted with the parent spec. Phase A0 inventory is `A0-INVENTORY.md` in this directory. This file does not change the phase gates. Conflicts found during inventory are recorded there as change requests.

## Implementation Plan A: Domain Model, Migration, Agent Capture and UI

Scope: Replace the old Specs/Plans product model and screens. Depends on: approval of 01-MASTER-SPEC.md. Follow-on: 03-PLAN-RESOLVER-AND-REPLAY.md and 04-VALIDATION-SPEC-AND-PLAN.md. Plan versions are independent of spec versions.

## Execution operating rules

- Use the project's existing AX turn protocol and all applicable rules/skills. First inventory actual code, branch state, open changes, DB schema and consumers. No speculative bulk rename.
- Use a dedicated feature branch or isolated worktree that can run real tests. Never move, replace or delete the project's .ax folder or ax.db* to simulate a clean system. Test migrations with a copied fixture database in a temporary project.
- Before any code changes, record baseline test outcomes and collect row counts and hashes from a read-only existing database snapshot. Provide a reversible rollout and migration dry-run evidence.
- Develop from explicit failing tests to implementation and then refactor. Tests may evolve only after corresponding approved spec changes. Run relevant legacy tests after each phase.
- A completed plan phase means code, tests, migration evidence, documentation and compatibility checks are complete; do not mark a phase done based only on generated code.

## Phase A0: Inventory and replacement design (must happen first)

- Enumerate the current schema and migration system: spec registry, policy_revisions, session/turn/agent tables, context exports, continuity harness and replica sync.
- Map all existing write/read integrations: web API, CLI, MCP, hooks, preflight, Markdown importer, global policy and UI routes.
- Identify data ownership, exact format of historic plan-as-spec rows, potential duplicate ids, current edits/unsaved work and legacy approvals.
- Record proposed data migration mappings, API v1 compatibility window, feature-flag cutover and rollback path. Define indexes and concurrency boundaries.
- Produce a reviewed design decision record with a schema diagram, user flows and acceptance-test mapping for SPEC-01..06, SPEC-14..18, SPEC-20.

Exit gate: inventory report; no unknown destructive migration; reviewed schema proposal and failing tests for key new entities.

## Phase A1: First-class spec/plan model and revision engine

- Add/migrate first-class plans and plan_revisions (or equivalent) with exactly one owning spec, distinct stable ID, arbitrary many approved/draft history rows and provenance.
- Preserve existing spec identities/revisions; add explicit kind on new records rather than inferring from revision source. Store alternatives as separate plan IDs; store modifications as revisions of the same plan ID.
- Enforce lifecycle: creation with initial plan in one transaction; incomplete imported legacy specs marked needs_plan; prevent execution-ready approval without plan.
- Add plan compatibility binding to exact spec revision and support multiple selected plans per workstream and turn.
- Introduce explicit decision/history entities only when existing storage cannot satisfy immutable linked audit.
- Add optimistic concurrency / conflict responses and idempotent capture keys. Never truncate revision history.

Proof: integration and property tests for FK integrity, transaction rollback, revision monotonicity, approvals, parallel edits, imported orphan behavior, alternatives and separate plan histories.

## Phase A2: Legacy migration, non-destructive and auditable

- Build dry-run classifier for old specs/spec_revisions using paths, source type, links and user decisions; use confidence categories unambiguous, ambiguous, unclassified.
- Preserve original rows, revision bodies, hashes, approval events, session/turn bindings and file seals; map unambiguous legacy plan records to first-class plans with old ID alias/redirect.
- Leave uncertain items readable with clear migration review queue. Never invent a parent spec or attach a plan to an arbitrary spec.
- Provide migration reports: row counts and SHA hashes before/after, orphan check, mapping decisions, API compatibility, unresolved count.
- Exercise upgrade, partial failure/retry, rollback-before-cutover, rollback-after-cutover strategy and replica catch-up using fixture copies. Keep backward-compatible reads or a versioned API until clients are cut over.

Proof: exact matching historical payloads, no missing revisions, repeatable migration, rollback proof, no overwrite of the user's .ax assets.

## Phase A3: Independent services and contracts

- Implement transactional services for CreateSpecWithPlan, CreateAlternativePlan, ReviseSpec, RevisePlan, ApproveRevision, SelectPlan, LinkTurn, CompareVersions, ListTree and InspectProvenance.
- Expose server endpoints following existing AX conventions; use typed DTOs with explicit kind and version/hash fields and permission validation.
- Expose equivalent CLI and MCP paths; confirm read-only/project ACL enforcement and actionable error codes.
- Make all actions retry-safe, with revision conflict responses and separate mutation audit events; preserve pointer-based compact preflight behavior.

Proof: contract tests shared by CLI, MCP and HTTP; negative authorization tests; cross-project and wrong-kind rejection.

## Phase A4: Agent-turn authoring and provenance

- Connect finished-plan capture to a first-class Plan identity instead of treating it as a generic spec; route finished spec updates into spec drafts, plan changes into plan drafts.
- Persist origin session_id, turn_id, AX agent_id, provider-issued ID where available, model, source (plan_mode, file, turn, edit), precise based-on revision and file/commit references.
- Support explicit update_spec / update_plan actions during a turn. Proposals may be bundled atomically, but each artifact owns its revision history.
- Detect drift against approved revisions; open review instead of rewriting approvals. Revisit plan compatibility when an approved spec changes.
- Capture only complete artifacts. Add tests for stream interruptions, retries, agent switching, ambiguous prompts and concurrent edits.

Proof: inspect one realistic multi-turn example and trace its separate spec/plan history to one agent, a session and the exact turns.

## Phase A5: Complete Specs/Plans UX overhaul

Replace the current flat list rather than adding a new screen next to it.

- Design navigation: Specs tree, child Plans, individual revision history; filters by project, PBI/work item, agent, session and status.
- Add spec details: goals, measurable acceptance criteria, approval/history, alternate plans, rules/skills applicability, linked code/tests and past decisions.
- Add plan details: phases/workstreams, active selection(s), compatibility with spec version, step progress, review and diff against previous revision.
- Create initial-spec-plus-plan and alternative-plan flows; introduce review queue for legacy orphans, conflicts and draft approvals.
- Add provenance drill-down (spec, plan, session, turn, agent) and direct links to logging and evidence.
- Reserve integration points for Authority, Manifest and Replay panels, which Plan B will populate.
- Maintain compact, keyboard-accessible, mobile-responsive layouts; preserve current AX theming and no unnecessary UI decoration.

Proof: browser/component tests for tree state, version switching, edit concurrency, approval/role checks, mobile/keyboard accessibility, empty/conflict/error states and links.

## Phase A6: Compatibility cutover and handoff

- Run end-to-end flows on migrated fixture and a fresh project. Compare old and new API behavior for clients expected to remain supported.
- Gate new UI/capture on rollout flag until migration reconciliation passes. Publish explicit rollback instructions.
- Update current product documentation and developer API references without new release-by-release pages.
- Deliver integration map and open items to Plan B; list every new API/data shape Plan B depends on.

Release gate, amended by the owner on 2026-11-22: SPEC-01..07 and SPEC-20, no data-loss or critical security regression, migration report signed off. SPEC-12..18 are out of this plan. They stay in the master spec for Plan B, with SPEC-08..11 and SPEC-19. Do not claim 95% replay fidelity in Plan A.

## Required phase report format

For each phase record: scope; edited paths; migration operations; new/reused entities; spec acceptance IDs; test commands and observed results; negative controls; unresolved risks; human approval decisions; commits/branch identity; exact source revision; next-phase prerequisites.
