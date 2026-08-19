---
title: Policy Engine
description: IDE-agnostic rules and skills for AI agents — stored in .ax/policy/, indexed locally, delivered via MCP without reading source files.
---

**ax v2.0.0+** ships a **policy engine**: project-local rules and skills that work in any IDE or agent harness — not tied to Cursor rules or a single vendor format.

Policy files live under `.ax/policy/`, are indexed into SQLite (`ax.db`), and reach the agent through **MCP tools** or the **prompt-hook**. Agents should **not** open `.mdc` / `SKILL.md` files on disk when policy MCP tools are available — the matched text is returned in the `inject` field.

## Quick start

```bash
ax init                    # creates .ax/policy/ and seeds default preflight rules/skills
ax policy index            # index policy files into ax.db
ax policy sync [--fix]     # verify/restore ax_preflight instruction files
ax policy match "deploy"   # test which rules/skills match
ax policy capture "always use mobile-first CSS"   # propose rule from directive language
ax policy storage status   # show effective files vs database mode
ax web --open              # edit rules/skills in the browser
```

On **`ax init`** and **`ax install`**, ax also seeds machine-wide policy. Existing files are left alone **unless** the template gained `alwaysApply: true` or a `require-skill` guard the on-disk copy does not have yet (then ax rewrites from the template):

| Location | Content |
|---|---|
| `~/.ax/global_policy/rules/` | `old-coder-mandatory` (CRITICAL, **alwaysApply**, `guard: require-skill: "old-coder"`) — agents must follow the old-coder workflow for implementation work |
| `~/.ax/global_policy/skills/` | `old-coder` (**alwaysApply**, injects on every turn including empty prompts), `old-coder-api` (matched on API triggers; load full body via `ax_skill`) |
| `~/.cursor/skills/` | Same skill bundles for Cursor agent discovery |

**Enforcement model:** Rules and skills with `alwaysApply: true` are injected on **every** `ax_preflight` turn (including empty or one-word prompts). Always-apply inject is never hard-truncated. `ax_guard` blocks Write/Delete when a CRITICAL rule declares `guard: require-skill: "old-coder"` unless that skill is indexed, approved, enabled, and `alwaysApply`. Policy files under `.ax/policy/` and `crates/ax-policy/templates/` are exempt so seed/index can repair a missing skill.

Every `ax init` re-imports all policy layers (including global) into `ax.db`. After install alone, run `ax init` or `ax policy index --force` in any project once.

Source: [AmazingAng/old-coder](https://github.com/AmazingAng/old-coder) (MIT). Project init also copies rollout skills into `<project>/.cursor/skills/`.

Upgrade to **ax v2.1.2+** and restart your agent so MCP exposes `ax_preflight`, `ax_rules`, `ax_skill`, `ax_guard`, and `ax_policy_capture`.

---

## Policy capture (v2.1.1+)

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

Saved rules go to **database** or **files** depending on `policy.storage` in `ax.json` — see [Policy storage](#policy-storage-v211) below.

---

## Policy storage (v2.1.1+; hybrid in v4.3+)

| Mode | Source of truth | Typical workflow |
|---|---|---|
| `files` | `.ax/policy/*.mdc` and `SKILL.md` on disk | Edit in git, `ax policy index` syncs to DB |
| `database` | `ax.db` policy tables | Edit in ax web or via capture; `ax policy export` for git |

**Hybrid storage:** `ax.json` sets the **project default**. Each rule/skill may override with frontmatter `storage: files|database` (Command Center list toggle or `ax policy storage set-item`). Delivery to agents remains via MCP from `ax.db` either way — the toggle chooses where edits are authoritative.

```bash
ax policy storage status                 # show project + global mode + policy.roots
ax policy storage database --migrate           # propose: scan repo + interview questions
ax policy storage database --migrate --yes     # apply: switch + import all candidates
ax policy storage files --migrate              # export DB → files when switching
ax policy storage database --global            # set default in ~/.ax/config.json
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

- **`policy.roots`** — mount directories (absolute or relative to the config file) as extra policy layers. Optional `member` limits a root to one workspace member name/path. Roots appear in `ax policy storage status` and Command Center settings.
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

Both modes support the same **scopes**. Frontmatter field `scope` (and the DB column) records where an item lives. Capture interviews ask for scope; Command Center editors show a Scope selector.

### Policy scopes

| Scope | Path | Pack / git |
|---|---|---|
| `company` | `~/.ax/global_policy/` | Never packed |
| `workspace` | `<workspace>/.ax/policy/` | Included in default pack export |
| `project` | `<project>/.ax/policy/` | Included in default pack export |
| `private_user` | `~/.ax/private_policy/` | Never packed |
| `private_project` | `<project>/.ax/policy-private/` | Never packed (gitignored) |

Merge order on index: company → workspace → project → private_user → private_project (later wins on the same id/name).

### Database migration scan (v2.1.2+)

When switching to **database** with `--migrate`, ax does not import only `.ax/policy/`. It **recursively scans the project** for:

| Pattern | Examples |
|---|---|
| `*.mdc` with YAML frontmatter | `.ax/policy/rules/`, `.cursor/rules/`, nested packages |
| `**/SKILL.md` with frontmatter | `.ax/policy/skills/`, `.cursor/skills/`, monorepo subfolders |

Skipped automatically: `node_modules`, `target`, `dist`, IDE bootstrap (`.cursor/rules/ax.mdc`), invalid frontmatter, duplicate ids.

**Two-step flow:**

1. **Propose** — `ax policy storage database --migrate` (no `--yes`): prints each candidate with interview questions (import yes/no, storage destination, id/name, level, alwaysApply, triggers, globs, priority, tags, keep/remove source file). Does **not** change storage or import yet.
2. **Apply** — after interview: `ax policy storage database --migrate --yes` switches to database mode and upserts all candidates into `ax.db`.

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

| Channel | Cursor | Claude Code |
|---|---|---|
| MCP — agent calls `ax_preflight` | Required | Supported |
| Prompt-hook — auto `<ax_policy>` inject | No | Yes |
| `ax_skill(name)` on demand | Yes | Yes |
| `ax_guard(path)` before writes | Yes | Yes |

In **Cursor**, policy is **pull-only**: the agent must call `ax_preflight` at turn start. MCP server instructions include this when `.ax/policy/` is indexed.

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

---

## Authoring rules

Rules live in `.ax/policy/rules/<id>.mdc` — YAML frontmatter plus a markdown body:

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
| `tags` | Required for Command Center filtering; free-form labels. Use `local` or `noshare` to opt out of pack export |
| `share` | Optional alias — normalized to tag `shared` on parse (legacy; default export no longer requires it) |

Disable without deleting:

```bash
ax policy disable mobile-first
ax policy enable mobile-first
```

The same fields are stored in `ax.db` (`policy_rules.enabled`, `policy_rules.status`). Command Center exposes an **Enabled** toggle on the Rules/Skills tables and in each rule/skill editor. That is separate from **Always apply** (matching): a rule must have Always apply, globs, or triggers to save — otherwise matching would never fire.

---

## Authoring skills

Skills live in `.ax/policy/skills/<name>/SKILL.md`:

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
---
# Workflow steps (markdown)
```

When `alwaysApply` is `true`, `ax_preflight` injects the skill body on **every** turn (including empty prompts) under **Skills (always apply)** — the same contract as always-apply rules. Otherwise the skill matches on triggers/description and appears under **Suggested skills**. Load a specific workflow anytime with `ax_skill({ name: "deploy" })`.

Give every skill at least one `tags` value (same as rules) so Command Center can filter it.

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
| `forbid-content` | Write content contains the substring or `/regex/` |
| `require-content` | Write to a path matching the rule's `globs` is missing the substring or `/regex/` |
| `require-skill` | Write/Delete unless that skill is indexed, enabled, approved, and `alwaysApply: true`. Exempt: `.ax/policy/**` and `crates/ax-policy/templates/**` |

`old-coder-mandatory` ships with `require-skill: "old-coder"` so implementation writes fail closed if the skill is missing or not always-apply.

### Command Center: label filter

On **Policy → Rules** and **Policy → Skills**:

1. Type in the search box to filter by id/name **and** to see matching tag suggestions
2. Select a suggestion (Enter / click) to add a **label chip** — multiple chips use AND (item must have all selected tags)
3. Click a chip or Backspace to remove it
4. Click a tag badge in the table to toggle that label into the filter
5. Editors require at least one tag before save

---

## Per-project pack sync

Share selected rules/skills with teammates via **git** (same project repo) — not `ax share` (LAN Command Center) and not a cloud registry.

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

`azdo-fullstack` adds Azure DevOps ticket-to-release **skills** (full workflows with checklists — refinement → development → testing → PR → pipelines → release) and matching **rules**. It complements built-in methodology skills (`design-first`, `tdd`, `systematic-debugging`) rather than replacing them. Re-run with `--force` after upgrading ax to refresh expanded skill bodies.

Install writes project files under `.ax/policy/`, then **imports into `ax.db`** when `policy.storage` is `database` (so MCP/`ax policy skill` show the new bodies without a separate `ax policy index --force`).

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

Command Center: **Policy → Sync** (status, export/import, toggles), **Policy → Review** queue, label autocomplete on Rules/Skills (type to suggest tags, chips for multi-label AND filter; click a tag in the table to toggle it), layer filters, Scope on editors, and enable checkboxes. Every rule and skill should carry at least one `tags` value so you can filter (e.g. `azdo`, `cicd`, `workflow`). CLI equivalents: `ax policy pack …`, `ax policy review …`, `ax policy share …`, `ax policy enable|disable`.

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
ax policy pack export|import|status
ax policy review list|show|approve|reject
ax policy guard --file path        # test CRITICAL guard on a path
ax policy capture <prompt> [--yes] [--json] [--file path]
ax policy storage status [--json]
ax policy storage database [--migrate] [--yes] [--global] [--json]
ax policy storage files [--migrate] [--global] [--json]
```

---

## ax web editor

```bash
ax web --port 7070 --open
```

Open **Policy → Rules** or **Policy → Skills** in the sidebar to edit frontmatter and markdown, save to disk, and re-index automatically.

Selecting a row opens a master-detail blade (slides in once). Switching to another row while the blade is open keeps the panel in place and reveals the new metadata/body (no repeat slide-in).

In the rule/skill editor, drag the handle between **Metadata** and the markdown panel to resize the meta column (persisted in the browser). In edit mode, drag the bar between the markdown source and the live preview to resize those panes. Double-click the meta handle to reset its width.

![Policy Rules — sortable table with level badges, globs, triggers, capture and match testing](/screenshots/cc-policy-rules.png)

![Policy Skills — task-specific instructions agents load on demand via ax_skill](/screenshots/cc-policy-skills.png)

Use **Test match** to preview which rules and skills would inject for a given prompt — the same matcher that runs during `ax_preflight`:

![Test match — simulate a prompt and preview the full policy inject output](/screenshots/cc-policy-match.png)

Set `AX_WEB_READONLY=1` for browse-only mode.

---

## Parallel instruction sources

ax policy does not replace other systems:

| Source | Loaded by |
|---|---|
| `.ax/policy/` | ax MCP + prompt-hook |
| `.cursor/rules`, `.cursor/skills` | Cursor (separate) |
| Recall MCP | Recall OS projects (separate) |

**Do not duplicate ax team policy in `.cursor/rules/`.** Rules and skills under `.ax/policy/` are indexed into `ax.db` and delivered via `ax_preflight` MCP inject.

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
| `AX_WEB_READONLY` | Disable saves in ax web |

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
ax policy test          # match, guard, bootstrap, subagents checks
ax policy test --json   # machine-readable output
```

### Bootstrap vs team policy

| Layer | Location | Purpose |
|---|---|---|
| IDE bootstrap | `.cursor/rules/ax.mdc`, `AGENTS.md` | Reminder to call `ax_preflight` |
| Team policy | `.ax/policy/` → MCP inject | Full CRITICAL rules and skills |

Do not duplicate team rules in `.cursor/rules/` — delivery is MCP-only.

---

## Related

- [Configuration](/getting-started/configuration/#policy-rules-and-skills) — where policy files live
- [CLI](/reference/cli/) — full command list including `ax policy`
- [MCP Server](/reference/mcp-server/) — policy tools alongside code tools
- Maintainer architecture notes: [POLICY_ENGINE.md](https://github.com/GaryWenneker/ax/blob/main/docs/POLICY_ENGINE.md) on GitHub
