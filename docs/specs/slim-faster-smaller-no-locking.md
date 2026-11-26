# Headless (slim) build: faster, smaller, no locking — plan

Status: **DRAFT — not approved.** This is a research result and a proposal. No code change in this plan is committed. Date: 2026-11-26. Baseline: `main` at `8c55a710` (ax 10.1.1).

Scope: the headless build (`ax-cli --no-default-features --features "minimal-grammars mimalloc"`, profile `release-slim`). Most fixes live in shared crates, so the full build benefits too.

## Implementation status (2026-11-26)

The user asked to start executing the plan. The plan stays a draft and has not been approved as a spec. Implemented on `main`, not pushed:

| Item | Commit | Result | Verified by |
|---|---|---|---|
| L1, L2, L3 | `eca86af4` | `ax.lock` is never deleted; stale detection asks `flock`, not the PID; `kill(2)` instead of spawning `kill`; lock released on finalize errors | new unit tests (fail on 10.1.1 code); `scripts/slim-concurrency.sh` FAILs on 10.1.1, PASSes now |
| L5 | `9785608b` | busy or unflushed WAL no longer records an incident that switches the watcher off | new integration test (fails before, passes after) |
| F1 | `1677d2a6` | 20.6 → 17.4 s | identical edge, node and unresolved-row hashes |
| F2 + L4 | `c1ed3dd1` | WAL after index with MCP attached 13.8 → 0.75 MB; busy retries capped at one busy timeout in total | unit test for the retry budget; WAL measured |
| F3, step 1 | `57fc5156` | resolved edges in one transaction: 17.4 → 14.3 s | identical hashes |
| F3, step 2 | `3049db34` | node lookups memoized in the resolve loop: 14.3 → **7.0 s**, peak RSS 224 → 266 MB | identical hashes |

Not done:
- **L4 `BEGIN IMMEDIATE`** for the read-then-write transactions in `ax-usage`, `ax-global-db` and `ax-mcp/remote/oauth.rs`: off the index path; with L1 fixed, the reproduced 517 error no longer occurs.
- **L6** (network filesystems): open decision 3. Docs now say to keep `.ax/` on local disk.
- **L7** (MCP catch-up sync holding the engine mutex): follow-up. It changes daemon start-up ordering and needs an MCP-level test.
- **S1, S3, S4, S5**: decisions for the user (download format, GitHub features, time zones, languages).
- **S2**: dropped, see the table.
- **Linux container measurements**: not re-run, because Podman storage on the build machine is damaged.

## Measurement setup

- Machine: macOS, Apple silicon, 14 cores, 48 GB.
- Benchmark project: a copy of this repository's `crates/`, with 660 indexed files, about 98,000 references to resolve, a 54 MB `ax.db`, and 10,499 nodes and 53,066 edges after indexing.
- Binary: the slim default (`bin-slim-mimalloc`, 31.3 MB) unless stated otherwise.
- Index times: `ax index . --force`, median of three runs.
- Linux numbers come from the earlier v10.1.1 work, in `slim/README.md`. Podman storage on this machine is damaged (a repair that needs your decision is pending), so no new container runs were possible this round.

## Summary ranked by gain against risk

| # | Change | Kind | Gain (measured or estimated) | Risk | Recommendation |
|---|---|---|---|---|---|
| L1 | `FileLock::release` must not delete a lock it does not hold, and the lock file is never unlinked | locking | fixes a reproduced double-writer bug | low | **do first** |
| L2 | Stale-lock check without spawning `kill`, and no deletion of live lock files | locking | fixes lock loss in distroless and across PID namespaces | low | **do first** |
| L3 | RAII lock guard so error paths release `ax.lock` | locking | fixes a lock held for good after a failed sync in long-lived processes | low | do |
| F1 | Delete resolved references in one transaction | speed | **20.5 → 17.4 s (−15%)**, sys time 15.3 → 12.9 s (measured) | low | do |
| S1 | Ship `.tar.xz` instead of `.tar.gz` | size (download) | **12.0 → 7.9 MB** download for arm64 (measured) | low | do |
| L4 | Writers use `BEGIN IMMEDIATE`; cap total retry time | locking | removes `(code: 517) database is locked` (reproduced) and 36-minute worst-case waits | low–medium | do |
| L5 | Integrity marker: no false `wal_uncheckpointed` / `writers_active` incidents | locking | stops the watcher from being switched off after a container restart | low | do |
| F2 | Truncate the WAL after a full index and set `journal_size_limit` | speed / disk | WAL 143 MB → ~0 after index (measured 143 MB next to a 54 MB db) | low | do |
| L6 | Detect network and shared filesystems and do not use WAL there | locking | prevents "database is locked" and corruption on Azure Files/NFS | medium | do, with a clear message |
| F3 | Resolve against an in-memory symbol index instead of one SQLite round trip per lookup | speed | estimated resolution 13 s → 2–4 s, total ~17 s → ~7 s (not measured) | medium | do after F1, behind tests |
| L7 | MCP: tool calls must not wait for a catch-up sync | locking | removes multi-second tool stalls at daemon start | medium | do |
| S2 | Drop `aws-lc-rs` (one TLS crypto backend) | size | ~0.5–0.8 MB (estimate from bloat) | — | **dropped**: `ax mcp remote serve` verifies RS256 tokens; jsonwebtoken's only other backend pulls in the `rsa` crate (RUSTSEC-2023-0071) and is not clearly smaller |
| S3 | Optional `github` feature (octocrab) | size | ~0.6–1.0 MB (estimate) | low; costs GitHub PR features | your decision |
| S4 | Drop or gate `chrono-tz` | size | ~1 MB (estimate) | low | do, if time zones are not needed in slim |
| S5 | Grammar sets per use (for example without Ruby and PHP) | size | Ruby 2.1 MB + PHP 2.1 MB of parse tables (measured per archive) | low; costs languages | your decision |
| S6 | `opt-level = "z"` | size | −4.4 MB, but indexing +12% (measured) | — | **not recommended** |
| S7 | UPX-compressed binary | size | ~33 → ~9 MB on disk (xz proxy) but +~0.3 s on every start | — | **not recommended** |
| F4 | PGO | speed | user CPU is not the bottleneck | — | **not now** |

## 1. No locking

What "locking" covers here:
1. Cross-process: `.ax/ax.lock`, the writer lock.
2. SQLite: `database is locked` / `SQLITE_BUSY`.
3. In-process waits: mutexes and async locks.

Two findings were reproduced on this machine. The rest come from reading the code; the references are below.

### Reproduction (macOS, slim binary, benchmark project)

```text
t=3.0   ax index --force running, .ax/ax.lock exists, contains its PID (68031)
t=3.1   ran `ax status` → .ax/ax.lock is GONE
t=17.5  `ax sync` (started right after) ran *concurrently* with the index instead of waiting,
        and failed: "database error: (code: 517) database is locked"
t=26.7  index finished
```

`ax status` during an index answered in 0.06 s at 4, 9 and 14 s into the index. So plain readers are not blocked by WAL; the problems are with writers.

### L1. The lock file is deleted by processes that do not hold it (reproduced)

- `crates/ax-utils/src/file_lock.rs`, `release()` (around line 135):
  - it always calls `std::fs::remove_file(&self.path)`, even when `self.file` is `None`, meaning this instance never held the lock;
  - `Drop` calls `release()`.
- Every short-lived `Ax` (`ax status`, `ax query`, and the MCP `ensure_policy_fresh` swap in `crates/ax-mcp/src/engine.rs:177-189`) therefore unlinks the indexer's lock file. The next writer creates a new inode and gets its own exclusive `flock`, so two writers run at once.
- Even a correct release that unlinks has a race. A waiter that already opened the old inode gets the lock while a third process creates a new file and locks that one.
- **Fix:**
  - never unlink `ax.lock`; keep the file and use `flock` on that one stable inode;
  - `release()` only unlocks when it holds the lock;
  - the PID stamp stays informational only, for messages and `ax unlock`.
- **Verify:**
  - a new integration test in `crates/ax-utils/tests/` that spawns two child processes. Process A holds the lock, B creates and drops an unheld `FileLock`, and C's `try_acquire` must still fail;
  - the shell reproduction above must show `ax sync` waiting (the lock file stays), then completing without error.

### L2. Stale-lock detection deletes live locks in containers

- `clear_stale_lock` (`file_lock.rs:160-171`) deletes the file when the PID is unreadable or `is_pid_alive` is false.
- `is_pid_alive` (`crates/ax-utils/src/process.rs:194-200`) spawns `kill -0`:
  - the distroless image has no `kill` binary, so every PID counts as dead;
  - across PID namespaces (two containers sharing a volume) the PID is meaningless;
  - `EPERM` (another user's process) counts as dead;
  - an empty file in the window between `open` and `stamp_pid` counts as stale.
- It runs on every `Database::open` (`crates/ax-db/src/lib.rs:110`), `Ax` open (`crates/ax-core/src/lib.rs:156`), policy store open (`crates/ax-policy/src/store.rs:759`) and every acquire retry.
- The kernel releases a `flock` when its holder dies, so a stale *file* never blocks anyone. Deleting it is never needed.
- **Fix:**
  - remove the automatic `clear_stale_lock` calls; keep it only for an explicit `ax unlock`;
  - replace the `kill` spawn with `libc::kill(pid, 0)` (or `rustix`), treating `EPERM` as alive.
- **Verify:**
  - a unit test on `is_pid_alive(1)` and on our own PID with no `kill` on `PATH`;
  - the L1 test, run inside the distroless image.

### L3. The lock stays held after an error

- `index_all`, `sync` and `index_files_locked` use `?` on `finalize_after_extract` (`crates/ax-core/src/lib.rs:250`, `297`, `377`), which skips the `release()` at `:255`, `:306`, `:382`.
- In a long-lived MCP or watch process the lock is then held until the process exits, and every other writer waits 180 s and fails.
- **Fix:** acquire returns a guard; release happens in `Drop` of the guard, not by hand.
- **Verify:** a test that injects a failing finalize step and then acquires the lock from a second `FileLock`.

### L4. `SQLITE_BUSY_SNAPSHOT` and runaway waits

- **Code 517** (seen in the reproduction) is `SQLITE_BUSY_SNAPSHOT`: a deferred transaction that read first and then tried to write after another writer committed. `busy_timeout` cannot fix this; only a retry of the whole transaction can.
- Read-then-write deferred transactions:
  - `crates/ax-usage/src/spec_hierarchy.rs:382`, `529`, `800`, `1283`
  - `crates/ax-global-db/src/access.rs:333`, `policy.rs:404`
  - `crates/ax-mcp/src/remote/oauth.rs:102`
- On the index path, L1 is the cause; with one writer at a time it does not happen.
- `with_busy_retry` (`crates/ax-db/src/lib.rs:79-96`) runs 12 attempts on top of a 180 s `busy_timeout`. That is up to ~36 minutes per step, which looks like a hang.
- `Database::open` always writes (`INSERT OR IGNORE INTO schema_versions` in `crates/ax-db/src/schema.sql:10`, applied on every open via `schema.rs:60-89`). So read-only commands also need the write lock for a moment.
- **Fix:**
  - write transactions start with `BEGIN IMMEDIATE`; in sqlx 0.8 that means `pool.begin_with("BEGIN IMMEDIATE")`, or an explicit statement on a pooled connection;
  - `with_busy_retry` gets a total deadline equal to `busy_timeout`, not 12× it;
  - skip the schema write when `PRAGMA user_version` (or the `schema_versions` row) is already current.
- **Verify:**
  - a two-process test: a writer loop plus `ax sync` plus an MCP `tools/call` loop for 60 s gives zero `database is locked` errors;
  - `ax status` on a database held by `BEGIN EXCLUSIVE` returns read data (or a clear message within the timeout) instead of waiting minutes.

### L5. Integrity incident marker switches the watcher off by mistake

- `run_quick_check` (`crates/ax-db/src/integrity.rs:409-445`) runs at MCP daemon start (`crates/ax-mcp/src/daemon.rs:514-516`), forced after an unclean shutdown such as a killed container, while that daemon's own catch-up sync is writing.
- `wal_uncheckpointed` comes from `wal_has_frames` ("`-wal` larger than 32 bytes", `integrity.rs:858-863`). After a PASSIVE checkpoint the file never shrinks, so this is almost always true. A 400 ms `busy_timeout` hit becomes `writers_active`.
- Both record an incident. `writers_blocked()` then makes `spawn_background_watch` skip until someone runs `ax db repair` (`crates/ax-core/src/lib.rs:439-441`).
- **Fix:**
  - judge the WAL from `PRAGMA wal_checkpoint(PASSIVE)` results (`busy`, `log`, `checkpointed`), not the file size;
  - treat `writers_active` as a transient warning and retry later; it is not an incident;
  - run the quick check after the catch-up sync, not during it.
- **Verify:** kill the daemon (`kill -9`) during an index, restart it, and check that `.ax/integrity.json` reports `ok: true` and the watcher runs. Run this in the existing `crates/ax-db/tests/integrity.rs` style, plus one process-level test.

### L6. WAL on network and shared filesystems

- WAL needs a shared-memory `-shm` file and working POSIX locks. On SMB/Azure Files, NFS and some FUSE mounts, that gives either false `database is locked`, or no locking at all (with `nobrl`), which risks corruption.
- Nothing in the code checks for this: no `statfs`, and no `locking_mode`, `mmap_size` or `journal_size_limit` set anywhere.
- The earlier measurement also showed a macOS→Podman bind mount taking about 124 s, against about 36 s on the container disk.
- **Fix:**
  - in `Database::open`, call `statfs` on `.ax/`. On `nfs`, `cifs`/`smb2`, `fuse.*` or `9p`/`virtiofs`, fall back to `journal_mode=DELETE`, warn once, and print the advice "keep `.ax/` on local disk".
  - `AX_DB_JOURNAL=wal|delete` overrides the choice.
  - In the Azure job script and docs, put `.ax` on an `EmptyDir` volume, not an Azure Files share.
- **Verify:**
  - a unit test with a mocked filesystem type;
  - a manual run on an SMB mount: two processes (an index loop plus a status loop) show no lock errors with `DELETE`.

### L7. In-process waits in the MCP server

- The engine wraps `Ax` in a tokio `Mutex` (`crates/ax-mcp/src/engine.rs:62`). `run_catch_up_sync` holds it across `ax.sync()` (`:159-175`), and a sync can turn into a full `index_all` after an extraction-version change. Every tool call waits meanwhile.
- `index_mutex` (tokio) is held across the blocking file-lock wait, whose `std::thread::sleep` blocks a runtime worker (`crates/ax-utils/src/file_lock.rs:65`).
- The MCP process runs two `Ax` instances, one for the engine and one for the watcher (`engine.rs:108`), each with its own pool and `FileLock`.
- No std `Mutex` is held across `.await`, and `block_on` is only used at CLI entry points.
- **Fix:**
  - read-only tools use a read pool that does not take the engine mutex;
  - catch-up sync runs on the watcher's single writer;
  - the file-lock wait goes to `spawn_blocking` or an async poll.
- **Verify:** MCP `tools/call ax_explore` latency during a forced re-index stays under 200 ms. Today it waits for the whole sync.

## 2. Faster

### Where the time goes now (macOS, slim default)

| Phase | Time |
|---|---|
| Scan + parse (parallel) | ~0.8–2.0 s |
| Write nodes/edges | ~4.2 s |
| Resolve cross-references | **~16.8 s** |
| Optimize (ANALYZE + checkpoint) | < 0.1 s |
| Total | 20.5 s wall, 11.0 s user, 15.3 s sys |

- Peak RSS: 230 MB during a full index; 80 MB for `ax sync`.
- `/usr/bin/time -l` counted 3.2 million involuntary context switches.
- A `sample` of the resolve phase shows threads mostly waiting (`__psynch_cvwait`, `semaphore_wait_trap`). The work is `pwrite`, `fsync` and `pread`.

**Why:** `ReferenceResolver::resolve_all` (`crates/ax-resolution/src/resolver.rs:40-183`) walks about 98,000 references one by one.
- Each lookup is one or more async sqlx queries, and each query is a round trip to sqlx's SQLite worker thread.
- Each resolved reference was deleted in its own autocommit transaction (`queries.delete_unresolved_ref`, `crates/ax-db/src/queries.rs:940`). That is ~44,000 commits for the benchmark project (98,353 found, 54,185 still unresolved).
- The SQLite work is cheap; the thread hand-offs and commits are not.

### F1. Batch the deletes (measured)

- Experiment, run in a scratch worktree and not committed:
  - collect resolved references in a `Vec`;
  - delete them in one transaction at the end of `resolve_all`, with a new `QueryBuilder::delete_unresolved_refs_batch`.
- Result, three alternating runs each:

| Binary | Wall (s) | User (s) | Sys (s) |
|---|---|---|---|
| Slim default | 20.51 / 20.20 / 20.67 | 11.0 | 15.3 |
| Batched delete | 18.08 / 17.59 / 17.36 | 10.3 | 12.9 |

- The result graph is the same: 10,499 nodes, 53,066 edges and 54,185 unresolved references in both runs.
- Change:
  - `crates/ax-db/src/queries.rs`: add the batch delete, using a prepared statement in one transaction;
  - `crates/ax-resolution/src/resolver.rs`: both loops push to `to_delete`.
- Better still: delete with a temporary table and one `DELETE … WHERE rowid IN (…)`.
- **Verify:**
  - `ax-resolution` tests;
  - the same node, edge and unresolved counts on the benchmark;
  - a benchmark under 18 s.

### F2. WAL size after indexing (measured)

- After a full index the `-wal` file is 143 MB next to a 54 MB `ax.db`.
- `wal_checkpoint(PASSIVE)` (`crates/ax-db/src/lib.rs:184`) copies the frames back but never shrinks the file. In a sandbox with limited disk, that is 2.7× the database size again.
- **Fix:**
  - after a full index, try `wal_checkpoint(TRUNCATE)` with a short busy timeout, and fall back to PASSIVE if readers are present;
  - set `PRAGMA journal_size_limit = 67108864` (64 MB) on every connection.
- **Verify:** the `-wal` file is under 64 MB after `ax index --force`, and still a single file with MCP readers attached.

### F3. In-memory symbol index for resolution (estimate)

- Load the node names the resolver needs once, into `HashMap<name, Vec<candidate>>` per kind, at the start of `resolve_all`. Then resolve without a SQLite round trip per lookup.
- Inserts of resolved edges are already batched elsewhere.
- Once lookups are pure CPU, resolution can run in parallel per file with rayon.
- Estimated: resolution from ~13 s (after F1) to 2–4 s, so a full index of ~7 s instead of ~17 s, with more gain on Linux, where sys time is higher. Not measured: this needs care with framework resolvers (`framework_resolve::try_resolve`) that run their own queries.
- Files:
  - `crates/ax-resolution/src/name_matcher.rs`, `resolver.rs`;
  - a new `symbol_index.rs`;
  - `crates/ax-db/src/queries.rs`: one bulk load query.
- Memory: ~10k nodes on this project means a few MB. For 1M nodes, cap the size and fall back to SQLite.
- **Verify:**
  - edge and unresolved counts identical on the benchmark, and on a PHP/Laravel and a TypeScript fixture;
  - the resolution tests (34);
  - a benchmark target under 10 s.

### F4. Not worth it now

- **PGO:** user CPU is ~10 s of a 17–20 s run, and most of that is SQLite plus thread hand-offs. Fix F1 and F3 first, then re-measure.
- **Per-crate `opt-level = 3`:** measured last round at 20.3–20.5 s for +0.8 to +7.2 MB. No gain.
- **`target-cpu`:** CPU is not the bottleneck, and x86-64-v3 can crash with SIGILL on older hosts.
- **Thread scaling:** not measured this round. Parsing is already parallel and short (≤ 2 s); resolution is single-threaded and will only scale after F3.

### What a sandbox user feels today (already fast)

| Operation | Time |
|---|---|
| `ax --version` | 7 ms |
| `ax status` | 61 ms |
| `ax query` / `ax callers` | 76–81 ms |
| MCP `initialize` | 8 ms warm; 112 ms first start (cold page cache) |
| MCP `tools/list` (39 tools) | 1 ms |
| First `ax_explore` | 15 ms warm; 100 ms cold |
| `ax sync`, one changed file | 0.54–0.63 s, 80 MB RSS |
| `ax sync`, nothing changed | 0.08 s |

Start-up needs no work. The full index (`ax init` / `ax index`) is the only slow path.

## 3. Smaller

### Where the 31.3 MB (macOS) / 33.2 MB (Linux arm64) goes

From `size -m` and `cargo bloat` on an unstripped slim build:

| Part | Size | Notes |
|---|---|---|
| Machine code (`__text`) | 13.0 MiB | spread over many crates, see below |
| Read-only data (`__const`) | ~15.0 MB | mostly tree-sitter parse tables |
| — core grammars | ~10.9 MB | TypeScript+TSX 2.86, Ruby 2.11, PHP 2.10, Rust 1.12, C 0.63, Python 0.45, Java 0.41, JS 0.36, Go 0.22, others < 0.2 each |
| Embedded templates (`include_str!`) | ~0.9 MB | policy skill templates (ax-policy 0.79 MB) |
| Other data and unwind | ~2 MB | chrono-tz tables (estimated ~1 MB), `mime` tables 0.33 MB, `aws-lc` precomputed tables 0.15 MB |

Largest code contributors (`cargo bloat --crates`):
- the ax crates together, about 6 MB: `ax` 1.2, `ax_mcp` 1.2, `ax_usage` 0.78, `ax_policy` 0.57, `ax_core` 0.43, `ax_memory` 0.42, `ax_resolution` 0.36, `ax_neural` 0.34, `ax_remote` 0.34, `ax_db` 0.24;
- `std` 0.91;
- regex 0.43 (`regex_automata` + `regex_syntax`), plus `fancy_regex` 0.11;
- TLS/HTTP about 1.3: `aws_lc_sys` 0.31, `rustls` 0.28, `ring` 0.10, `octocrab` 0.19, `reqwest` 0.19, `hyper`/`hyper_util`/`axum` 0.25;
- `clap_builder` 0.15, plus 0.3 MB of derive code for the command tree;
- 2.5 MiB that cargo bloat cannot attribute (C code: SQLite, tree-sitter lexers, zstd).

### Candidates

| Item | Saving | Functionality cost | Files |
|---|---|---|---|
| **S1** `.tar.xz` release archives | download 12.0 → 7.9 MB (measured `xz -9e`) | none; needs `xz` in the installer, with a fallback to `.tar.gz` | `scripts/package-slim-release.sh`, `install-slim.sh`, `release.yml` |
| **S2** one crypto backend: `jsonwebtoken` without `aws_lc_rs`; octocrab with `default-features = false` and rustls-ring | ~0.5–0.8 MB (aws-lc code plus tables) | none | `crates/ax-mcp/Cargo.toml`, `crates/ax-remote/Cargo.toml`, workspace `Cargo.toml` |
| **S3** `github` feature for octocrab (ax-remote/ax-ship) | ~0.6–1.0 MB | `ax ship` GitHub PR features and remote GitHub calls exit with "not in this build" | `crates/ax-remote`, `crates/ax-ship`, `crates/ax-cli/Cargo.toml` |
| **S4** `chrono-tz` in ax-usage behind a feature (UTC or local offset only) | ~1 MB (estimate) | named time zones in usage reports | `crates/ax-usage` |
| **S5** grammar profiles: `grammars-web` (TS/JS/PHP/Vue/Svelte/Astro/Twig/Liquid), `grammars-systems` (Rust/C/Go/Python/Java), and so on | up to ~4 MB per left-out pair (Ruby 2.1, PHP 2.1) | those languages get files but no symbols | `crates/ax-extraction/Cargo.toml`, `src/languages` |
| **S6** `opt-level = "z"` | −4.4 MB | +12% index time (23.0 s vs 20.6 s, measured) | profile |
| **S7** UPX | on disk ~33 → ~9 MB (xz proxy: 7.9 MB; zstd -19: 9.2 MB) | +~0.27 s to decompress on **every** start, and the memory is not shared between processes. MCP and CLI start often, and `ax status` takes 61 ms today. | — |
| **S8** fat LTO on Linux | already used in CI (33.2 vs 36.0 MB with thin LTO) | — | — |

Expected result with S2+S3+S4 (no language loss): about 29 MB on macOS and ~31 MB on Linux arm64; with S5 (no Ruby/PHP), about 25 MB. Downloads with S1: about 7–8 MB. The estimates are from bloat attribution and still need a build to confirm.

## Verification for the whole plan

- **Gauntlet as before:**
  - `cargo build --workspace`;
  - `cargo clippy --workspace --all-targets -- -D warnings`;
  - clippy on the slim feature set;
  - tests for `ax-cli` (full and slim), `ax-extraction`, `ax-resolution`, `ax-db` and `ax-utils`.
- **New concurrency test**, `scripts/slim-concurrency.sh`, run in the distroless and Ubuntu images: in one container, start `ax index --force`, then run in parallel for 60 s:
  - an `ax status` loop;
  - an MCP `ax_explore` loop;
  - an `ax sync` after a file change;
  - one `kill -9` of the indexer followed by a restart.
  - Pass:
    - no `database is locked`;
    - no two writers at once (the PID log shows the lock order);
    - `integrity.json` stays `ok`;
    - the final counts equal those of a clean index.
- **Benchmarks:** `slim/README.md` tables updated (macOS and Linux container), and the same node, edge and unresolved counts.

## Open decisions for you

1. **Languages (S5):** keep all core grammars, or offer grammar profiles? Dropping Ruby and PHP saves about 4 MB; dropping PHP hurts Laravel/Drupal users.
2. **GitHub features in slim (S3):** may `ax ship`'s GitHub PR features leave the headless build for about 0.6–1 MB?
3. **Network filesystems (L6):** on SMB/NFS, fall back to `journal_mode=DELETE` with a warning (proposed), or refuse to open and require local disk?
4. **Release archives (S1):** switch the slim assets to `.tar.xz`, or publish both formats for one release?
5. **Order:** proposed L1–L3, then F1 + F2 + L4 + L5 (one PR), then L6 + L7, then F3, then S1–S4. Locking first, because it is a correctness bug in every build, not only slim.
