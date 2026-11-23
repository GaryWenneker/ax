# AX Specification Workspace: UI/UX Overhaul Specification

> Accepted by the owner on 2026-11-22 at 16:57 (UTC+1) with the words "eerst voer het volgende uit (treat as accepted)". The text below is the accepted text. Only Markdown headings and list markers were added. The "Status" line is kept as written; the owner's acceptance supersedes it.

Status: Proposed / ready for agent review

Type: Product and interface specification

Scope: Replace the current flat Specs management experience while preserving existing data, version history, permissions, editor behavior and cross-module navigation.

Related: AX overarching Specs/Plans overhaul, Context Resolver, Authority Validator, Replay Validation.

## 1. Problem and evidence

The supplied screenshots show a flat Specs table mixing entries marked Plan with other specs; filter chips for All/Plans/Specs and status; title/path/version/logging/memory columns; a modal-like split Markdown/WYSIWYG editor with numerous version chips and diff controls. These affordances exist, but relationships, scope, ownership, active plan, and historical provenance are not immediately discoverable. Logging and Memory are exposed as repeated row buttons rather than a unified contextual view.

Do not assume: the screenshots prove a particular backend schema or that any given association is already persisted. The implementation agent must inspect actual repository models, APIs, migrations, and tests before changing them.

## 2. Outcomes

1. Users can find a specification, inspect all its plans, distinguish alternatives from execution phases, and understand current status without navigating a flat mixed list.
2. Users can manage specifications and plans independently, with immutable versions, explicit approval workflows, and traceable session/turn/agent origins.
3. Every spec and plan provides one-click, bidirectional access to scoped sessions, turns, memory, logging, decisions, rules, skills, and validation evidence.
4. The editor supports drafting, review, comparison, and approval without losing context.
5. Existing data remains accessible throughout migration; unresolved legacy relationships are not guessed.

## 3. Conceptual model and navigation hierarchy

Project / Workspace → Specifications → Linked Plans → Plan Steps / Execution Runs.

A spec is the authoritative definition of desired outcomes and constraints. A plan describes an implementation strategy, which may be a sequential phase or an alternative strategy. A plan may link to multiple specs; the primary UX groups plans under the spec being viewed without implying single-parent ownership. Use a many-to-many relationship with a relation type (implements, depends_on, alternative_to, phase_of) and optional primary presentation context. Each spec and plan has stable ID, independent versions, status, provenance, timestamps, and linked evidence.

Avoid forcing every legacy plan to have a parent during migration. New executable plans must link to at least one spec before approval, unless a documented exception is approved.

## 4. Information architecture

### 4.1 Main navigation

Rename the current Specs entry to Specifications (or Specs & Plans if consistent with AX terminology). The page has two presentation modes sharing one data source:

- Workspace view (default): searchable, expandable specification rows with nested plans, status and progress summaries. Users can switch project/workspace and filter by owner, technology, status, validation state, or recent activity.
- All documents view: a compact, sortable table for power users, preserving familiar column controls and fast filtering, with explicit Type and Parent/Linked Specs columns.

Provide separate Unlinked / Needs review section for migrated documents with uncertain classification or associations. Never silently fabricate a parent.

### 4.2 Three-pane desktop layout

Left: Specification explorer (resizable, collapsible). Project selector, search, filters, expandable specs with plan count, plan status and validation indicator; optional favorites and recent items. Each plan is clearly indented with icon and Phase / Alternative designation. A spec may be expanded without opening its editor.

Center: Detail workspace. Header with title, ID, type, status, current version, source and owner; breadcrumb; tabs Overview, Content, Plans (spec only), History, Validation, Dependencies. Show spec requirements and acceptance criteria prominently. For plan details show related spec(s), objective, phase/checklist, blockers, execution state, and version history. Edit opens a full-height editor within the center workspace, not an opaque overlay that hides navigation.

Right: Context & evidence panel (collapsible, resizable). Tabs Activity, Sessions, Turns, Memory, Logging, Decisions, Rules & Skills, Context Manifest. Each tab is automatically filtered to the current spec/plan and, where relevant, the selected version or historical time. Show counts, applied filters, timestamp range, origin links, and missing-data state. Never suggest empty results mean 'no history' when indexing is incomplete.

### 4.3 Smaller screens

At tablet width, explorer becomes a drawer and evidence becomes a secondary panel. On mobile, use one main column with tabs and a bottom-sheet navigation mechanism; do not shrink the desktop three-pane layout. Maintain deep links and keyboard access.

## 5. Primary flows

### A. Find and inspect a spec

Search by title, ID, PBI/issue ID, tags, linked plan, agent, or session. Results show document type, parent/related specs, status and last change. Open spec, expand its plans, inspect active plan and associated evidence without losing search position.

### B. Create a spec and its plans

Create spec with title, objective, requirements, constraints, acceptance criteria, source/PBI reference, and owner. Add plans through New plan, Link existing plan, or Generate proposed plan (the latter must remain a proposal until reviewed). Plan fields: objective, type (phase/alternative), steps, prerequisites, risks, test strategy, status, and linked specs. Support multiple plans, ordering for phases, and explicit alternative selection. Do not automatically approve generated text.

### C. Edit and review

Use Draft → In review → Approved → Change requested → Superseded/Archived states with role-appropriate actions. Changes to an approved document create a new draft revision; approved revision remains the active authoritative version until a replacement is approved. Allow line/section-level diff, side-by-side versions, author/agent provenance, review comments, change rationale, and unsaved-change protection. Concurrent changes require conflict handling; no last-write-wins overwrites.

### D. Navigate context in both directions

From a spec or plan, select Sessions, Turns, Memory, Logging or Decisions to open records filtered by exact document ID, optionally revision, project, agent, and time window. Clicking a record opens the corresponding native module with its filters and return link intact. From any such record, follow Related specification / Related plan back to the exact document revision. Preserve navigation state and offer a visible filter summary with Clear and Change scope. Linkage must be based on stable IDs and provenance, not only title similarity. Uncertain inferred links must be labelled Suggested and cannot masquerade as confirmed.

### E. Historical view and replay

Choose Current or As of turn/time. In historical mode show the spec revision, plans, decisions, applicable rules/skills and context evidence available at that point. Clearly label historical mode and prevent accidental edits of old snapshots. Provide Open context manifest and Create replay test actions when supported. Exclude later information from time-travel results.

### F. Legacy migration triage

Present Needs review items with original path, detected type, proposed links, confidence, source evidence and explicit Confirm / Reclassify / Leave unlinked actions. Keep old IDs, original paths and revisions; migration must be reversible or recoverable. No silent mass reparenting.

## 6. Visual design requirements

Follow AX's existing dark, compact, monochrome visual language with restrained lime accents for interactive focus. Do not color every status bright green. Use consistent badges for document type, lifecycle state, authority state, and validation state; do not conflate them. Use dense but readable row spacing, subtle indentation and disclosure chevrons for hierarchy. Avoid repetitive Logging and Memory buttons in each table row; replace with contextual counts and a single Context entry point. Provide empty/loading/error/partial-indexing states, skeletons where appropriate, and sticky contextual header. Keep document title prominent, raw filesystem path secondary. Make all controls accessible by keyboard, with visible focus and WCAG AA contrast.

## 7. Data and API contract (conceptual, verify before implementation)

- Specification: stable ID, project ID, title, metadata, lifecycle status, approved revision ID, timestamps.
- Plan: stable ID, title, type, execution status, approved revision ID, metadata.
- SpecPlanLink: spec ID, plan ID, relationship type, ordering, primary display flag, provenance, approval state.
- DocumentRevision: immutable revision ID, document ID/type, content hash, content, parent revision, author/agent, session ID, turn ID, created timestamp, review state.
- ContextEvidenceLink: document ID + optional revision ID, target kind (session/turn/memory/log/decision/rule/skill/manifest), target ID, link origin (explicit, derived, suggested), confidence and valid time interval.
- AuthorityState: applicable approved version, conflict flags, completeness and source provenance.

Required operations: list hierarchy with pagination; fetch spec/plan with revisions and links; create draft; diff revisions; submit/approve/reject; add/remove relationship with audit; fetch filtered evidence with cursor and time scope; resolve back-reference; fetch historical snapshot. All mutations must be permission checked and audited. Implement database indexes for project+status, link pairs, revision lookup and evidence target lookup; avoid N+1 calls.

## 8. Integration requirements

- Context Resolver consumes approved authoritative versions by stable ID, not whatever document is currently open in the editor.
- Authority Validator surfaces missing, conflicting, superseded or unapproved sources in the workspace.
- Rules and Skills are contextual references, with scope, priority and effective version visible; selecting a plan does not automatically override higher-priority rules.
- Replay Validation attaches results and evidence to the exact spec/plan revisions and manifest used.
- Logging and Memory native modules accept scoped deep-link filters and return-link context.
- CLI/MCP parity: equivalent read/navigation and mutation operations should be accessible through approved interfaces, with no dependence on a browser-only state.
- URL/deep links are stable and shareable within permission boundaries. Unauthorized targets must not leak titles, metadata or contents.

## 8A. Collaborative verification checkboxes (mandatory)

Every specification, plan, phase, step, requirement, and acceptance criterion MUST support an explicit, persistent verification checklist. Both the assigned AI agent and authorized human users can check and uncheck items directly in AX. A checked box is not merely Markdown decoration: it is a durable, version-aware, auditable workflow event.

### 8A.1 User experience

- Show a checkbox beside each actionable item in the spec/plan detail and editor, with a compact progress indicator (verified / total) and filters (All, Open, Agent checked, Awaiting human validation, Accepted, Reopened, Blocked).
- Clicking an item opens its requirement, validation evidence, responsible actor, timestamp, linked session/turn, and any blocking issues. A user can check or uncheck an item without editing the whole document.
- Agent updates appear live or after refresh and must not silently overwrite concurrent user changes.
- Preserve Markdown compatibility (- [ ], - [x]) for import/export, but do not use raw Markdown position or title as item identity. Every item has a stable ID and is tied to an exact spec/plan revision.
- Support nested checklists and roll-up progress, but do not automatically mark a parent verified solely because all children are checked.
- Provide keyboard and screen-reader operation, bulk operations with confirmation, and a clear distinction between agent-reported completion and human acceptance.

### 8A.2 Validation states and permissions

- Separate implementation_state (open, in_progress, agent_completed, reopened, blocked) from validation_state (not_reviewed, pending_review, accepted, rejected, waived).
- Agent may check/uncheck its own completion status and attach evidence. A human with edit permission may check/uncheck completion and accept/reject validation. An agent must not impersonate human acceptance or auto-approve critical requirements.
- Every check/uncheck requires an actor ID/type, time, source session/turn where applicable, revision ID, reason for reopening or rejection, and optional test/commit/manifest links.
- If a verified requirement is materially changed in a newer revision, mark its prior validation as stale for that revision and require revalidation; retain historical approval.
- A failed test, missing evidence, or changed implementation can reopen an item. Never erase previous validation records.
- Critical security/architecture acceptance criteria require explicit independent verification and must all be accepted before release. A checkbox alone is not evidence.

### 8A.3 Persistence and API

- Introduce stable checklist_item_id, document_id, document_revision_id, parent_item_id, item_type, required_evidence, and ordered presentation metadata.
- Store immutable checklist_events (checked, unchecked, submitted_for_review, accepted, rejected, reopened, waived) and derive current status from authorized events. Provide optimistic concurrency/version checks and idempotency keys.
- Expose list/filter, update-state, event-history, evidence-attachment, and revision-diff operations through the existing authorized AX interfaces. Agent actions must work through CLI as well as any permitted MCP/API interface.
- Enforce RBAC, reject invalid transitions, avoid double-counting shared plans, and keep historical/as-of-turn snapshots reproducible.
- Preserve checklist status during import/migration only when an existing explicit mapping is trustworthy; otherwise initialize not_reviewed rather than inventing completion.

### 8A.4 Verification checklist for the overhaul itself

The following checkboxes are part of this specification and must be imported into AX as independently tracked items. Initial state is unchecked. An agent may check implementation items only after recording evidence; a human may reopen them at any time. Human sign-off is a separate action.

- [ ] CHK-001 Inventory existing Specs/Plans models, UI, permissions and migration constraints; attach findings.
- [ ] CHK-002 Implement stable spec/plan identities, links and immutable revisions; attach schema and tests.
- [ ] CHK-003 Migrate legacy content without data loss; attach dry-run and reconciliation report.
- [ ] CHK-004 Build hierarchy explorer, search, filtering and deep links; attach UI/e2e evidence.
- [ ] CHK-005 Implement independent spec and plan editing, diff and review; attach tests.
- [ ] CHK-006 Implement scoped navigation to sessions, turns, memory, logs and decisions; attach e2e evidence.
- [ ] CHK-007 Implement time-travel/as-of-turn filtering and backlinks; attach tests.
- [ ] CHK-008 Implement interactive agent/human check/uncheck, independent validation states and audit history; attach tests.
- [ ] CHK-009 Implement evidence attachments, stale-validation detection and reopening; attach tests.
- [ ] CHK-010 Verify permissions, accessibility, concurrency and CLI parity; attach test reports.
- [ ] CHK-011 Validate shadow rollout, rollback and legacy data parity; attach operational evidence.
- [ ] CHK-012 Complete human review of all required criteria and record release sign-off.

### 8A.5 Checklist acceptance tests

- AC-16: Agent checks an item with evidence; human sees agent completion but not automatic human approval.
- AC-17: Human unchecks/reopens the item; agent sees the new state and the complete history remains visible.
- AC-18: Two simultaneous edits produce a safe conflict response, never a silently lost update.
- AC-19: A changed approved requirement becomes pending revalidation in the new revision; historical approval stays queryable.
- AC-20: Markdown import/export round-trips checklist IDs and statuses without duplication or loss.
- AC-21: As of turn N displays only events known at N, including check/uncheck history.
- AC-22: Critical acceptance cannot be approved by an agent alone, and failed or missing evidence blocks release.
- AC-23: CLI-driven checklist updates work without requiring MCP and enforce the same authorization/audit rules.

## 9. Agent execution plan

Phase 0: Inventory and UX contract. Inspect current Specs page, editor, backend endpoints, spec registry, revision persistence, navigation, memory/logging filters, auth, and tests. Produce a compatibility matrix and identify unsupported assumptions. Confirm UI states with screenshots or storybook fixtures before migration.

Phase 1: Model and migration. Implement or adapt first-class spec/plan entities, relationship table, immutable revisions, evidence links and migration diagnostics. Build migration dry-run and integrity checks. Preserve old records and provide rollback/recovery.

Phase 2: Navigation and explorer. Deliver new workspace hierarchy, search, filter, sorting, deep-link routing, unlinked queue, and all-documents view. Keep existing editor functional.

Phase 3: Detail and editing. Deliver spec/plan detail pages, plan creation/linking, lifecycle, version diff, review workflow, concurrency protection, and accessible split editor.

Phase 4: Context evidence. Deliver right evidence panel and bidirectional filtered navigation to sessions, turns, memory, logging, decisions, rules and skills. Add historical snapshot mode and context manifest links.

Phase 5: Hardening and rollout. Accessibility, performance, permission tests, migration reconciliation, shadow comparison, feature flag, monitoring and rollback. Retire old page only after data parity and acceptance signoff.

Every phase must expose its own checkable items in AX, permit agent and human check/uncheck, and require evidence before agent completion or human acceptance. Each phase must include implementation, automated tests, migration evidence where relevant, screenshots for desktop/mobile, and a concise completion report. Do not mark a phase complete based only on code compiling.

## 10. Acceptance criteria (minimum)

1. A spec with three plans displays as one expandable spec with three distinguishable child plans; an alternative plan is not mistaken for a sequential phase.
2. A plan linked to two specs appears in both contexts with one stable identity and one revision history.
3. Search by ID, title and external PBI finds the correct document and retains filters after returning from detail.
4. Editing an approved spec creates a new draft and leaves the approved revision unchanged until approval.
5. Users can compare two spec revisions and two plan revisions independently.
6. Clicking Memory or Logging from a spec/plan opens the native module with exact ID-based filters, and returning restores the document/revision context.
7. Clicking a linked turn from a plan shows that turn; clicking the plan backlink returns to the same revision.
8. Switching to As of turn N excludes every later revision and decision, including later rule/skill changes.
9. Missing, unindexed or ambiguous evidence is shown as such; AX never presents it as verified completeness.
10. A migrated legacy plan with no trustworthy parent remains in Needs review, with its original content and version history intact.
11. Unauthorized users cannot read protected documents or infer protected links through filters or counts.
12. All essential flows work by keyboard and on narrow screens; focus, contrast and error handling meet accessibility requirements.
13. On representative data volumes, expanding a spec and opening evidence remains responsive; agent must set and report measurable latency budgets after inspecting existing performance baselines.
14. Existing editor capabilities (Markdown, WYSIWYG, split view, revision comparison) remain available or have explicitly approved replacements.
15. Replay reports link to the exact spec, plan, manifest and validation run, and can navigate back without losing scope.
16. All checklist acceptance tests AC-16 through AC-23 in section 8A pass.

## 11. Non-goals

This UI overhaul does not itself prove the 95% replay reproduction or 100% critical compliance targets; those belong to the independent validation engine. It must, however, expose their evidence accurately. Do not rewrite unrelated AX navigation, reimplement the entire memory engine, or replace existing storage without a documented migration reason.

## 12. Deliverables

- Updated UX information architecture and component inventory.
- Data/API compatibility assessment and migration plan.
- Working desktop/tablet/mobile specification workspace.
- Reusable explorer, document header, plan list, editor, revision comparison, evidence panel, and filter/deep-link components.
- Automated integration/e2e/accessibility tests and screenshots.
- Migration audit, feature flag, rollback plan, and final acceptance matrix.
- Persistent collaborative checklist UI, agent/CLI update contract, event audit log, and human validation workflow.

Definition of done: every acceptance criterion is demonstrated with test evidence, all legacy documents remain accessible, and the new workspace replaces the flat mixed list without losing context or provenance.

## Owner decisions after acceptance

Recorded on 2026-11-22, after Phase 0, from the owner's answers. The accepted text above is unchanged; these decisions refine it.

1. Live database: when the owner turns the `spec_workspace` flag on for the live project, the workspace tables may be created there and CHK-001 to CHK-012 imported as tracked items.
2. Criterion 11 ("protected documents") means project and role access. Nothing about a document, including its title, counts, and links, reaches a caller without access to its project, and filters and counts do not reveal it.
3. Section 8A.2 gains the implementation state `completed` for completion reported by a human. `agent_completed` stays for completion reported by an agent.
4. The navigation entry is named "Specs & Plans".
