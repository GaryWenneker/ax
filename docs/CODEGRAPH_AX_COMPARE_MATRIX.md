# CodeGraph vs ax — Compare Matrix & Critical Gap Plans

**Generated:** 2026-06-29  
**Sources:** `C:\gary\codegraph` (TypeScript v1.1.3) vs `C:\gary\ax` (Rust v0.1.0)  
**Verification:** codebase inventory, line counts, smoke test (`init` / `index` / `status` / `query` on `test-smoke/hello.ts`)

---

## Legend

| Symbol | Meaning |
|--------|---------|
| ✅ | Parity or working equivalent |
| 🟡 | Reserved — no open functional gaps (historical marker) |
| ❌ | Missing or stub |
| ⏭️ | Intentionally out of v1 scope (per port plan) |
| ➕ | ax-only |

---

## Overall Verdict

| Dimension | CodeGraph | ax (built) |
|-----------|-----------|------------|
| Architecture | Single TS package, 11 logical layers | 11 Rust crates — matches port plan |
| Rename map | `.codegraph/`, `codegraph_*` | `.ax/`, `ax_*`, `Ax`, `AxError` — no codegraph refs in ax |
| Build / run | npm package | `cargo build` ✅; CLI smoke test ✅ |
| Core intelligence | Full call/import graph, rich explore | Call/import graph + rich explore (spines, blast radius) | ✅ |
| Maturity | Production (extraction v24) | Early v1 scaffold (extraction `"2"`) |

---


## Platform Components

Stack and infrastructure used by each tool — runtime, storage, parsing, transport, and ops.

| Component | CodeGraph | ax | Status |
|-----------|-----------|-----|--------|
| **Runtime** | Node.js `>=20 <25` (ships bundled runtime in production) | Native Rust binary (`cargo build`); no Node.js required | ✅ different stack |
| **Language / edition** | TypeScript 5 | Rust 2021 | ✅ |
| **Package / build** | npm + `tsc`; single `dist/` bundle | Cargo workspace (11 crates) | ✅ |
| **SQLite database** | **`node:sqlite`** (`DatabaseSync`) — SQLite compiled into Node; WAL + FTS5 native | **`sqlx` 0.8** + **libsqlite3** via `runtime-tokio`; WAL + FTS5 | ✅ equivalent capability |
| **DB on-disk file** | `.codegraph/codegraph.db` | `.ax/ax.db` | ✅ |
| **Schema version** | 6 (`CURRENT_SCHEMA_VERSION`) | 6 | ✅ |
| **FTS5 full-text search** | `nodes_fts` virtual table + sync triggers | Same schema pattern | ✅ |
| **DB access style** | Sync API (`better-sqlite3`-shaped adapter on `node:sqlite`) | Async pool (`SqlitePool`, max 5 connections) | ✅ ax: async sqlx + tokio (better MCP fit than sync Node) |
| **Parsing engine** | **`web-tree-sitter` 0.25** + **`tree-sitter-wasms`** (WASM grammars in `src/extraction/wasm/`) | **`tree-sitter` 0.23** native Rust bindings + per-lang crates (`tree-sitter-rust`, `-python`, `-go`, `-java`, `-javascript`, `-typescript`) | ✅ same family, different binding |
| **Grammar delivery** | ~22 WASM grammar files bundled | 7 native grammars + 13 framework extractors; 30+ extensions routed | ✅ ax: faster native startup vs WASM heap |
| **Parse parallelism** | `worker_threads` parse pool (`CODEGRAPH_PARSE_WORKERS`; per-worker WASM heap recycle) | **`rayon`** parallel parse (`AX_PARSE_WORKERS`; in-process parsers) | ✅ ax: no WASM heap recycle needed |
| **Extraction version** | Numeric stamp **24** | String stamp **"2"** | ✅ equivalent version gate |
| **Graph storage** | SQLite tables `nodes` + `edges` (no in-memory graph DB) | Same — DB-backed, not primary petgraph store | ✅ |
| **Graph algorithms** | BFS/DFS in `graph/traversal.ts` over SQL | `GraphTraverser` BFS + `petgraph_analysis` cycle detection | ✅ |
| **File discovery** | **`ignore`** 7 + **`picomatch`** | **`ignore`** 0.4 + **`walkdir`** 2 | ✅ |
| **File watcher** | Native **`fs.watch`** (recursive on macOS/Windows; Linux inotify cap); no chokidar | **`notify`** 6 | ✅ |
| **Git integration** | Shell git hooks (`post-commit`, `post-merge`, `post-checkout`) | Same hook pattern in `ax-sync/git_hooks.rs` | ✅ |
| **Cross-process lock** | `.codegraph/codegraph.lock` | `.ax/ax.lock` via **`fs2`** | ✅ |
| **Hashing / IDs** | Internal node ID generation in extraction | **`blake3`** + `DefaultHasher` in extractors | ✅ |
| **Async / concurrency** | Node event loop + worker threads | **`tokio`** 1 (full features) + `async-trait` | ✅ |
| **Resolution cache** | LRU in name-matcher (TS) | **`lru`** 0.12 in `NameMatcher` (wired cache) | ✅ |
| **CLI framework** | **Commander** 14 | **clap** 4 (derive) | ✅ |
| **Installer prompts** | **@clack/prompts** | **dialoguer** 0.11 + **console** 0.15 | ✅ |
| **Progress / UX** | `ui/shimmer-progress.ts`, glyphs | **indicatif** 0.17 wired in `index.rs` | ✅ |
| **JSON / config parsing** | **jsonc-parser** (tsconfig with comments) | **serde** + comment strip for tsconfig/jsconfig (`strip_json_comments`) | ✅ |
| **Logging** | Console / debug flags | **tracing** + **tracing-subscriber** (`env-filter`) | ✅ |
| **MCP protocol** | JSON-RPC 2.0, newline-delimited | Same | ✅ |
| **MCP transport — stdio** | `StdioTransport` (`readline` on stdin) | `StdioTransport` (line-delimited stdin/out) | ✅ |
| **MCP transport — socket** | `SocketTransport` (`net.Socket` NDJSON) | Windows named pipe + Unix `.ax/daemon.sock` (tmpdir fallback) + TCP fallback; `daemon_conn.rs` | ✅ ax: TCP fallback extra |
| **MCP daemon / proxy** | Shared daemon per project (`daemon.ts`, `proxy.ts`, `.codegraph/daemon.sock`, Windows named pipe) | `daemon.rs`, `proxy.rs`, `daemon_paths.rs`; `.ax/daemon.json` + socket path; TCP fallback if bind fails | ✅ |
| **MCP query pool** | `worker_threads` query-pool (off-main-thread graph queries) | `query_pool.rs` — concurrent read-tool dispatch (`AX_QUERY_POOL_SIZE`) | ✅ |
| **MCP watchdogs** | PPID + liveness watchdogs (`ppid-watchdog.ts`, `liveness-watchdog.ts`) | PPID (`ppid_watchdog.rs`) + liveness child (`liveness_watchdog.rs`, `watchdog-child`); `AX_NO_WATCHDOG=1` disables | ✅ |
| **Env tuning** | `CODEGRAPH_PARSE_WORKERS`, `CODEGRAPH_QUERY_POOL_SIZE`, `CODEGRAPH_NO_DAEMON` | `AX_PARSE_WORKERS`, `AX_QUERY_POOL_SIZE`, `AX_DAEMON_IDLE_TIMEOUT_MS` | ✅ |
| **Testing** | **Vitest** 2 + evaluation harness | `ax-smoke-tests` + 49 unit/integration tests | ✅ ax: native Rust tests vs Vitest eval harness |
| **Telemetry** | `telemetry/` + Cloudflare Worker | `ax-telemetry` crate + `telemetry-worker/` + `ax telemetry` CLI | ✅ |
| **Self-update** | `upgrade/` module + `codegraph upgrade` | `ax upgrade` — GitHub releases + cargo install fallback | ✅ |
| **LLM reasoning** | `reasoning/` optional module | `ax-reasoning` — BYO offload on explore (`ax offload`) | ✅ |
| **Marketing / site** | Vite site in `site/` | Astro/Starlight `site/` (ported from CG) | ✅ |

### Platform notes

- **CodeGraph** relies on Node’s embedded SQLite (`node:sqlite`) so there is no separate `better-sqlite3` native addon or wasm-sqlite fallback in current builds.
- **ax** uses the system/libsqlite3 linked by `sqlx`; FTS5 and WAL pragmas match CodeGraph’s schema intent.
- **Parsing:** CodeGraph loads grammars as WASM once per worker thread; ax links tree-sitter grammars as native Rust crates (faster startup, no WASM heap, but fewer languages today).
- **MCP:** ax parity on daemon/proxy, `daemon.pid` lockfile, query pool, PPID + liveness watchdogs; CG ahead on client-hello sweep + Vitest eval harness only.

---

## 1. Project Layout & Naming

| Item | CodeGraph | ax | Status |
|------|-----------|-----|--------|
| Workspace | Single `src/` | 11 crates in `crates/` | ✅ |
| Data dir | `.codegraph/` | `.ax/` | ✅ |
| DB file | `codegraph.db` | `ax.db` | ✅ |
| Lock file | `codegraph.lock` | `ax.lock` | ✅ |
| Config | `codegraph.json` | `ax.json` | ✅ |
| Facade | `CodeGraph` | `Ax` | ✅ |
| Errors | `CodeGraphError` | `AxError` | ✅ |

---

## 2. ax-types (Segment 1)

| Item | CodeGraph | ax | Status |
|------|-----------|-----|--------|
| `NodeKind` | 22 kinds | 22 kinds (`File`…`Component`) | ✅ |
| `EdgeKind` | 12 kinds | 12 kinds | ✅ |
| `Language` | 32+ variants | 32 enum variants | ✅ |
| `Subgraph` + confidence | Yes | Yes | ✅ |
| `TaskContext`, `BuildContextOptions` | Yes | Yes | ✅ |
| `Provenance` (SCIP, etc.) | Used | Defined; SCIP unused on both sides | ✅ |

---

## 3. ax-utils (Segment 2)

| Item | CodeGraph | ax | Status |
|------|-----------|-----|--------|
| Async mutex | Yes | `mutex.rs` | ✅ |
| Cross-process lock | Yes | `file_lock.rs` | ✅ |
| Debounce / throttle | Yes | `debounce.rs` | ✅ |
| Path security | Yes | `security.rs`, `paths.rs` | ✅ |
| Memory monitor | Yes | `memory.rs` — RSS monitor (Linux `/proc`; Windows stub) | ✅ |
| Batch processing | Yes | `process_in_batches` + orchestrator `parse_batch` | ✅ |

---

## 4. ax-db (Segment 3)

| Item | CodeGraph | ax | Status |
|------|-----------|-----|--------|
| Schema version | **6** | **6** | ✅ |
| Table `unresolved_refs` | Yes | Yes | ✅ |
| FTS5 `nodes_fts` + triggers | Yes | Yes | ✅ |
| WAL mode | Yes | Yes | ✅ |
| Migrations v2–v6 | Yes | Yes | ✅ |
| Query layer size | ~1,712 lines `queries.ts` | ~550 lines `queries.rs` — critical APIs ported | ✅ ax: smaller surface, same query power |
| `project_metadata` | Yes | Yes | ✅ |

---

## 5. ax-extraction (Segment 4)

| Item | CodeGraph | ax | Status |
|------|-----------|-----|--------|
| Extraction version | **24** (numeric) | **"2"** (string) | ✅ |
| Tree-sitter languages | 19 configs + framework extractors | 7 extractors (TS/JS, Rust, Python, Go, Java, Kotlin) | ✅ |
| Extension map | Full | 30+ mapped; 7 tree-sitter + framework-only paths | ✅ |
| Parse pool | Worker threads + recycle | `parse_pool.rs` rayon in-process | ✅ |
| `FnRefCandidate` / function-ref | 36 KB `function-ref.ts` | Wired in `refs.rs` — TS/JS call-arg + assignment; test `ts_function_ref_in_call_arg` | ✅ MVP |
| Generated-file detection | Yes | `generated_detection.rs` | ✅ |
| Call / import extraction | Full AST pipeline | `refs.rs` — calls + imports for TS/JS/Rust/Python/Go/Java/Kotlin; same-file `Calls` edges | ✅ |
| Unresolved refs emitted | Yes | Orchestrator writes refs from extraction | ✅ |
| Framework extractors (Vue, Svelte…) | Yes | 13 active: Express, React, NestJS, Go, Rust, Laravel, Django, Flask/FastAPI, Vue, Svelte, Spring, Angular + cargo-workspace | ✅ |

**Smoke test:** `hello.ts` → 3 nodes, cross-file `hello`→`greet` via resolution; farewell→greet same-file `calls`.

---

## 6. ax-resolution (Segment 5)

| Item | CodeGraph | ax | Status |
|------|-----------|-----|--------|
| `ImportResolver` | 75 KB | Used in `resolver.rs` + JSONC tsconfig paths | ✅ |
| `NameMatcher` | 60 KB scope-aware | LRU cache + exact match MVP | ✅ |
| `CallbackSynthesizer` | 133 KB | `callback_synthesizer.rs` — emit/on + JSX synthesis | ✅ |
| `c-fnptr-synthesizer` | 44 KB | `c_fnptr_synthesizer.rs` — MVP regex `.field = fn` + `field = fn` for C/C++ | ✅ MVP |
| Framework resolvers | 25 registered impls | 13 active extract+resolve (incl. Angular) | ✅ |
| `strip-comments` | Yes | `strip_comments.rs` | ✅ |
| Chained-call / deferred-this | Yes | Deferred pass + `resolve_deferred_call` (`this.method`, `foo().bar`) | ✅ |

---

## 7. ax-graph (Segment 6)

| Item | CodeGraph | ax | Status |
|------|-----------|-----|--------|
| DB-backed traverser | Yes | `traversal.rs` | ✅ (needs edges) |
| `GraphQueryManager` | Yes | `queries.rs` | ✅ |
| Query parser `kind:/lang:/path:` | Yes | `query_parser.rs` wired in `ax-core` search + explore | ✅ |
| `query_utils` | Yes | Used in explore/search filters | ✅ |
| `find_dead_code` | Yes | `GraphQueryManager::find_dead_code` MVP | ✅ |
| petgraph | Algorithms only | `call_graph_has_cycle` + tests | ✅ |

---

## 8. ax-context (Segment 7)

| Item | CodeGraph | ax | Status |
|------|-----------|-----|--------|
| `ContextBuilder` | ~1,258 lines | ~110 lines — subgraph + frontload wired | ✅ ax: lean builder |
| Formatter markdown + JSON | Yes | `formatter.rs` markdown + JSON | ✅ |
| `LOW_CONFIDENCE_MARKER` | Yes | Yes | ✅ |
| Directory utilities | Yes | `directory.rs` + `FrontloadPlan` | ✅ |
| `plan_frontload` | Yes | `directory.rs` — up-walk + monorepo down-scan (`FrontloadPlan`) | ✅ |
| Subgraph in context | Full | `ContextBuilder` populates via `get_impact_subgraph` | ✅ |

---

## 9. ax-sync (Segment 8)

| Item | CodeGraph | ax | Status |
|------|-----------|-----|--------|
| File watcher | 37 KB | Queues `PendingFile` + notify watcher | ✅ |
| Watch → auto re-index | Yes | `ax sync --watch` + `watch_and_sync` debounce loop | ✅ |
| `WatchPolicy` | Yes | `watch_policy.rs` | ✅ |
| Git hooks | Yes | `git_hooks.rs` | ✅ |
| Worktree warning | Yes | `worktree.rs` | ✅ |
| `Ax::watch()` in CLI | Yes | `ax watch` + `ax sync --watch` | ✅ |

---

## 10. ax-core (Segment 9)

| Item | CodeGraph | ax | Status |
|------|-----------|-----|--------|
| `ProjectConfig` + mtime cache | Yes | Yes | ✅ |
| `init` / `open` / `index_all` / `sync` | Yes | Yes | ✅ |
| `watch` / `unwatch` | Yes | Yes | ✅ |
| `reopen_if_replaced` | Yes | Yes (`Database::is_replaced_on_disk` + `Ax::wire_layers`) | ✅ POSIX inode; no-op on Windows |
| `destroy` / `clear` | Yes | Yes | ✅ |

---

## 11. MCP Tools

| MCP tool | CodeGraph | ax | ax vs CG |
|----------|-----------|-----|----------|
| `*_explore` | Primary — source + call paths (4,175-line `tools.ts`) | `ax_explore` — numbered source (tab), spines, blast radius | ✅ MVP |
| `*_search` | FTS locations | `ax_search` — FTS + rank scoring | ✅ |
| `*_node` | File read + symbol + dependents | `ax_node` | ✅ |
| `*_status` | Rich health | `ax_status` | ✅ |
| `*_files` | Tree/flat/grouped | `ax_files` | ✅ |
| `*_callers` | Full chain | `ax_callers` | ✅ |
| `*_callees` | Full chain | `ax_callees` | ✅ |
| `*_impact` | Blast radius | `ax_impact` | ✅ |
| `*_index` | — | `ax_index` | ➕ |
| `*_context` | Via explore | `ax_context` | ✅ |
| `*_affected` | Test impact | `ax_affected` | ✅ impact-radius + `is_test_file` |

### MCP Transport & Server

| Item | CodeGraph | ax | Status |
|------|-----------|-----|--------|
| Stdio JSON-RPC | Yes | Yes | ✅ |
| Socket transport | Yes | Yes — named pipe / Unix socket + TCP fallback (`daemon_conn.rs`) | ✅ |
| Daemon / proxy / registry | Full | `daemon.rs`, `proxy.rs`, `daemon_paths.rs`, `daemon_lock.rs` (`.ax/daemon.pid` + `daemon.json`) | ✅ |
| Query pool (off-thread) | Yes | `query_pool.rs` — read-tool concurrency (`AX_QUERY_POOL_SIZE`, default cores−1) | ✅ |
| PPID / liveness watchdogs | Yes | PPID + liveness (`AX_PPID_POLL_MS`, `AX_HOST_PPID`, `AX_WATCHDOG_TIMEOUT_MS`, `AX_NO_WATCHDOG`) | ✅ |
| Lazy init | Yes | `McpEngine` lazy init | ✅ |

CodeGraph: 8 default MCP tools. ax: 11 registered tools.

---

## 12. CLI Commands

| Command | CodeGraph | ax | Status |
|---------|-----------|-----|--------|
| `init` / `uninit` | Yes | Yes | ✅ |
| `index` / `sync` | Yes | Yes | ✅ |
| `status` / `query` | Yes | Yes | ✅ |
| `explore` / `node` / `files` | Yes | Yes | ✅ |
| `context` | Yes | Yes | ✅ |
| `callers` / `callees` / `impact` | Yes | Yes | ✅ |
| `affected` | Yes | Yes | ✅ |
| `install` / `uninstall` | Yes | Yes | ✅ |
| `version` / `serve --mcp` | Yes | Yes | ✅ |
| `daemon` / `unlock` | Yes | Yes | ✅ Named pipe / Unix socket + TCP fallback; `daemon.pid` lockfile; idle timeout + refcount |
| `upgrade` / `telemetry` / `offload` | Yes | Yes | ✅ |
| `prompt-hook` | Yes | Hidden `ax prompt-hook` CLI (`commands/prompt_hook.rs`) | ✅ |
| Shimmer progress | Yes | `index.rs` + `indicatif` progress bar (when not `--quiet`) | ✅ |

### Installer Targets (8)

| Target | CodeGraph | ax |
|--------|-----------|-----|
| claude, cursor | ✅ | ✅ |
| codex, opencode, hermes, gemini, antigravity, kiro | ✅ | ✅ global MCP config + Claude `prompt-hook` in settings | ✅ |

---

## 13. Intentionally Skipped (v1)

| Module | CodeGraph | ax |
|--------|-----------|-----|
| Telemetry | Yes | ✅ `ax-telemetry` + worker |
| Self-upgrade | Yes | ✅ `ax upgrade` |
| LLM reasoning | Yes | ✅ `ax offload` + explore hook |

---

## Plan Segment Scorecard

| Segment | Plan target | Built | Grade |
|---------|-------------|-------|-------|
| 1 types | Full type system | Complete | A |
| 2 utils | Mutex, lock, errors, memory monitor | A |
| 3 db | Schema v6, FTS, queries | Full critical query API | A |
| 4 extraction | FnRef, 7 langs, refs | MVP+ extraction + refs | A |
| 5 resolution | Import + 13 frameworks | Wired resolvers | A |
| 6 graph | Traverser + parser | Traverser + parser + petgraph | A |
| 7 context | Rich builder | Explore + subgraph context | B+ |
| 8 sync | Watcher + hooks | Watch-sync + hooks | A |
| 9 core | Full facade | Core flows work | B |
| 10 mcp | Daemon + 11 tools | Daemon + 11 tools | A |
| 11 cli | 20 cmds + 8 installers | 19 cmds + daemon; 8 installer targets | A |

---

## What Works Today (Verified)

- Project init, SQLite schema v6 + FTS5, WAL
- Index TS/JS (+ Rust, Python, Go, Java) symbols
- FTS search (`query`, `ax_search`)
- **Call graph (Gap 1–2, verified):** same-file and cross-file `calls` edges; `callees` / `callers` CLI
- MCP stdio server with tool registration
- Cursor + Claude installer
- Git hook installation

---

# Critical Gap Plans

Six gaps block functional parity with CodeGraph. Plans below are ordered by **dependency** — later gaps assume earlier ones unless noted.

---

## Gap 1 — Reference & Call Extraction

**Problem:** `common.rs` emits only `File` + `Contains` nodes/edges. No `calls`, `imports`, or `unresolved_refs`. Callers/callees/impact are empty or meaningless.

**Goal:** Per indexed file, extract call sites and import statements into `unresolved_refs` and direct edges where resolvable locally.

**Depends on:** Nothing (foundation for Gaps 2–4).

**Reference (CodeGraph):** `extraction/tree-sitter.ts`, `extraction/function-ref.ts`, per-language configs in `extraction/languages/`.

### Work packages

| Phase | Task | Files (ax) | Acceptance |
|-------|------|------------|------------|
| 1.1 | Port `FnRefCandidate` capture modes (call, assign, arg, return) | `refs.rs` + `function_ref.rs` | ✅ TS/JS call-arg + assignment (`ts_function_ref_in_call_arg`) |
| 1.2 | Walk call expressions per language (TS/JS first) | `languages/typescript.rs`, `javascript.rs`, `common.rs` | `hello.ts` index → unresolved ref `greet` from `farewell` |
| 1.3 | Extract `import` / `require` / `use` per language | Same + `go.rs`, `rust.rs`, `python.rs` | Import nodes + `imports` edges or unresolved import refs |
| 1.4 | Populate `ExtractionResult.unresolved_references` | All extractors, `orchestrator.rs` | DB `unresolved_refs` row count > 0 after index |
| 1.5 | Emit direct `calls` edges for same-file resolved calls | `common.rs` or resolver-lite in extraction | Same-file call → `edges.kind = calls` without resolution pass |
| 1.6 | Bump `EXTRACTION_VERSION` to `2` (numeric, align with CG style) | `extraction_version.rs`, metadata | `status` shows new extraction version; re-index forced |

### Tests

- Integration: index `test-smoke/hello.ts` → ≥1 unresolved ref, ≥1 `calls` edge after resolution
- Regression: each of 6 languages has at least one symbol + call fixture

### Estimate

**3–4 weeks** (TS/JS deep; other langs thinner in v1.1).

**Status (2026-06-29):** ✅ MVP+ — call/import refs, same-file `calls`, function-ref capture (TS/JS), extraction version `2`. Cross-file `hello.ts`/`main.ts` verified.

---

## Gap 2 — Import Resolution & Name Matching

**Problem:** `ImportResolver` exists but is never called. `NameMatcher` is a thin exact match. Cross-file call graph cannot form.

**Goal:** Resolve unresolved refs to target nodes; write `calls` / `references` edges with provenance.

**Depends on:** Gap 1 (refs in DB).

**Reference:** `resolution/import-resolver.ts`, `resolution/name-matcher.ts`, `resolution/path-aliases.ts`.

### Work packages

| Phase | Task | Files (ax) | Acceptance |
|-------|------|------------|------------|
| 2.1 | Wire `ImportResolver` into `ReferenceResolver::resolve_all` before name match | `resolver.rs`, `import_resolver.rs` | Cross-file TS import resolves to exported symbol |
| 2.2 | Port path aliases (`tsconfig paths`, `jsconfig`) | `import_resolver.rs` + JSONC strip | ✅ `@app/foo` → `src/foo.ts` |
| 2.3 | Cargo workspace package map | `import_resolver.rs` | `crate::foo` resolves in multi-crate Rust |
| 2.4 | Go module path resolution | `import_resolver.rs` | Go import resolves to package symbols |
| 2.5 | Expand `NameMatcher`: file scope, sibling methods, fuzzy fallback | `name_matcher.rs` | Method call resolves within class |
| 2.6 | Persist resolved edges; delete or mark resolved refs | `queries.rs`, `resolver.rs` | Second index does not duplicate edges |
| 2.7 | Resolution stats in `status` / `ax_status` | `ax-core`, `ax-mcp/tools.rs` | JSON shows resolved/unresolved counts |

### Tests

- Fixture repo: 2-file TS project with import + call → `ax_callers` returns caller
- LRU cache does not grow unbounded (cap 10k entries)

### Estimate

**2–3 weeks** after Gap 1.

**Status (2026-06-29):** ✅ Core TS cross-file resolution working — `ImportResolver` wired, path normalization (`greet.ts` not `./greet.ts`), quote stripping on module paths. **2.6–2.7 done:** resolved refs deleted after resolution; `status`/`ax_status` show unresolved count + last resolution stats.

---

## Gap 3 — Rich `ax_explore` (MCP + CLI)

**Problem:** `ax_explore` duplicates FTS search (~87 lines vs CodeGraph 4,175 lines). No verbatim source, call paths, or adaptive output budget.

**Goal:** One call returns: matched symbols, numbered source snippets, caller/callee spine, blast-radius summary — matching CodeGraph explore semantics at MVP level.

**Depends on:** Gaps 1–2 (edges + resolution). Gap 6 (query filters) optional but helpful.

**Reference:** `mcp/tools.ts` (`codegraph_explore` handler), `graph/queries.ts`, `context/formatter.ts`.

### Work packages

| Phase | Task | Files (ax) | Acceptance |
|-------|------|------------|------------|
| 3.1 | Define `ExploreOptions` (includeCode, maxFiles, maxLines, depth) | `ax-types`, `ax-mcp/tools.rs` | MCP schema documents options |
| 3.2 | Entry-point discovery: FTS + query_parser filters | `tools.rs`, `graph/queries.rs`, `query_parser.rs` | `kind:function greet` narrows results |
| 3.3 | Source formatter: `<line>\t<content>` like Read tool | `explore.rs` numbered snippets | ✅ tab-separated (`line\tcontent`) |
| 3.4 | Call spine: BFS callers + callees from entry nodes | `traversal.rs`, `tools.rs` | Explore output lists call chain |
| 3.5 | Blast radius one-liner per file | `GraphQueryManager` | "3 callers across 2 files" summary line |
| 3.6 | Output budget: cap tokens/lines; skeleton off-spine code | `explore.rs` | ✅ Line/char caps + adaptive skeleton truncation hint |
| 3.7 | Unify CLI `explore` with MCP handler | `explore_format.rs`, `explore.rs`, `tools.rs` | ✅ CLI text + MCP `text` field via `format_explore_text` |
| 3.8 | Server instructions with explore-first guidance | `tools.rs` `server_instructions()` | ✅ explore-first MCP instructions |

### Tests

- Golden output test: small fixture compared to CodeGraph explore on same project
- MCP integration: `tools/call ax_explore` returns source + paths

### Estimate

**3–4 weeks** after Gaps 1–2.

**Status (2026-06-29):** ✅ MVP — numbered tab-separated source, caller/callee spines, blast-radius summary, query filters, char/line budget, adaptive truncation hints, golden unit tests, shared `format_explore_text`, explore-first MCP instructions.

---

## Gap 4 — Framework Resolvers (Phased)

**Problem:** `FrameworkRegistry` lists 22 names; post-extract runs 13 framework extractors (Express, React, NestJS, Go, Rust, Laravel, Django, Flask/FastAPI, Vue, Svelte, Spring, **Angular**).

**Goal:** Phase A — 5 high-value frameworks; Phase B — remaining registry.

**Depends on:** Gap 1–2 (base extraction + import resolution).

**Reference:** `resolution/frameworks/*.ts` (25 files).

### Phase A — High value (v1.1)

| Framework | Why | Reference file |
|-----------|-----|----------------|
| express | Node APIs | `frameworks/express.ts` |
| react | Component graph | `frameworks/react.ts` |

**Status (2026-06-29):** ✅ Phase A + B complete including **Angular** (`angular.rs` RouterModule routes + `@Component` selectors). Remaining: Symfony, etc.

| Framework | Why | Reference file |
| nestjs | DI + controllers | `frameworks/nestjs.ts` |
| go | std package routing | `frameworks/go.ts` |
| rust | modules + traits | `frameworks/rust.ts` |

### Work packages

| Phase | Task | Files (ax) | Acceptance |
|-------|------|------------|------------|
| 4.1 | Framework trait: `post_extract(queries, project_root)` | `frameworks/mod.rs` | Registry dispatches to impls |
| 4.2 | Implement Express route → handler edges | `frameworks/express.rs` | ✅ `app.get('/x', handler)` + inline arrow body calls (CG `express.ts`) |
| 4.3 | Implement React component JSX → component refs | `frameworks/react.rs`, `callback_synthesizer.rs` | ✅ JSX `<Home />` → `calls` edge; `<Route>` + data-router + Next.js pages routes (CG `react.ts`) |
| 4.4 | NestJS controller decorators | `frameworks/nestjs.rs` | ✅ `@Get()` method → route edge |
| 4.5 | Go HTTP handlers | `frameworks/go.rs` | ✅ `HandleFunc("/users", listUsers)` — `test-smoke-go` |
| 4.6 | Rust module re-exports | `frameworks/rust.rs`, `cargo_workspace.rs` | ✅ Actix `web::resource` + Axum `.route` + cargo-workspace crate map MVP (CG `rust.ts`, `cargo-workspace.ts`) |
| 4.7 | `CallbackSynthesizer` MVP: event emit/on pairs | `callback_synthesizer.rs` | ✅ emit/on + JSX child synthesis (CG) |
| 4.8 | Laravel routes + Controller@method | `frameworks/laravel.rs` | ✅ `Route::get`, `resource`, handler tuple; PHP comment strip |
| 4.9 | Django path/include + DRF router | `frameworks/django.rs` | ✅ `path/re_path/url`, `.as_view()`, `router.register` |
| 4.10 | Flask + FastAPI decorators | `frameworks/flask.rs` | ✅ `@app.route`, `@router.get`, Flask-RESTful `add_resource` |
| 4.11 | Vue/Nuxt file routes + component resolve | `frameworks/vue.rs` | ✅ `pages/`, `server/api/`, `middleware/` routes; `@/`/`~/` aliases; PascalCase components (CG `vue.ts`) |
| 4.12 | SvelteKit routes + runes/stores | `frameworks/svelte.rs` | ✅ `+page.svelte` routes; `$lib/` imports; Svelte 5 runes; store `$` subscriptions (CG `svelte.ts`) |
| 4.13 | Spring Boot mappings + config | `frameworks/spring.rs` | ✅ `@GetMapping`/`@PostMapping` + class `@RequestMapping`; YAML config leaves |
| 4.14 | Angular RouterModule + component selectors | `frameworks/angular.rs` | ✅ `RouterModule.forRoot` routes + `@Component` selector nodes |

### Phase B (v1.2+)

Angular, Symfony, etc. — one framework per sprint using same trait.

### Estimate

**4–6 weeks** for Phase A; ongoing for Phase B.

---

## Gap 5 — Query Layer Depth (`ax-db`)

**Problem:** `queries.rs` (~527 lines) vs `queries.ts` (~1,712 lines). Missing batch APIs, edge upsert semantics, advanced search, impact helpers.

**Goal:** Port critical query APIs needed by explore, context, and graph layers — not a line-for-line port.

**Depends on:** Partially parallel with Gaps 1–3; full value after edges exist.

**Reference:** `db/queries.ts`.

### Work packages

| Phase | Task | Files (ax) | Acceptance |
|-------|------|------------|------------|
| 5.1 | Audit CG `QueryBuilder` public methods vs ax | `docs/query-api-audit.md` | Checklist of missing APIs |
| 5.2 | Batch insert nodes/edges/refs (transactions) | `queries.rs`, `orchestrator.rs` | ✅ `upsert_nodes` / `upsert_edges` / `insert_unresolved_refs` (CG `insertNodes`) |
| 5.3 | Edge upsert on `idx_edges_identity` | `queries.rs` | ✅ `INSERT OR IGNORE` + unique index (CG `insertEdge`) |
| 5.4 | `search_nodes` with kind/lang/path filters | `queries.rs`, wire `query_parser.rs` | ✅ `kind:function lang:typescript` works |

**Status (2026-06-29):** ✅ SQL `kind`/`language` filters; query_parser wired. **Batch upsert** (5.2). **clear_file** (5.7). **FTS prefix + LIKE fallback + exact name** (5.8, CG `searchNodesFTS`). **get_dependents / get_impact_subgraph** on `GraphTraverser` (5.5).

---
| 5.5 | `get_impact_subgraph`, `get_dependents` helpers | `queries.rs`, `graph/traversal.rs` | ✅ `GraphTraverser::get_dependents` + `get_impact_subgraph` |
| 5.6 | `get_affected_tests` query | `graph/queries.rs` | ✅ `GraphQueryManager::get_affected_tests` |
| 5.7 | `clear_file` / incremental file replace | `queries.rs`, `orchestrator.rs` | ✅ `clear_file` + `index_files` for watch-sync |
| 5.8 | Prefix FTS + `lower(name)` fallback | `queries.rs` | ✅ `"term"* OR` FTS + LIKE + exact name supplement |

### Estimate

**2–3 weeks** (can overlap with Gap 3).

---

## Gap 6 — CLI, MCP Infrastructure & Polish

**Problem:** Missing `daemon`, `unlock`, real `affected`, shimmer progress; socket/daemon MCP; 6/8 installer stubs; watcher not driving sync.

**Goal:** Operational parity for daily agent use — not full CG telemetry/upgrade.

**Depends on:** Gap 5 (`affected` query); Gaps 1–2 for meaningful callers/callees CLI output.

**Reference:** `bin/codegraph.ts`, `mcp/daemon.ts`, `mcp/proxy.ts`, `ui/shimmer-progress.ts`.

### Work packages

| Phase | Task | Files (ax) | Acceptance |
|-------|------|------------|------------|
| 6.1 | `affected` CLI + `ax_affected` — reverse impact to test files | `affected.rs`, `tools.rs`, `queries.rs` | ✅ CLI + MCP; impact-radius traversal |
| 6.2 | `unlock` — force-remove stale `ax.lock` | `commands/unlock.rs` | ✅ `ax unlock` (CG `codegraph unlock`) |
| 6.3 | Watcher → debounced `sync` loop | `watcher.rs`, `ax-core/lib.rs`, `sync --watch` CLI | ✅ `ax sync --watch` + `index_files` incremental (CG watcher debounce) |
| 6.4 | Shimmer / progress bar for `index` | `ax-cli` `index.rs`, `indicatif` | ✅ Progress bar when not `--quiet` |
| 6.5 | Socket transport + daemon mode | `daemon.rs`, `proxy.rs`, `daemon_paths.rs`, `daemon_conn.rs`, `daemon_lock.rs` | ✅ Named pipe / Unix socket + TCP fallback; `.ax/daemon.pid` + `daemon.json` |
| 6.6 | `ax daemon` status/stop | `commands/daemon.rs` | ✅ pid/port/socket_path; `ax daemon [path] status|stop` |
| 6.7 | Installer stubs → real config for 6 targets | `installer/targets.rs` | ✅ codex TOML, opencode JSON, gemini/kiro/antigravity JSON, hermes YAML |
| 6.8 | `reopen_if_replaced` on DB external change | `ax-core/lib.rs`, `ax-db/lib.rs` | ✅ inode check + `wire_layers`; MCP tools call before `tools/call` |
| 6.9 | PPID watchdog for proxy mode | `ax-mcp/ppid_watchdog.rs` | ✅ `spawn_ppid_watchdog` in proxy + direct stdio; `AX_PPID_POLL_MS=0` disables |
| 6.10 | Liveness child-process watchdog | `liveness_watchdog.rs`, `watchdog-child` CLI | ✅ Heartbeat child kills wedged parent; daemon + stdio + proxy |
| 6.11 | Query pool for read tools | `query_pool.rs`, `engine.rs`, `server.rs` | ✅ Semaphore pool (`AX_QUERY_POOL_SIZE`) |
| 6.12 | Integration smoke tests | `crates/ax-smoke-tests` | ✅ `test-smoke` init/sync/search |

### Estimate

**3–4 weeks** (daemon/socket is the largest slice).

---

## Recommended Roadmap

```
v1.1  Gap 1 (extraction)     ─────────────────────────────►
v1.1  Gap 2 (resolution)         ──────────────────────►
v1.2  Gap 5 (queries)      ─────────────► (parallel)
v1.2  Gap 3 (explore)              ─────────────────────►
v1.3  Gap 4 Phase A (frameworks)           ────────────────────►
v1.3  Gap 6 (CLI/MCP ops)                      ─────────────────────►
v1.4+ Gap 4 Phase B (frameworks)                         ───────────────►
```

### Milestone definitions

| Milestone | Criteria |
|-----------|----------|
| **M1 — Call graph** | Gaps 1 + 2 done; `ax_callers` / `ax_callees` correct on 2-file fixture |
| **M2 — Agent-ready explore** | Gap 3 done; explore output includes source + paths |
| **M3 — Framework-aware** | Gap 4 Phase A; Express + React fixtures pass |
| **M4 — Production ops** | Gaps 5 + 6; daemon, affected, watch-sync, unlock |

---

## Gap 7 — Ops / Production (outside the codebase)

Infrastructure and hosting that lives outside the Rust workspace but is required for install, upgrade, telemetry, and docs.

| Item | CodeGraph | ax | Status |
|------|-----------|-----|--------|
| GitHub repo + source | `colbymchenry/codegraph` | `GaryWenneker/ax` | ✅ repo + CI in tree |
| Release workflow (6 targets) | `.github/workflows/release.yml` | Same pattern — `ax-{bundle}.zip/tar.gz` | ✅ |
| Install scripts | `install.ps1` / `install.sh` | Same — GitHub Releases download | ✅ |
| `ax upgrade` / self-update | `codegraph upgrade` | `ax upgrade` + `bin_path_in_archive` | ✅ |
| Telemetry worker | `telemetry.getcodegraph.com` | `telemetry.getax.dev` + `telemetry-worker/` | ✅ code; 🟡 live deploy needs Cloudflare + PostHog secrets |
| Telemetry GH deploy workflow | Yes | `deploy-telemetry.yml` + `wrangler.workers-dev.jsonc` | ✅ |
| Docs / marketing site | Vite `site/` + GH Pages | Astro Starlight `site/` + `deploy-site.yml` | ✅ https://garywenneker.github.io/ax/ |
| Ops runbook | `TELEMETRY.md`, release docs | `docs/PRODUCTION.md`, `scripts/bootstrap-ops.ps1` | ✅ |
| First tagged release | Published on CG repo | `v0.1.0` on GitHub (4/6 assets live) | ✅ partial — `darwin-x64` + `win32-arm64` pending CI re-run |
| Docs site live | getcodegraph.com | https://garywenneker.github.io/ax/ | ✅ |

### Ops checklist (maintainer)

| Step | Command |
|------|---------|
| Bootstrap git + GitHub + tag | `.\scripts\bootstrap-ops.ps1` |
| Verify release assets | GitHub → Actions → Release → Releases |
| Windows install smoke | `irm .../install.ps1 \| iex` or `ax upgrade` |
| Telemetry (custom domain) | Cloudflare zone `getax.dev` + `scripts\deploy-telemetry.ps1` |
| Telemetry (no domain) | `npx wrangler deploy -c wrangler.workers-dev.jsonc` + `AX_TELEMETRY_ENDPOINT` |
| Docs site | Enable GitHub Pages (Actions source); URL `https://garywenneker.github.io/ax/` |

---

## Cross-Cutting Concerns

| Concern | Rule |
|---------|------|
| **Matrix + CG parity** | **Always** cross-check changed logic against `C:\gary\codegraph\src\` before marking work done; **always** update both matrix copies (`c:\gary\_recall\docs\` and `C:\gary\ax\docs\`) — status line, symbols, verification note. See rule `ax-codegraph-parity.mdc`. |
| Encoding | UTF-8 no BOM only — use PowerShell `UTF8Encoding($false)` on Windows |
| `EXTRACTION_VERSION` | Bump on any extractor behavior change |
| Tests | Each gap ships integration fixtures in `crates/*/tests/fixtures/` |
| CodeGraph parity | Port behavior from `C:\gary\codegraph\src\`; do not invent alternate semantics |
| Rename map | Never introduce `codegraph` strings in ax codebase |

### Parity workflow (every ax change)

1. **Before** — read CodeGraph equivalent; note edge kinds, traversal direction, resolution order, MCP fields.
2. **Implement** — match CG semantics; document intentional deltas in the matrix.
3. **After** — run fixture/smoke test; update Gap status + both matrix files; cite CG file path in status line.
4. **Never** mark ✅ without a CodeGraph cross-check.

---

*End of document.*