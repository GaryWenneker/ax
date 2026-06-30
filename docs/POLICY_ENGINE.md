# ax Policy Engine

IDE-agnostic rules and skills for AI agents — stored in `.ax/policy/`, indexed locally, delivered via MCP, CLI, prompt-hook, and **ax web**.

## Quick start

```bash
ax init                    # creates .ax/policy/rules and skills dirs
ax policy index            # index policy files into ax.db
ax policy match "deploy"   # test which rules/skills match
ax web --open              # edit rules/skills in browser
```

## Authoring

### Rules — `.ax/policy/rules/<id>.mdc`

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

### Skills — `.ax/policy/skills/<name>/SKILL.md`

```yaml
---
name: deploy
description: Use when user says deploy or zet live.
triggers: ["deploy", "zet live"]
---
# Workflow steps
```

Commit `.ax/policy/` to git — team-shared, IDE-agnostic.

## MCP tools

| Tool | Purpose |
|---|---|
| `ax_preflight` | Turn-start: matched rules + skills |
| `ax_rules` | List or match rules |
| `ax_skill` | Load skill by name |
| `ax_guard` | Pre-write CRITICAL checks |
| `ax_explore` | Code structure (unchanged) |

## CLI

```bash
ax policy index [--force]
ax policy match "prompt" [--file path] [--json]
ax policy rules [--json]
ax policy skills [--json]
```

## Environment

| Variable | Effect |
|---|---|
| `AX_NO_POLICY` | Skip policy in prompt-hook |
| `AX_POLICY_MAX_CHARS` | Injection cap (default 16000) |
| `AX_WEB_READONLY` | Browse-only ax web |

## ax web

```bash
ax web --port 7070 --open
```

Navigate to **Rules** or **Skills** in the sidebar. Edit frontmatter + markdown body, save to disk, auto re-index.

See [POLICY_ENGINE_PLAN.md](./POLICY_ENGINE_PLAN.md) for full architecture.
