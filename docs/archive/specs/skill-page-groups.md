# SPEC — Command Center skill groups (collapsible)

- Tier: 2
- Setup plan:
  - Tools to install: none
  - Git: work on the current branch; no checkpoint commits unless the user asks
  - Isolation: none (landing tree) — Command Center UI and policy crates already live here; a fresh worktree would miss `web-ui/node_modules` and `target-dev/`
  - Files the gauntlet will add, by path:
    - `docs/specs/skill-page-groups.md` (this spec)
    - `docs/specs/skill-page-groups-EVIDENCE.md` (evidence report)
    - `crates/ax-policy/data/skill-groups.json` (canonical catalog)
    - `crates/ax-policy/src/skill_groups.rs` (resolve + tests)
    - `crates/ax-policy/src/skill_groups.rs` tests + parse/serialize tests in `crates/ax-policy/src/parse.rs`
    - `crates/ax-web/web-ui/src/skillGroups.ts` (import catalog JSON; list grouping + expand helpers)
    - `tools/gauntlet-skill-groups.sh` (entry point for layers we can run)
  - New dependencies: none (reuse existing cargo tests, `tsc`, JSON import in Vite)

## API gates (old-coder-api)

Scope: internal Command Center / `ax web` JSON · existing surface.

| Gate | Result |
|---|---|
| Boring | Additive optional `group` on skill JSON; optional `groups` catalog on `GET /skills`. No new resource noun required. |
| Don't break userspace | Additive only. Existing clients ignore unknown fields. Do not rename `tags` or remove the flat list. |
| Authentication | Unchanged (`ax web` local hub). |
| Authorization | Unchanged workspace-scoped policy store. |
| Idempotency | `PUT` skill already keyed by name; `group` is a field on that resource. N/A new mutating verb. |
| Blast radius | Catalog is a small static JSON; list still returns all skills (bounded per project). |
| Pagination | N/A — skill lists remain project-bounded (existing pattern). |
| Expensive fields | Catalog is static, not a join. |
| No implementation leakage | Catalog ids are kebab-case product ids, not SQLite column names. |

## Analysis (why this order)

Existing shipped skills cluster around an agent turn: **start the session**, **explore**, **design/implement under quality constraints**, **test/debug**, **review/PR**, **CI/release/deploy**, then **project/product/comms/tooling**. The catalog follows that sequence so the Skills page reads as a workflow, not an A–Z dump.

Inference aliases below map **legacy skills that have no `group` field** into the catalog so the page is grouped on first load. New saves persist `group` explicitly.

## Canonical catalog (fixed order)

`id` is the persisted value. `aliases` match tag or skill name (case-insensitive) only when `group` is absent or empty. Empty groups are **not rendered** on the list. The editor offers **every** catalog id except that empty groups still appear in the picker.

| order | id | label | What belongs here | Seed aliases (legacy) |
|---:|---|---|---|---|
| 10 | `session-protocol` | Session & protocol | Turn start, preflight, startup | `preflight`, `startup`, `workflow` |
| 20 | `agent-orchestration` | Agent orchestration | Subagents, Task tool, multi-agent | `subagents` |
| 30 | `exploration` | Exploration & research | Code search, understand, onboarding | `onboarding`, `explore` |
| 40 | `architecture` | Architecture & domain | Bounded contexts, domain graph | `domain`, `graph` |
| 50 | `design` | Design & UX | Design-first, UI/UX workflows | `design`, `design-first` |
| 60 | `implementation-quality` | Implementation & evidence | Spec/gauntlet/old-coder | `old-coder`, `evidence`, `quality` |
| 70 | `apis-contracts` | APIs & contracts | HTTP/JSON, public surfaces | `old-coder-api`, `api` |
| 80 | `testing` | Testing | TDD, test plans, test skills | `testing`, `tdd`, `azdo-testing` |
| 90 | `debugging` | Debugging | Systematic debugging, incident diagnosis | `debugging`, `systematic-debugging` |
| 100 | `refactoring` | Refactoring | Safe structural change | `refactor`, `refactoring` |
| 110 | `security` | Security | Auth, secrets, threat models | `security`, `auth` |
| 120 | `performance` | Performance | Latency, profiling, budgets | `performance`, `perf` |
| 130 | `data-persistence` | Data & persistence | DB, migrations, storage | `database`, `sql` |
| 140 | `conventions` | Conventions & style | Naming, prefixes, house style | `conventions`, `no-ab-prefix` |
| 150 | `documentation` | Documentation | Guides, README, site docs | `docs`, `documentation` |
| 160 | `code-review` | Code review | Review workflows | `code-review`, `azdo-code-review` |
| 170 | `pull-requests` | Pull requests | Open/update PRs, pre-PR checks | `pr`, `pre-pr-check`, `pull-request` |
| 180 | `git` | Git & version control | Branching, history, remotes | `git` |
| 190 | `cicd` | CI/CD & pipelines | Pipelines, build-once deploy-many | `cicd`, `pipeline`, `azdo-pipelines` |
| 200 | `release` | Release & versioning | Tags, ship, version bumps | `ship`, `release`, `azdo-release` |
| 210 | `deploy-ops` | Deploy & operations | Production deploy, runbooks | `deploy` |
| 220 | `observability` | Observability | Logging, traces, metrics | `logging`, `observability` |
| 230 | `project-management` | Project management | Tickets, AzDO, refinement | `azdo`, `azdo-refinement`, `azdo-development` |
| 240 | `product` | Product workflows | Domain product skills (mail, notifications) | `product`, `feature-information`, `noti` |
| 250 | `communication` | Communication | Slack, email, tone/style of answers | `communication`, `preq`, `auti` |
| 260 | `integrations` | Integrations | Third-party APIs not covered above | `integrations` |
| 270 | `tooling` | Tooling & environment | CLI reinstall, local toolchain | `tooling`, `ax-reinstall` |
| 999 | `ungrouped` | Ungrouped | No group and no alias hit | _(fallback only)_ |

Alias collision rule: first catalog row in **order** whose alias matches a tag or the skill `name` wins. Explicit `group` always wins (must be a catalog id; unknown id → `ungrouped` for display, still round-tripped in YAML if present).

## Scenarios

```gherkin
Feature: Grouped, collapsible Skills list with a durable catalog

  Scenario: Catalog has a fixed order
    Given the canonical skill-groups.json catalog
    When groups are listed for the editor
    Then ids appear in the order column sequence 10..270 then 999
    And every id in the table above is present even if no skill uses it

  Scenario: Empty groups are omitted from the list
    Given visible skills resolve only to session-protocol and testing
    When the Skills page builds group nodes
    Then rendered group ids are exactly [session-protocol, testing]
    And security, refactoring, and other empty catalog ids are absent

  Scenario: Empty groups remain assignable
    Given no skill is in group security
    When the skill editor loads the group picker
    Then security is still an option with label "Security"

  Scenario: Explicit group wins over aliases
    Given a skill named startup with tags [preflight] and group testing
    When resolve_skill_group runs
    Then the resolved group id is testing

  Scenario: Legacy skill without group uses aliases
    Given a skill named old-coder with tags [old-coder, evidence] and no group
    When resolve_skill_group runs
    Then the resolved group id is implementation-quality

  Scenario: No group and no alias is ungrouped
    Given a skill named mystery with tags [zzz] and no group
    When resolve_skill_group runs
    Then the resolved group id is ungrouped

  Scenario: Ungrouped node is hidden when unused
    Given every visible skill resolves to a catalog id other than ungrouped
    When the Skills page builds group nodes
    Then ungrouped is not rendered

  Scenario: Group nodes expand and collapse
    Given a rendered group node testing with two skills
    When the group header is toggled once
    Then the child skill rows are not shown
    When it is toggled again
    Then the child skill rows are shown
    And aria-expanded reflects the open state

  Scenario: Filter does not show empty groups
    Given skills in testing and session-protocol
    When the user filters so only testing skills remain
    Then only the testing group node is rendered

  Scenario: Create skill can persist a currently empty group
    Given group performance has zero skills
    When a new skill is saved with group performance
    Then frontmatter contains group: performance
    And GET skill JSON includes group performance
    And the Skills list shows the Performance group with that skill

  Scenario: GET /api/policy/skills stays compatible
    Given an existing client that reads skills[].name and skills[].tags
    When GET /skills returns the new payload
    Then those fields are unchanged in type and meaning
    And skills[].group is an added optional string
    And groups is an added optional array of {id, label, order}

  Scenario: Invalid group id is not a 500
    Given a saved skill with group not-a-real-id
    When the skill is listed
    Then HTTP 200
    And resolved display group is ungrouped
```

```gherkin
Feature: Grouped, collapsible Rules list (same catalog as skills)

  Scenario: Empty groups are omitted from the Rules list
    Given visible rules resolve only to conventions and exploration
    When the Rules page builds group nodes
    Then rendered group ids are exactly [exploration, conventions]
    And empty catalog ids are absent

  Scenario: Seeded rule id without group uses aliases
    Given a rule id explore-before-grep with no group
    When resolve_skill_group runs with name=id
    Then the resolved group id is exploration

  Scenario: GET /api/policy/rules stays compatible
    Given an existing client that reads rules[].id and rules[].tags
    When GET /rules returns the new payload
    Then those fields are unchanged in type and meaning
    And rules[].group is an added optional string
    And groups is an added optional array of {id, label, order}

  Scenario: Matching ignores rule group
    Given two rules that differ only in group
    When ax_preflight matches
    Then group is not used as a match input
```

## Must NOT

- Remove or flatten the existing table columns when no skill is selected (grouped list **replaces** the ungrouped tbody; detail split still works).
- Require a DB migration that drops tags or other skill columns.
- Show empty catalog groups on the Skills list.
- Hide empty groups from the editor picker.
- Break `GET/POST/PUT /api/policy/skills` by renaming or removing existing JSON fields.
- Change matching (`ax_preflight` skill selection) based on group — group is display + persistence only.
- Add npm dependencies.

## Revisions

- 2026-08-27: Initial spec from Command Center Skills page analysis and shipped skill tags.
- 2026-08-27: Approved by user request to adapt the app so grouping works as specified (`ik wil dat je de app aanpast zodat dit zo werkt`).
- 2026-08-27: Same catalog, collapse, empty-group, and persist behaviors apply to **Policy → Rules** (`doe ook voor de rules`). Rule YAML `group`, `policy_rules.skill_group` (schema v19), additive `GET /api/policy/rules` `groups` catalog. Matching still ignores `group`. Seeded rule ids resolve via catalog aliases (name = rule `id`).
