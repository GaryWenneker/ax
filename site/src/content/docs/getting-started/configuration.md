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

When the file already exists, ax only adds missing scaffold keys and keeps everything else. If the file is not valid JSON (for example after a hand edit left a trailing comma) or its root is not an object, ax leaves it unchanged and prints a warning naming the file. Saving share settings, in `~/.ax/config.json` or in a project `ax.json`, likewise refuses to rewrite a file it cannot parse and reports the error instead.

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

### Review comment language

`reviews.commentLanguage` chooses the language of comments an agent posts on a pull request. Set it in the project's `ax.json`. The project value wins over the same key in `~/.ax/config.json`. When neither is set, the language is English.

Allowed codes: `en`, `nl`, `de`, `fr`, `es`, `pt`, `it`, `pl`, `sv`, `da`, `tr`.

```json
{
  "reviews": { "commentLanguage": "en" }
}
```

Chat replies, website and repo docs, UI strings, and commit messages stay **English** — never Dutch or other non-English product copy. Only pull-request review comments follow `reviews.commentLanguage` (default English when unset).

Re-index (`ax index`) after changing either config file.

### Open Knowledge Format (OKF) export

Optional `okf` section controls the portable Markdown OKF bundle written by `ax export okf`. Paths are **relative** to the project root:

```json
{
  "okf": {
    "enabled": true,
    "outDir": "knowledge",
    "autoExportOnSync": false,
    "kinds": [],
    "azdoWiki": {
      "enabled": false,
      "remote": "",
      "local": ".ax/wiki-okf",
      "subdir": "okf",
      "commitMessage": "chore: refresh Open Knowledge Format (OKF) bundle"
    }
  }
}
```

See [Open Knowledge Format (OKF)](/guides/okf/) for export, `--check`, and optional Azure DevOps Wiki publish.

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

Stored in `~/.ax/config.json` under a separate `"offload"` key. Lets ax delegate heavy reasoning queries to an OpenAI-compatible API.

Offload is off until you switch it on. When it is on, `ax explore` sends the explore text, which contains numbered project source, to that API. `ax init` never switches it on: when it finds a provider key such as `OPENAI_API_KEY` in the environment, it only prints the `ax offload set-endpoint` command that would. `ax explore --json` prints the structured result and never calls the offload API. Turn offload on with `ax offload set-endpoint <url> --key-env <VAR>` and off with `ax offload clear`.

```json
{
  "offload": {
    "url": "https://api.openai.com/v1",
    "model": "gpt-4o",
    "key_env": "OPENAI_API_KEY",
    "style": "plain"
  }
}
```

| Key | Default | Description |
|---|---|---|
| `url` | — | OpenAI-compatible base URL. Offload is on only when this (or `AX_OFFLOAD_URL`) is set |
| `model` | the provider's default | Model name. Without it, a known provider uses its default model (OpenAI `gpt-4o`, Cerebras `gpt-oss-120b`, Groq `llama-3.3-70b-versatile`, Ollama `llama3`). Together AI, Fireworks and OpenRouter have no default: offload stays off and `ax offload status` shows a `problem` until you set a model. Other endpoints fall back to `gpt-oss-120b` |
| `key_env` | — | Name of the env var that holds the API key. It is sent only to the configured `url`: when `AX_OFFLOAD_URL` points to a different endpoint, ax sends no key there (set `AX_OFFLOAD_KEY` for it) |
| `effort` | not sent | `low` / `medium` / `high`, sent as `reasoning_effort`. Set it only for a reasoning model: non-reasoning models such as `gpt-4o` reject the parameter |
| `style` | `plain` | Response style hint |

`ax offload set-endpoint`, `ax offload clear` and `ax init` change only their own section of `~/.ax/config.json` and replace the file atomically. When the file is not valid JSON, or its top level is not an object, they stop with an error naming the file and leave it as it is.

All keys can also be set via environment variables (`AX_OFFLOAD_URL`, `AX_OFFLOAD_MODEL`, `AX_OFFLOAD_KEY`, `AX_OFFLOAD_EFFORT`, `AX_OFFLOAD_STYLE`). Env vars take precedence over the file.

Offload is off by default. An API key in your environment (`OPENAI_API_KEY`, `CEREBRAS_API_KEY`, and so on) does not turn it on: ax sends explore context to an outside model only after you save an endpoint with `ax offload set-endpoint <url> --key-env <VAR>` or set `AX_OFFLOAD_URL`. `ax init` and `ax offload status` name a provider whose key they find, as a suggestion. `AX_OFFLOAD_DISABLE=1` turns offload off even when an endpoint is configured.

Request limits are set with environment variables only:

| Variable | Default | Description |
|---|---|---|
| `AX_OFFLOAD_TIMEOUT_MS` | `20000` | Longest wait for one request. Zero or an invalid value uses the default. Connecting to the endpoint (DNS and connect) is limited to 2 seconds |
| `AX_OFFLOAD_MAXTOKENS` | `12000` | `max_tokens` sent with each request. Zero or an invalid value uses the default |
| `AX_OFFLOAD_BACKOFF_MS` | `30000` | After the endpoint cannot be reached (refused, unreachable, or no answer within the 2 s connect timeout) or drops the connection without a reply, explores skip offload for this long and return the plain output at once. A slow reply that hits `AX_OFFLOAD_TIMEOUT_MS` does not start the back-off. `0` retries on every explore |
| `AX_OFFLOAD_STRIP` | off | `1` removes ax's own navigation hints from the explore output before it is sent; retrieved source lines are never changed |

`ax offload status` shows every one of these values.

When the provider rejects a request or cannot be reached, ax falls back to the plain explore output and logs a warning with the status code and the start of the provider's reply (once per process; every time with `AX_OFFLOAD_DEBUG=1`). The API key is never logged.

MCP graph calls log estimated context-token savings in **`~/.ax/usage.db`**. Query with [`ax savings`](/reference/cli/#ax-savings). In [ax-admin](/guides/remote-admin/) the **Usage** page shows cost, tool economics and budget. See [Token savings](/guides/token-savings/). Imported agent turns are quoted separately with [`ax costs`](/reference/cli/#ax-costs) and compared to a local plan with [`ax budget`](/reference/cli/#ax-budget). See [Agent budget](/guides/budget/). Pi runs stay in Pi; Ax can record their estimates with [`ax agent economics`](/reference/cli/#ax-agent-economics). See [Pi integration](/guides/pi/).

## Policy rules and skills

ax stores agent instructions under `.ax/policy/`:

| Path | Format |
|---|---|
| `.ax/policy/rules/*.mdc` | YAML frontmatter + markdown rule body |
| `.ax/policy/skills/*/SKILL.md` | YAML frontmatter + skill workflow |

Run `ax policy index` after editing policy files (or let `ax init` / `ax sync` re-index automatically). List what is indexed with [`ax policy rules`](/reference/cli/#ax-policy-rules-path) and [`ax policy skills`](/reference/cli/#ax-policy-skills-path), and test which rules a prompt matches with [`ax policy match`](/reference/cli/#ax-policy-match-prompt-path). In [ax-admin](/guides/remote-admin/) the **Rules** and **Skills** pages browse them; it has no rule or skill editor, so you edit the files in your own editor.

### Policy storage mode

Policy can live in **files** (`.ax/policy/` on disk) or **database** (`ax.db` tables). Set per project in `ax.json` or globally in `~/.ax/config.json`:

```json
{
  "policy": {
    "storage": "database"
  }
}
```

```bash
ax policy storage status           # show effective mode (project + global)
ax policy storage database         # scan policy folders, show per-item interview (no import)
ax policy storage database --yes   # import all candidates into ax.db, then switch
ax policy storage files --yes      # export DB → files, then switch
```

The migration scan walks the **entire codebase** for `.mdc` rules and `SKILL.md` skills — including `.cursor/rules/`, `.cursor/skills/`, and other paths — not only `.ax/policy/`. Bootstrap files (e.g. `.cursor/rules/ax.mdc`) are skipped. Each candidate includes interview questions (import yes/no, id, level, triggers, globs, priority, storage destination).

In **database** mode, edits via `ax policy capture` (or the MCP tool `ax_policy_capture`) write to SQLite; run `ax policy export` to materialize files for git. In **files** mode, `ax policy index` syncs disk → DB for delivery.

See the full [Policy Engine](/guides/policy-engine/) guide.

<a id="command-center-axshiptoml"></a>

## Quality gate (`.ax/ship.toml`)

ax seeds `.ax/ship.toml` on `ax init` when missing. It configures the ship quality-gate pipeline that [`ax ship --evaluate` and `ax ship --ci`](/reference/cli/#ax-ship-path) run, optional SonarQube, and the PR remote (Azure DevOps by default):

| Section | Purpose |
|---|---|
| `[ship]` | Target branch, and the dashboard port of `ax ship --watch` (full ax only) |
| `[quality_gate]` | Pipeline steps and test runner |
| `[remote]` | GitHub or Azure DevOps draft PR provider |
| `[sonar]` | Optional local SonarQube via Podman |
| `[reviewers]` | Map git blame emails to reviewer identities |

If `.ax/ship.toml` cannot be read or parsed (for example `web_port = "7070"` with quotes, or an unclosed string), ax logs the error with the file name and runs with default ship settings. While the file is broken, ax never writes to it: saving settings from the Command Center and the automatic update of `git_roots` are refused with that error, so your `[remote]`, `[sonar]` and other settings stay on disk. Fix or remove the file to save again.

The keys of each section are listed in [Configuration (`.ax/ship.toml`)](/guides/command-center/#configuration-axshiptoml) of the Command Center guide. That guide describes the full ax, which is being phased out; the file and its quality-gate keys are the same in the headless build.

## Where data lives

Per-project data lives in `.ax/` at your project root (SQLite database `ax.db`, schema v8). Global config lives in `~/.ax/config.json`. Nothing leaves your machine.
