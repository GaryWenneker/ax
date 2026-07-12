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

Commit `.ax/policy/` to git so your team shares the same agent instructions everywhere.

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

## Policy storage (v2.1.1+)

| Mode | Source of truth | Typical workflow |
|---|---|---|
| `files` | `.ax/policy/*.mdc` and `SKILL.md` on disk | Edit in git, `ax policy index` syncs to DB |
| `database` | `ax.db` policy tables | Edit in ax web or via capture; `ax policy export` for git |

```bash
ax policy storage status                 # show project + global mode
ax policy storage database --migrate           # propose: scan repo + interview questions
ax policy storage database --migrate --yes     # apply: switch + import all candidates
ax policy storage files --migrate              # export DB → files when switching
ax policy storage database --global            # set default in ~/.ax/config.json
```

Configure in `ax.json`:

```json
{ "policy": { "storage": "database" } }
```

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
 3. ax matches rules (alwaysApply, globs, triggers) and skills (triggers, description)
         │
         ▼
 4. MCP returns:
      • rules[]   — id, level, score, full body
      • skills[]  — name, description, full body (max 2)
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

Set `AX_NO_POLICY=1` to skip prompt-hook injection. Set `AX_POLICY_MAX_CHARS` to cap inject size (default `16000`).

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
alwaysApply: false
globs: ["**/*.css", "**/*.tsx"]
triggers: ["mobile", "responsive"]
priority: 100
---
# Rule body (markdown)
```

| Field | Purpose |
|---|---|
| `id` | Stable identifier (filename without `.mdc`) |
| `level` | `CRITICAL`, `WARNING`, or `INFO` |
| `alwaysApply` | Inject on every turn when `true` |
| `globs` | Match when listed files are in scope |
| `triggers` | Match when user intent contains these phrases |
| `priority` | Higher wins when multiple rules match |

---

## Authoring skills

Skills live in `.ax/policy/skills/<name>/SKILL.md`:

```yaml
---
name: deploy
description: Use when the user says deploy or push to production.
triggers: ["deploy", "production"]
---
# Workflow steps (markdown)
```

When triggers match, `ax_preflight` includes the skill body in `inject`. Load a specific workflow anytime with `ax_skill({ name: "deploy" })`.

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
| `AX_POLICY_MAX_CHARS` | Cap injected policy text (default 16000) |
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
