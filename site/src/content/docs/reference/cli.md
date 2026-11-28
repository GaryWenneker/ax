---
title: CLI
description: Reference for ax commands, arguments, and flags in the current main source.
---

Run `ax <command> --help` for the same information from the installed binary. Global help: `ax --help`.

The MCP server (`ax serve --mcp`) is started by your agent — you do not run it manually. See [MCP Server](/reference/mcp-server/).

This page describes the CLI of the headless ax: the CLI and the MCP server, without a built-in interface. The visual interface is [`ax-admin`](#ax-admin), a separate program.

## Which build has which command

There are two builds of `ax`. The headless build has the CLI and the MCP server. The full ax adds the Command Center, the desktop client and self-update. The full ax is being phased out; see [Headless ax and ax-admin](/guides/phase-out/). Today the headless build is released for Linux only ([Headless Linux build](/guides/headless-linux/)); on macOS and Windows the released ax is the full ax, and its CLI and MCP server are the same commands.

Every command on this page is in both builds, except these:

| Command | In the headless build |
|---|---|
| [`ax web`](#ax-web-path) | Exits with a message that it is not part of this build |
| [`ax share`](#ax-share-path) | Exits with a message that it is not part of this build |
| [`ax desktop`](#ax-desktop-path) | Exits with a message that it is not part of this build |
| [`ax ship --watch`](#ax-ship-path) | Exits with a message that it is not part of this build. `ax ship --evaluate`, `--ci` and `--draft` work |
| [`ax upgrade`](#ax-upgrade-version) | Exits with a message that self-update is disabled. Run the installer again to upgrade. The update notice after other commands is off too |
| [`ax server`](#ax-server-sync-apply-and-run) | Removed. The command does not exist |

These commands belong to the full ax. Their sections below are marked "Full ax only".

The headless installer (`install-slim.sh`) installs the binary only. It does not configure agents; run [`ax install`](#ax--ax-install) yourself for that.

## Global options

| Flag | Description |
|---|---|
| `-h`, `--help` | Print help for `ax` or a subcommand |
| `-V`, `--version` | Print installed version (same as `ax version`) |

Running `ax` with **no subcommand** starts the interactive installer (same as `ax install`).

## Which project a command uses

A command given a `path` uses that path. Without one, a command that reads or updates a project (`status`, `query`, `callers`, `explore`, `recall`, `policy`, `spec`, `neural`, `daemon`, `db`, …) and the agent hooks use the nearest directory at or above the current one that holds `.ax/ax.db`, the same way the MCP server finds its project. `~/.ax` is the global ax folder and is never taken for a project. So these commands work from any subdirectory of a project.

`init`, `install`, `uninit`, `index`, `sync`, `ship` and `bootstrap` act on the current directory itself and never reach up into a parent project.

## Environment variables

| Variable | Effect |
|---|---|
| `AX_FORCE_COLOR=1` | Force ANSI colors |
| `AX_UNICODE=1` | Force Unicode spinners/glyphs on Windows |
| `AX_ASCII=1` | Force ASCII glyphs everywhere |
| `NO_COLOR` | Disable ANSI colors |
| `AX_NO_UPDATE_CHECK=1` | Skip background upgrade notices after commands |
| `AX_NO_POLICY=1` | Skip policy injection in prompt-hook |
| `AX_NO_POLICY_CAPTURE=1` | Skip directive capture hints in prompt-hook |
| `AX_READ_GUARD=off` | Disable the read-guard hook (every Read/Grep passes) |
| `AX_READ_GUARD_STATE` | Path of the read-guard state file (default `~/.ax/read-guard.json`) |
| `AX_POLICY_MAX_CHARS` | Cap contextual policy inject (default `16000`). Always-apply rules are never hard-truncated. |
| `AX_DAEMON_EXE_CHECK_MS` | How often the MCP daemon checks whether its binary was replaced, in milliseconds (default `30000`; `0` turns the check off). See [`ax daemon`](#ax-daemon-path-statusstoprestart) |
| `AX_TELEMETRY=0` / `DO_NOT_TRACK=1` | Disable anonymous telemetry |
| `AX_MS_CLIENT_ID` | Optional custom Azure AD public client ID for OneDrive policy share — defaults to the built-in Microsoft app if unset |
| `AX_OFFLOAD_URL`, `AX_OFFLOAD_KEY`, … | Override explore offload settings |

## Configuration files

| File | Scope | Contents |
|---|---|---|
| `~/.ax/config.json` | Global | `index`, `offload`, `policy.storage` |
| `<project>/ax.json` | Per-project | `share`, policy overrides, `memory.perTurn` — each project has its own remote share config |
| `<project>/.ax/` | Per-project | `ax.db`, lock file, optional `policy/`. `.ax/.gitignore` keeps everything except `policy/` out of git |

See [Configuration](/getting-started/configuration/) for the full schema.

---

## Install & uninstall

### `ax` / `ax install`

Interactive installer — writes MCP config for detected AI agents (Cursor, Claude Code, Codex, opencode, Gemini CLI, Antigravity, Kiro, Hermes, VS Code Copilot, Windsurf, Zed). Does **not** index a project. After `ax install`, run `ax init` in each directory where you want to use ax. Prompt-hook and stop-hook are Claude Code-specific. The turn hooks for [per-turn memories](/guides/memory/#per-turn-memories) are added for Cursor and Claude Code. The [read-guard hook](#ax-read-guard) is added for every agent that can block a tool call: Cursor, Claude Code, VS Code Copilot, Codex, Gemini CLI, and Windsurf. See [Integrations](/reference/integrations/) for per-agent details.

| Argument / flag | Type | Description |
|---|---|---|
| `--yes` | flag | Non-interactive: skip prompts, install detected agents |
| `--all` | flag | Configure every supported agent, not only detected ones |
| `--target <id>` | string | Wire a single agent (e.g. `vscode`, `cursor`); ids are case-insensitive |
| `--path <dir>` | string | Project root for workspace MCP files (default: current directory). |
| `--mode <mode>` | `full` \| `cli-only` | `full` (default) wires MCP/hooks; `cli-only` (also `cli_only`, any case) seeds IDE instruction files without MCP, plugins, IDE hooks, or daemons (all supported IDEs — see [CLI-only mode](/guides/cli-only/)). Any other value is rejected |

Agent configs and hooks get the first `ax` on your `PATH`, exactly as found there: a shim such as `~/.local/bin/ax` stays the shim, so a later upgrade of the binary behind it needs no reinstall. With no `ax` on `PATH`, the binary running the install is written, and the report says so. When the two differ, the report names both. A Claude Code hook whose `ax` path is stale is repaired in place; your own hooks stay as they are.

```bash
ax install
ax install --yes
ax install --yes --all
ax install --target vscode
ax install --yes --target vscode --path .
ax install --mode cli-only
ax install --mode cli-only --target vscode
ax install --mode cli-only --target cursor
ax install --mode cli-only --target windsurf
```

`--mode cli-only` with no `--target` seeds every distinct IDE instruction surface plus the shared `AGENTS.md` / `.agents/skills/ax-cli` skill. It never writes MCP server configs. Full file matrix: [CLI-only mode](/guides/cli-only/).

### `ax uninstall`

Remove ax entries from agent MCP configuration files. Does not delete `~/.ax` or project `.ax/` indexes.

```bash
ax uninstall
```

---

## Project lifecycle

### `ax init [path]`

`ax init --all` is what the full ax installer and `ax upgrade` (full ax only) run after the binary is in place. It walks four levels under your home directory, skips hidden folders and dependency folders, and refreshes every directory that already has `.ax/ax.db`. A git repository or other software project without that database is left untouched. It does not prompt. A nested package inside an existing ax project is left to that project. `AX_SKIP_PROJECT_INIT=1` skips the scan. After `ax install`, run `ax init` in each directory where you want to use ax. `--all` does not do that.

Initialize a project: create `.ax/` (database, lock, `ship.toml`), index the project, install git hooks, install the Cursor savings hook (`ax savings hook install`), import Claude and Cursor savings logs (`ax savings import --all`), then offer the agent installer. A workspace init runs the savings steps once, after every member.

On **first init**, runs a full index. If `.ax/ax.db` already exists, runs an incremental `ax sync` instead — use `ax index` when you need a full rebuild.

`ax init` also writes `.ax/.gitignore`, so only `.ax/.gitignore` and the team policy in `.ax/policy/` can be committed. The database, logs, backups and `ship.toml` stay local. Lines you add yourself are kept and take precedence: add `!ship.toml` to share the quality-gate config. Your project's own `.gitignore` is not touched.

| Argument / flag | Type | Description |
|---|---|---|
| `path` | optional | Project root (default: current directory). Not used with `--all`. |
| `--workspace` | flag | Discover monorepo members (Cargo workspace + nested `.ax/`), write `members` to `ax.json`, and init each member |
| `--all` | flag | Initialize every project discovered under the home directory, without prompts |

**Choosing IDEs.** After the stack question, `ax init` asks which IDEs and agents ax should connect, in the same menu (arrow keys or j/k, space toggles, enter confirms, esc keeps the defaults). IDEs installed on this machine are marked "found": the CLI is on PATH, or the app is installed (`/Applications` on macOS, `%LOCALAPPDATA%\Programs` on Windows, the binary or `/usr/share` on Linux; Continue by its VS Code or Cursor extension). A config folder alone does not count, because ax creates those itself.

- The choice is saved in the project's `ax.json` as `agents.ides`. The next `ax init` pre-checks it, so pressing enter keeps it.
- Every IDE you leave unchecked that still has ax configured is disconnected: ax removes its config from disk, the same way `ax uninstall` does, and lists the files. Most of those configs live in your home folder, so that IDE is disconnected for every project on the machine. IDEs without ax config are not touched.
- Without a terminal (CI), init connects the saved IDEs, or the found ones when nothing is saved, and saves nothing. Set `AX_INIT_IDES="cursor vscode"` (or `none`) to answer the question in a script. That answer is saved like a typed one.

```bash
ax init
ax init ./services/api
ax init --workspace
ax init --all
```

### `ax bootstrap [path]`

Apply the versioned architecture seed. The seed version is independent of the ax binary version. A second run does not duplicate entities, relationships, rules, or skills. The command does not start Pi.

| Argument / flag | Type | Description |
|---|---|---|
| `path` | optional | Project root (default: current directory) |
| `--dry-run` | flag | Print the seed plan and write nothing |
| `--verify` | flag | Check required entities, relationships, rules, and skills. Exit non-zero when any are missing |
| `--json` | flag | Machine-readable result (`seedVersion`, `status`, `entitiesCreated`, `relationshipsCreated`, `rulesCreated`, `skillsCreated`) |

```bash
ax bootstrap
ax bootstrap --dry-run
ax bootstrap --verify
ax bootstrap --json
```

### `ax uninit [path]`

Remove the git sync hooks, then delete the `.ax/` directory. This permanently removes everything in `.ax/`: the index and stored memories (`ax.db`), project rules and skills (`policy/`), shared memories (`memory/`) and backups. The command lists what `.ax/` holds and asks before deleting. Without a terminal (a script or CI) it deletes nothing unless `--yes` is given. When a git hook cannot be cleaned, the command stops before `.ax/` is touched.

| Argument | Type | Description |
|---|---|---|
| `path` | optional | Project root (default: current directory) |
| `--yes`, `-y` | flag | Delete without asking |

```bash
ax uninit
ax uninit --yes
ax uninit ./old-project
```

### `ax index [path]`

Full re-index from scratch (scan → extract → resolve). Use when the watcher is off or after large git operations. Dot-directories such as `.git` and `.ax` are skipped; **`.scripts/` is indexed** (for example `.scripts/wcag`).

| Argument / flag | Type | Description |
|---|---|---|
| `path` | optional | Project root |
| `--force` | flag | Clear the database before indexing |
| `--quiet` | flag | No progress bar or summary |
| `--verbose` | flag | Reserved for extra diagnostics |
| `--all` | flag | Index every workspace member listed in root `ax.json` |

```bash
ax index
ax index ./services/api --force
ax index --quiet
ax index --all
```

### `ax sync [path]`

Incremental update — re-parses only changed files.

| Argument / flag | Type | Description |
|---|---|---|
| `path` | optional | Project root |
| `--quiet` | flag | No progress bar or summary |
| `--watch` | flag | Watch filesystem and auto-sync until Ctrl+C |
| `--all` | flag | Sync every workspace member listed in root `ax.json` |

Also refreshes git hooks when the memory-capture line is missing (idempotent upgrade path).

In a directory without `.ax/` (a fresh git worktree, for example), `ax sync --quiet` exits 0, prints nothing and creates nothing, so the git hooks stay silent there. Without `--quiet` it reports `project not initialized - run ax init` and exits 1.

```bash
ax sync
ax sync --quiet
ax sync --watch
ax sync --all
```

### `ax watch [path]`

Alias for `ax sync --watch`.

| Argument / flag | Type | Description |
|---|---|---|
| `path` | optional | Project root |
| `--quiet` | flag | No progress bar or summary |

```bash
ax watch
ax watch --quiet
```

### `ax plugins`

Lists the project's extractor plugins (`.ax/plugins/<name>/plugin.toml`) and controls which ones may run. A plugin can run any command, so `ax index`, `ax sync` and the MCP server skip every plugin until you trust it. Trust is stored in `~/.ax/plugin-trust.json` per plugin folder and is bound to a hash of every file in that folder (`plugin.toml`, the scripts or wasm module it runs, and anything else there); changing, adding or removing any of them needs a new `ax plugins trust`. See [Extractor plugins](/guides/plugins/).

| Subcommand | Description |
|---|---|
| `list [path] [--json]` | Each plugin, its extensions, the command it runs, and whether it is trusted |
| `trust [--path <p>] [names…]` | Trust all plugins of the project, or only the named ones |
| `untrust [--path <p>] [names…]` | Remove the trust again |

```bash
ax plugins list
ax plugins trust terraform
ax plugins untrust
```

### `ax status [path]`

Index statistics: node/edge/file counts, **document inventory by extension** (markdown, office, PDF, other opaque types), unresolved refs, pending sync, and last indexed time. When `.ax/integrity.json` records a database incident, the text and `--json` include it and name `ax db repair`. The project daemon runs a quick check once a day, and again after a crash. A broken full-text index, cue index, or vector map is rebuilt then. Stored rows, including a dangling edge, wait for `ax db repair`. The incident file is written when something remains. A recorded incident also skips the background file watcher until a later check passes.

| Argument / flag | Type | Description |
|---|---|---|
| `path` | optional | Project root |
| `--json` | flag | Machine-readable JSON (`stats.docsByExtension`, `pendingFiles`) |

```bash
ax status
ax status --json
```

### `ax unlock [path]`

Clear a stale `.ax/ax.lock`. When nobody holds the lock it is cleared right away and no process is touched. When a live process holds it, `ax unlock` stops that process only, and only when it is an ax process: SIGTERM first, SIGKILL after 5 seconds. Other ax processes, such as MCP daemons of other projects, keep running. A lock held by something that is not ax is reported and left alone. Prefer `ax daemon restart` first — writers normally wait up to 180s for the lock and clear dead PIDs automatically.

| Argument | Type | Description |
|---|---|---|
| `path` | optional | Project root |

```bash
ax unlock
ax unlock ./repo-with-stale-lock
```

---

## Graph query

### `ax query <text>`

Full-text symbol search (FTS5). For natural-language questions use `ax explore`.

| Argument / flag | Type | Description |
|---|---|---|
| `text` | required | Search string |
| `--kind` | string | Filter by node kind (`function`, `class`, `file`, …) |
| `--limit` | number | Max results |
| `--json` | flag | JSON array output |

```bash
ax query UserService
ax query "handleRequest" --kind function --limit 10
ax query auth --json
```

### `ax explore [query…]`

Natural-language explore — same output shape as the `ax_explore` MCP tool. Optional BYO LLM offload via `ax offload` rewrites the text answer; `--json` prints the structured result and never calls the offload API.

| Argument / flag | Type | Description |
|---|---|---|
| `query` | optional (words) | Question or symbol names (multi-word allowed) |
| `--json` | flag | Structured JSON |

```bash
ax explore "how does auth flow work"
ax explore ax_preflight token savings --json
```

### `ax node [name]`

One symbol's details, or a file with line numbers and dependents — same as `ax_node` MCP.

| Argument | Type | Description |
|---|---|---|
| `name` | optional | Symbol name or file path |

```bash
ax node handleRequest
ax node crates/ax-cli/src/main.rs
```

### `ax files`

List indexed files and detected languages.

| Flag | Type | Description |
|---|---|---|
| `--format` | string | Output format (e.g. `tree` when supported) |
| `--json` | flag | JSON with paths and languages |

```bash
ax files
ax files --format tree --json
```

### `ax context <task>`

Build task-oriented markdown context for agent prompts.

| Argument | Type | Description |
|---|---|---|
| `task` | required | Task description |

```bash
ax context "add rate limiting to the login endpoint"
```

### `ax callers <symbol>`

Find symbols that call the given function/method/class. Output is JSON.

| Argument | Type | Description |
|---|---|---|
| `symbol` | required | Symbol name |

```bash
ax callers handleRequest
```

### `ax callees <symbol>`

Find symbols called by the given function/method. Output is JSON.

| Argument | Type | Description |
|---|---|---|
| `symbol` | required | Symbol name |

```bash
ax callees run_preflight
```

### `ax impact <symbol>`

Blast-radius subgraph — what breaks if you change this symbol. Output is JSON.

| Argument | Type | Description |
|---|---|---|
| `symbol` | required | Symbol name |

```bash
ax impact AuthService
```

### `ax cycles`

List call-graph cycles: strongly connected components with more than one symbol, printed as `a → b` chains. Same engine as the `ax_cycles` MCP tool.

| Flag | Type | Default | Description |
|---|---|---|---|
| `--limit` | number | `50` | Max cycles to report |
| `--json` | flag | — | Machine-readable JSON |

```bash
ax cycles
ax cycles --limit 10 --json
```

### `ax path <from> <to>`

Shortest path between two symbols over Calls / References edges. Prints the chain, or an error when either symbol is not in the index. Same engine as the `ax_path` MCP tool.

| Argument / flag | Type | Description |
|---|---|---|
| `from` | required | Start symbol |
| `to` | required | End symbol |
| `--json` | flag | Machine-readable JSON |

```bash
ax path handleRequest saveUser
ax path handleRequest saveUser --json
```

### `ax api <module>`

Public API surface of a module: exported symbols whose file path starts with the given prefix, with kind and file. Same engine as the `ax_api` MCP tool.

| Argument / flag | Type | Default | Description |
|---|---|---|---|
| `module` | required | — | Module name or path prefix (for example `src/auth`) |
| `--limit` | number | `200` | Max exported symbols |
| `--json` | flag | — | Machine-readable JSON |

```bash
ax api src/auth
ax api crates/ax-share/src --limit 50 --json
```

---

## Architecture insights

Whole-graph analysis built on the same engine as the `ax_insights` / `ax_report` MCP tools. In ax-admin, the **Insights** page shows the communities and the most connected nodes. See [Architecture Insights](/guides/architecture-insights/).

### `ax insights [path]`

Detect subsystems (Leiden community detection), rank **god nodes** (most-connected concepts by in+out degree), and flag **surprising connections** (edges that cross both a community and a top-level module boundary).

| Argument / flag | Type | Default | Description |
|---|---|---|---|
| `path` | optional | cwd | Project root |
| `--resolution` | number | `1.0` | Cluster granularity — higher yields more, smaller communities |
| `--god-limit` | number | `20` | Max god nodes to show |
| `--surprising-limit` | number | `20` | Max surprising connections to show |
| `--json` | flag | — | Machine-readable JSON |

Community assignments are persisted (`node_communities` table) and only recomputed after `ax index` / `ax sync` or when you re-run with a different `--resolution`.

```bash
ax insights
ax insights --resolution 1.4 --god-limit 30
ax insights --json
```

### `ax report [path]`

Render a full Markdown architecture report: god nodes, communities with member counts, surprising connections, dead code, an unresolved-refs summary, and a set of suggested questions templated from the top god nodes and communities.

| Argument / flag | Type | Default | Description |
|---|---|---|---|
| `path` | optional | cwd | Project root |
| `--out` | string | `AX_REPORT.md` | Output file at project root |
| `--resolution` | number | `1.0` | Cluster granularity |
| `--stdout` | flag | — | Print to stdout instead of writing a file |

```bash
ax report
ax report --out docs/ARCHITECTURE.md
ax report --stdout
```

### `ax validate [path]`

Graph hygiene report: isolated symbols (no call or import edges), dangling edges (an edge whose target is missing), and orphan docs (Markdown with no documents / references edges). Use `--ci` to fail a pipeline on a dirty graph. Counts are always complete; the lists are capped at 200 isolated symbols and 100 orphan docs (`isolatedCount` and `orphanDocsCount` in `--json` carry the full numbers).

| Argument / flag | Type | Default | Description |
|---|---|---|---|
| `path` | optional | cwd | Project root |
| `--ci` | flag | — | Exit non-zero when dangling edges or isolated symbols exist |
| `--json` | flag | — | Machine-readable JSON |

```bash
ax validate
ax validate --json
ax validate --ci
```

### `ax export okf [path]` / `ax export concepts [path]`

Export an **Open Knowledge Format (OKF)** Markdown bundle from the indexed graph — one YAML-frontmatter page per concept with Calls / Called by links. Configure the relative output path under `okf` in `ax.json` (default `.ax/knowledge`). See [Open Knowledge Format (OKF)](/guides/okf/).

| Argument / flag | Type | Default | Description |
|---|---|---|---|
| `path` | optional | cwd | Project root |
| `--out` | path | `okf.outDir` or `.ax/knowledge` | Output directory |
| `--limit` | int | `0` | Max concepts (`0` = all) |
| `--check` | flag | — | Validate `index.md` + relative links |
| `--ci` | flag | — | With `--check`: exit non-zero on issues |
| `--publish-wiki` | flag | — | Publish bundle to `okf.azdoWiki` git remote |
| `--dry-run` | flag | — | With `--publish-wiki`: preview only |
| `--no-push` | flag | — | Commit wiki locally without push |
| `--json` | flag | — | Machine-readable output for check/publish |

```bash
ax export okf
ax export okf --out knowledge
ax export okf --check --ci
ax export okf --publish-wiki --dry-run
ax export concepts
```

`ax export concepts` is an alias for `ax export okf`.

### `ax export graph [path]`

Export the knowledge graph for external tools. Includes Leiden community ids, degree, and god-node flags.

| Argument / flag | Type | Description |
|---|---|---|
| `path` | optional | Project root |
| `--format` | string | `html` \| `json` \| `dot` \| `graphml` \| `gexf` \| `cypher` \| `mermaid` \| `plantuml` (default: `json`) |
| `--out` | path | Output file (default depends on format) |
| `--resolution` | float | Cluster granularity (default: `1.0`) |
| `--limit` | int | Max nodes by degree (default: `3000`) |

```bash
ax export graph --format json
ax export graph --format graphml --out graph.graphml
ax export graph --format cypher --out import.cypher
ax export graph --format mermaid --out graph.mmd
```

### `ax export graph-html [path]`

Export the graph as a single self-contained, interactive HTML file (inline JSON + a small force-directed renderer) — portable, no server required. Node color = community, node size = degree, docs render as distinct squares.

| Argument / flag | Type | Default | Description |
|---|---|---|---|
| `path` | optional | cwd | Project root |
| `--out` | string | `graph.html` | Output file |
| `--resolution` | number | `1.0` | Cluster granularity |
| `--limit` | number | `3000` | Max nodes to include (top by degree) |

```bash
ax export graph-html
ax export graph-html --out graph.html --limit 1500
```

---

## Memory vault

See the [Memory vault guide](/guides/memory/).

### `ax remember <text>`

Store a durable project memory (decision, fix, convention). Flags similar existing memories so contradictions get updated instead of duplicated.

| Argument / flag | Type | Description |
|---|---|---|
| `text` | optional | The memory content — what to remember and why. Omit with `--confirm` |
| `--title` | string | Short title (defaults to first line) |
| `--kind` | string | `decision` \| `bug_fix` \| `architecture` \| `convention` \| `note` |
| `--tag` | string (repeat) | Tag |
| `--file` | string (repeat) | Related file path |
| `--trigger` | string (repeat) | Exact cue name. Same as `ax_remember` `triggers` |
| `--confirm` | string | Confirm a candidate memory by id. Same as `ax_remember` `action=confirm` |
| `--purge` | string | Purge a memory by id (row, cues, links, vectors, tombstone). Same as `ax_remember` `action=purge` |
| `--when` | string | Prospective trigger (`file:glob` \| `symbol:name` \| `error:pat` \| `command:pat`). Stored as a `when:` tag. A file glob matches the whole project-relative path: `*` stays within one folder, `**` spans folders, and a pattern without `/` (`*.sql`, `Cargo.toml`) matches the file name in any folder. For the other kinds, a glob must match the whole value and plain text matches anywhere in it |
| `--json` | flag | JSON output, including `suggested_triggers` on a saved memory |

```bash
ax remember "We use tiktoken o200k_base for token counts" --kind decision --tag tokenizer
ax remember "Sonar proxy strips Content-Encoding after decompression" --kind bug_fix --file crates/ax-web/src/sonar_proxy.rs
```

### `ax recall [query]`

Exact cue, id, or free-text search. `--cue` and `--id` see every tier, including a demoted turn. `--depth archive` searches only the archive and is not what preflight injects. `--depth backup` searches zstd/JSONL snapshots under `.ax/backups/memory/` (also not injected).

| Argument / flag | Type | Default | Description |
|---|---|---|---|
| `query` | optional | — | Free-text query. Same as `ax_recall` `query` |
| `--cue` | string | — | Exact trigger, title, or tag. Same as `ax_recall` `cue` |
| `--id` | string | — | Full body and its cues. Same as `ax_recall` `id` |
| `--depth` | `hot` \| `archive` \| `backup` | `hot` | `archive` searches the permanent tier. `backup` searches memory snapshots. Same as `ax_recall` `depth` |
| `--limit` | number | `5` | Max results (≤50) |
| `--json` | flag | — | JSON output (`matches`, `cues`) |

```bash
ax recall --cue reinstall-all
ax recall "why tiktoken"
ax recall --id mem-123 --json
ax recall "old graph change" --depth archive
ax recall "purged phrase" --depth backup
```

### `ax history [query]`

When did I change X: dated agent turns (prompt, outcome, files, commits) and git commits that touched a file, symbol, or topic, newest first. See [Memory](/guides/memory/#when-did-i-change-x).

| Argument / flag | Type | Default | Description |
|---|---|---|---|
| `query` | optional | — | File path, symbol name, or topic (required unless `--id`) |
| `--since` | `YYYY-MM-DD` | — | Only entries on or after this date |
| `--limit` | number | `20` | Max entries (1–50) |
| `--id` | string | — | Show one turn memory with its full outcome |
| `--depth` | `hot` \| `archive` \| `backup` | `hot` | `archive` includes demoted turns, graph diffs, and activity; `backup` searches like `hot`. Same as `ax_history` `depth` |
| `--json` | flag | — | JSON output (`entries`, `count`) |

```bash
ax history review_language.rs
ax history "review comment language" --since 2026-09-01
ax history --id turn-1a2b3c4d5e6f7a8b
```

### `ax capture-git`

Mine recent non-merge git commits into memories (the "why" behind changes). Skips trivial messages and already-captured commits; safe to re-run.

| Flag | Type | Default | Description |
|---|---|---|---|
| `--limit` | number | `100` | Number of commits to scan |
| `--quiet` | flag | — | No output (used by git hooks) |
| `--json` | flag | — | JSON output |

Runs automatically from **post-commit** and **post-merge** git hooks after `ax init`. Skips trivial messages and already-captured commits.

```bash
ax capture-git
ax capture-git --limit 50
ax capture-git --limit 1 --quiet    # same as the post-commit hook
```

### `ax memory export`

Export memories that carry a tag (default: `shared`) to `.ax/memory/shared.jsonl` for team git sync. Memories tagged `local`, `noshare` or `private`, disabled memories, and kinds the import skips (turns, folder docs and kinds ax does not know) are not exported.

| Flag | Default | Description |
|---|---|---|
| `--tag` | `shared` | Only export memories with this tag |
| `--out` | `.ax/memory/shared.jsonl` | Output path |
| `--quiet` | off | No summary (for git hooks) |

```bash
ax remember "Adopt GraphQL federation" --kind decision --tag shared
ax memory export
ax memory export --tag team --out ./shared-memories.jsonl
```

### `ax memory import`

Import JSONL memories (upsert by id). Used after `git pull` or via the post-merge hook when `"memorySync": true` is set in `ax.json`.

Imported rows are untrusted data: they get the source `import`, and a row whose text reads like an instruction to the agent waits as a candidate until you confirm it (`ax remember --confirm`). The import skips lines with an unknown kind, turns and folder docs, and ids you purged. An existing memory is replaced only when it is itself shared (imported earlier or tagged `shared`) and the shared line is newer; a local memory you never shared is never overwritten.

```bash
ax memory import
ax memory import --path ./shared-memories.jsonl
```

### `ax memory prune-file-changes`

Remove automatic `file_change` memories (`changed <path>`) whose every path the index excludes: builtin skip folders such as `target`, `node_modules`, and `.git`, dot-folders such as `.claude/worktrees`, `ax.json` excludes, and gitignored paths (every `.gitignore`, `.git/info/exclude`, and the global excludes file). Sync and the file watcher no longer write these memories; this command removes the ones older builds wrote. Other memory kinds are never touched.

Without `--apply` it is a dry run: it prints how many memories match, the folders they name, and deletes nothing. With `--apply`, each matched memory is purged with its cues, links, and vector, and its id is tombstoned so import and restore cannot bring it back.

| Flag | Default | Description |
|---|---|---|
| `--apply` | off | Delete the matched memories |
| `--json` | off | Print the report as JSON (`dry_run`, `scanned`, `matched`, `purged`, `kept`, `areas`, `sample`) |

```bash
ax memory prune-file-changes
ax memory prune-file-changes --json
ax memory prune-file-changes --apply
```

### `ax docs-catalog sync`

Build `documentation-catalog` memories from an optional git wiki, `.docs/`, agent skills, and script READMEs, import them into `ax.db`, and run `ax sync`.

| Flag | Default | Description |
|---|---|---|
| `--skip-wiki-pull` | off | Use the existing wiki clone without `git pull` |
| `--dry-run` | off | Write the JSONL only; skip the import and graph sync |
| `--json` | off | Print the sync report as JSON |

```bash
ax docs-catalog sync
ax docs-catalog sync --dry-run --json
ax docs-catalog sync --skip-wiki-pull
```

Configure it in `ax.json` under `docsCatalog`. Every key is optional, and there are no built-in wiki defaults: without `wiki_remote` the wiki step is skipped (`wikiAction: "not-configured"`).

| Key | Default | Description |
|---|---|---|
| `name` | project folder name | Used in memory titles |
| `wiki_remote` | none | Git URL of the wiki to clone and pull |
| `wiki_local` | `.current/wiki` | Local clone path |
| `wiki_apps_subdir` | `""` | Wiki subfolder to scan for sections |
| `wiki_root_url` | `wiki_remote` | Browser URL stored in the catalog |
| `wiki_integrations_dir` | `Integrations` | Folder of integration pages under `wiki_apps_subdir` |
| `wiki_products_page` | none | Markdown page whose table rows list products |
| `wiki_products_skip` | `[]` | First-cell values to leave out of the products list |
| `skill` | none | Skill name mentioned in the refresh instructions |
| `jsonl_path` | `.ax/memory/documentation-catalog.jsonl` | Output file |
| `docs_root` / `skills_root` / `scripts_root` | `.docs` / `.agents/skills` / `.scripts` | Workspace sources |

Every path key must stay inside the project: absolute paths, `..` and symbolic links that lead out of the project are refused, and the sync stops with an error naming the key. `wiki_apps_subdir`, `wiki_integrations_dir` and `wiki_products_page` must be relative paths inside the wiki. A `wiki_remote` that starts with `-` is refused, because git would read it as an option.

The wiki `git clone` and `git pull` never prompt: they run without a terminal or credential prompt and stop with an error after 10 minutes. A private wiki therefore needs credentials git can use on its own (a credential helper, an SSH key or a token in the URL).

```json
{
  "docsCatalog": {
    "name": "Contoso",
    "wiki_remote": "https://dev.azure.com/contoso/Docs/_git/Docs.wiki",
    "wiki_apps_subdir": "Applications",
    "wiki_products_page": "Products.md"
  }
}
```

---

## Sessions, turns & CLI-only parity

Shared engines with MCP (`ax_preflight`, `ax_session`, `ax_durable`, stash/expand). No MCP transport required. Guide: [CLI-only mode](/guides/cli-only/). Repo notes: [`docs/CLI_ONLY.md`](https://github.com/GaryWenneker/ax/blob/main/docs/CLI_ONLY.md).

### `ax preflight`

Run the same policy/memory/working-context inject as MCP `ax_preflight`.

```bash
ax preflight --prompt "How does auth refresh work?"
printf '%s' "$PROMPT" | ax preflight --prompt-stdin --json
```

### `ax session`

Working-context notes for a conversation (`add`, `get`, `update`, `compact`, `clear`, `fork`, `handoff`).

```bash
SESSION=$(ax session start --json | jq -r .session)
printf '%s' '{"action":"add","facts":["Auth uses SameSite=Lax"]}' \
  | ax session add --session "$SESSION" --stdin
ax session get --session "$SESSION"
```

### `ax durable`

Transcript and task persistence (same engine as `ax_durable`).

```bash
ax durable append --session "$SESSION" --kind user --body-stdin <<'EOF'
Continue the auth investigation
EOF
ax durable read --session "$SESSION"
ax durable fork --session "$SESSION" --json
ax durable handoff --session "$SESSION" --input-stdin <<'EOF'
{"note":"Hand off to a fresh thread"}
EOF
ax durable doc-put --session "$SESSION" --kind brief --body-stdin <<'EOF'
# Brief
EOF
ax durable doc-get --session "$SESSION" --kind brief
```

### `ax turn`

Explicit turn lifecycle without IDE hooks. Finish writes a memory with `source=cli-turn`. When the finish input has `decisions` or `open_questions`, finish also adds them to the session's working context (the notes `ax_session` keeps). That needs an index; when the write fails, the receipt lists the reason in `warnings` and the turn still finishes.

The receipt's `files` are the files git reports as changed during the turn, renames by their new path. Files that were already modified or untracked when the turn began are left out unless they changed again during the turn. At most 200 files are listed; a larger set adds a `warnings` entry with the full count. Each git call stops after 30 seconds.

Snapshots are stored under `.ax/cli-turns/<session>/<turn>.json`. The `--session` and `--turn` values must be 1 to 128 ASCII letters, digits, `-`, `_`, `.` or `:`, and not dots only; any other value is rejected, so a turn cannot read or write files outside that folder.

```bash
TURN=$(printf '%s' "$PROMPT" | ax turn begin --session "$SESSION" --prompt-stdin --json | jq -r .turnId)
ax turn event --session "$SESSION" --turn "$TURN" --kind finding --body "cookie cleared on soft reload"
printf '%s' "Root cause: SameSite mismatch" \
  | ax turn finish --session "$SESSION" --turn "$TURN" --outcome-stdin --json
ax turn status --session "$SESSION" --turn "$TURN"
```

### `ax cache`

Stash and expand large tool results (`ax_stash` / `ax_expand` / `ax_cache_status`).

```bash
ax cache status
printf '%s' "$BIG" | ax cache stash --stdin --json
ax cache expand --id "$ID"
```

### `ax doctor`

CLI-only or full host diagnostics (covers the MCP `ax_diagnostics` surface).

```bash
ax doctor --mode cli
ax doctor --mode all --json
```

`--mode` takes `cli` (the default) or `all`; any other value is rejected.

`ax doctor` also runs a quick integrity check of `.ax/ax.db` and names `ax db repair` when that check finds a problem. When an older MCP build is registered, it names that process and says to restart the MCP server in Cursor. It does not stop the server.

### `ax db`

Check and repair the project database (`.ax/ax.db`), the usage database, and the global database.

```bash
ax db check
ax db check --db all --json
ax db repair --dry-run
ax db repair
```

`ax db check` exits non-zero when it finds a problem. `ax db repair` writes a `VACUUM INTO` snapshot into `.ax/backups/db/` before it changes a readable database. A file that cannot be vacuumed is copied as bytes, including its write-ahead log. Repair rebuilds indexes and full-text search, removes cues, links, edges, file contents, and unresolved refs that point at a missing parent, extracts again any project file that still has a node on a dangling edge, checkpoints the write-ahead log, and applies migrations that have not run. An embedding that is not 1024 bytes is reported and left for a later re-embed. It does not delete a memory to make a check pass. A file that is not a database is copied aside and left in place.

`--stop-writers` stops the project daemon and retries. A file that is still locked is left unchanged. The lock finding names the process id and command when `lsof` can see who has the file open. It does not stop MCP, or `ax web` in the full ax.

### Guard mapping (`ax_guard`)

MCP `ax_guard` has no dedicated CLI twin. Use:

- `ax read-guard --ide <ide>` — IDE pre-tool hook that redirects whole-file reads to the graph
- `ax doctor --mode cli` — host/policy diagnostics before a CLI-only run

Policy CRITICAL rules still apply when the CLI invokes shared tools in-process.

---

## Global index

### `ax global`

Aggregate project indexes into one machine-wide database, `~/.ax/global.db`. `sync` copies nodes, files, and edges from a project's `.ax/ax.db`. Files with the same hash in more than one project are recorded as shared knowledge and cross-project references. Set `AX_GLOBAL_DB` to use a different database path.

| Subcommand | Description |
|---|---|
| `init` | Create `~/.ax/global.db` (or `AX_GLOBAL_DB`) and apply the schema |
| `sync [path]` | Copy this project's `.ax/ax.db` into the global database (default: cwd) |
| `status` | Show the database path and project, node, document, and shared-knowledge counts |

```bash
ax global init
ax global sync
ax global sync ./services/api
ax global status
```

---

## Git & testing

### `ax diff`

Symbol-level blast radius for the current git diff vs a base branch.

| Flag | Type | Default | Description |
|---|---|---|---|
| `--base` | string | `main` | Base branch or ref |
| `--json` | flag | — | JSON output |

```bash
ax diff --base main
ax diff --base origin/main --json
```

### `ax test-impact`

Test-function impact analysis (git diff + TIA graph). When git cannot diff against the base (an unknown ref, a shallow clone without it, or no git repository), the command fails with the git error instead of reporting zero changed files.

| Flag | Type | Default | Description |
|---|---|---|---|
| `--base` | string | `main` | Base branch or ref |
| `--json` | flag | — | JSON output |

```bash
ax test-impact --base main
ax test-impact --base develop --json
```

### `ax affected [files…]`

Reverse impact: find **test files** affected by changes. See [Affected Tests in CI](/guides/affected-tests/).

| Argument / flag | Type | Default | Description |
|---|---|---|---|
| `files` | optional (repeat) | — | Changed source file paths |
| `--stdin` | flag | — | Read changed paths from stdin |
| `--base` | string | `main` | Base branch when no files given |
| `-d`, `--depth` | number | `5` | Max dependency traversal depth |
| `-f`, `--filter` | string | — | Glob filter for test files |
| `-j`, `--json` | flag | — | JSON output |
| `-q`, `--quiet` | flag | — | File paths only |

```bash
ax affected src/auth/login.ts
git diff --name-only main | ax affected --stdin
```

---

<a id="command-center"></a>

## Quality gate, LSP and agent hooks

### `ax ship [path]`

Git-aware quality gate and draft PRs. The MCP tool `ax_ship` runs the same evaluation (`mode`: `evaluate` or `ci`). `--watch`, with `--port` and `--open`, starts the dashboard of the full ax and is not in the headless build; see [Which build has which command](#which-build-has-which-command).

| Argument / flag | Type | Default | Description |
|---|---|---|---|
| `path` | optional | cwd | Project root |
| `--watch` | flag | — | Full ax only. Watch git events and open dashboard |
| `--evaluate` | flag | — | Run one quality-gate evaluation |
| `--ci` | flag | — | Headless CI: evaluate, print JSON on stdout, summary on stderr, **exit 1** if the gate failed |
| `--draft` | flag | — | Create draft PR after quality gate |
| `--title` | string | — | PR title |
| `--port` | number | `7070` | Full ax only. Dashboard port for `--watch` |
| `--open` | flag | — | Full ax only. Open browser for `--watch` |
| `--auto-commit` | flag | — | Force-enable Aider-style checkpoint commit before this evaluation, overriding `.ax/ship.toml` `[auto_commit]` for this run only |
| `--revert-on-fail` | flag | — | With `--auto-commit`, `git reset --mixed` the checkpoint if the quality gate fails (file contents stay on disk, uncommitted) |
| `--quiet` | flag | — | With `--evaluate`: print nothing when the gate passes and one line on stderr (`ax: quality gate failed: <steps>`) when it fails. Exits 0 whether the gate passes or fails. The git hooks ax installs use this. In a directory without `.ax/` it does nothing and exits 0 |

```bash
ax ship --evaluate
ax ship --evaluate --quiet       # what the git hooks run
ax ship --ci                     # CI / GitHub Actions
ax ship --evaluate --auto-commit --revert-on-fail
ax ship --draft --title "feat: memory vault"
ax ship --watch --open           # full ax only
```

Example workflows:

- [GitHub Actions](https://github.com/GaryWenneker/ax/blob/main/docs/examples/github-actions-ship.yml)
- [GitLab CI](https://github.com/GaryWenneker/ax/blob/main/docs/examples/gitlab-ci-ship.yml)
- [Azure Pipelines](https://github.com/GaryWenneker/ax/blob/main/docs/examples/azure-pipelines-ship.yml)

---

### `ax share [path]`

Full ax only. Not in the headless build. The full ax is being phased out; see [Headless ax and ax-admin](/guides/phase-out/).

Share Command Center on the LAN with a bearer token (read-only session).

| Argument / flag | Type | Default | Description |
|---|---|---|---|
| `path` | optional | cwd | Project root |
| `--port` | number | `7070` | Listen port |
| `--bind` | string | `0.0.0.0` | Bind address |
| `--open` | flag | — | Open browser with token URL |
| `--token` | string | random | Share token |

```bash
ax share --open
```

### `ax lsp status` / `ax lsp enrich`

Optional Language Server enrichment. `enrich` calls `textDocument/definition` on
unresolved refs and writes edges with confidence `exact`. Status uses `rustup which`
and project `node_modules/.bin`, not PATH shims alone.

```bash
ax lsp status
ax lsp enrich --limit 100
rustup component add rust-analyzer
```

---

### `ax stop-hook`

Claude Code `Stop` / `SubagentStop` hook target — internal command wired automatically by `ax install` for Claude Code, not meant to be run manually. Reads Claude's JSON payload on stdin, runs `ax_guard`-equivalent checks against the uncommitted files inside the ax project (`git status --porcelain -z --untracked-files=all`, so new files in new folders and non-ASCII names count, and a project in a subfolder of the repository works), up to 40 files per turn (the block reason says how many were left out), and on a CRITICAL violation prints `{"decision": "block", "reason": "..."}` so Claude fixes the issue before the turn actually ends. Honors `stop_hook_active` to avoid infinite loops and no-ops when there's no indexed policy. It also writes the [per-turn memory](/guides/memory/#per-turn-memories) for the turn.

| Env var | Effect |
|---|---|
| `AX_NO_STOP_HOOK=1` | Disable — hook exits immediately without checking or writing a turn memory |

```bash
# Wired automatically:
ax install                       # adds Stop + SubagentStop hooks for Claude Code
```

---

### `ax read-guard`

Pre-tool hook target, wired automatically by `ax install` and not meant to be run by hand. Reads the IDE's hook JSON on stdin and steers agents from raw file reads to the ax graph:

- The **first whole-file read** of an indexed source file in a conversation is denied. The reply lists up to eight of the file's symbols as `ax_node("…")` calls.
- The **first search for a bare symbol name** that the graph knows (`Grep`, `rg`, `grep`, `git grep`) is denied. The reply lists the matches with file and line and points to `ax_node`, `ax_callers`, and `ax_impact`.
- **Repeating the identical call is allowed**, so an agent that really needs the raw file (for example right before an edit) is never stuck.

Partial reads (`offset`/`limit`, `head`, `sed -n`, a piped `cat`) pass where the IDE sends the range to the hook. Cursor's `preToolUse` payload carries only `file_path`, so in Cursor a partial Read of an indexed file is denied once too, and the identical retry passes. Non-source files, regex or free-text searches, symbols the graph does not know, and anything outside an indexed project always pass. Shell commands are only inspected for their first simple command. Every error allows the call. Memory of denied calls is kept per conversation for 4 hours in `~/.ax/read-guard.json`.

| `--ide` | Hooks file | Event (matcher) |
|---|---|---|
| `cursor` | `~/.cursor/hooks.json` | `preToolUse` (`Read\|Grep\|Shell`) |
| `claude` | `~/.claude/settings.json` (Claude Code and VS Code Copilot) | `PreToolUse` (`Read\|Grep\|Bash`) |
| `codex` | `~/.codex/hooks.json` | `PreToolUse` (`^Bash$`). Codex only runs new hooks after you trust them once with `/hooks` |
| `gemini` | `~/.gemini/settings.json` | `BeforeTool` (`read_file\|search_file_content\|grep\|run_shell_command`) |
| `windsurf` | `~/.codeium/windsurf/hooks.json` | `pre_read_code`, `pre_run_command` |

Zed, Continue, Kiro, opencode, Antigravity, and Hermes have no blocking tool hook; they get the graph-first instructions only. A hooks file that is not valid JSON is left unchanged and reported by `ax install`. `ax uninstall` removes only the read-guard and turn-hook entries.

| Env var | Effect |
|---|---|
| `AX_READ_GUARD=off` | Disable — every call passes (`0`, `false`, `no` also work) |
| `AX_READ_GUARD_STATE` | Alternative state file path |

---

## Daemon

### `ax daemon [path] [status|stop|restart]`

MCP background daemon control (shared index connection per project). IDEs attach as stdio proxies; `restart` clears a stuck daemon and stale locks.

The daemon follows its binary. It checks every 30 seconds (`AX_DAEMON_EXE_CHECK_MS`) whether its `ax` file was replaced, for example by an upgrade, and then stops cleanly. A proxy on a newer `ax` build restarts an older daemon on its own binary; a proxy on an older build attaches to a newer daemon and leaves it running. When the daemon goes away, the proxies reconnect or start a new one without the IDE noticing. A call that was in flight gets the error `ax daemon restarted; retry the call`. If no daemon is back within 15 seconds, the proxy exits with the reason on stderr.

| Argument / subcommand | Description |
|---|---|
| `path` | Optional project root (before subcommand) |
| `status` | Show pid, port, socket from `.ax/daemon.json` (default) |
| `stop` | Stop the running daemon. It first checks that the recorded pid is still an ax daemon, sends SIGTERM, waits up to 5 seconds, then sends SIGKILL; it reports "stopped" only once the process is gone and fails otherwise. A pid that now belongs to another program is left running and the stale `.ax/daemon.json` is removed |
| `restart` | Stop (the same way) + start a fresh daemon (Command Center **Reload MCP** uses the same path). Restarts are serialized by `.ax/daemon-restart.lock`, so restarts started from several proxies or windows at once never stop each other's new daemon; a proxy whose restart finds the daemon already on its build skips it |

```bash
ax daemon status
ax daemon stop
ax daemon restart
ax daemon ./repo status
```

---

## Version & upgrade

### `ax version`

Print installed version. Aliases: `ax -V`, `ax --version`.

```bash
ax version
ax --version
```

### `ax upgrade [version]`

Full ax only. Not in the headless build. The full ax is being phased out; see [Headless ax and ax-admin](/guides/phase-out/).

To upgrade a headless build, run its installer again or pull a newer image; see [Headless Linux build](/guides/headless-linux/#install).

Self-update from GitHub Releases, with [getax.wenneker.io/releases](https://getax.wenneker.io/releases/latest.txt) as the fallback. Non-interactive — no confirmation prompt.

For private repositories and higher rate limits, `ax upgrade` and `ax upgrade --check` send a GitHub token from `AX_GITHUB_TOKEN`, `GITHUB_TOKEN` or `GH_TOKEN`, or else from `gh auth token`. The `gh` call runs with stdin closed and is stopped after 3 seconds. The background update notice after other commands uses only the environment variables and never runs `gh`.

Versions compare as semver: `2.1.0-rc.1` is lower than `2.1.0`, so a pre-release build is offered the final release. A release tag or `latest.txt` value that is not a `major.minor.patch` version (with an optional `-pre` suffix), such as `nightly` or an HTML error page, is ignored and never shown as an update.

Every downloaded archive is checked against the release's `SHA256SUMS` (fetched from GitHub when possible, otherwise from getax) before it is extracted. A missing checksum list, a missing entry or a different digest stops the upgrade and leaves the installed binary alone. A `--local` archive is checked against a `SHA256SUMS` in the same folder when one is there.

| Argument / flag | Type | Description |
|---|---|---|
| `version` | optional | Pin a release tag (e.g. `v3.0.0`) |
| `--check` | flag | Check for updates without installing |
| `--local [archive]` | flag / path | Install from local `dist/ax-<platform>.zip` (maintainers); optional explicit archive path |

```bash
ax upgrade
ax upgrade v3.0.0
ax upgrade --check
ax upgrade --local
ax upgrade --local dist/ax-win32-x64.zip
```

On Windows the process exits immediately; a detached helper finishes the binary swap. Open a new terminal and run `ax version` to verify.

The first regular `ax` command that a new version runs inside an initialized project rewrites the managed IDE instruction files there once, and records that version in `.ax/ide-seed-version`. The hidden hook commands, `ax version`, `ax uninit` and `ax uninstall` never do this.

---

## Telemetry

### `ax telemetry [on|off|status]`

Anonymous usage telemetry (command names and coarse buckets — never source code or paths).

| Argument | Description |
|---|---|
| `on` | Enable telemetry |
| `off` | Disable telemetry |
| `status` | Show current setting (default when omitted) |

```bash
ax telemetry status
ax telemetry on
ax telemetry off
```

`ax install` without `--yes` asks once whether to share usage data and stores the answer. When it runs without a terminal (a script, CI or an IDE task), it does not ask and stores nothing, so the question comes back on the next interactive install.

---

## Context savings

### `ax savings`

Estimated context-token savings from graph queries. Stored locally in `~/.ax/usage.db`. Each graph MCP call, and each CLI command with that MCP twin (`ax explore`, `ax query`, `ax node`, and the rest of the graph commands), logs response size and a counterfactual file-read estimate. Commands with no twin, such as `ax install`, are not logged. The combined total is what `ax savings` reports.

| Flag | Type | Default | Description |
|---|---|---|---|
| `--period` | string | `month_to_date` | `week` \| `month_to_date` \| `month` \| `year` \| `custom` |
| `--from` | `YYYY-MM-DD` | — | Start date (required for `custom`) |
| `--to` | `YYYY-MM-DD` | — | End date (optional for `custom`) |
| `--json` | flag | — | JSON summary with per-tool and daily breakdown |

```bash
ax savings
ax savings --period week --json
ax savings import --all
ax savings import --claude --cursor
ax savings tag-session --session-id <uuid> --model composer-2.5-fast
ax savings hook install
```

| Subcommand | Description |
|---|---|
| `import --all` | Import Cursor + Claude Code session transcripts |
| `tag-session` | Manually record model name for a session id |
| `hook install` | Install the Cursor sessionStart hook into `~/.cursor/hooks/` (executable) and merge it into `~/.cursor/hooks.json`; a `hooks.json` that is not valid JSON is left untouched with a message |

In ax-admin, the **Usage** page shows cost and savings; it needs the admin grant. See [Token savings](/guides/token-savings/).

### `ax pricing`

Daily model price sync from [OpenRouter](https://openrouter.ai/models) (no API key). Snapshots land in `~/.ax/usage.db` and drive the USD estimates of `ax savings`. The MCP server syncs once per calendar day; use this command to force or inspect.

| Subcommand | Description |
|---|---|
| `sync` | Fetch today's prices (`--force` re-fetches even if already synced) |
| `status` | Last sync, row counts |
| `list` | Latest OpenRouter rates (`--json`) |
| `history <model>` | Daily series for the one catalog model that best matches the id or name, over the last `--days` calendar days (`--json`) |

```bash
ax pricing sync
ax pricing sync --force
ax pricing status
ax pricing list
ax pricing list --json
ax pricing history claude-sonnet --days 30
```

Optional overrides in `~/.ax/pricing.toml` always win over synced rates.

### `ax costs`

Quote imported agent turns from the price catalog. Known events are estimates. Missing tokens or a missing model rate stay unknown and are left out of the dollar total. This does not replace `ax savings`.

| Subcommand | Description |
|---|---|
| (default) | Summary for the selected period (`--period`, `--from`, `--to`, `--json`) |
| `today` | Spend since local midnight |
| `month` | Month to date |
| `session` | Breakdown labeled by model within each reported group |
| `model` | Breakdown by model |
| `project` | Breakdown by project |

```bash
ax costs
ax costs today
ax costs month
ax costs model
```

### `ax agent economics`

Estimated tokens and catalog cost for a Pi session recorded by the Ax adapter. Tool output tokens are estimates of what was delivered into context. Tool execution itself is not priced as model tokens. Missing rates stay unknown. Without `--session` the totals cover every recorded session and the report says `(all sessions)`; `--session` limits every figure to that session. Each model is listed once.

```bash
ax agent economics
ax agent economics --session abc123
ax agent economics --json
```

### `ax agent optimize`

Estimated avoidable context from recorded tool calls. The report is advisory. `optimization.mode` defaults to `observe`. `optimize` approves only high-confidence deterministic rules and does not rewrite Pi's transcript.

```bash
ax agent optimize
ax agent optimize --json
```

### `ax budget`

Plan, simulate, and store a local monthly budget. Decisions are allow, warn, or deny. Ax does not block a provider API call.

| Subcommand | Description |
|---|---|
| `plan` | Monthly, daily, and hourly allowance (`--json`) |
| `simulate` | Scale recorded cycle cost (`--cycles`, `--days`, `--model`, `--strategy`, `--cost-per-cycle`, `--json`) |
| `set` | Write `~/.ax/config.json` (`--monthly`, `--currency`, `--usd-per-eur`, `--working-days`, `--hours`, `--warning`, `--critical`, `--hard`) |

```bash
ax budget plan
ax budget simulate --cycles 500
ax budget set --monthly 60 --currency eur --usd-per-eur 1.08 --working-days 22 --hours 8 --mode balanced --context-tokens 12000
```

### `ax mcp remote`

Run project-scoped MCP behind your existing HTTPS tunnel. The dedicated listener uses an available loopback port; it does not expose the Command Center administration API of the full ax. Native browser login uses Authorization Code + PKCE and the operating system credential store.

```bash
ax mcp remote register --config /absolute/path/remote-mcp.json --name "Team Ax"
ax mcp remote register --config /absolute/path/remote-mcp.json --native-client-id YOUR_NATIVE_PUBLIC_CLIENT_ID --cursor-client-id YOUR_CURSOR_PUBLIC_CLIENT_ID --vscode-client-id YOUR_VSCODE_PUBLIC_CLIENT_ID
ax mcp remote serve --config /absolute/path/remote-mcp.json --port 0
ax mcp remote serve --config /absolute/path/remote-mcp.json --port 0 --socket /absolute/path/ax-remote.sock
ax mcp remote login --url https://ax.example.com/projects/my-project/mcp --client-id YOUR_NATIVE_CLIENT_ID
ax mcp remote status
ax mcp remote proxy --url https://ax.example.com/projects/my-project/mcp
ax mcp remote logout --url https://ax.example.com/projects/my-project/mcp
```

`register` saves validated public project export metadata without starting the server or signing in. `serve` registers metadata automatically. The optional Unix socket supports macOS/Linux. `login --write` requests additional memory/policy permissions; the server must also grant them. Configure your OAuth issuer and client registrations before connecting. See [Remote MCP and browser login](/guides/remote-mcp/) for configuration, Cloudflare routing, ChatGPT registration and deployment limits.

### `ax mcp audit`

Correlate `<project>/.ax/mcp-verbose-*.log` (daily files) with a Cursor agent transcript and score MCP quality (preflight, enrichment, explore-before-grep, correlation). Persists a snapshot to `.ax/audit/latest.json`. Exit code `2` when critical findings are present.

CLI graph and policy commands (`ax explore`, `ax node`, `ax query`, `ax remember` / `ax recall`, `ax preflight`, `ax policy match|index|guard|skill`, …) append the same inbound/outbound shapes as MCP (branded `[ax]`) from **ax 8.4.4**, plus Internal domain lines. See [MCP Logging & Quality — Directions](/guides/mcp-quality/#directions-inbound-outbound-internal).

Scoring notes:

- Enrichment attaches `enrich` / `enrich done` lines (including `final_inject_chars`) to the open `ax_preflight` cluster so empty-inject false positives are avoided.
- Cursor transcripts often lack JSON timestamps; the auditor parses embedded `<timestamp>…</timestamp>` markers and carries them onto following tool calls so the rolling window matches verbose activity.
- Untimed transcripts (no embed/JSON ts) are capped and capacity-matched against the verbose window; when verbose is quiet in-window, the whole-session transcript tail is dropped so idle windows do not false-flag `UncorrelatedTool` / `VerboseGap`.
- Tool mix prefers the richer of verbose vs transcript counts for explore/graph/preflight/guard (`CallDynamicTool` included).
- Default session pick prefers the recent transcript with the most ax tool calls (avoids linking an empty newest chat).
- Verbose lines may include `session=<uuid>` (from the Cursor sessionStart hook / `AX_CURSOR_SESSION_ID`) for tighter transcript↔log correlation — run `ax savings hook install` once per machine.
- MCP error clusters that recover with a successful same-tool retry within 60s do not penalize the score.
- When verbose log files exist but have no clusters in the window, `UncorrelatedTool` is medium (widen `--window-minutes` or pass `--session`) instead of critical "enable verbose".

| Flag | Type | Default | Description |
|---|---|---|---|
| `--session` | uuid \| path | latest transcript | Cursor session id or `.jsonl` path |
| `--window-minutes` | number | `30` | Rolling window when no `--session` |
| `--json` | flag | — | Machine-readable `QualitySnapshot` |

```bash
ax mcp audit
ax mcp audit --window-minutes 60
ax mcp audit --session <uuid>
ax mcp audit --session path/to/transcript.jsonl --json
```

---

## Cursor auth switching

### `ax cursor auth`

Save and restore Cursor IDE auth sessions for fast subscription switching. Snapshots `cursorAuth/*` keys from `%APPDATA%\\Cursor\\User\\globalStorage\\state.vscdb` plus `auth.json` into `~/.ax/cursor-auth/`. The snapshots hold your Cursor access and refresh tokens, so on macOS and Linux that folder is readable only by you (0700) and each file in it is 0600.

| Subcommand | Description |
|---|---|
| `status` | Show live plan, email, login method |
| `list` | List saved profiles |
| `save <name>` | Snapshot current Cursor auth |
| `use <name>` | Apply a saved profile (restart Cursor after) |
| `show <name>` | Inspect a saved profile without applying |

| Flag | Applies to | Description |
|---|---|---|
| `--label` | `save` | Human-readable profile label |
| `--from-auth-json` | `save` | Bootstrap from stale `auth.json` only |
| `--email`, `--membership`, `--subscription-status`, `--sign-up-type` | `save` | Override metadata when bootstrapping |
| `--force` | `use` | Apply while Cursor is running (still restart after) |
| `--json` | all | JSON output |

Close Cursor before `use`, then restart Cursor after switching.

```bash
ax cursor auth status
ax cursor auth save enterprise --label "Work"
ax cursor auth save personal --from-auth-json --email you@gmail.com --membership pro_plus
ax cursor auth list
ax cursor auth use personal
ax cursor auth use enterprise
```

---

## Explore offload

### `ax offload`

Configure optional LLM offload for `ax explore` (BYO OpenAI-compatible API). Stored in `~/.ax/config.json`. Offload is off until `set-endpoint` saves an endpoint or `AX_OFFLOAD_URL` is set; an API key in the environment alone does not enable it, and `ax init` only prints the command when it finds a provider key.

#### `ax offload status`

Show current endpoint and key env var name. When offload is off and a provider key is in the environment, `suggestion` names the command that would enable it. `last_error` holds the last failed offload request made by the same process (status code and the start of the provider's reply); a separate `ax offload status` run starts with none.

```bash
ax offload status
```

#### `ax offload set-endpoint <url>`

| Argument / flag | Type | Description |
|---|---|---|
| `url` | required | Base URL (must end with `/v1`) |
| `--key-env` | string | Environment variable holding the API key |

```bash
ax offload set-endpoint https://api.openai.com/v1 --key-env OPENAI_API_KEY
```

#### `ax offload clear`

Remove offload configuration.

```bash
ax offload clear
```

---

## Neural

### `ax neural`

Neural Intelligence is required. Project `ax.json` cannot turn it off. Memory embeddings stay `hash-v1`. The default binary does not link an ML runtime. Context ranking, prediction, and graph suggestions run in process and do not call a language model. CLI JSON matches the `ax_neural_*` MCP tools, and the flags use the same fields. A value in the `neural` section of `ax.json` that is invalid keeps its default while every valid setting still applies; each invalid value and each unknown key (for example a misspelled section name) is listed in `configError`, which `explain` shows and a refused `feedback` or `predict` repeats. `--path` and the current directory resolve to the nearest project with `.ax/ax.db`, the same root the MCP server uses.

```bash
ax neural status
ax neural explain
ax neural benchmark
ax neural evaluate
ax neural feedback run-42 parser success --skill-id parse-errors --context-id memory:parser-lines
ax neural graph
ax neural export --out neural-feedback.json
```

- `status` shows measured numbers when the project database answers: feedback rows, the token delta from dropped duplicate blocks, rank latency per preflight, the last evaluated precision, and promoted models. A number that was not measured is reported as unavailable.
- `benchmark` scores the fused hash-v1 recall pipeline on the built-in coding fixture in a temporary database. It reports hits per question family and the measured query latency. It does not open `.ax/ax.db` and it does not download a model.
- `evaluate` holds out each coding-fixture question in turn, predicts its memories from the rest, and stores precision, false-positive rate, and coverage for `status`.
- `feedback` takes `--context-id`, `--skill-id`, and `--rule-id` (repeat each for more), plus `--retry-count`, `--input-tokens`, and `--output-tokens`. It is refused unless `neural.learning.feedback_collection` is true.
- `learn` trains a ranking prior from this project's stored feedback and never promotes it. It reads stored feedback only while `neural.learning.feedback_collection` is true, and reports `applied: false` with a reason when collection is off or there are no feedback rows. The MCP tool also takes `rows` to train on rows you pass instead.
- `graph` reads this project's edges and successful feedback and stores suggestions labeled `predicted`. It never writes edges. `--edges '<json array>'` previews suggestions for the edges you pass and stores nothing. It returns nothing while `neural.graph.enabled` is false.
- `rank <query> --blocks '<json array>'` ranks blocks the way preflight does.
- `register <model-id> <version> <checksum>` takes `--model-type`, `--dataset-version`, `--platforms`, and `--metrics '<json object>'`. Registering does not promote.
- `promote` is refused unless you pass `--explicit` and the version was registered with bake-off evidence in `--metrics`: a `bakeoff.families` object (`{family: {scored, hits}}`) that scores the same questions as the built-in coding fixture, hits every `named_trigger`, `abstention` and `delivery_budget` question, and beats hash-v1 on `paraphrase`. hash-v1 is scored on the fixture during the promote.
- `export --out <file>` writes feedback rows for an offline trainer. It leaves out the project path, execution ids, and secret-shaped rows. It has no MCP tool and sends nothing.

---

## Web UI

Full ax only. Not in the headless build. The full ax is being phased out; see [Headless ax and ax-admin](/guides/phase-out/).

For a visual interface next to the headless build, use [`ax-admin`](#ax-admin).

### `ax web [path]`

Full ax only. Not in the headless build. The full ax is being phased out; see [Headless ax and ax-admin](/guides/phase-out/).

Local web UI — graph browser, policy editor, token usage dashboard, Command Center tab.

| Argument / flag | Type | Default | Description |
|---|---|---|---|
| `path` | optional | cwd | Project root |
| `--port` | number | `7070` | Listen port |
| `--open` | flag | — | Open browser after start |

Set `AX_WEB_READONLY=1` for browse-only mode.

When the port is taken by an earlier `ax web`, that process is asked to stop (then force-stopped after 3 seconds) and the new server takes the port. When any other program holds the port, `ax web` leaves it running and exits with a message naming the program; pick another port with `--port`.

`ax web` stops on Ctrl+C, and on Unix also on SIGTERM (systemd, Docker, an IDE). Requests still open get 3 seconds to finish; live event streams from an open Command Center tab are then closed, so a tab left open never keeps the server running.

On start, `ax web` checks GitHub for a newer release (skips the 24-hour `~/.ax/update-check.json` cache) and prints the boxed notice to stderr **before** the server blocks. Disable with `AX_NO_UPDATE_CHECK=1`. Short commands still check after they exit.

```bash
ax web
ax web ./my-repo --port 8080 --open
```

### `ax desktop [path]`

Full ax only. Not in the headless build. The full ax is being phased out; see [Headless ax and ax-admin](/guides/phase-out/).

Native wgpu Command Center (egui/eframe). Embeds `ax-web` in-process — same `/api` as the browser UI, no browser required. See [Desktop Client](/guides/desktop-client/).

| Argument / flag | Type | Default | Description |
|---|---|---|---|
| `path` | optional | cwd | Project root (must contain `.ax/ax.db`) |
| `--port` | number | `7070` | Embedded `ax-web` listen port |
| `--bind` | string | `127.0.0.1` | Bind address |
| `--allow-anonymous-lan` | flag | off | Serve a non-loopback `--bind` without sign-in |

A bind address other than loopback needs Command Center sign-in (**Settings → Security**). Without it the server refuses to start, because anyone who can reach the address could read the project and run the agent. `--allow-anonymous-lan` (or `AX_WEB_ALLOW_ANONYMOUS_LAN=1`) starts it anyway and prints a warning. Even then, owner-only actions and the agent terminal work only from a browser on this computer at `http://127.0.0.1`.

```bash
ax desktop
ax desktop --port 17070
ax desktop ./my-repo --port 17070 --bind 127.0.0.1
```

### Desktop client binary (`ax-desktop`)

Full ax only. Not in the headless build. The full ax is being phased out; see [Headless ax and ax-admin](/guides/phase-out/).

Standalone binary from `crates/ax-desktop-client` (same UI as `ax desktop`):

```bash
cargo run -p ax-desktop-client -- .
./target-dev/release/ax-desktop . --port 17070
```

---

## Policy

Policy commands manage `.agents/` rules and skills (legacy `.ax/policy/{rules,skills}` still indexes). See [Policy Engine](/guides/policy-engine/).

Most policy subcommands accept an optional `[path]` project root (default: current directory).

### `ax policy index [path]`

Index `.ax/policy/` into SQLite. In **database** mode without `--force`, shows DB counts only.

| Flag | Description |
|---|---|
| `--force` | Re-import from disk (database mode) or full replace (files mode) |

```bash
ax policy index
ax policy index --force
```

### `ax policy import [path]`

Import `.mdc` / `SKILL.md` from disk into database (merge — keeps DB-only rows).

```bash
ax policy import
ax policy import ./my-project
```

### `ax policy dedup [path]`

Remove project rules and skills that `~/.ax/global.db` already holds. A longer project copy is first promoted into the global row as a new version. Every removed body is kept as a revision; files on disk are not touched. This also runs automatically after sync, policy index, `ax global sync`, `ax install`, and, in the full ax, in `ax web` every 10 minutes. Exits non-zero when the run stops on an error. `--dry-run` on a `global.db` from before this feature reports that the schema is older; any real run upgrades it.

```bash
ax policy dedup --dry-run
ax policy dedup
ax policy dedup --json
```

### `ax policy pull <git-url> [path]`

Clone a remote git policy registry into `.ax/policy/vendored/<name>/`, merge its `rules/*.mdc` and `skills/<name>/` into the project's agents folder (`.agents/` unless `policy.agentsDir` says otherwise), and re-index.

- The new clone replaces the vendored copy only after `git clone` succeeded; a failed clone keeps the previous copy.
- A local rule or skill with the same name that differs from the remote one is kept and reported. Pass `--force` to replace it. Identical items are left alone.
- Symlinks in the remote repository are never followed or copied, and a `rules/` or `skills/` folder that resolves outside the clone is refused.
- A remote tree larger than 5,000 files, 50 MB or 16 nested folders is refused before anything is written.
- A URL that starts with `-` is refused.

| Argument / flag | Type | Description |
|---|---|---|
| `git-url` | required | HTTPS or SSH git URL |
| `--name` | string | Vendor subdirectory name (default: repo name) |
| `--force` | bool | Replace local rules and skills that differ from the remote copy |

```bash
ax policy pull https://github.com/acme/ax-org-policy.git
ax policy pull git@github.com:acme/ax-org-policy.git --name org
```

### `ax policy export [path]`

Export database policy to files.

| Flag | Default | Description |
|---|---|---|
| `--out` | `.ax/policy/export` | Output directory |

```bash
ax policy export
ax policy export --out ./policy-backup
```

### `ax policy match <prompt> [path]`

Policy search. Test which indexed rules and skills match a prompt. This does not search the public skills catalog; that is [`ax skill search`](#ax-skill).

| Argument / flag | Type | Description |
|---|---|---|
| `prompt` | required | Prompt text to match |
| `--file` | string (repeat) | Open/changed file paths for glob matching |
| `--json` | flag | JSON output |

```bash
ax policy match "deploy to production" --file src/ship.rs
ax policy match "utf8 bom" --json
```

### `ax policy rules [path]`

List indexed rules.

| Flag | Description |
|---|---|
| `--json` | JSON output |

```bash
ax policy rules
ax policy rules --json
```

### `ax policy skills [path]`

List indexed skills.

| Flag | Description |
|---|---|
| `--json` | JSON output |

```bash
ax policy skills
ax policy skill startup
```

### `ax skill`

Skill search. `ax skill search <query>` queries the public skills catalog. Install a skill from GitHub, GitLab, Azure Repos, a well-known URL, or a local directory. Publish sends one named skill. Searching rules and skills already in the project is [`ax policy match`](#ax-policy-match-prompt-path).

```bash
ax skill search react
ax skill add vercel-labs/agent-skills --skill web-design-guidelines
ax skill add https://gitlab.com/group/repo
ax skill add https://dev.azure.com/org/project/_git/repo
ax skill add https://example.com/.well-known/agent-skills/pdf
ax skill list
ax skill publish pdf --remote https://github.com/you/skills.git
ax skill audit pdf
ax skill audit pdf --verdict verdict.json
ax skill audit pdf --source owner/repo
ax skill brief
ax skill brief pdf
ax policy quality english-only
ax skill report-installs --on
```

Installed skills are usable immediately. For a well-known URL, every file the index lists must be relative to the index or on the same origin (scheme, host and port); a file on another server is refused, and so is a redirect that leaves that origin. Each download stops at 25 MiB. `ax skill audit` stores a quality report for one skill and does not hide it. `ax skill brief` prints the edits for skills and rules. `ax policy quality <id>` audits one rule file and stores the same kind of report. Preflight includes both, and `ax_rules` returns rule briefs in `audits`. The agent edits the named file and leaves the skill or rule enabled. `--source` audits a catalog candidate without installing it. An agent submits a verdict JSON file with `verdict` (`pass`, `revise`, or `reject`), scores `clarity`, `trigger`, `steps`, `files`, and `safety` from 1 to 5, and findings whose `evidence` quotes the skill text. Reporting an install to skills.sh stays off until `ax skill report-installs --on`.

### `ax policy skill <name> [path]`

Print one skill body (markdown). `~/.ax/global.db` wins over the project `ax.db` on the same name.

| Argument | Description |
|---|---|
| `name` | Skill name |

```bash
ax policy skill release
ax policy skill azdo-pr-review
```

### `ax policy guard <file>`

Pre-write CRITICAL guard check. Built-in: UTF-8/BOM encoding, secrets paths. Generic: any CRITICAL rule can add a `guard: forbid-path: "<glob>"`, `guard: forbid-content: "<substring or /regex/>"`, `guard: require-content: "<substring or /regex/>"`, or `guard: require-skill: "<name>"` directive line to its body to opt into this check without code changes. Both content directives respect the rule's `globs`: `forbid-content` applies project-wide only when the rule declares no globs — no separate flag needed, directives are picked up automatically from indexed policy.

| Argument / flag | Type | Description |
|---|---|---|
| `file` | required | Path relative to project root |
| `-p`, `--path` | string | Project root (default: cwd) |
| `--delete` | flag | Run delete guard instead of write (default: write) |
| `--json` | flag | JSON output |

```bash
ax policy guard README.md
ax policy guard .env --json
ax policy guard old-file.txt --delete
```

### `ax policy test [path]`

Policy health checks that hold on any initialized project: policy indexed, the `startup` skill present, every alwaysApply rule matched and injected, a new UTF-8 file allowed by the guard, and the bootstrap files in place. Checks that need a particular rule or folder (a CRITICAL UTF-8 rule, a secrets rule, `.cursor/`) report `skipped` when it is absent. Exits with status 1 when any check fails.

| Flag | Description |
|---|---|
| `--json` | JSON output |

```bash
ax policy test
ax policy test --json
```

### `ax policy sync [path]`

Verify managed policy instruction files (`.agents/skills/startup/SKILL.md`, …) and IDE bootstrap files (`.cursor/rules/ax.mdc`, `.continue/rules/ax.md`, `.continue/mcpServers/ax.json`, `AGENTS.md`, etc.). Required managed files are reported when missing or drifted; customized content is not rewritten unless you pass `--force`.

| Flag | Description |
|---|---|
| `--fix` | Restore **missing** managed files from embedded init templates |
| `--force` | With `--fix`, also overwrite drifted/customized managed files (destructive) |

```bash
ax policy sync
ax policy sync --fix
ax policy sync --fix --force
ax policy history
ax policy restore-item <id>
```

After `--fix` restores `.ax/policy/` files in database mode, run `ax policy import` so `ax.db` picks up the updated skill bodies.
### `ax policy capture <prompt> [path]`

Propose or save a team rule from directive language (`always`, `you must`, `@rule`, …).

| Argument / flag | Type | Description |
|---|---|---|
| `prompt` | required | User prompt containing a directive |
| `--file` | string (repeat) | Open files (for glob inference) |
| `--yes` | flag | Save with defaults (skip interview) |
| `--json` | flag | JSON proposal or result |

```bash
ax policy capture "always update docs when adding a feature"
ax policy capture "never commit secrets" --file .env --json
```

### `ax policy storage`

Show or set policy storage mode (`files` vs `database`). Supports a **project default** plus **per-item overrides**, and lists configured `policy.roots` mounts.

#### `ax policy storage status [path]`

Show effective storage mode, config paths, project/global values, and `policy.roots`.

| Flag | Description |
|---|---|
| `--json` | JSON output |

```bash
ax policy storage status
ax policy storage status --json
```

#### `ax policy storage database [path]`

Set **database** as the project default (`ax.db`). Without `--yes`, prints a scan/plan only.

| Flag | Description |
|---|---|
| `--global` | Write to `~/.ax/config.json` instead of project `ax.json` |
| `--yes` | Import all candidates into `ax.db`, then switch the config, then **delete** `.agents/` / `.ax/policy/` markdown. When the import fails, the config and the files stay as they were |
| `--keep-files` | With `--yes`, import but leave markdown on disk |
| `--json` | JSON output |

```bash
ax policy storage database                # preview scan (no writes)
ax policy storage database --yes          # exclusive: DB is source of truth, files removed
ax policy storage database --yes --keep-files
```

#### `ax policy storage files [path]`

Set **files** as the project default source of truth (`.agents/` on disk).

| Flag | Description |
|---|---|
| `--global` | Write to `~/.ax/config.json` |
| `--yes` | Export every rule/skill from `ax.db` into `.agents/`, then switch the config. When the export fails, the config stays as it was |
| `--json` | JSON output |

```bash
ax policy storage files                   # preview
ax policy storage files --yes             # exclusive: files are source of truth
ax policy storage files --global
```

#### `ax policy storage set-item <id> <files|database> [path]`

Set a **per-item** storage override for one rule id or skill name (hybrid mode).

| Flag | Description |
|---|---|
| `--keep-file` | When switching to `database`, keep the markdown/stub file on disk |
| `--json` | JSON output |

```bash
ax policy storage set-item utf8-no-bom database
ax policy storage set-item startup files
ax policy storage set-item mobile-first database --keep-file --json
```

### `ax policy enable <id> [path]`

Enable a rule or skill so matcher/preflight include it again.

```bash
ax policy enable mobile-first
```

### `ax policy disable <id> [path]`

Disable a rule or skill without deleting it (frontmatter `enabled: false` + DB column).

```bash
ax policy disable mobile-first
```

### `ax policy agents-dir [name] [path]`

Show or set the folder that holds on-disk rules and skills. The default suggestion is `.agents`. The value is `policy.agentsDir` in project `ax.json`. When you set a new name and the old folder exists, ax renames it. `ax init` asks for this name when it is not saved yet.

```bash
ax policy agents-dir
ax policy agents-dir .agents
ax policy agents-dir team-policy
```

### `ax policy stack`

Opt-in language, framework, and CMS policy. Core rules and skills still come from `ax init`. Stacks install only their own files under `.agents/` and record the choice in `ax.json` (`policy.stacks`, `policy.stackDetect`) plus `.ax/stacks.lock.json`.

| Id | Depends on |
|---|---|
| `dotnet` | |
| `java` | |
| `react` | |
| `nextjs` | `react` |
| `angular` | |
| `vue` | |
| `php` | |
| `laravel` | `php` |
| `drupal` | `php` |
| `sitecore` | `dotnet` |
| `optimizely` | `dotnet` |
| `rust` | |
| `python` | |
| `go` | |
| `typescript` | |
| `javascript` | |
| `c` | |
| `cpp` | |
| `ruby` | |
| `swift` | |
| `kotlin` | |
| `dart` | |
| `svelte` | |
| `astro` | |
| `scala` | |
| `lua` | |
| `luau` | |
| `objc` | |
| `r` | |
| `pascal` | |

```bash
ax policy stack list
ax policy stack detect
ax policy stack apply dotnet react
ax policy stack status
ax policy stack upgrade
ax policy stack remove react
```

| Subcommand | Description |
|---|---|
| `list` | List the stack ids shipped with ax (`--json`) |
| `detect [path]` | Print the stacks detected in the project without writing anything (`--json`) |
| `apply [ids…] [path]` | Install stacks into project policy and record them in `ax.json` (`--force`, `--yes`, `--json`) |
| `remove <id> [path]` | Remove one stack's managed files when they still match the lock (`--json`) |
| `status [path]` | Compare installed stacks with the embedded catalog (`--json`) |
| `upgrade [path]` | Refresh files that still match the lock from the current catalog (`--json`) |

`apply` with no ids uses `policy.stacks`. When that list is empty it prints a detection proposal and, on a terminal, asks before writing. Pass `--yes` to apply the proposal without a prompt. `--force` overwrites files you edited. Files you changed are otherwise left in place.

`ax init` asks for stacks every time stdin is a terminal, including when the project was already initialized. Enter keeps the current set (or the detection on a first choice). `none` installs core policy only.

### `ax policy pack`

Per-project shared pack under `.ax/policy/shared/` for git team sync. Default export includes all packable (project + workspace) enabled items; company/private scopes and tags `local` / `noshare` are skipped.

#### `ax policy pack export [path]`

| Flag | Default | Description |
|---|---|---|
| `--tag` | `shared` | Default `shared` = all packable scopes; any other value filters by that frontmatter tag |
| `--out` | `.ax/policy/shared` | Pack directory |
| `--quiet` | | Suppress summary |

```bash
ax policy pack export
ax policy pack export --tag team
```

#### `ax policy pack import [path]`

Imports the shared pack into the local policy store, then refreshes **all** IDE bootstraps (Cursor, Continue, Claude, …) and refreshes the MCP config of IDEs that already have ax (limited to `agents.ides` in `ax.json` when saved). It never connects an IDE you left off.

| Flag | Description |
|---|---|
| `--pack` | Pack directory (default `.ax/policy/shared`) |
| `--force` | Overwrite conflicting local items without staging pending |
| `--quiet` | Suppress summary |

```bash
ax policy pack import
ax policy pack import --force
```

#### `ax policy pack status [path]`

```bash
ax policy pack status --json
```

#### `ax policy pack install [name] [path]`

Install a built-in pack into **project** scope (`.ax/policy/`), then import/re-index so MCP sees the new items. In **database** storage mode the install force-imports files into `ax.db` (not counts-only). Company/private scopes are not modified.

| Flag | Description |
|---|---|
| `--list` | List available built-in packs (no install) |
| `--force` | Overwrite existing rules/skills with the same id/name (use after upgrading to refresh expanded skill bodies) |
| `--json` | JSON output |

Built-in pack `azdo-fullstack` ships full Azure DevOps ticket-to-release **skills** (workflows + checklists) and matching **rules**. `azdo-code-review` requires Story scope, sibling-pattern checks, both bounds, and a named test per finding.

```bash
ax policy pack install --list
ax policy pack install azdo-fullstack
ax policy pack install azdo-fullstack --force
```

#### `ax policy pack zip [path]`

Build a portable `.ax-policy.zip` of selected **shareable** rules and skills (project/workspace, enabled). Private and disabled items are rejected.

```bash
ax policy pack zip --out team.ax-policy.zip --name "Team pack" --rules utf8-no-bom,english-only --skills startup
```

### `ax policy restore`

Preview or install a portable zip into `.agents/`. New items default to install. Conflicts default to skip (including when the local file is newer). Pass `--decisions` to skip a new item, overwrite a conflict, or merge selected hunks.

```bash
ax policy restore --preview team.ax-policy.zip
ax policy restore team.ax-policy.zip --decisions decisions.json
```

`decisions.json` maps `"rule:<id>"` / `"skill:<name>"` to `overwrite`, `skip`, `{ "action": "merge", "acceptHunks": [0] }`, or `{ "action": "merge", "hunks": [{ "index": 0, "take": "both" }] }` (`take` is `local`, `package`, `both`, or `none`). Preview JSON includes `newer` and each packed path may include `contentHash` (blake3). Missing decision keys: **new** → install (`overwrite`), **conflict** → skip. Invalid hunk indexes fail the restore. Accepted writes are recorded in the local hash-on-change revision log.

### `ax policy review`

Review pending pack imports when `policy.requireReview` is true.

```bash
ax policy review list
ax policy review show mobile-first
ax policy review approve mobile-first
ax policy review reject mobile-first
```

| Subcommand | Description |
|---|---|
| `list [path]` | List pending rules and skills (`--json`) |
| `show <id> [path]` | Show a pending item and its diff against the local copy (`--json`) |
| `approve <id> [path]` | Move a pending item into active policy |
| `reject <id> [path]` | Drop a pending item |

### `ax policy share`

Remote policy share sync from any git host (GitHub, GitLab, Azure DevOps, on-prem) or OneDrive / SharePoint. See [Remote Policy Share](/guides/policy-sharing/).

Config is stored in **`<project>/ax.json`** under the `"share"` key (per project only). `ax policy share config` shows it.

#### `ax policy share config [path]`

Show this project's share configuration (provider, import mode, URLs, `ax.json` path, last sync).

| Flag | Description |
|---|---|
| `--json` | JSON output |

```bash
ax policy share config
ax policy share config --json
ax policy share config ./services/api
```

#### `ax policy share sync [path]`

Pull (and optionally push) rules, skills, and shared memory from the configured remote.

| Flag | Description |
|---|---|
| `--pull` | Pull from remote (default when neither flag is set) |
| `--push` | Push local pack to remote (OneDrive, git, or GitLab `/api/v4` — see [Remote Policy Share](/guides/policy-sharing/)) |
| `--json` | JSON sync status |

```bash
ax policy share sync
ax policy share sync --pull
ax policy share sync --push
ax policy share sync --pull --push
ax policy share sync --json
```

---

## Authentication

### `ax auth microsoft`

Microsoft device-code sign-in for OneDrive / SharePoint policy share. Works out of
the box using a built-in Microsoft public client — no Azure app registration
needed. Set `AX_MS_CLIENT_ID` only to override with your own Azure AD app (e.g.
tenants that block first-party app consent). Tokens stored in `~/.ax/auth/microsoft.json`.

#### `ax auth microsoft login`

Start interactive device code flow — open the verification URL and enter the code shown in the terminal.

```bash
ax auth microsoft login

# optional: use your own Azure AD app instead of the built-in default
export AX_MS_CLIENT_ID="xxxxxxxx-xxxx-xxxx-xxxx-xxxxxxxxxxxx"
ax auth microsoft login
```

#### `ax auth microsoft logout`

Clear stored Microsoft tokens.

```bash
ax auth microsoft logout
```

#### `ax auth microsoft status`

Show whether you are signed in and which account is active.

| Flag | Description |
|---|---|
| `--json` | JSON output |

```bash
ax auth microsoft status
ax auth microsoft status --json
```

---

## Hidden / internal commands

Not for daily use — invoked by agents, installers, or upgrade helpers.

| Command | Purpose |
|---|---|
| `ax serve --mcp` | Stdio MCP server (agent-launched) |
| `ax serve --mcp --daemon` | Background MCP daemon |
| `ax prompt-hook` | Claude `UserPromptSubmit` hook (stdin JSON) |
| `ax read-guard --ide <ide>` | Pre-tool hook that redirects whole-file reads and symbol searches to the graph (stdin JSON) |
| `ax turn-hook start\|response\|end` | Cursor `beforeSubmitPrompt` / `afterAgentResponse` / `stop` and Claude `UserPromptSubmit` hook that writes [per-turn memories](/guides/memory/#per-turn-memories) (stdin JSON, prints nothing) |
| `ax watchdog-child` | MCP liveness watchdog child |
| `ax upgrade-apply` | Windows upgrade swap helper |

---

## Specifications and plans

`ax spec <id>` prints the stored body (`id` or `id@version`). Specifications and plans can also be stored as separate rows. That hierarchy stays off until you turn it on, and turning it off restores the flat list and the previous capture path. Rows already written stay. Nothing is dropped.

When a turn finishes, each changed `docs/specs/*.md` or `docs/plans/*.md` file is stored as a draft. `docs/specs/<id>/SPEC.md` belongs to spec `<id>`. The Markdown copy of an existing spec, such as the `SPEC.md` written on approval, is compared with the approved body instead of being stored as another spec. A body equal to the latest revision adds no draft, and a changed file binds the session only when it has no spec yet. Trailing newlines are ignored when bodies and the Markdown copy are compared.

Preflight also scans the IDE plan folders. A session named in a plan file, and the live session for the newest plan in `~/.cursor/plans`, are bound to that plan only when the session has no spec yet or was bound by an earlier scan. A spec bound by a capture or an edit in the session stays bound.

A plan file's spec id is the slug of its file name. When another plan file already has that id, such as `plan.md` in a second project, the new file's id gets the first 8 hex digits of its path's SHA-256 (`plan-1a2b3c4d`). A file keeps its id when it changes later. Spec ids from a plan request or another file path are a slug of at most 48 characters; a longer one is cut and gets the 8-digit hash of the full text, so two requests that start the same stay two specs. A scan reads at most 400 plan files.

```bash
ax spec billing-spec
ax spec-flag
ax spec-flag off
```

`ax spec-flag` with no value prints the flag. `on` and `off` set it. A JSON object such as `{"enabled":false}` is also accepted.

While the flag is on, finished plan capture does not invent a parent spec. Those ids are listed by:

```bash
ax spec-review
```

Create a spec and its first plan in one request. The argument is the JSON body of `POST /api/specs/with-plan`. An empty plan is rejected. Repeating the same spec, plan, session, and turn returns the first pair.

```bash
ax spec-create '{"projectId":"ax","specId":"billing","specTitle":"Billing","specBody":"Charge the recorded amount.","planId":"ledger","planTitle":"Ledger","planBody":"Record the ledger.","sessionId":"session-1","turnId":"turn-1"}'
```

The same JSON contract is used by the HTTP routes and the MCP tools.

```bash
ax spec-plan '{"projectId":"ax","specId":"billing","planId":"ledger-alt","planTitle":"Other ledger","planBody":"Record it another way.","sessionId":"session-1","turnId":"turn-2"}'
ax spec-revise '{"planId":"ledger","basedOn":1,"body":"Record the ledger, then check the total.","sessionId":"session-1","turnId":"turn-3"}'
ax spec-amend '{"specId":"billing","basedOn":1,"body":"Charge the recorded amount, then reconcile.","sessionId":"session-1","turnId":"turn-2"}'
ax spec-link '{"sessionId":"session-1","specId":"billing"}'
ax spec-select '{"workstreamId":"ws-1","planId":"ledger","planVersion":1,"sessionId":"session-1","turnId":"turn-1"}'
ax spec-compare '{"planId":"ledger","left":1,"right":2}'
ax spec-tree '{"projectId":"ax"}'
ax spec-provenance '{"planId":"ledger","version":1}'
ax spec-policy '{"artifactKind":"spec","artifactId":"billing","policyRevisionId":1}'
ax spec-compatibility '{"selectionId":"ws-billing:ledger:1:session-1:turn-1"}'
```

`ax spec-import` stores a specification that has no plan yet, as a draft that needs a plan.

`ax spec-classify` records an owner decision that documents are specifications or plans. Pass either `ids` or `allPlanMode` (every document with a plan-mode revision). Each call adds rows, and earlier classifications stay as history. Specs & Plans lists a document by its newest classification and keeps the detected kind as `detectedKind`.

Both commands have HTTP routes. There is no MCP tool for them yet.

```bash
ax spec-import '{"projectId":"ax","specId":"billing-rules","specTitle":"Billing rules","specBody":"Round to cents.","sessionId":"session-1","turnId":"turn-1"}'
ax spec-classify '{"allPlanMode":true,"docType":"spec","actor":"owner","reason":"These plan-mode documents are specifications."}'
```

Approving a new spec body marks selections of plans written against the previous spec hash as `compatibility_review_required`. Selecting a plan revision later gets the same mark when that revision was written against another spec body than the approved one. An edited plan revision keeps the spec revision its parent was written against. Other specs are left alone, and the plan text is not rewritten. `ax spec-compatibility` records that the selection was checked. Revising a spec id that does not exist returns `missing spec`.

`ax spec-policy` records which policy revision applies. A `body` field is rejected. The stored link has the revision id and content hash, not the rule text. Sending the same link again keeps the one row.

A stale `basedOn` on `ax spec-revise` or `ax spec-amend` comes back as an error containing `conflict`, with `base` (the revision you named), `head` (the current revision), and `incoming` (the body that was not stored). Nothing is merged. An empty spec body is rejected. Approval reads the member's role. A `roles` field in the JSON is ignored. The command binds the active owner and forces the actor to be human, so it only runs in an interactive terminal: it shows what will be approved and in whose name, and records the approval only after you type `approve`. Run without a terminal (an agent's shell tool, a script, a pipe), it is refused before anything is read. The full ax, which is being phased out, can also approve in its Command Center; see [Command Center](/guides/command-center/).

```bash
ax spec-approve '{"projectId":"ax","artifactKind":"plan","artifactId":"ledger","version":1,"reason":"Ready to build."}'
```

The Command Center of the full ax serves the same operations over HTTP. The headless build has no such routes; use the CLI commands.

| Route (full ax only) | CLI |
|---|---|
| `POST /api/specs/with-plan` | `ax spec-create` |
| `POST /api/specs/alternative-plan` | `ax spec-plan` |
| `POST /api/specs/import` | `ax spec-import` |
| `POST /api/specs/classify` | `ax spec-classify` |
| `POST /api/specs/plan-revision` | `ax spec-revise` |
| `POST /api/specs/spec-revision` | `ax spec-amend` |
| `POST /api/specs/session-link` | `ax spec-link` |
| `POST /api/specs/plan-selection` | `ax spec-select` |
| `POST /api/specs/plan-compare` | `ax spec-compare` |
| `POST /api/specs/tree` | `ax spec-tree` |
| `POST /api/specs/provenance` | `ax spec-provenance` |
| `POST /api/specs/member-approval` | `ax spec-approve` |
| `POST /api/specs/flag` | `ax spec-flag` |
| `GET /api/specs/review` | `ax spec-review` |
| `POST /api/specs/policy-link` | `ax spec-policy` |
| `POST /api/specs/compatibility` | `ax spec-compatibility` |

`GET /api/specs` stays the flat list while the flag is off. While the flag is on it returns the hierarchy and does not import legacy plan files. Failures use `{ "ok": false, "error": "..." }`.

## Quick reference

```bash
# Lifecycle
ax init [path]
ax sync --watch
ax status --json

# Graph
ax explore "auth flow" --json
ax query UserService --kind class
ax callers handleRequest
ax path handleRequest saveUser
ax cycles

# Architecture insights
ax insights --json
ax validate --ci
ax report --out AX_REPORT.md
ax export graph-html --out graph.html

# Memory
ax remember "we picked X because Y" --kind decision
ax recall "why X"
ax capture-git

# Git / ship
ax diff --base main --json
ax test-impact --base main
ax ship --evaluate

# Specifications
ax spec billing-spec
ax spec-flag
ax spec-review

# Policy
ax policy storage status
ax policy match "deploy" --file src/app.ts
ax policy capture "always validate input"
ax policy share sync
ax auth microsoft status

# Upgrade (full ax only)
ax upgrade
ax upgrade --local
```

## `ax-admin`

`ax-admin` is a separate program with its own installer. It manages a remote Ax through that environment's remote MCP address and serves its interface on this computer only. See the [ax-admin guide](/guides/remote-admin/).

```bash
ax-admin
```

```bash
ax-admin --port 0 --no-open
```

```bash
ax-admin connections
```

```bash
ax-admin version
```

| Option or command | Meaning |
|---|---|
| `--port N` | Loopback port for the interface (default 7080; `0` picks a free port and prints it) |
| `--no-open` | Do not open the browser |
| `connections` | List saved connections: name, address and kind of credential. Never keys |
| `version` | Print the version |

`AX_ADMIN_HOME` selects the folder that holds `connections.json` (default `~/.ax-admin`).

## `ax server sync`, `apply` and `run`

Full ax only. Not in the headless build. The full ax is being phased out; see [Headless ax and ax-admin](/guides/phase-out/).

An enrolled Replica contributes native and locally edited knowledge to the configured Primary and receives the combined cluster snapshot. Empty peers and omitted records cannot delete existing local data. Conflicting origins use separate IDs; prior accepted revisions remain retained. Both nodes must support protocol 3. Full graph, indexed source, logs and project-scoped conversation history travel in verified chunks alongside the knowledge exchange.

```bash
ax server sync --config ./ax-replica.json
ax server status --config ./ax-replica.json
# Also make received memories available in this node's vault.
ax server apply --config ./ax-replica.json
# Continue applying memory copies; rules/skills require review.
ax server run --config ./ax-replica.json --auto-apply-memories
```

`sync` stages received state; `apply` and `run --auto-apply-memories` project the full graph, source, logs, conversation history and memory under separate node identities. Settings pairing enables complete application automatically. Locally edited imports stay intact and are contributed as variants; received policy is not activated automatically. See [Standalone Ax](/guides/standalone-ax/#empty-primary-existing-replica-and-conflicts) for enrollment, limits and retained source history.
