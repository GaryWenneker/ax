---
title: Policy Engine
description: IDE-agnostic rules and skills for AI agents — stored in .agents/, indexed locally, delivered via MCP without reading source files.
---

ax ships a **policy engine**: project-local rules and skills that work in any IDE or agent harness — not tied to Cursor rules or a single vendor format.

Policy files live under `.agents/rules/` and `.agents/skills/`, are indexed into SQLite (`ax.db`), and reach the agent through **MCP tools** or the **prompt-hook**. Agents should **not** open `.mdc` / `SKILL.md` files on disk when policy MCP tools are available — the matched text is returned in the `inject` field. Private overlays stay under `.ax/policy-private/` and `~/.ax/private_policy/`. Disabled items move to `.ax/policy-inactive/` (gitignored). Legacy `.ax/policy/{rules,skills}` is still read for one release and migrated into `.agents/` on index/init.

## Quick start

```bash
ax init                    # creates .agents/ and seeds default preflight rules/skills
ax policy index            # index policy files into ax.db
ax policy sync [--fix] [--force]  # verify; --fix creates an unseen template; --force writes a removed or drifted one back
ax policy history                 # removal record
ax policy restore-item <id>       # put one removed rule or skill back
ax policy match "deploy"   # test which rules/skills match
ax policy capture "always use mobile-first CSS"   # propose rule from directive language
ax policy storage status   # show effective files vs database mode
ax policy rules            # list the indexed rules
ax policy skills           # list the indexed skills
```

Rules and skills are files. Edit them with any editor, then run `ax policy index`. To browse them, use `ax policy rules` and `ax policy skills`, or the **Rules** and **Skills** pages of [ax-admin](/guides/remote-admin/). ax-admin has no rule or skill editor; see [Editing and browsing](#editing-and-browsing).

On **`ax init`** and **`ax install`**, ax also seeds machine-wide policy. Existing files are left alone **unless** the template has a higher `seedVersion` than the on-disk copy, or gained `alwaysApply: true` or a `require-skill` guard the copy does not have yet. In those cases ax rewrites the file from the template, including local edits:

| Location | Content |
|---|---|
| `~/.ax/global_policy/rules/` | `old-coder-mandatory` (CRITICAL, **alwaysApply**, `guard: require-skill: "old-coder"`) — agents must follow the old-coder workflow for implementation work, including the review loop before EVIDENCE |
| `~/.ax/global_policy/skills/` | `old-coder` (**alwaysApply**, injects on every turn including empty prompts), `old-coder-api` (matched on API triggers; load full body via `ax_skill`), `review-loop` (code review loop, see below), `pr-review-comments` (review of a colleague's pull request, see below), `pr` (draft pull request, then review until a round has zero findings), `dotnet-code-review`, `typescript-review`, `react-review` |
| `~/.cursor/skills/` | Same skill bundles for Cursor agent discovery |

**Enforcement model:** Rules and skills with `alwaysApply: true` are injected on **every** `ax_preflight` turn (including empty or one-word prompts). Always-apply inject is never hard-truncated. `ax_guard` blocks Write/Delete when a CRITICAL rule declares `guard: require-skill: "old-coder"` unless that skill is indexed, approved, enabled, and `alwaysApply`. Policy files under `.ax/policy/` and `crates/ax-policy/templates/` are exempt so seed/index can repair a missing skill.

Every `ax init` re-imports all policy layers (including global) into `ax.db`. After install alone, run `ax init` or `ax policy index --force` in any project once.

Source: [AmazingAng/old-coder](https://github.com/AmazingAng/old-coder) (MIT). Project init also copies rollout skills into `<project>/.agents/skills/` (linked from `.cursor/skills/`): `pr`, `old-coder`, `old-coder-api`, `dotnet-code-review`, `typescript-review`, and `react-review`. `review-loop` and `pr-review-comments` stay machine-wide (`~/.ax/global.db` and `~/.cursor/skills/`) so a project copy does not duplicate them. The `pr` skill reviews the draft it opened and keeps fixing until a review round has zero findings.

### Review loop

`old-coder-mandatory` adds a **review loop** between the gauntlet and EVIDENCE: SPEC → RED → GREEN → REFACTOR → GAUNTLET → REVIEW LOOP → EVIDENCE. The agent loads it with `ax_skill({ name: "review-loop" })`. Each round does four things:

1. **Skill check.** It works out the stacks from the diff and `ax.json` `policy.stacks`, then loads each stack's review skill (for example `rust-review`) through `ax_skill`. Each skill is recorded as `usable`, `missing`, or `empty`.
2. **Stack review.** It reviews the diff against every usable skill plus a generic checklist. Findings get an id (`R<round>-<n>`), a severity, a `file:line`, and the skill section they come from.
3. **Process findings.** Behavioral findings start with a RED test. The user decides any disputed finding.
4. **Repeat.** It runs a new round until a round has zero findings. There is no round cap.

EVIDENCE includes a table of review rounds. `review-loop` is also stored as a company skill in `~/.ax/global.db`, so `ax_skill("review-loop")` works in projects that have no local copy. If that store fails, install and init print a note and carry on.

### Reviewing a colleague's pull request

The review loop fixes findings, which is right for your own change but wrong for someone else's pull request. For a colleague's PR, the agent loads `ax_skill({ name: "pr-review-comments" })` instead. It runs the same skill check and stack review, but it does not change any code. Every finding becomes a draft comment, and the agent asks you one question per comment (`AskQuestion` in Cursor, `AskUserQuestion` in Claude Code). Each question offers one or more proposed texts (`Post: …`), `Do not post`, and free text for your own wording. The agent posts only the comments you chose, then lists what it posted and what it skipped. Before the comments, it always tries to link the work item to the pull request (`az repos pr work-item add` on Azure DevOps, or an artifact link on GitHub) and reports the error when that attempt fails. `pr-review-comments` is seeded next to `review-loop` and stored in `~/.ax/global.db` the same way. Comments the agent posts on a pull request use `reviews.commentLanguage` from ax settings (`dutch-pr-comments` documents that override). Chat with the user, website docs, commit messages, and other product copy stay **English** (`english-only`).

Restart your agent so MCP exposes `ax_preflight`, `ax_rules`, `ax_skill`, `ax_guard`, and `ax_policy_capture`.

---

## Policy capture

When a user gives a **durable directive** — phrases like `always`, `you must`, `never`, `@rule`, or Dutch equivalents — agents can turn it into a team rule without hand-authoring YAML.

**Flow:** detect directive → propose rule + interview questions → confirm with user → save.

```bash
# Preview proposal (no write)
ax policy capture "always run tests before committing"

# Save with defaults (skip interview — use in scripts only)
ax policy capture "you must not commit secrets" --yes
```

In Cursor/Claude, call **`ax_policy_capture`** with `action: "propose"` first. Ask each item in `questions[]` (level, globs, triggers, etc.), then `action: "save"` only after explicit user confirmation.

The prompt-hook may inject `<ax_capture_hint>` when a directive is detected. Disable with `AX_NO_POLICY_CAPTURE=1`.

Saved rules go to **database** or **files** depending on `policy.storage` in `ax.json` — see [Policy storage](#policy-storage) below.

---

## Policy storage

| Mode | Source of truth | Typical workflow |
|---|---|---|
| `files` | `.agents/rules/*.mdc` and `.agents/skills/*/SKILL.md` on disk | Edit in git, `ax policy index` syncs to DB |
| `database` | `ax.db` policy tables | Add rules via capture; `ax policy export` writes them to files for git |

**Hybrid storage:** `ax.json` sets the **project default**. Each rule/skill may override with frontmatter `storage: files|database` (set with `ax policy storage set-item`). Delivery to agents remains via MCP from `ax.db` either way — the toggle chooses where edits are authoritative.

```bash
ax policy storage status                 # show project + global default (files vs database)
ax policy storage database               # preview: scan rules/skills (no writes)
ax policy storage database --yes         # import all into ax.db and delete .agents/.ax/policy files
ax policy storage database --yes --keep-files
ax policy storage files                  # preview export
ax policy storage files --yes            # export ax.db → .agents/ (files become source of truth)
ax policy storage database --global      # set default in ~/.ax/config.json
ax policy storage set-item utf8-no-bom database
ax policy storage set-item startup files --keep-file
```

Configure in `ax.json`:

```json
{
  "policy": {
    "storage": "files",
    "roots": [
      {
        "id": "team-shared",
        "path": "D:/ax-policy-shared",
        "scope": "workspace"
      },
      {
        "id": "api-rules",
        "path": "../other-repo/.ax/policy",
        "scope": "project",
        "member": "api"
      }
    ]
  }
}
```

### External roots and stubs

- **`policy.roots`** — mount directories (absolute or relative to the config file) as extra policy layers. Optional `member` limits a root to one workspace member name/path. Roots appear in `ax policy storage status`.
- **Stubs** — a committed file under `.ax/policy/` can point elsewhere:

```yaml
---
id: utf8-no-bom
level: CRITICAL
alwaysApply: true
storage: files
source: "root:team-shared/rules/utf8-no-bom.mdc"
---
<!-- ax:stub — body loaded from source -->
```

Index loads the body from `source` (absolute path or `root:<id>/…`). Broken sources are skipped with a soft warning — preflight never crashes. Saving updates the external file and keeps the stub metadata.

Both modes support the same **scopes**. Frontmatter field `scope` (and the DB column) records where an item lives. Capture interviews ask for scope; in a file, set `scope` in the frontmatter.

### Policy scopes

| Scope | Path | Pack / git |
|---|---|---|
| `company` | `~/.ax/global_policy/` | Never packed |
| `workspace` | `<workspace>/.agents/` | Included in default pack export |
| `project` | `<project>/.agents/` | Included in default pack export |
| `private_user` | `~/.ax/private_policy/` | Never packed |
| `private_project` | `<project>/.ax/policy-private/` | Never packed (gitignored) |
| inactive (local) | `<project>/.ax/policy-inactive/` | Never packed (gitignored) |

Merge order on index: company → workspace → project → inactive overlay → private_user → private_project (later wins on the same id/name).

**Agent lookup (`ax_preflight`, `ax_skill`, `ax policy skill`):** merge the current project `ax.db` with `~/.ax/global.db` (`global_policy_skills`). A **global** row with the same name wins.

### One copy per name (dedup)

A rule or skill stored at the global level is not kept again in a project `ax.db`. Rows in `global.db` have a `level`: `global` rows are the machine-wide copies agents load, and `mirror` rows are the per-project copies written by `ax global sync`. Sync never writes a mirror for a name that already has a global row, and only global rows are loaded or counted.

`ax policy dedup` applies the cleanup (`--dry-run` only lists it). The checks that run on their own during `ax install`, `ax global sync` and indexing are dry runs: they report how many duplicates they found and change nothing. Run `ax policy dedup` to remove them.

The cleanup compares each duplicate with the global copy:

| Case | Result |
|---|---|
| Same text as the global copy (surrounding whitespace and `\r\n` vs `\n` ignored) | The project row is removed |
| Global copy is longer (trimmed text, line endings ignored) | The project row is removed |
| Project copy is longer | The project copy replaces the global body as a new version, then the project row is removed |
| Equal length, different text | The copy changed most recently wins (file time for the project copy, last global revision for the global copy) |
| Several global rows with one name | The winner by the same rules stays; the others are removed |

Every change is versioned. Global bodies keep up to 20 versions in `global_policy_revisions` (sources `save`, `dedup-promote`, `dedup-removed`, `delete`, and `baseline` for the body a row had before versioning existed); a removed project row gets a project revision with source `dedup`, so `ax policy restore` can bring it back. Files on disk are never edited or deleted: the import skips names that exist at the global level instead.

The cleanup runs after `ax sync`, `ax index` and the watch sync, after every policy index (`ax init`, `ax policy index`, MCP `ax_policy_index`), after `ax global sync`, and at the end of `ax install`. Run it by hand with `ax policy dedup [--dry-run] [--json]`. Without a `~/.ax/global.db` it does nothing. A run that stops on an error (an unreadable row, for example) changes nothing further: the CLI exits non-zero, and the sync hooks log a warning. `AX_GLOBAL_DB` points all of this at another global database.

`ax init` does not write the skills stored in `global.db` (`review-loop`, `pr-review-comments`) into a project's `.agents/skills/`; they reach every project from the global level.

### Database migration scan

When switching to **database** (`ax policy storage database`), ax does not import only `.ax/policy/`. It **recursively scans the project** for:

| Pattern | Examples |
|---|---|
| `*.mdc` with YAML frontmatter | `.agents/rules/`, `.ax/policy/rules/` (legacy), `.cursor/rules/`, nested packages |
| `**/SKILL.md` with frontmatter | `.agents/skills/`, `.ax/policy/skills/` (legacy), `.cursor/skills/`, monorepo subfolders |

Skipped automatically: `node_modules`, `target`, `dist`, IDE bootstrap (`.cursor/rules/ax.mdc`), invalid frontmatter, duplicate ids.

**Two-step flow:**

1. **Propose** — `ax policy storage database` (no `--yes`): prints each candidate. Does **not** change storage or import yet.
2. **Apply exclusive** — `ax policy storage database --yes` switches to database mode, upserts candidates into `ax.db`, and **deletes** `.agents/` / `.ax/policy/` markdown. `--keep-files` skips deletion. `.cursor/` bootstrap files stay.

Use `--json` on either step for agent-driven interviews.

---

## How policy flows — overview

```text
  .ax/policy/rules/*.mdc  ──┐
  .ax/policy/skills/*/SKILL.md ──┼──►  ax policy index  ──►  ax.db (SQLite)
                                 │
                                 │     MatchInput: prompt + open files
                                 │              │
                                 │              ▼
                                 │     deterministic matcher
                                 │              │
                                 │              ▼
                                 └──►  inject: <ax_policy>…full bodies…</ax_policy>
                                              │
                    ┌─────────────────────────┼─────────────────────────┐
                    ▼                         ▼                         ▼
              ax_preflight              ax_rules / ax_skill         ax_guard
              (MCP — turn start)        (MCP — list / load)       (MCP — pre-write)
                    │
                    └──► optional: ax prompt-hook auto-inject (Claude Code)
```

**Filesystem = source of truth.** **SQLite = delivery index.** The agent consumes policy from MCP responses, not from Read on `.ax/policy/`.

---

## Single turn — step by step

```text
 1. User sends prompt
         │
         ▼
 2. Agent calls ax_preflight({ prompt, files })
         │
         ▼
  3. ax matches rules (alwaysApply, globs, triggers) and skills (alwaysApply, triggers, description)
         │
         ▼
 4. MCP returns:
      • rules[]   — id, level, score, full body
      • skills[]  — name, description, full body (all alwaysApply skills, then up to 2 trigger-matched)
      • inject    — ready-to-apply <ax_policy> markdown block
         │
         ▼
 5. Agent applies CRITICAL rules before editing
         │
         ▼
 6. For code structure: ax_explore or ax_context  (NOT policy)
         │
         ▼
 7. Before Write/Delete: ax_guard({ path }) when CRITICAL rules exist
         │
         ▼
 8. Agent responds to user
```

---

## Delivery by agent type

| Channel | Cursor | Claude Code | CLI-only |
|---|---|---|---|
| MCP — agent calls `ax_preflight` | Required | Supported | — |
| Prompt-hook — auto `<ax_policy>` inject | No | Yes | — |
| `ax_skill(name)` on demand | Yes | Yes | `ax policy skill <name>` |
| `ax_guard(path)` before writes | Yes | Yes | `ax policy guard <path>` |
| Turn / preflight without MCP | — | — | `ax preflight` / `ax turn begin` |

In **Cursor**, policy is **pull-only**: the agent must call `ax_preflight` at turn start. MCP server instructions include this when `.ax/policy/` is indexed. On **CLI-only** projects (`ax install --mode cli-only`), agents use the CLI equivalents — see [CLI-only mode](/guides/cli-only/).

Set `AX_NO_POLICY=1` to skip prompt-hook injection. Set `AX_POLICY_MAX_CHARS` to cap **contextual** inject size (default `16000`). Always-apply rules are never hard-truncated: if they exceed the cap, the inject grows so `ax_preflight` still delivers a complete `Rules (always apply)` block. Oversized skills are omitted with an `ax_skill` hint instead of cutting always-apply rules mid-body.

---

## Policy tools vs code tools

| Tool | Layer | Use for |
|---|---|---|
| `ax_preflight` | Policy | Turn-start rules + skills + `inject` |
| `ax_rules` | Policy | List or match rules |
| `ax_skill` | Policy | Load one skill by name |
| `ax_guard` | Policy | UTF-8 BOM, secrets paths, other CRITICAL checks |
| `ax_explore` | Code graph | How does X work, call paths, blast radius |
| `ax_context` | Code graph | Task-oriented markdown from the graph |

**`ax_context` is not policy.** Do not read `.ax/policy/` skill files when MCP policy tools work.

### Enforcing graph-first reads

The `explore-before-grep` rule asks agents to use the graph. The `ax read-guard` hook enforces it in every IDE with a blocking tool hook: Cursor, Claude Code, VS Code Copilot, Codex, Gemini CLI, and Windsurf. `ax install` adds it. The first whole-file read of an indexed source file, or the first search for a symbol the graph knows, is denied with the matching `ax_node` / `ax_callers` calls. Repeating the same call is allowed, so an agent is never blocked from the raw file. Turn it off with `AX_READ_GUARD=off`. See [`ax read-guard`](/reference/cli/#ax-read-guard).

---

## Authoring rules

Rules live in `.agents/rules/<id>.mdc` — YAML frontmatter plus a markdown body:

```yaml
---
id: mobile-first
level: CRITICAL
scope: project
alwaysApply: false
globs: ["**/*.css", "**/*.tsx"]
triggers: ["mobile", "responsive"]
priority: 100
enabled: true
status: approved
tags: ["ui"]
group: design
---
# Rule body (markdown)
```

| Field | Purpose |
|---|---|
| `id` | Stable identifier (filename without `.mdc`) |
| `level` | `CRITICAL`, `WARNING`, or `INFO` |
| `scope` | `company`, `workspace`, `project`, `private_user`, or `private_project` (default `project`) |
| `alwaysApply` | Inject on every turn when `true` |
| `globs` | Match when listed files are in scope |
| `triggers` | Match when user intent contains these phrases |
| `priority` | Higher wins when multiple rules match |
| `enabled` | When `false`, matcher/preflight skip the rule (default `true`) |
| `status` | `approved` (active), `pending` (review queue), or `rejected` |
| `tags` | Free-form labels; give every rule at least one. `ax policy pack export --tag <tag>` selects by tag. Use `local` or `noshare` to opt out of pack export |
| `group` | Optional catalog folder id that groups rules (same catalog as skills). Matching ignores this field |
| `share` | Optional alias — normalized to tag `shared` on parse (legacy; default export no longer requires it) |

Any other frontmatter key is kept as an extra property (`files`, `kind`, `aliases`, a checkbox, a list). ax stores it, writes it back, and includes it in `ax_preflight`, `ax_rules`, and `ax_skill` under a `Properties:` block when the rule matches. The same applies to skills. Empty values are dropped. Known fields above stay schema fields and are not repeated as properties.

To add a property, write the key in the frontmatter of the rule file and run `ax policy index`. The value can be text, a list, a number, or a checkbox (`true` / `false`). A key with the name of a built-in field (for example `tags`) is that field, not an extra property.

Disable without deleting:

```bash
ax policy disable mobile-first
ax policy enable mobile-first
```

The same fields are stored in `ax.db` (`policy_rules.enabled`, `policy_rules.status`). `enabled` is separate from `alwaysApply` (matching): give a rule `alwaysApply`, globs, or triggers, otherwise matching never fires.

---

## Authoring skills

Skills live in `.agents/skills/<name>/SKILL.md`:

```yaml
---
name: deploy
description: Use when the user says deploy or push to production.
scope: project
alwaysApply: false
triggers: ["deploy", "production"]
enabled: true
status: approved
tags: ["ops"]
group: deploy-ops
---
# Workflow steps (markdown)
```

When `alwaysApply` is `true`, `ax_preflight` injects the skill body on **every** turn (including empty prompts) under **Skills (always apply)** — the same contract as always-apply rules. Otherwise the skill matches on triggers/description and appears under **Suggested skills**. Load a specific workflow anytime with `ax_skill({ name: "deploy" })`.

Give every skill at least one `tags` value (same as rules).

Extra frontmatter keys on a skill work the same way as on a rule. A skill can carry `globs`, `files`, `kind`, or any other key the rule schema does not use. When the skill is injected, those properties are sent with the body. `ax_skill` returns them on the skill row as `properties`. Add them in the frontmatter of `SKILL.md`.

### Linking rules, skills and memories

A rule, skill, or memory body can link to another item Obsidian-style: `[[pre-pr-check]]`, `[[skills/pr]]`, `[[global/skills/review-loop]]`, or `[[pr|the PR skill]]`. When `ax_preflight` delivers an item, it also delivers what that item links to: one hop, at most five items per turn, and never a disabled or unapproved one. A rule delivered this way has the match reason `link:rule/<id>` (or `link:skill/…`, `link:memory/…`). Links in code blocks are ignored. Name lookup and the vault side are in the [Obsidian Vault](/guides/obsidian-vault/#links) guide.

The shipped `pr` skill opens the pull request as a draft, then reviews that draft. It keeps fixing findings on the same branch until a review round has zero findings. That review follows `review-loop`. Comments are not posted for the author to approve.

### Guard directives

Any **CRITICAL** rule can opt into the static `ax_guard` gate by putting one of these lines in its body (quotes required):

```text
guard: forbid-path: "**/*.pem"
guard: forbid-content: "eval("
guard: require-content: "requireAuth("
guard: require-skill: "old-coder"
```

| Directive | When it blocks |
|---|---|
| `forbid-path` | Path matches the glob (Write or Delete) |
| `forbid-content` | Write content contains the substring or `/regex/`. Scoped to the rule's `globs` when it declares any, project-wide when it declares none — so a rule can ban a pattern in one directory without blocking the code next door that legitimately needs it |
| `require-content` | Write to a path matching the rule's `globs` is missing the substring or `/regex/` |
| `require-skill` | Write/Delete unless that skill is indexed, enabled, approved, and `alwaysApply: true`. Exempt: `.agents/**`, `.ax/policy/**`, and `crates/ax-policy/templates/**` |

`old-coder-mandatory` ships with `require-skill: "old-coder"` so implementation writes fail closed if the skill is missing or not always-apply.

The full ax, which is being phased out, also has label filters, catalog groups and Auto-group for rules and skills in its Command Center; see [Command Center](/guides/command-center/).

---

## Per-project pack sync

Share selected rules/skills with teammates via **git** (same project repo), not through a cloud registry.

1. Export the pack → commit `.ax/policy/shared/`
2. Colleagues pull and import

Default export (`--tag shared`) includes all **project** and **workspace** rules/skills that are enabled and approved. Company and private scopes are never packed. Opt out of the team pack with tags `local` or `noshare`. A custom `--tag foo` still filters to items that carry that tag.

```bash
ax policy pack export
# commit .ax/policy/shared/
ax policy pack import
ax policy pack status
```

Set `"policySync": true` in project `ax.json` so git hooks run export on post-commit and import on post-merge (run `ax sync` once after enabling).

### Built-in packs

Install optional project packs shipped with ax (does not enable them in the ax product repo by default):

```bash
ax policy pack install --list
ax policy pack install azdo-fullstack
ax policy pack install azdo-fullstack --force
```

`azdo-fullstack` adds Azure DevOps ticket-to-release **skills** (full workflows with checklists — refinement → development → testing → PR → pipelines → release) and matching **rules**. The `azdo-code-review` skill reviews against the Story: scope creep, sibling patterns in the repo, both ends of a bound (not only the max), and a named test for each gap. It complements built-in methodology skills (`design-first`, `tdd`, `systematic-debugging`) rather than replacing them. Re-run with `--force` after upgrading ax to refresh expanded skill bodies.

### Stacks

Core policy (startup, debugging, TDD, design-first, and the preflight rules) is seeded for every project. Language, frontend, and CMS skills are **not** copied on init. The on-disk folder is `policy.agentsDir` in `ax.json` (default `.agents`). `ax init` asks for that name when it is not set yet. Change it later with `ax policy agents-dir <name>`.

```bash
ax policy stack detect
ax policy stack apply dotnet nextjs
```

Save the choice in project `ax.json`:

```json
{
  "policy": {
    "stacks": ["dotnet", "react"],
    "stackDetect": "off"
  }
}
```

`ax init` asks which stacks to install on every run when stdin is a terminal, including a second init. The list is a colored checkbox menu, grouped into Platforms, Web, PHP, CMS, Systems, and Languages. Up and down arrows (or j and k) move through the stacks, Space toggles one, and Enter confirms. Installed stacks and newly detected stacks start checked. Clear every box for core policy only. If the menu cannot open, type ids separated by spaces, or `none`. A non-interactive run keeps the saved `policy.stacks` and does not block.

`ax policy stack apply` writes `.ax/stacks.lock.json` (template version and content hash per file). Re-apply skips a file you edited unless you pass `--force`. `ax policy stack status` reports catalog drift; `ax policy stack upgrade` refreshes files that still match the lock.

Language stacks follow the languages ax indexes: `rust`, `python`, `go`, `typescript`, `javascript`, `c`, `cpp`, `ruby`, `swift`, `kotlin`, `dart`, `svelte`, `astro`, `scala`, `lua`, `luau`, `objc`, `r`, and `pascal`, in addition to `dotnet` (C#), `java`, and `php`. C# stays on the `dotnet` stack. Config formats ax also parses (YAML, XML, properties) are not stacks.

The `dotnet` stack ships the `dotnet-code-review` skill. It has twelve sections, and each rule appears once:

1. naming and casing (Framework Design Guidelines)
2. layout and syntax
3. design (SOLID, DDD, CQRS)
4. dependency injection lifetimes
5. CLR and memory
6. async and concurrency
7. EF Core
8. ASP.NET Core
9. security
10. observability and errors
11. resilience and testability
12. a fixed output format with a verdict, a score, and a severity per finding

The `nextjs` stack (which pulls in `react`) ships the `nextjs-review` skill. It also has twelve sections:

1. structure and naming
2. TypeScript
3. server and client components
4. Server Actions
5. data fetching and caching
6. client state and React 19
7. performance and assets
8. styling and UI (only when the repo already uses Tailwind or a headless UI library)
9. security
10. errors and observability
11. testing
12. the same output format

It does not repeat rules that are already in `react-review`.

Every other stack ships a review skill of the same depth: 10 to 12 stack-specific review sections with 73 to 103 concrete rules, and the same output format. Each rule appears once. Some skills build on another skill: `laravel` and `drupal` build on `php-review`, `sitecore` and `optimizely` on `dotnet-code-review`, `typescript` on `javascript-review`, `luau` on `lua-review`, and `cpp` and `objc` on `c-review`. Such a skill names its base, tells the agent to load it too, and repeats none of its rules. Where the two conflict, the building skill's rule wins.

`ax policy stack upgrade` (or the next `ax init`) replaces an older copy you have not edited.

Install writes project files under `.ax/policy/`, then **imports into `ax.db`** when `policy.storage` is `database` (so MCP/`ax policy skill` show the new bodies without a separate `ax policy index --force`).

### Portable zip packages

Sitecore-style **item pick → zip → restore with conflict preview**. This is not git pack sync and not OneDrive.

```bash
ax policy pack zip --out team.ax-policy.zip --name "Team pack" --rules utf8-no-bom --skills startup
ax policy restore --preview team.ax-policy.zip
ax policy restore team.ax-policy.zip --decisions decisions.json
```

1. **Package.** `ax policy pack zip` builds a `*.ax-policy.zip` of the rules and skills you name. Only shareable items go in: project and workspace scope, enabled. Private and disabled items are never packed.
2. **Preview.** On the other machine, `ax policy restore --preview <zip>` compares every item in the package with the local file. Each item is new, identical or a conflict, and the preview JSON says which side is newer (`newer`). Packs include a blake3 `contentHash` per file so “changed” does not depend on zip timestamps; a mismatched hash is invalid.
3. **Restore.** `ax policy restore <zip>` installs the package. **New** items are installed. Conflicts are skipped (including local-newer files), so an older package does not replace a newer local file unless you say so. Pass `--decisions decisions.json` to choose per item.

`decisions.json` maps `"rule:<id>"` / `"skill:<name>"` to `overwrite`, `skip`, `{ "action": "merge", "acceptHunks": [0, 2] }`, or `{ "action": "merge", "hunks": [{ "index": 0, "take": "both" }] }`. Missing keys: **new** → install, **conflict** → skip (never overwrite a newer local file unless listed as overwrite).

A restore writes into `.agents/`, then re-indexes policy. Files that are not ax schema (typical Cursor `.mdc` / `SKILL.md` without ax `id`/`level` or skill `description`) stay on disk and are **skipped** during index and during MCP session policy refresh (`ensure_policy_ready`); they do not abort restore or `ax_preflight` as `validation_failed`. Parsed disabled or private-scope files under `.agents/` still fail the leak gate.

Team-wide moderation stays on git: merge a PR, then pack a zip (or attach the zip as a CI artifact). A restore is a local accept or skip per item, not a substitute for pull-request review.

### Local revision history

ax keeps a **local revision log** of up to 20 snapshots per rule or skill (blake3 hash-on-change). A snapshot is stored only when the serialized file hash changes, for example when `ax policy restore` writes an accepted item. Edits you make directly in the files are not recorded; use git for those. This is not git history.

The headless CLI has no command that lists these snapshots. The full ax, which is being phased out, shows them under **History** in its Command Center; see [Command Center](/guides/command-center/). For a rule or skill that was removed, use `ax policy history` and `ax policy restore-item <id>` (see [Preserve customized rules and skills](#preserve-customized-rules-and-skills)).

### IDE-agnostic delivery (Cursor ↔ Continue ↔ …)

Team rules live in `.ax/policy/` + `ax.db` and are delivered by MCP `ax_preflight` — not by copying rule bodies into each IDE. Pack sync is therefore agent-independent:

```text
You (Cursor)  →  pack export  →  git  →  pack import  →  colleague (Continue)
colleague     →  pack export  →  git  →  pack import  →  you (Cursor)
```

On every `ax policy pack import` (including the post-merge hook), ax:

1. Indexes shared rules/skills into the local policy store
2. Re-seeds **all** IDE bootstrap files (`.cursor/rules/ax.mdc`, `.continue/rules/ax.md`, Claude, AGENTS.md, …)
3. Writes Continue MCP at `.continue/mcpServers/ax.json` (project-scoped, commit-friendly) and refreshes MCP for any other detected agents

Colleagues only need `ax` on PATH and their IDE open on the project. Continue is included in init seeding (`ax init` / `ax policy sync --fix`).

**Context for teammates** stays on the existing memory path: tag memories `shared`, use `ax memory export` / `import` and optional `"memorySync": true`.

### Optional review gate

```json
{
  "policySync": true,
  "policy": {
    "requireReview": true
  }
}
```

When `requireReview` is true, pack imports land under `.ax/policy/pending/` and are not matched until approved:

```bash
ax policy review list
ax policy review show <id>
ax policy review approve <id>
ax policy review reject <id>
```

Pack sync, the review queue, remote share and enabling all go through the CLI: `ax policy pack …`, `ax policy review …`, `ax policy share …`, `ax policy enable|disable`. ax-admin has no policy review page and no policy sync page. Every rule and skill should carry at least one `tags` value so you can filter (e.g. `azdo`, `cicd`, `workflow`).

For org-wide sync outside the project repo, see [Remote Policy Share](/guides/policy-sharing/).

---

## MCP tools (policy)

When `.ax/policy/` is indexed, the MCP server also lists:

| Tool | Purpose |
|---|---|
| `ax_preflight` | Turn-start: matched rules + skills + full `inject` text |
| `ax_rules` | List or match rules |
| `ax_skill` | Load a skill by name |
| `ax_guard` | Pre-write checks for CRITICAL rules (UTF-8, secrets paths) |
| `ax_policy_capture` | Propose or save a rule from directive language in a prompt |

Code-structure tools (`ax_explore`, etc.) are unchanged. See [MCP Server](/reference/mcp-server/).

Call `ax_preflight` at the start of each agent turn when policy is enabled. Call `ax_guard` with the target file path before editing project files.

---

## CLI

```bash
ax policy index [--force]
ax policy sync [--fix]     # verify/restore managed preflight instruction files
ax policy match "prompt text" [--file path] [--json]
ax policy rules [--json]
ax policy skills [--json]
ax policy skill <name>
ax policy enable <id-or-name>
ax policy disable <id-or-name>
ax policy pack export|import|status|install|zip
ax policy restore [--preview] <zip> [--decisions decisions.json]
ax policy review list|show|approve|reject
ax policy share config|sync
ax policy guard path               # test CRITICAL guard on a path
ax policy capture <prompt> [--yes] [--json] [--file path]
ax policy storage status [--json]
ax policy storage database [--yes] [--keep-files] [--global] [--json]
ax policy storage files [--yes] [--global] [--json]
```

---

## Editing and browsing

Rules and skills are files, so you edit them where they live:

1. Open `.agents/rules/<id>.mdc` or `.agents/skills/<name>/SKILL.md` in any editor and change the frontmatter or the markdown body.
2. Run `ax policy index` to bring the change into `ax.db`. An agent can call `ax_policy_index` instead, and in [ax-admin](/guides/remote-admin/) **Operations → Reload policy** does the same for the connected project (admin grant needed).
3. Check the result with `ax policy match "<prompt>"`.

To browse what is indexed:

```bash
ax policy rules [--json]
ax policy skills [--json]
ax policy skill <name>
```

In ax-admin, the **Rules** page lists the policy rules and shows which rules match a prompt, and the **Skills** page opens a skill by name. ax-admin has no rule or skill editor, so changes always go through the files and `ax policy index`.

**Project vs global database:** `ax global sync` copies each project’s `policy_rules` / `policy_skills` into `~/.ax/global.db`. Those copies are storage, not agent-wide matching: agents still match only the current project database.

The full ax, which is being phased out, also has rule and skill editors in its Command Center; see [Command Center](/guides/command-center/).

## Policy search

Policy search looks through rules and skills that are already indexed. It is separate from skill search ([`ax skill search`](/reference/cli/#ax-skill)), which queries the public catalog.

`ax policy match` previews which rules and skills would inject for a given prompt — the same matcher that runs during `ax_preflight`:

```bash
ax policy match "deploy to production" --file src/ship.rs
ax policy match "utf8 bom" --json
ax policy rules
ax policy skills
```

Agents match rules with `ax_rules`. In ax-admin, the **Rules** page shows which rules match a prompt.

---

## Parallel instruction sources

ax policy does not replace other systems:

| Source | Loaded by |
|---|---|
| `.agents/` | ax MCP + prompt-hook |
| `.cursor/rules`, `.cursor/skills` | Cursor (separate) |
| Recall MCP | Recall OS projects (separate) |

**Do not duplicate ax team policy in `.cursor/rules/`.** Rules and skills under `.agents/` are indexed into `ax.db` and delivered via `ax_preflight` MCP inject. Cursor skill folders may symlink to `.agents/skills/`.

**Exception — IDE bootstrap:** `ax init` seeds each agent's native instructions surface (create or repair). These files only tell the agent to call `ax_preflight`; they are not team policy.

| IDE | Dedicated file | Default instructions link |
|-----|------------------|---------------------------|
| Cursor | `.cursor/rules/ax.mdc` | — (`alwaysApply` rule) |
| Claude Code | `.claude/rules/ax.md` | marker block in `.claude/CLAUDE.md` |
| Codex / opencode | — | marker block in `AGENTS.md` |
| Gemini CLI | — | marker block in `GEMINI.md` |

Legacy `.cursor/rules/ax-agent-workflow.mdc` is migrated to `ax.mdc` on init.

Cursor-only conveniences (e.g. a local dev reinstall skill) may stay in `.cursor/skills/`.

Run `ax policy sync` to verify managed policy files and warn about duplicate `.cursor/rules/` entries.

---

## Environment

| Variable | Effect |
|---|---|
| `AX_NO_POLICY=1` | Skip policy injection in prompt-hook |
| `AX_NO_POLICY_CAPTURE=1` | Skip directive capture hints in prompt-hook |
| `AX_POLICY_MAX_CHARS` | Cap **contextual** policy inject (default 16000). Always-apply rules **and** always-apply skills are never hard-truncated. |
| `AX_READ_GUARD=off` | Disable the read-guard hook |

---

## Troubleshooting

### MCP `ax_rules` returns empty or preflight only shows IDE bootstrap

1. Reload ax MCP in Cursor (Settings → MCP) after `ax` upgrade or reinstall.
2. Check DB: `ax policy rules` — should list indexed rules.
3. If DB is empty but `.ax/policy/` exists: `ax policy import` or `ax policy index --force`.
4. Restart daemon: `ax daemon stop`, then reload MCP.

### `ax policy guard` CLI errors

Use file-first syntax (write check is default):

```bash
ax policy guard crates/ax-cli/src/main.rs
ax policy guard path/to/file.rs --delete
ax policy guard -p /path/to/project file.rs
```

Do not pass `write` as a positional argument — it was parsed as project path in older versions.

### Policy smoke tests

```bash
ax policy test          # index, match, inject, guard and bootstrap checks
ax policy test --json   # machine-readable output
```

### Bootstrap vs team policy

| Layer | Location | Purpose |
|---|---|---|
| IDE bootstrap | `.cursor/rules/ax.mdc`, `AGENTS.md` | Reminder to call `ax_preflight` |
| Team policy | `.ax/policy/` → MCP inject | Full CRITICAL rules and skills |

Do not duplicate team rules in `.cursor/rules/` — delivery is MCP-only.

---



## Preserve customized rules and skills

Install, `ax init`, and `ax policy sync --fix` create a bundled rule or skill only when this project has never had it. A file you deleted stays deleted. The removal is kept in `.ax/policy-history.jsonl` (and `policy_history` in the project database) and, for machine-wide policy, in `~/.ax/global_policy/history.jsonl`. Hand-edited files that are still on disk are left alone. `ax policy history` lists those events. `ax policy restore-item <id>` or `ax policy sync --fix --force` writes one item back and records `restored`. A template added in a later ax version, which the history has never seen, is still created once. The graph, memories, and other rows in `.ax/ax.db` are not replaced by an upgrade.

## Related

- [Configuration](/getting-started/configuration/#policy-rules-and-skills) — where policy files live
- [CLI](/reference/cli/) — full command list including `ax policy`
- [MCP Server](/reference/mcp-server/) — policy tools alongside code tools
- Maintainer architecture notes: [POLICY_ENGINE.md](https://github.com/GaryWenneker/ax/blob/main/docs/POLICY_ENGINE.md) on GitHub
