---
title: MCP Server
description: The tools ax exposes to AI agents over MCP.
---

ax runs as a [Model Context Protocol](https://modelcontextprotocol.io/) server. Agents configured by the installer launch it automatically — you don't start it by hand:

```bash
ax serve --mcp
```

When a `.ax/` index exists, the agent gets the tools below. In a workspace with **no** index, the server announces itself inactive and lists **no** graph tools — the agent works normally with its built-in tools, and indexing stays your decision.

When `.ax/policy/` is indexed (**ax v2.0.0+**), policy tools are listed automatically. See [Policy Engine](/guides/policy-engine/).

## Default catalog: the graph read surface

By default the server lists the **turn contract** plus the **whole graph read surface**:

| Group | Tools |
|---|---|
| Turn contract | `ax_preflight`, `ax_policy_capture`, and (when policy exists) `ax_rules` / `ax_skill` / `ax_guard` |
| Graph reads | `ax_explore`, `ax_search`, `ax_node`, `ax_callers`, `ax_callees`, `ax_impact`, `ax_path`, `ax_cycles`, `ax_api`, `ax_context`, `ax_affected`, `ax_insights`, `ax_report`, `ax_status`, `ax_sync`, `ax_remember`, `ax_recall` |

`ax_explore` remains the one call that usually answers a whole question: give it a natural-language question or a bag of symbol and file names and it returns the **verbatim, line-numbered source** of the relevant symbols grouped by file, plus call paths and a blast-radius summary. Reach for the narrower tools when you already know exactly what you want.

**Why the default is no longer minimal.** Earlier versions listed six tools and left the rest callable-but-unlisted. That created a contradiction: ax's own CRITICAL rules tell agents to call `ax_search` before `Grep` and `ax_sync` after edits, while `tools/list` said those tools did not exist. Agents that trusted the catalog fell back to filesystem sweeps — the behaviour the rules exist to prevent. The full audit is in `docs/audits/2026-08-19-preflight-graph-only/`.

The cost is a larger `tools/list` payload on every turn, which we accepted deliberately. A coherence test (`crates/ax-mcp/src/tool_filter.rs`) now fails the build if a rule names a tool that is neither advertised nor explicitly gated, so the catalog and the policy cannot drift apart again.

`ax_files` stays unlisted on purpose: graph queries supersede it. It remains callable.

## Opt-in tools

These mutate the index, spawn language servers, or run the quality gate, so they stay out of the default catalog. They remain fully functional via `tools/call` — the allowlist controls discovery, not access.

Enable with the `AX_MCP_TOOLS` environment variable: a comma-separated allowlist of short names (or full `ax_*` names), or `all` for everything.

```bash
AX_MCP_TOOLS=ship,lsp
# or
AX_MCP_TOOLS=all
```

:::caution[Known gap]
The `prefer-mcp-ops` rule tells agents to use `ax_index`, `ax_lsp`, `ax_ship`, and `ax_policy_index` instead of shelling out to the CLI, but an agent reading only `tools/list` will not see them. If your agents rely on that rule, set `AX_MCP_TOOLS=all`. This is tracked as a deliberate trade-off, not an oversight — `GATED_BY_DESIGN` in `tool_filter.rs` records it.
:::

Each read tool also has a CLI equivalent (`ax node` / `query` / `callers` / `callees` / `impact` / `files` / `status`) for scripts and non-MCP harnesses.

## Operational tools (v4+)

Opt-in per the section above (e.g. `AX_MCP_TOOLS=all` or `lsp,ship`). Mirror high-value CLI session ops — agents never need to shell out for these:

| Tool | Purpose |
|---|---|
| `ax_index` | Re-index; optional `force: true` clears and full-indexes (default behaves like `ax_sync`) |
| `ax_lsp` | `action: "status"` lists language servers; `action: "enrich"` resolves Exact edges (`limit` optional) — same as `ax lsp status\|enrich` |
| `ax_ship` | Quality-gate pipeline: `mode: "evaluate"` (default) or `"ci"`. Returns the ship report JSON. For `ci`, MCP `isError` is set when the gate fails — **never** exits the MCP process |
| `ax_policy_index` | Re-index / import rules and skills from `.ax/policy/` (`force` optional) |
| `ax_diagnostics` | Correlate editor/linter diagnostics with the graph |

`ax_sync` is **not** in this list — it is advertised by default, because `prefer-mcp-ops` requires agents to call it after edits rather than running `ax sync` in a shell.

```json
ax_sync({})
ax_index({ "force": true })
ax_lsp({ "action": "enrich", "limit": 200 })
ax_ship({ "mode": "ci" })
ax_policy_index({ "force": true })
```

## Policy tools (v2.0.0+)

When `.ax/policy/` contains indexed rules or skills, the server also exposes:

| Tool | Purpose |
|---|---|
| `ax_preflight` | Turn-start: matched rules + skills + `inject` (always-apply rules and always-apply skills always complete; contextual rules/skills may be omitted with an `ax_skill` / `ax_rules` hint) + auto-injected `<ax_index>` snapshot. Policy match failures return a degraded payload, never MCP `isError`. |
| `ax_rules` | List all rules or match against a prompt |
| `ax_skill` | Load the full markdown body of a skill by name |
| `ax_guard` | Block or warn before writes that violate CRITICAL rules. Built-in checks: UTF-8 BOM/encoding, secrets paths. **Generic gate:** any CRITICAL rule can opt in without code changes by adding a `guard: forbid-path: "<glob>"`, `guard: forbid-content: "<substring or /regex/>"` (scoped by that rule's `globs` when it has any), `guard: require-content: "<substring or /regex/>"` (scoped by that rule's `globs`), or `guard: require-skill: "<name>"` (skill must be approved and `alwaysApply`) line to its body. |

Agents should **not** read `.ax/policy/` files when these tools are available — policy is indexed locally and returned in MCP responses.

**Cursor:** call `ax_preflight` at turn start (MCP pull only — no prompt-hook).

**Claude Code:** prompt-hook may auto-inject `<ax_policy>…</ax_policy>` before the model sees the prompt, in addition to MCP tools. A `Stop`/`SubagentStop` hook (`ax stop-hook`) also runs a lightweight post-flight check on turn end — see [Turn-end post-flight (Claude Code)](#turn-end-post-flight-claude-code) below.

Call `ax_guard` with the target file path before editing project files. See [Policy Engine](/guides/policy-engine/) for flow diagrams and delivery channels.

**Not policy:** `ax_context` builds code-graph task context. Use `ax_preflight` for rules and skills.

### Turn-end post-flight (Claude Code)

`ax install` wires two more Claude Code hooks alongside the existing `UserPromptSubmit` prompt-hook: `Stop` and `SubagentStop`, both running `ax stop-hook`. On turn end it scans the working-tree files changed since the last commit (`git status --porcelain`) through the same `ax_guard` checks described above, and — only on a CRITICAL violation — returns `{"decision": "block", "reason": "…"}` so Claude fixes the issue before actually finishing. It always honors `stop_hook_active` to avoid looping, and no-ops entirely when there's no `.ax/policy/` or the project isn't indexed. Disable with `AX_NO_STOP_HOOK=1`.

### Diagnostics bridge: `ax_diagnostics`

ax has no way to read editor/LSP state itself. `ax_diagnostics` closes that gap from the agent side: pass in diagnostics you already gathered (Cursor's Problems panel / `ReadLints`, `tsc`, `ruff`, `eslint`, …) and get back graph-correlated context — which files intersect CRITICAL-guarded paths, and which tests `ax_affected` says are impacted by those same files. Always listed, regardless of whether policy is indexed (the guard/test correlation just degrades to empty).

```json
ax_diagnostics({ "diagnostics": [{ "path": "src/lib.rs", "line": 42, "severity": "error", "message": "…", "source": "rustc" }] })
```

## Memory tools

The [memory vault](/guides/memory/) adds two tools:

| Tool | Purpose |
|---|---|
| `ax_remember` | Store a durable project memory (decision, fix, convention). Returns similar existing memories so contradictions get updated instead of duplicated. |
| `ax_recall` | Hybrid search (full-text + vector similarity) over stored memories |

`ax_preflight` also recalls memories relevant to the prompt and injects the top matches automatically, so agents rarely need to call `ax_recall` by hand. It also injects an `<ax_index>` block with node/edge counts and **document inventory by extension** (markdown, office, PDF, other opaque types) on every turn — agents do not need a separate `ax_status` call to see what docs are indexed.

Git hooks run `ax capture-git --quiet` on every commit — commit messages with real context become `kind: git` memories without agent action. Agents should still call `ax_remember` for durable decisions that commit messages do not capture.

Responses larger than ~3k tokens carry a one-line `[ax] token budget` hint suggesting a narrower query or lower depth, nudging agents to keep context small.

## Lean responses (token savings)

Every `tools/call` reply is `{ content: [{ type: "text", text }], structuredContent?, isError }`. The `content.text` block is what strict clients and Cursor feed the model; `structuredContent` is machine-readable metadata for clients that consume it.

By default ax runs **lean**: it never ships the same data twice. The authoritative payload lives in `content.text`, and `structuredContent` is projected down to just the fields not already present in the text:

| Tool | `content.text` | Lean `structuredContent` |
|---|---|---|
| `ax_explore` | Numbered source + caller/callee spine | `query`, `summary`, `blastRadius`, compact `entries` (name/file/lines/score) — no source or neighbor duplication |
| `ax_preflight` | `inject` block (full rule/skill/memory/index bodies) | counts + `directiveDetected`, `captureProposal`, `guardRequired`, `mode`, `instruction`, `indexStats`, `pendingFiles` — no body duplication |
| `ax_status` | Markdown status summary + doc breakdown | `stats`, `lastIndexedAt`, `pendingFiles`, `policy` — no `text` duplication |
| `ax_context` | Markdown task context | `query`, `summary`, `stats`, `relatedFiles` — no `subgraph`/`codeBlocks` duplication |
| `ax_skill` | Skill body | metadata envelope (no `body`) |
| `ax_search` / `ax_node` / `ax_callers` / `ax_callees` / `ax_impact` / `ax_files` / `ax_affected` | Compact one-line-per-symbol list | omitted (text is authoritative) |

Savings measurement (`ax savings`) always runs against the full pre-projection payload, so the leaner wire format never distorts the numbers.

Set `AX_MCP_FULL=1` to restore the full `structuredContent` for every tool (for clients that read only structured data).

### Verbose MCP logging (Cursor Output)

To watch what each tool receives, how preflight enrichment builds the inject block, and what ax sends back on the wire — without changing agent-facing payloads — follow the full guide: **[MCP Logging & Quality](/guides/mcp-quality/)**. Short version:

1. Enable **Logging → Verbose MCP logging** in Command Center (writes `[ui] verbose_mcp = true` to `.ax/ship.toml` for the **active** project), **or** set `AX_MCP_VERBOSE=1` in the MCP server environment.
2. Reconnect / restart the ax MCP server in Cursor.
3. In Command Center (`ax web`), watch the status bar **Logging** chip (shows the latest tool / activity). Click it to open the **Logging** page — a table of today's `<project>/.ax/mcp-verbose-YYYY-MM-DD.log` for the active workspace (**newest at top**; scroll down for earlier days; **Scroll to new** returns to the live top; logs are not cleared from the UI).

![MCP Logging — live verbose stream with kind filters and Call Inspector](/screenshots/cc-logging.png)

4. Open the status-bar **Q** (Quality) chip for the metrics slide-out (correlation, enrichment, findings, **Copy fixpack**). CLI: `ax mcp audit`. Install `ax savings hook install` for `session=` tags.

![MCP Quality — correlation, enrichment, tool mix, findings, and Copy fixpack](/screenshots/cc-mcp-quality.png)

5. Optionally also use **View → Output → ax / MCP Logs**, or open the log file directly.

Lines are prefixed with `[ax-mcp]`. They are written to the log file (and optionally Cursor Output). They never append debug text to `content.text` or `structuredContent`. When a Cursor session id is known, lines also include `session=<uuid>` so `ax mcp audit --session <uuid>` can filter the verbose log.

`ax_guard` accepts the documented `path` + `operation` shape and common aliases (`paths[]`, `file`, `action=edit|write|delete`) so agents do not burn retries on `path required` validation errors.

### Response budget environment variables

| Variable | Default | Purpose |
|---|---|---|
| `AX_MCP_FULL` | unset | `1`/`true`/`yes` restores full `structuredContent` on every tool |
| `AX_MCP_VERBOSE` | unset | `1`/`true`/`yes` emits inbound/enrichment/outbound traces to stderr (Cursor Output); same as Logging → Verbose MCP logging |
| `AX_EXPLORE_MAX_LINES` | 40 | Max source lines per `ax_explore` snippet |
| `AX_EXPLORE_MAX_SOURCE_CHARS` | 2000 | Max source characters per `ax_explore` snippet |
| `AX_CONTEXT_MAX_BLOCKS` | 6 | Max code blocks in an `ax_context` response |
| `AX_CONTEXT_MAX_BLOCK_CHARS` | 1200 | Max characters per `ax_context` code block |

Explicit tool params (`maxLinesPerSnippet`, `maxSourceChars`) still override the env defaults per call.

## Architecture tools

For whole-graph understanding — subsystems, hot spots, and unexpected coupling — two tools sit on top of the graph analysis engine. See [Architecture Insights](/guides/architecture-insights/).

| Tool | Purpose |
|---|---|
| `ax_insights` | Leiden **communities** (subsystems), **god nodes** (most-connected concepts), **surprising connections**, and **suggestedQuestions** (same templates as `ax report`). Params: `resolution`, `godLimit`, `surprisingLimit`. |
| `ax_report` | Full Markdown architecture report (god nodes, communities, surprising links, dead code, unresolved refs, suggested questions). Returns `{ markdown }`. Param: `resolution`. |

Every edge also carries a **confidence** tag — `extracted` (read straight from the AST), `inferred` (resolved by a heuristic/name-matching pass), or `ambiguous` (one of several candidate targets was picked) — surfaced in `ax_node` / `ax_callers` / `ax_callees` output and the Command Center edge badges. Markdown docs (`.md`/`.mdx`) are indexed as `Doc` nodes with full parsing; PDF, Office, and other opaque formats appear as `Doc` nodes (presence only, no content extraction). Counts by extension are auto-injected via `ax_preflight` and available in `ax status --json` as `stats.docsByExtension`.

## Source store: snippets come from the graph

Every snippet an agent sees — `ax_explore` source blocks, `ax_context` code blocks — is served from the **source store** in `ax.db` (`file_contents`, schema v17), never read from your working tree at query time.

This is what makes a graph query answerable *from the graph*: its cost and its answer do not depend on a filesystem sweep, and freshness becomes checkable instead of assumed.

**Freshness is verified, not assumed.** Stored text carries the content hash it had at index time. Every read compares that against `files.content_hash`:

| Outcome | What you get |
|---|---|
| Hashes match | The snippet, verified current |
| Hashes differ | One bounded, scoped re-index of just those files, then a retry. If the index is busy, you get the stored text prefixed with `(source stale: … run ax_sync to refresh)` |
| No stored text | `(source not stored: … run ax index to backfill the source store)` |
| Over the size cap | `(source not stored: … exceeds the N byte source-store cap)` |

There is deliberately **no disk fallback**. A fallback would hide the one failure mode you need to see, and would make the guarantee unverifiable. A stale or missing snippet is always labelled as such.

**Size cap.** Files above 1 MB store no text; override with `AX_SOURCE_STORE_MAX_BYTES` (clamped to 64 KiB–64 MiB). Oversized files get the explicit over-cap marker.

**What gets stored.** Only files a parser claims — the same admission test the index scan uses. Snippets are served for graph nodes, and only a parsed file has any, so storing anything else costs space no read could ever use. This matters because `ax_sync`'s watcher reports raw paths: during a `cargo build` it sees every `.o` and `.rmeta` written under the build directory. On this repo the store is **3.8 MB of text over 499 files**; storing everything the watcher reported instead cost 91 MB.

`ax sync` also drops stored text that no indexed file claims, so a database written by an earlier build recovers on the next sync without a full re-index.

**Snippets show the indexed state.** A snippet reflects what the graph knows, which is the point: the line numbers a query reports and the text it shows come from the same index and always agree. Edit a file without syncing and its snippet keeps showing the last indexed text — the pre-store behaviour was to read the new text while still reporting the old line numbers, which silently misaligned the two. `ax_status` reports files pending sync; `ax_sync` closes the gap.

**Backfilling after upgrade.** A database indexed before v17 has file rows but no stored source, so snippets would report "not stored". `ax sync` backfills unchanged files without re-parsing them, and `ax index` rebuilds the store outright. Until coverage is complete, `ax_status` and the auto-injected `<ax_index>` preflight block say so:

```
Source store: 0/499 files — snippets for the rest report "source not stored"; run ax_index (or ax_sync) to backfill
```

Coverage counts the files that can own a snippet — parsed, under the cap — not every row in `files`. This repo indexes 3,839 file rows and only 500 of them are code; the rest are SVG assets and build output no parser claims, and that count keeps rising during a `cargo build`. Measuring against those would report a gap that no re-index can ever close.

**Not covered.** The Command Center source viewer (`ax web`) still reads files directly: it shows a human the file's *current* content, which is a different job from answering a graph query. Index-time scanning also walks the tree — that is how the graph gets built.

Enforcement lives in `crates/ax-context/tests/no_query_time_disk_reads.rs`, a fail-closed gate over the query-path modules, plus the CRITICAL `graph-only-query-path` policy rule that makes `ax_guard` block a write reintroducing a disk read. Rerun every layer with `.\scripts\gauntlet-graph-only.ps1`.

## Shared daemon (multi-client)

`ax serve --mcp` prefers a **per-project daemon**: the IDE process is a thin stdio proxy; one daemon owns `.ax/ax.db`. That lets Cursor and Takumi share one writer.

If the daemon cannot start in time, each client falls back to an **embedded** engine — concurrent writers then produce `database is locked` and agents enter **DEGRADED**. Recover with Command Center **Reload MCP** (hamburger / sidebar Ops) or `ax daemon restart`, then restart MCP servers in the IDE. See [Troubleshooting](/docs/troubleshooting/#mcp-hits-database-is-locked--agents-go-degraded).

## How agents should use it

ax *is* the pre-built search index. For "how does X work?", architecture, a flow ("how does X reach Y"), or where-is-X questions — and while editing code — an agent should answer with `ax_explore` and stop, typically with **zero file reads**, rather than re-deriving the answer with `grep` + `Read`. A direct ax answer is one to a few calls; a grep/read exploration is dozens.

**Prefer MCP ops over shell CLI.** When the ax MCP server is connected, agents must call `ax_sync`, `ax_index`, `ax_lsp`, `ax_ship`, `ax_policy_index`, `ax_remember` / `ax_recall` instead of running the matching `ax …` commands in a terminal. Shell CLI is reserved for DEGRADED mode (MCP unreachable) or ops with no MCP tool (`install`, `upgrade`, `web`, `share`, `ship --watch`). IDE bootstrap files (`AGENTS.md`, `.cursor/rules/ax.mdc`, …) and the CRITICAL `prefer-mcp-ops` policy rule encode the same mapping.

The MCP server delivers this guidance to the main agent automatically, in the MCP `initialize` response (`server_instructions`). Because subagents and non-MCP harnesses never see that response, the installer also writes a short marker-fenced section into each agent's instructions file. Re-sync with `ax policy sync --fix` after upgrading ax.
