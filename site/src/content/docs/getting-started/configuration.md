---
title: Configuration
description: ax is zero-config by default. One optional ax.json handles per-project overrides; one optional ~/.ax/config.json sets global defaults across every project.
---

ax is **zero-config by default** — language support is automatic from the file extension, and there is nothing to wire up per language. Two optional config files let you tune behaviour at different scopes:

| File | Scope | Purpose |
|---|---|---|
| `~/.ax/config.json` | **Global** — every project on your machine | Index defaults (extensions, exclude, includeIgnored) + LLM offload settings |
| `<project-root>/ax.json` | **Per-project** | Overrides or additions on top of the global defaults |

Per-project values always win. Lists (`exclude`, `includeIgnored`) are **unioned** — global entries plus per-project entries are both applied.

## What ax skips out of the box

- **Dependency, build, and cache directories** — `node_modules`, `vendor`, `dist`, `build`, `target`, `.venv`, `Pods`, `.next`, and the like across every [supported stack](/reference/languages/) — even with no `.gitignore`.
- **Anything in your `.gitignore`** — honored in git repos via git, and in non-git projects by reading `.gitignore` directly (root and nested).
- **Files larger than 1 MB** — generated bundles, minified JS, vendored blobs.

## Global config (`~/.ax/config.json`)

`ax install` creates this file automatically with an empty scaffold:

```json
{
  "index": {
    "extensions": {},
    "exclude": [],
    "includeIgnored": []
  }
}
```

Edit it to apply defaults to **every project** on your machine. For example, always map `.vue` files and always exclude coverage output:

```json
{
  "index": {
    "extensions": {
      ".vue": "typescript",
      ".svelte": "typescript"
    },
    "exclude": [
      "**/coverage/**",
      "**/__snapshots__/**"
    ],
    "includeIgnored": []
  }
}
```

The same file holds [LLM offload settings](#llm-offload-offload) under a separate `"offload"` key — the two sections are independent.

## Per-project config (`ax.json`)

Place `ax.json` at your project root for project-specific overrides. Values here are merged **on top of** the global config:

```json
{
  "extensions": {
    ".dota_lua": "lua",
    ".tpl": "php"
  },
  "exclude": ["static/metronic/**"],
  "includeIgnored": ["packages/", "services/"]
}
```

Commit the file to share settings with your team.

Re-index (`ax index`) after changing either config file.

---

## Excluding a tracked directory

`.gitignore` only affects files git **doesn't already track** — it can't drop a directory you've committed. For a vendored theme, SDK, or asset bundle that is checked into the repo, list it under `exclude` in `ax.json` (or in the global config to exclude it everywhere):

```json
{
  "exclude": ["static/", "**/vendor/**"]
}
```

Each entry is a gitignore-style pattern matched against project-root-relative paths. It applies to tracked files too and takes precedence over everything else.

## Custom file extensions

If your project uses a non-standard extension for a [supported language](/reference/languages/) — say `.dota_lua` for Lua — map it in `ax.json` or in the global config:

```json
{
  "extensions": {
    ".dota_lua": "lua",
    ".h": "cpp"
  }
}
```

Each value is a supported language id. Project-level mappings win over global ones on conflict.

## Indexing nested git repositories

ax respects `.gitignore`, so a gitignored directory stays out of the graph — including any git repositories nested inside it. To opt a gitignored directory back in (e.g. a super-repo of independent clones), use `includeIgnored`:

```json
{
  "includeIgnored": ["packages/", "services/"]
}
```

ax descends into the listed directories and indexes each embedded repo by its own `git ls-files`, so every child repo's own `.gitignore` is still honored.

A few things to know:

- **Untracked** nested repos (ones you haven't gitignored) are indexed automatically — `includeIgnored` is only for the ones your `.gitignore` excludes.
- Built-in skips like `node_modules` are never re-included, even inside an opted-in directory.

## LLM offload (`"offload"`)

Stored in `~/.ax/config.json` under a separate `"offload"` key. Lets ax delegate heavy reasoning queries to an OpenAI-compatible API:

```json
{
  "offload": {
    "url": "https://api.openai.com/v1",
    "model": "gpt-4o",
    "key_env": "OPENAI_API_KEY",
    "effort": "low",
    "style": "plain"
  }
}
```

| Key | Default | Description |
|---|---|---|
| `url` | — | OpenAI-compatible base URL (required to enable offload) |
| `model` | `gpt-oss-120b` | Model name |
| `key_env` | — | Name of the env var that holds the API key |
| `effort` | `low` | `low` / `medium` / `high` |
| `style` | `plain` | Response style hint |

All keys can also be set via environment variables (`AX_OFFLOAD_URL`, `AX_OFFLOAD_MODEL`, `AX_OFFLOAD_KEY`, `AX_OFFLOAD_EFFORT`, `AX_OFFLOAD_STYLE`). Env vars take precedence over the file.

## Where data lives

Per-project data lives in `.ax/` at your project root (SQLite database `ax.db`). Global config lives in `~/.ax/config.json`. Nothing leaves your machine.
