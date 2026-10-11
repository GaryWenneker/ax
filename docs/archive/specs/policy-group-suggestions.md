# SPEC — Content-based group suggestions for Rules and Skills

- Tier: 2
- Spec approval: **pending** (this document is the approval artifact)
- Setup plan:
  - Tools to install: none
  - Git: current branch; no checkpoint commits unless the user asks
  - Isolation: none (landing tree) — Command Center + `ax-policy` already live here; a fresh worktree would miss `web-ui/node_modules` and `target-dev/`
  - Files the gauntlet will add, by path:
    - `docs/specs/policy-group-suggestions.md` (this spec)
    - `docs/specs/policy-group-suggestions-EVIDENCE.md` (after implementation)
    - `crates/ax-policy/src/skill_groups.rs` (content scorer + tests)
    - `crates/ax-policy/data/skill-groups.json` and `crates/ax-web/web-ui/src/skill-groups.json` (optional extra `keywords` if aliases are not enough; keep copies identical)
    - `crates/ax-db/src/migrations.rs` (schema v20) + `crates/ax-db/tests/migration_v20.rs`
    - `crates/ax-policy` parse/index/store/types for suggestion vs confirmed
    - `crates/ax-web/src/policy.rs` (suggest + confirm routes)
    - `crates/ax-web/web-ui` Rules/Skills list + editors
    - `tools/gauntlet-skill-groups.sh` (extend) or `tools/gauntlet-group-suggestions.sh`
    - site docs + README (same change as the feature)
  - New dependencies: none

## Product model

Three mutually exclusive states per rule and per skill:

| State | List folder | Visual | YAML `group` |
|---|---|---|---|
| **Confirmed** | that catalog id | no suggestion flag | written (`group: <id>`) |
| **Suggested** | the suggested catalog id | **Suggested** chip + **Confirm** | **not** written |
| **Ungrouped** | `ungrouped` only if at least one item is in this state | no chip | empty |

`ax_preflight` / matcher **never** read group or suggestion.

### How a suggestion is computed (deterministic, local, no LLM)

Haystack = lowercase concatenation of:

- rule `id` or skill `name`
- tags (space-separated)
- body (truncated at 8_192 UTF-8 bytes)

For each catalog group except `ungrouped`, in **catalog order**:

- Score starts at 0.
- For each string in `aliases` plus the group `id`, if that string (lowercase) appears as a **substring** of the haystack, add **1**.
- Extra: if the item `id`/`name` **equals** an alias or the group id, add **2** (so id `explore-before-grep` still wins exploration even when the body is generic).

Pick the group with the **highest** score. Ties: first in catalog order. If the winning score is **0**, result is **no suggestion** (ungrouped, not suggested).

This is the same catalog as today’s folders. Content (body) participates; id/tags still help.

### When suggestions are created (once)

Run the scorer and **persist a suggestion** only when **all** of:

1. There is no confirmed group (`group` empty in YAML / confirmed flag false).
2. No suggestion has been stored yet (`group_suggested` empty).

Triggers:

- **New save** (create rule/skill): if the editor did not pick a catalog group other than leaving it unset, persist a suggestion from content. Picking a group in the editor is **confirm** (write YAML `group`, no chip).
- **Page open**: Command Center Rules page calls `POST /api/policy/rules/suggest-groups`; Skills page calls `POST /api/policy/skills/suggest-groups`. The handler is **idempotent**: only fills rows that still have no confirmed group and no stored suggestion. Opening the page a second time must **not** recompute or move existing suggestions.
- Re-index from files: if YAML has `group:`, that item is **confirmed**. If YAML has no `group`, **keep** any existing DB suggestion (do not wipe). If YAML has no `group` and DB has no suggestion, leave empty until the next page-open suggest (or create save).

### Confirm and edit

- **Confirm** (list button): write YAML `group` to the suggested id, set confirmed, **clear** the suggestion flag. Chip disappears. Item stays in that folder.
- **Edit** (group `<select>` in editor / inline workspace): choosing any catalog id (including a different folder or `ungrouped`) is **confirm** of that choice. Chip disappears. `ungrouped` means confirmed-empty: no later auto-suggest (user already chose).

**Confirmed-empty (`ungrouped` after explicit pick)** vs **never suggested**: persist `group_confirmed=1` with empty group so page-open does not suggest again.

### UI (English)

- Nested list row: muted **Suggested** chip next to the id/name; **Confirm** button in the actions cell (does not open the editor).
- `aria-label`: `Confirm group {label} for {id}`.
- Confirmed rows: no chip, no Confirm.
- Empty groups still hidden; a suggestion **counts** as occupying a folder (folder appears).
- Collapse state unchanged (`rules-groups-collapsed` / `skills-groups-collapsed`).

## API gates (old-coder-api)

Scope: internal Command Center / `ax web` · existing policy JSON.

| Gate | Result |
|---|---|
| Boring | Additive fields on existing list rows. New POST on the existing `/api/policy/rules` and `/skills` nouns: `suggest-groups` (bulk, idempotent) and `{id}/confirm-group` (single). |
| Don't break userspace | Keep `group` as the **display** catalog id (confirmed or suggested). Add `groupSuggested: boolean` (true only in Suggested state). Add `groupConfirmed: boolean`. Existing clients that only read `group` still place the row; they ignore the new flags. Do not remove alias resolution from scorer. |
| Authentication | Unchanged (`ax web` local hub). |
| Authorization | Unchanged workspace store; `AX_WEB_READONLY=1` → 403 on POST confirm/suggest. |
| Idempotency | `POST …/suggest-groups` is intrinsically idempotent (fills only empty). `POST …/confirm-group` is idempotent: confirming an already-confirmed item is 200 no-op. Duplicate confirm is acceptable (low stakes). |
| Blast radius | Suggest walks project-bounded rule/skill lists (same as GET). Body scan capped at 8_192 bytes per item. |
| Pagination | N/A — existing project-bounded lists. |
| Expensive fields | Suggestion is stored, not recomputed on every GET. GET remains a read. |
| No implementation leakage | JSON uses `groupSuggested` / `groupConfirmed`, not SQLite column names. |

### Endpoints

`POST /api/policy/rules/suggest-groups`  
`POST /api/policy/skills/suggest-groups`  
→ `200` `{ "updated": <number of rows that received a first suggestion> }`  
Does not change confirmed items. Does not overwrite an existing suggestion.

`POST /api/policy/rules/{id}/confirm-group`  
`POST /api/policy/skills/{name}/confirm-group`  
→ `200` updated row. `404` if missing. If already confirmed, **200 no-op**. If ungrouped with no suggestion, **200 no-op**.

`GET /api/policy/rules` and `GET /api/policy/skills` stay reads. Additive fields only. **No** suggest side effect on GET (page calls POST then GET).

List row display group:

```
if confirmed && group set → that id
else if suggestion set → that id, groupSuggested=true
else → ungrouped, groupSuggested=false
```

## Schema (v20)

`policy_rules` and `policy_skills`:

- `group_suggested TEXT` — catalog id or NULL
- `group_confirmed INTEGER NOT NULL DEFAULT 0` — 1 after confirm or explicit editor group (including explicit ungrouped)

Existing `skill_group` column remains the **confirmed** YAML group (empty if none). Suggestion is **not** copied into YAML until confirm.

Bump `CURRENT_SCHEMA_VERSION` to 20; pin prior migration tests to `== 20`.

## Scenarios

```gherkin
Feature: Content-based group suggestions

  Scenario: Body text suggests a folder
    Given a rule id "custom-rule" tags [] body containing "never use UTF-16" and "BOM"
    And no YAML group and no stored suggestion
    When suggest-groups runs
    Then group_suggested is conventions
    And group_confirmed is 0
    And YAML has no group field
    And GET list shows group conventions and groupSuggested true

  Scenario: Id match still suggests without waiting for body
    Given a skill name "explore-before-grep" with empty body and no group
    When suggest-groups runs
    Then group_suggested is exploration

  Scenario: Zero score stays ungrouped
    Given a rule id "zzzz" body "asdf qwer" tags []
    When suggest-groups runs
    Then group_suggested is empty
    And display group is ungrouped
    And groupSuggested is false

  Scenario: Suggest is once-only
    Given an item already has group_suggested testing
    When the body is changed to only talk about deploy
    And suggest-groups runs again
    Then group_suggested remains testing

  Scenario: Confirmed YAML group is never auto-replaced
    Given YAML group: security and group_confirmed 1
    When suggest-groups runs
    Then group remains security
    And groupSuggested is false

  Scenario: Confirm writes YAML and clears the chip
    Given a suggested item in conventions
    When POST confirm-group
    Then YAML contains group: conventions
    And group_confirmed is 1
    And group_suggested is empty
    And GET list groupSuggested is false
    And the item is still under Conventions

  Scenario: Confirm when already confirmed is a no-op
    Given a confirmed item
    When POST confirm-group
    Then HTTP 200
    And the stored group is unchanged

  Scenario: Editor pick confirms a different folder
    Given a suggested item in testing
    When the user saves group performance from the editor
    Then YAML group is performance
    And groupSuggested is false
    And the item appears under Performance

  Scenario: Explicit ungrouped blocks later auto-suggest
    Given the user saves group ungrouped from the editor
    When suggest-groups runs
    Then no suggestion is stored
    And the item stays in Ungrouped

  Scenario: New create without a picked group stores a suggestion
    Given POST create skill with empty group and body mentioning "pytest" and "tdd"
    Then the saved item is suggested into testing
    And YAML has no group

  Scenario: New create with a picked group is confirmed
    Given POST create with group: git
    Then groupSuggested is false
    And YAML has group: git

  Scenario: Page open fills only the untouched items
    Given rules A (no group, no suggestion) and B (already suggested)
    When POST rules/suggest-groups
    Then updated >= 1
    And B's suggestion is unchanged
    And A has a first suggestion or ungrouped if score 0

  Scenario: GET does not invent suggestions
    Given A has no group and no suggestion
    When GET /rules without POST suggest-groups
    Then A is ungrouped
    And groupSuggested is false

  Scenario: Readonly hub rejects writes
    Given AX_WEB_READONLY=1
    When POST suggest-groups or confirm-group
    Then HTTP 403

  Scenario: Matching ignores group fields
    Given two rules that differ only in group and suggestion flags
    When the matcher runs
    Then match results are identical
```

## Must NOT

- Use an LLM or network call to classify groups.
- Side-effect GET list.
- Let suggestion rewrite a confirmed group.
- Re-score on every page open.
- Feed `group` / suggestion into `ax_preflight` matching.
- Show empty catalog folders.
- Add npm/cargo dependencies.

## Revisions

- 2026-08-27: Draft from user request (content suggestion, visible flag, confirm, edit, auto-suggest on new + page open when none yet).
