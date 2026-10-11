# Ax preflight audit

No code was changed. This report is from the current implementation (`preflight` in `crates/ax-mcp/src/tools.rs`) and from read-only queries of the databases on this machine.

## Answers

**Why can preflight return another project's information?**

The MCP engine opens **one** SQLite file, `{resolved root}/.ax/ax.db`, the first time it initializes, and every later preflight in that process keeps that pool. Memories, the graph snapshot, and pending files all come from that file. There is no `project_id` column. Isolation is "whichever directory `ensure_initialized` picked."

On this machine that pick is wrong in two real cases:

1. **`/Users/gary/io` is itself an ax project.** Its `.ax/ax.db` (about 1.3 GB) indexes child trees, including `MijnVF/Vf.Portal/...` (4,112 files, 11,469 nodes whose path contains `Vf.Portal`). `find_nearest_ax_root` stops at the first directory that has `.ax`. A daemon or MCP process whose cwd or `--path` is `/Users/gary/io` never opens `/Users/gary/io/ax/.ax/ax.db`.
2. **The two memory titles exist only in `/Users/gary/io/MijnVF/.ax/ax.db`**, not in `/Users/gary/io/ax/.ax/ax.db`. Call-log rows that contain those titles are tagged `project=/Users/gary/io/MijnVF`. Preflight listed them because the engine was that project and `recall_for_prompt` matched them (limit 3, titles inlined with no character cap).

`projectPath` on the preflight call does **not** switch the database. It only changes the policy matcher's cwd.

**Why is preflight so large?**

Always-apply rule bodies are explicitly not cut when they pass `AX_POLICY_MAX_CHARS` (default 16,000). That cap applies only after the always-apply section. With `context_budget_tokens` unset (the default), the context catalog may add up to 1,500 tokens and the memory-title list up to 200 tokens **on top of** an already unbounded policy block. The tool text the model sees is the `inject` string. Structured content still also sends `indexStats` and the full `pendingFiles` array.

This chat's engine is the ax repo (graph line: 10,807 nodes, 2,443 code files). Those two memories are absent from this project's database, so a correctly rooted ax preflight cannot emit them. A preflight that does emit them was not reading `ax/.ax/ax.db`.

---

## 1. Current preflight pipeline

`call_tool_and_wrap` (`crates/ax-mcp/src/server.rs`) then `preflight` (`crates/ax-mcp/src/tools.rs`, lines 702–1054).

1. `ensure_initialized` — if no `Ax` is open yet, resolve a root and `Ax::open` → `{root}/.ax/ax.db`. The open instance is kept for the process.
2. Attach the chat id and turn count.
3. `resolve_preflight_cwd` — optional `projectPath` / `project_path`, else `ax.project_root()`. Used **only** as `MatchInput.cwd`.
4. `ax.match_policy` — rules and skills from **the engine pool**. Failures become an empty policy block; preflight still returns.
5. `recall_for_prompt(pool, prompt, 3)` — up to 3 scored memories from that same pool. If the match set contains links, `list(pool, 100_000, 0)` loads the vault to expand them.
6. `session_policy_inject` — builds `<ax_policy>`. Cursor clients drop bodies that already match `~/.cursor` or `{project}/.cursor`.
7. `format_index_inject_block` — graph counts plus at most 8 pending paths.
8. `<ax_memories>` titles for the 3 matches (no size cap).
9. Related turns, at most 1,200 characters. Review-language line.
10. If the context cache is on (default):
    - static `<ax_context_cache>` instructions
    - `<ax_chat session=…>`
    - session ledger from `~/.ax/usage.db` (counts only)
    - `<ax_session_context>` for this conversation **and** this project root
    - working-context notes
    - `<ax_context_catalog>` from `~/.ax/usage.db`, filtered to this session id, cap 1,500 tokens
    - `<ax_memory_titles>` from `list(pool, 200, 0)`, cap 200 tokens
11. Optional budget warning. Directive hint.
12. The model-facing text is `inject` (`tool_result_text`). `lean_structured` drops rule/skill/memory bodies from structured content but **keeps** `indexStats` and `pendingFiles`.

## 2. Context sources

| Block | Scope | What identifies the project | Database | Other project's data? | Size |
|---|---|---|---|---|---|
| `<ax_policy>` | Engine project, plus user-global Cursor files for "already loaded" suppression | SQLite path of the engine pool. Policy cache key is that filename | `{engine root}/.ax/ax.db` | Yes, if the engine root is the parent `io` tree or another repo. Home `~/.cursor` rules are intentionally global | Always-apply section is **not** hard-capped. Contextual section stops near 16,000 chars (`AX_POLICY_MAX_CHARS`). Per-rule inline default 250 tokens × 4 chars; per-skill 1,500 × 4. Short rules are sent in full |
| `<ax_index>` | Engine project | Same pool | Same `ax.db` | Yes, when that db indexed another tree. Parent `io` db has 44,209 files and 386,767 nodes | Counts plus ≤ 8 pending lines. The structured `pendingFiles` field is the **full** list |
| `<ax_memories>` | Engine project | Same pool. No project column on `memories` | Same `ax.db` | Yes. Proven: the two titles are rows in `MijnVF/.ax/ax.db` only | 3 matches, titles uncapped |
| `<ax_memory_titles>` | Engine project | `list` is `SELECT … FROM memories ORDER BY updated_at DESC LIMIT 200` | Same `ax.db` | Yes. Newest titles from whatever db is open, even when the prompt did not match them | 200 tokens (`AX_PREFLIGHT_MEMORY_TITLE_TOKENS`, default 200). Appended whenever `context_budget_tokens` is unset |
| Turn history | Engine project | `related_turns(pool, …)` | Same `ax.db` | Yes, same condition | 1,200 characters, 3 turns |
| `<ax_context_cache>` | Static text | None | None | No | Fixed paragraph |
| `<ax_chat>` / ledger | Session id | Ledger SQL filters `session_id` only, not project | `~/.ax/usage.db` via `usage_db_path()` (`AX_USAGE_DB` or `~/.ax/usage.db`) | Ledger is counts. It does not include other projects' text | One line |
| `<ax_session_context>` | Session **and** project | `scope` is `"{conversation}\\u001f{root}"` | `~/.ax/usage.db` (`mcp_reuse_cache`) | File paths from another project appear only if they were stored under this conversation+root. On disk, `files_json` for other roots does contain `MijnVF/Vf.Portal/...` | Token cap is `SESSION_CONTEXT_TOKENS` (passed into `format_session_context`). Rows limited by `CONTEXT_ROWS` |
| Working context | Session and project root argument | Root path + conversation | `~/.ax/usage.db` | Isolated by the root argument | Omitted when `known_context` still matches |
| `<ax_context_catalog>` | Session id **only** | `scope_catalog` keeps rows whose `session_id` equals the chat id. The SQL orders every session, then filters | `~/.ax/usage.db` (`mcp_session_index`, bodies in `mcp_context_cache`) | **Yes, across projects, inside one chat id.** Summaries are the first line of a previous tool reply (160 chars). Cached `ax_preflight` bodies in this db include the MijnVF memory titles (47 preflight bodies, ~1.4 MB of body text mentioning those titles) and `Vf.Portal` paths. Those bodies are not inlined; the catalog line is. A reused session id mixes projects because the table has no project column | Query limit 20. Format cap `min(room, 1500)` or 1,500 if no budget |
| `~/.ax/global.db` | Global graph | Not read by `preflight` | `~/.ax/global.db` | Contains 22,955 `Vf.Portal` paths and `cross_project_refs`. Not a preflight source today | n/a |
| Policy `OnceLock` cache | Process-global map | Key = sqlite filename | Reads through the caller’s pool | Entries do not cross files. A wrong pool still caches the wrong project | Whole rule and skill tables for that file |
| Pending-file registry | Process-global map | Key = project root | In-memory | Lookup is by root. A wrong engine root returns that root's pending set | Unbounded in structured `pendingFiles`; inject shows 8 |

## 3. Project isolation analysis

`Ax::open` (`crates/ax-core/src/lib.rs`) sets `db_path = get_ax_dir(root).join(DB_FILENAME)` and refuses a root that is not initialized. The project database is one file per root. **This is real for a correct root.** It was checked, not assumed:

- `/Users/gary/io/ax/.ax/ax.db` — zero rows matching `Local Database Schema`, `Global Database Strategy`, `Ax Memory System Setup`, or `Vf.Portal`.
- `/Users/gary/io/MijnVF/.ax/ax.db` — both memory rows, and 4,112 `Vf.Portal/...` files.
- `/Users/gary/io/.ax/ax.db` — the same portal files under the prefix `MijnVF/Vf.Portal/...` (3,800 file rows; SQLite `LIKE` is case-insensitive, so `portal/vf.portal` matches `Vf.Portal`). Those two memory titles are **not** in the parent db. The parent memories table has 13 rows; two bodies mention `vf.portal` as text.

`ensure_initialized` (`crates/ax-mcp/src/engine.rs`) returns immediately once `self.ax` is `Some`. The root is fixed for the life of the process:

```text
if self.ax is already open → return
else root = self.project_root OR find_nearest_ax_root(cwd)
Ax::open(root)
```

`find_nearest_ax_root` walks parents and returns the first directory where `.ax` exists. Because `/Users/gary/io/.ax` exists, any start directory under `io` that is **not** inside a nested initialized project resolves to the parent index (44k files, including MijnVF). A start inside `/Users/gary/io/ax` resolves to ax, which is why this chat's index line is the ax graph.

`resolve_preflight_cwd` can point policy matching at a different tree than the open database. Memories and the index do not follow `projectPath`.

Daemon: one `McpEngine` per daemon, one `project_root` (`serve_session`). A daemon started at `io` or `MijnVF` serves that database to every client attached to it.

## 4. Cross-project contamination root cause

Observed strings, traced to rows:

| Observed text | Where it actually is | How it enters preflight |
|---|---|---|
| `Local Database Schema - ax.db Structure` (`190133ad-dc38-4585-8070-3d6c06cb1499`, kind `architecture`) | `memories` in `/Users/gary/io/MijnVF/.ax/ax.db` only | `recall_for_prompt` → `format_memory_match_titles` → `<ax_memories>`. Usage-db call log: `tool=ax_preflight`, `project=/Users/gary/io/MijnVF`. The memory body itself names `/Users/gary/io/ax/.ax/ax.db`, which is why it looks like an ax memory while living in the MijnVF vault |
| `Global Database Strategy - Ax Memory System Setup` (`a7d00f7c-dd1b-49e0-8b62-5e7b4312f949`) | Same MijnVF `memories` row | Same path. Cached preflight bodies in `~/.ax/usage.db` (`mcp_context_cache`, tool `ax_preflight`) still contain the `<ax_memories>` block |
| `portal/vf.portal...` | File paths `Vf.Portal/...` in the MijnVF index, and `MijnVF/Vf.Portal/...` in the parent `io` index and in `~/.ax/global.db`. Also `mcp_reuse_cache.files_json` in `~/.ax/usage.db` | Index snapshot and pending files when the engine root is `MijnVF` or `io`. Reuse session lines when that conversation+root stored those paths. Catalog summaries when a cached reply's first line mentions them |

Not the cause: ax's own `.ax/ax.db` mixing rows in. It does not contain these strings.

Secondary leak, smaller: `mcp_session_index` / `mcp_context_cache` are global and keyed by session id, not project. `scope_catalog` drops other session ids. It does not drop other projects that shared the id.

## 5. Token and character growth

What stacks on a first preflight in a process with cache on and no `context_budget_tokens` (default `None`):

| Piece | Bound |
|---|---|
| Always-apply rules | **None.** Comment in `format_inject_block_with`: if they exceed `max_chars`, the inject grows instead of cutting mid-rule |
| Contextual rules and skills | Stop near 16,000 characters |
| Always-apply skills | Inline until the skill cap (default 6,000 characters), then a summary |
| Index block | Small (counts + 8 paths) |
| Matched memory titles | 3 titles, uncapped |
| Turn history | 1,200 characters |
| Context-cache instructions + chat line + ledger | A few hundred characters, fixed |
| Session context | Its own token cap; one line per reused tool call |
| Context catalog | Up to 1,500 tokens, and when the budget is unset this is **added even if policy already blew past 16k** (`optional_room` returns `None`, and `None` is treated as "fits") |
| Memory title list | Up to 200 tokens, same "fits" rule |
| Structured content | Full `pendingFiles` plus `indexStats`, duplicate of part of the text |

`optional_room` only withholds the catalog and the title list when a budget is set and already consumed. The default leaves the budget unset, so those blocks always join.

The biggest files on disk are not all sent: parent `io` db is 1.3 GB and `usage.db` holds ~1.4 MB of old preflight bodies that mention the foreign titles. Preflight sends counts and short lines from those stores, plus full rule text from the open project db.

## 6. Existing limits

- `AX_POLICY_MAX_CHARS` default 16,000 — does not apply to always-apply rules.
- `AX_POLICY_RULE_INLINE_TOKENS` default 250; `AX_POLICY_SKILL_INLINE_TOKENS` default 1,500.
- Recall limit 3. Vault `list` for titles uses 200 rows then a 200-token format cap.
- Link expansion may load 100,000 memory rows into the server. They are not all written into `inject`.
- Pending paths in the text: 8. Pending paths in JSON: not capped.
- Catalog: 20 rows queried, 1,500 tokens formatted, session-id filter.
- Turn block: 1,200 characters.
- `AX_CONTEXT_CACHE=off` or `AX_CONTEXT_CACHE_TOKENS=0` skips cache, ledger, catalog, session context, and the title list.
- `context_budget_tokens` in global config or project `ax.json` can shrink catalog and titles. Default is off, so it does not shrink policy text.
- Second preflight in a session can skip unchanged policy bodies (`delivered` map). The first call in a session pays full size.

## 7. Recommended architecture

Keep one project database per root. Make the open root obvious and sticky to the workspace that called the tool.

- Resolve the engine root from the workspace path the client sends, and refuse to keep serving after that path resolves to a different `.ax` than the one already open. A parent `io/.ax` must not win over a nested `ax/.ax` when the workspace is the nested repo.
- Do not treat `projectPath` as a policy-only override. Either it is unused, or memories, stats, and pending files use the same root as policy. Today they diverge.
- Keep usage and context-cache rows in `~/.ax/usage.db`, but store `project_root` on `mcp_session_index` and drop catalog rows whose root is not the engine root.
- Give always-apply rules a real ceiling (compact form plus `ax_rules`), instead of growing without a cap.
- When `context_budget_tokens` is unset, still cap the optional tail (catalog, title list) against a fixed preflight budget so an oversized policy block does not get another 1,700 tokens.

## 8. Minimal safe fix

Do not rename tools. Do not add endpoints.

1. **Root lock.** In `ensure_initialized` / `resolve_mcp_project_root`, if the client workspace is an initialized ax root, use that root even when a parent directory is also initialized. If an engine is already open on a different root, do not answer with the other database; open the workspace root or return a one-line mismatch. This stops MijnVF memories and `Vf.Portal` paths from appearing in an ax workspace, and stops the parent `io` index (44,209 files) from being the preflight graph.
2. **Catalog filter.** Pass the engine root into `recent_session_catalog` and keep only rows for that root. Requires the project column (or encoding the root into the existing session key the way `reuse_cache::scope` already does). Until the column exists, the safe minimal behavior is to keep the session filter and not include summaries whose cached body or `files_json` is known to be from another root — the reuse table already stores the root inside `conversation`.
3. **Stop uncapped growth.** Apply the existing 16,000-character budget to always-apply rules as well, using the compact rule block that already exists. Leave `context_budget_tokens` behavior, but when it is unset use a fixed remainder so catalog + memory titles cannot append after the policy block is already over the cap.
4. **Do not refill a deduped policy block.** When rule bodies are already delivered or IDE-loaded, do not spend that gap on full skill bodies (`startup`, `subagents`). Send the name and `ax_skill`. That is the Rules 19.8K and Skills 5.1K bars.
5. **Cache graph replies earlier.** Replies of a few thousand tokens still land inline (`inline_tokens=38391` in this chat). Lower the default cache threshold so Conversation stops holding full `ax_explore` / `ax_node` dumps.

Do these in the order in section 11. Step 1 is the only critical fix. Steps 3–5 are size work and must not ship as a way to hide a wrong database.

## 11. Work order and Context Continuity v2

The two failures stay separate. Shrinking the payload does not fix contamination, and a correct project can still be too big.

| Problem | Severity | Status in this audit |
|---|---|---|
| Cross-project data | Critical | Proven. Not a guess. The open engine root selects the only `ax.db`. `MijnVF/.ax/ax.db` holds the two memory titles. `/Users/gary/io/.ax/ax.db` holds `MijnVF/Vf.Portal/...`. `ax/.ax/ax.db` holds neither. `projectPath` does not switch the pool |
| Preflight too large | High | Proven. Always-apply rules are uncapped. Deduping them on turn 2 refilled the gap with full skill bodies |
| Too many memories | Medium | `recall` is 3. The title list is the newest 200 rows, then 200 tokens, from whatever pool is open |
| Too much graph/context | Medium | Index text is counts plus 8 paths. The large part is inline `ax_explore` / `ax_node` replies (`inline_tokens` about 39K in this chat) |
| No hard total budget | Medium | `context_budget_tokens` defaults to unset, and unset means the catalog and title list still append |
| No real conversation compaction | Low | Ledger and reuse stubs exist. They do not replace the transcript. Compaction is last |

**Do not** make preflight smaller by dropping arbitrary sections. A missing block would hide the wrong project instead of stopping it.

### v2: preflight is the restored working state

After the critical fix, `ax_preflight` stays the same tool. Its `inject` becomes a short restoration of this agent, this chat, this project — not a second copy of the IDE rules and not a sample of another repo.

One project root for the whole call. Policy, memories, index counts, pending files, reuse lines, and the catalog all use that root. If the workspace root and the already-open engine root differ, the call says so in one line and does not mix the two databases.

The restored state, in order, under one hard token budget:

1. **Project.** Root path and a one-line index fingerprint (node count, edge count, file count). No file list.
2. **Session.** Chat id, and whether this conversation already has notes. Unchanged notes stay one line when `known_context` matches.
3. **Already delivered.** Rule and skill ids whose bodies are in the IDE or were sent earlier this session. Names only.
4. **New or changed.** Bodies only for rules or skills that match this prompt and are not already in the agent's context. Everything else is `ax_rules` / `ax_skill` by name.
5. **Memories.** Up to three titles that matched **this** prompt in **this** project's vault. No newest-200 list.
6. **Working state.** Open questions, decisions, and files this chat already established. Graph answers already given are ids, not copies.
7. **Budget line.** Tokens used versus the preflight cap. Optional blocks stop when the cap is hit. They are not deleted at random; the order above is the priority, so compaction and catalog lose first, project identity never does.

Conversation compaction stays out of v2. The transcript is the client's. Preflight only refuses to paste it back in.

### Size control: level 0–100, not a percent of the model window

The screenshot's 256K is Cursor's window. It already contains the system prompt, tool definitions, IDE rules, and the conversation. Ax does not own that number, so "50%" cannot mean half of 256K. That would still allow a huge preflight, and 0% would delete the project line.

One setting, stored as tokens in the field that already exists: `context.budget_tokens` (`BudgetSettings.context_budget_tokens`, read from the global config and overridden by project `ax.json`). Nothing in the CLI `ax budget` command or the Command Center Settings page writes it today.

The user-facing value is a level from 0 to 100. The ceiling is a fixed 8,000 tokens, which is the ax preflight budget, not the model window.

| Level | Tokens | What still fits |
|---|---|---|
| 100 | 8,000 | Full restored state. Still a hard cap. Today's uncapped always-apply rules do not return |
| 50 | 4,000 | Default. Project, session, changed rule and skill bodies, three matching memory titles |
| 0 | 400 | Project line, session id, rule and skill names. No bodies, no catalog, no memory list |

Tokens = 400 + level × 76, rounded. Level 0 is 400, level 100 is 8,000. The same setting accepts an explicit token count; tokens win when both are set.

Priority inside the cap does not change with the level. Project identity is never the block that gets cut. Catalog, title list, and skill bodies drop first. A lower level does not hide another project's database. The root lock runs before the budget.

Where it is set:

- Settings page: one `SettingRow` (a single slider is allowed inline). Label "Preflight size", slider 0–100, token count beside it.
- CLI: `ax budget context --level 50` and `ax budget context --tokens 4000`. Global by default; `--project` writes the project `ax.json` override that `load_settings` already applies on top of the global file.

## 12. Execution plan

No implementation has landed. "voer uit" started a lookup of `save_global_budget` and `BudgetSectionPatch` only. This section is the plan to run in order. A later step does not start until the previous step's tests pass. Do not drop inject blocks to make a test green.

### Step 1 — Lock the engine to the workspace root

**Done when:** a parent `.ax` and a nested `.ax` no longer share one pool, and `projectPath` cannot point policy at one root while memories come from another.

Files:

- `crates/ax-mcp/src/engine.rs` — `ensure_initialized`. If an `Ax` is already open and `projectPath` or the request workspace resolves to a different initialized root, do not keep the old pool. Reopen that root, or return a one-line mismatch and refuse the tool. Prefer reopen when the new path is an initialized ax root.
- `crates/ax-mcp/src/tools.rs` — `resolve_preflight_cwd` and `preflight`. The cwd used for policy and `ax.db_pool()` / `ax.project_root()` must be the same path after this step.
- `crates/ax-context/src/directory.rs` — `find_nearest_ax_root` stays as it is. The fix is at the engine, not by ignoring a parent that is a real project when the workspace itself is not initialized.

Tests (new, temp dirs, never the repo `.ax`):

- Parent and child both initialized. Engine opened on the parent. Preflight with workspace = child returns the child's memory title and not the parent's.
- Engine opened on the child. A memory that exists only in the parent is absent.
- `projectPath` set to the other project: policy inject and memory titles come from that same project, or the call errors. They must not diverge.

### Step 2 — Level ↔ tokens, default 50

**Done when:** unset config means 4,000 tokens, level 0 is 400, level 100 is 8,000, and an explicit token count overrides the level.

Files:

- `crates/ax-usage/src/budget.rs` — `level_to_tokens(level: u8) -> u32` = `400 + level * 76`, clamp level to 0..=100. `tokens_to_level`. `BudgetSettings::default().context_budget_tokens` becomes `Some(4_000)`. `apply_json` already reads `context.budgetTokens`.
- Existing tests that assert `context_budget_tokens == None` for a missing key must expect `Some(4_000)` unless the test file sets the field.

Tests:

- level 0 → 400, level 50 → 4,000, level 100 → 8,000, level 101 clamps to 100.
- JSON `{"context":{"budgetTokens":1234}}` yields 1234, not the level default.
- Missing `context` yields 4000.

### Step 3 — CLI

**Done when:** `ax budget context --level N` and `--tokens N` write `context.budgetTokens` through `save_global_budget`. `--project` writes `ax.json` in that project with the same key. `--level` and `--tokens` together: tokens win. Invalid level (outside 0..=100) exits with an error and does not write.

Files:

- `crates/ax-cli/src/commands/budget_cmd.rs` — `run_context`.
- The clap enum that already dispatches `run_set` (search `budget_cmd::run_set` callers in `crates/ax-cli`). Add subcommand `context`.
- `BudgetSectionPatch.context_budget_tokens` already exists and `save_global_budget` already writes it. Do not add a second config file.

Tests:

- Temp `HOME`. `--level 0` writes `budgetTokens: 400`. `--tokens 1500` writes 1500. Both flags: tokens win. `--level 101` does not create or modify the file.

### Allowed band

The Settings slider and the CLI only accept this band. Values outside it are rejected and nothing is written.

| | Floor | Default | Ceiling |
|---|---|---|---|
| Level | 0 | 50 | 100 |
| Tokens | 400 | 4,000 | 8,000 |

400 is level 0 (project line and session id). 8,000 is level 100 (full restored state, still capped). There is no setting above 8,000 and no setting that turns the cap off. The model window (the 256K in the screenshot) is not part of this band.

### Step 4 — Settings page

**Done when:** Settings shows one inline `SettingRow` labeled "Preflight size": slider 0–100, token count beside it, loaded from the same global config, saved through the existing budget save path. Not a modal.

Files:

- `crates/ax-web/web-ui/src/pages/Settings.tsx` — row next to the other single-control settings (`setReviewCommentLanguage` is the pattern).
- The web route that already reads or writes global budget JSON. Extend it; do not add a new resource name.
- `crates/ax-web/web-ui` API helper used by Settings.

Tests: the web handler unit test that a POST of level 50 persists `budgetTokens: 4000` and a GET returns level 50. Browser check of the slider after `scripts/reinstall-cli.sh` and `ax web` restart, because the UI is compiled into the binary.

### Step 5 — Spend the budget inside preflight, in priority order

**Done when:** `inject` token count is ≤ `context_budget_tokens`. Cutting follows the v2 order. Project identity is last to go and is kept at level 0 (400 tokens is enough for that line plus the session id).

Files:

- `crates/ax-mcp/src/tools.rs` — `preflight`. Build the blocks in this order, stop when the next block does not fit: project line, session line, delivered rule/skill ids, new rule/skill bodies, three matched memory titles, working-context notes, catalog. Do not append the catalog or the newest-200 title list when the budget is already spent (`optional_room` already returns `Some(0)`; today `None` means "fits", and Step 2 removes that `None` by default).
- `crates/ax-policy/src/format.rs` — `format_inject_block_with`. Always-apply bodies participate in the same cap. Use `compact_rule_block` when a full body does not fit. Do not grow past the cap.
- When rule bodies were skipped as unchanged or IDE-loaded, do not fill that gap with full `startup` / `subagents` bodies if those names were already delivered or omitted earlier in the session. Send the name.

Tests:

- Budget 400: inject contains the project root and the session id, and does not contain a rule body or a memory body.
- Budget 4000: a matched memory title is present; a 201st memory title is absent.
- Always-apply fixtures whose bodies exceed 8,000 tokens produce an inject at or under the cap, and the omitted ids are named.
- Two projects from Step 1 still cannot leak at level 100.

### Step 6 — Report the token size on every tool call

**Done when:** the text the client receives for every MCP tool, including `ax_preflight`, ends with one line: `tokens=<N> budget=<cap>` where `N` is `count_tokens` of that text (the line itself excluded) and `cap` is the active preflight budget for `ax_preflight`, or the graph-reply cache threshold for the other tools.

Today `token_budget_hint` in `crates/ax-mcp/src/server.rs` skips `ax_preflight` and only speaks once a response is already large. Replace that skip. The line is always present, including for a small reply, so the preflight size is visible without guessing.

`ax_preflight` also puts the same numbers in the JSON as `injectTokens` and `budgetTokens`, next to the existing counts. The text line and the JSON fields use one measurement.

Tests:

- A preflight fixture under the cap: the trailing line equals the token count of the inject, and `injectTokens` matches it.
- `ax_search` and `ax_node` fixtures include the same trailing line.
- A reply that is only the line is not counted twice.

### Step 7 — Docs

**Done when:** README and the budget guide on the site mention `ax budget context --level`, the Settings slider, the band 400–8,000 tokens (level 0–100), and the `tokens=` line on every tool result. No new version number.

### Out of this plan

- Conversation compaction of the client transcript.
- New MCP tools or renamed tools.
- Deleting memories, rebuilding `~/.ax/global.db`, or moving any `.ax` directory.
- Lowering the graph-reply cache threshold. That is a follow-up after Step 5, not part of this run.

### Run order

1 → 2 → 3 → 4 → 5 → 6 → 7. After 6, one fresh `cargo test` of the touched crates (`ax-usage`, `ax-mcp`, `ax-policy`, `ax-cli`, `ax-web`) must pass. UI step 4 is not done until the embedded bundle is reinstalled.

Step 1 is the contamination fix. Step 3 is the size fix. Step 2 closes the global usage-db session mix.

## 10. Cursor context chart (this chat)

Screenshot: **112.3K / 256K (44%)**. Ax does not own every bar. What it can shrink:

| Bar | Tokens | Who writes it | Smarter behavior |
|---|---|---|---|
| System prompt | 1.6K | Cursor | Leave it. Not ax |
| Tool definitions | 3.8K | Cursor tool list | Leave it. Ax MCP schemas are part of the next row |
| Rules | **19.8K** | Cursor always-apply `.cursor/rules` **plus** the first `<ax_policy>` inject | Same CRITICAL rules are loaded twice on turn 1: once by the IDE, once inlined by preflight. `ide_loaded_keys` only skips a body when the `.cursor` file text matches the indexed body. Turn 2 of this chat did skip rule bodies (`Unchanged since earlier…`). The 19.8K is the first-turn double copy. Smarter: if the client is Cursor, send rule **ids and levels** for anything already in `.cursor/rules` or `~/.cursor/rules`, never the body |
| Skills | **5.1K** | IDE `old-coder` (noted as already loaded) **plus** full `startup` and `subagents` bodies | This turn's preflight omitted rule bodies and then spent the free budget on two long skills. Turn 1 had dropped those skills (`Skills omitted to keep always-apply rules…`) so they were not in `delivered`, and turn 2 sent them whole. `old-coder` is correctly named only. Smarter: a skill the IDE already loaded, or that was omitted once for size, stays a name plus `ax_skill` for the rest of the session. Do not fill the hole left by deduped rules with a second wave of skill prose |
| MCP and dynamic tools | 3.5K | Cursor MCP catalog | Leave the catalog. Do not add tools. Preflight text is not this bar; it lands in Conversation |
| Subagent definitions | 1.1K | Cursor | Leave it |
| Conversation | **77.2K** | This chat's tool results and the audit | Dominant cost. `<ax_session_ledger>` on the follow-up preflight: `stored_tokens=16800` (cached off the transcript) and `inline_tokens=38391` (still in the thread). The rest is the audit write-up plus graph replies that were under the cache threshold, so they stayed inline. Smarter: lower the default cache threshold so an `ax_explore` / `ax_node` reply of a few thousand tokens is stored and the model sees a stub; stop appending catalog and memory titles once the policy block is already large |

Rules + skills + the inline part of conversation are the ax-shaped slice (about 19.8 + 5.1 + 38K). System prompt, tool definitions, MCP catalog, and subagent definitions are Cursor chrome (about 10K) and should not be "fixed" inside preflight.

## 9. Tests required

- Two temp projects, each `ax init`, each with a distinct memory title. An engine opened on A, asked to preflight with workspace B, must not return A's title or A's file paths.
- A parent directory and a child directory both initialized. Preflight whose workspace is the child must report the child's node count, not the parent's, and must not list files that exist only under the parent index.
- `projectPath` set to the other project must not leave memories and `indexStats` on the old pool. Whatever the chosen rule is (ignore the override, or switch root), policy cwd and `db_pool` must match. Add a regression that fails on today's split.
- `mcp_session_index` rows for session S under project A and project B. Preflight for B must not print A's catalog summary.
- Always-apply rules whose combined bodies exceed 16,000 characters produce an inject at or under the cap (plus the fixed chrome), and the omitted ids are named.
- With `context_budget_tokens` unset, catalog plus memory titles are absent or capped once policy text is already over the preflight cap.
- Existing session reuse test stays: same conversation and same root still lists fresh entries; a different root does not.
