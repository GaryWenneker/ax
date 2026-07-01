---
title: Policy Engine
description: IDE-agnostic rules and skills for AI agents — stored in .ax/policy/, delivered via MCP, CLI, and ax web.
---

**ax v2.0.0+** ships a **policy engine**: project-local rules and skills that work in any IDE or agent harness — not tied to Cursor rules or a single vendor format.

Policy files live under `.ax/policy/`, are indexed into the same SQLite database as the code graph, and are injected at agent turn-start via MCP (`ax_preflight`) or the prompt-hook.

## Quick start

```bash
ax init                    # creates .ax/policy/rules and skills dirs
ax policy index            # index policy files into ax.db
ax policy match "deploy"   # test which rules/skills match
ax web --open              # edit rules/skills in the browser
```

Commit `.ax/policy/` to git so your team shares the same agent instructions everywhere.

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

Skills are loaded on demand when a turn matches their triggers — same semantics as agent skills, but stored in your repo.

## MCP tools (policy)

When `.ax/policy/` is non-empty, the MCP server also lists:

| Tool | Purpose |
|---|---|
| `ax_preflight` | Turn-start: matched rules + skill names |
| `ax_rules` | List or match rules |
| `ax_skill` | Load a skill by name |
| `ax_guard` | Pre-write checks for CRITICAL rules (UTF-8, secrets paths) |

Code-structure tools (`ax_explore`, etc.) are unchanged. See [MCP Server](/reference/mcp-server/).

## CLI

```bash
ax policy index [--force]
ax policy match "prompt text" [--file path] [--json]
ax policy rules [--json]
ax policy skills [--json]
ax policy skill <name>
ax policy guard --file path        # test CRITICAL guard on a path
```

## ax web editor

```bash
ax web --port 7070 --open
```

Open **Policy → Rules** or **Policy → Skills** in the sidebar to edit frontmatter and markdown, save to disk, and re-index automatically.

Set `AX_WEB_READONLY=1` for browse-only mode.

## Environment

| Variable | Effect |
|---|---|
| `AX_NO_POLICY=1` | Skip policy injection in prompt-hook |
| `AX_POLICY_MAX_CHARS` | Cap injected policy text (default 16000) |
| `AX_WEB_READONLY` | Disable saves in ax web |

## Related

- [Configuration](/getting-started/configuration/#policy-rules-and-skills) — where policy files live
- [CLI](/reference/cli/) — full command list including `ax policy`
- Maintainer architecture notes: [POLICY_ENGINE.md](https://github.com/GaryWenneker/ax/blob/main/docs/POLICY_ENGINE.md) on GitHub
