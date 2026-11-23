# Fix: concurrent first opens of a new global database

Status: draft, revision 1, waiting for your approval. Nothing is implemented yet.

Tier 3: this is concurrency in shared database-open code.

## Problem

`ax_global_db::open_pool` in `crates/ax-global-db/src/lib.rs` creates a connection pool. Each new connection switches the file to WAL mode. When several opens reach a brand-new file at the same moment, one switch holds the lock and SQLite answers the others with `database is locked` (code 5) straight away, even though the busy timeout is 180 seconds. `open_pool` does not retry, so those opens fail.

`web_auth::db` opens the global database on every request and turns any error into `500 {"error":"Database unavailable"}`. ax-web tests build the router against a new global database, so parallel requests in a test can fail:

- 3 of 5 runs of the spec recheck gauntlet failed on it.
- `changes_feed` failed 3 of 25 standalone runs.
- 13 of 2,400 concurrent opens of new databases failed; 0 of 1,600 on an existing database.

In daily use the global database already exists, so the race mostly hits tests. A fresh install can also hit it once when several ax processes start together.

The project database open, `ax_db::Database::open`, already wraps the same pool creation in `ax_db::with_busy_retry`. The global database open does not.

## Change

Wrap the pool creation in `ax_global_db::open_pool` in `ax_db::with_busy_retry`, the same way `ax_db::Database::open` does. That retries only lock errors (and SQLite's one-time "schema changed" error), up to 12 attempts, 40 to 440 ms apart. Nothing else changes.

## Behaviors

1. Concurrent first opens all succeed.
   - Given a path where no database exists yet,
   - when 8 tasks call `open_and_init` on it at the same moment (they wait on a barrier first), repeated over N new paths,
   - then every call returns a pool, `PRAGMA journal_mode` on it says `wal`, and the `team_members` table exists.
   - Test: `concurrent_first_opens_of_a_new_global_db_all_succeed` in `crates/ax-global-db/src/lib.rs`.
   - N is chosen during RED so the test fails in at least 9 of 10 runs against today's code, in at most 10 seconds per run. The chosen N and the 10 RED results go in EVIDENCE.
2. An error that is not a lock error still fails at once.
   - Given a path that is an existing directory,
   - when `open_and_init` is called on it,
   - then it returns an error within 1 second. Going through all 12 attempts would take at least 2.6 seconds.
   - Test: `an_unopenable_global_db_path_fails_without_retrying`.
   - Today's code already behaves this way, so this test is expected to pass before the fix. It is kept as regression armor and proven non-vacuous with a throwaway change that retries every error.
3. The two ax-web test files that flaked now pass every time.
   - `changes_feed` and `dav_links` each pass 25 of 25 runs.
   - Before the fix, `changes_feed` failed 3 of 25.

## Must not

Each line is checked in EVIDENCE.

- Real failures keep the same answer: `500 Database unavailable`. Check: this fix does not change `crates/ax-web/src/web_auth.rs`.
- No existing test is changed, skipped, or given a retry. Check: the fix's diff only adds test lines.
- The live `.ax` database and `~/.ax/global.db` are not touched. Every test and run uses temporary folders, with `AX_GLOBAL_DB`, `AX_HOME_DIR`, `AX_USAGE_DB`, and `HOME` pointed into them.
- `ax-db` is not changed. Every crate shares it.
- The signatures of `open_pool` and `open_and_init` stay the same.
- No new dependency. `with_busy_retry` is in `ax-db`, which `ax-global-db` already uses.

## Failure model

| Failure mode | What catches it |
| --- | --- |
| The retry sits in the wrong place, so the race still fires | Behavior 1, mutation testing |
| The retry also hides or slows real errors | Behavior 2 |
| The retry never ends | `with_busy_retry` stops after 12 attempts (existing code); behavior 2 bounds the time |
| Schema setup races when two first opens run it together | Behavior 1 runs all of `open_and_init`. On a new file the base schema already has `level`, `avatar_image`, and the `approver` role, so the two check-then-alter helpers do nothing |
| Several processes open a new file at once | Not tested directly (known limit). The same retry covers it |
| Another process holds the database locked for minutes | Known limit: an open now waits up to 12 busy timeouts instead of one, which is how the project database open already behaves |

## Gauntlet

One script, `scripts/global-db-first-open-gauntlet.sh`, reruns every layer and stops at the first failure.

- **RED:** behavior 1 fails in at least 9 of 10 runs on today's code.
- **Suites:** `cargo test -p ax-global-db` and all 17 ax-web suites, with zero new failures.
- **Stress:**
  - the `changes_feed` and `dav_links` test binaries, 25 runs each, 0 failures;
  - behavior 1, 20 runs, 0 failures.
- **Mutation:** `cargo mutants` limited to the changed lines of `crates/ax-global-db/src/lib.rs`. Every mutant is caught, or classified as equivalent with the reason.
- **Coverage:** `cargo llvm-cov` on `ax-global-db`. The layer fails unless every changed line runs.
- **Lint:** `cargo clippy -D warnings` on `ax-global-db` and `ax-web`, and `rustfmt --check` on changed files.
- **Real run:**
  - Start `ax web` from a debug build in 10 new temporary homes on a temporary port.
  - Send 10 parallel requests to `/api/specs` right away and count answers that are not 200.
  - Run this before and after the fix. The before count may be 0 if startup creates the database first; it is reported either way.
- **Checker controls:**
  - The stress loop is shown failing on a command that fails.
  - The coverage check is shown failing on an uncovered line.
- **Dependencies:** no change, so there is nothing to audit.

## Setup plan

- **Isolation:** a git worktree from `HEAD` at `/Users/gary/io/ax-wt/global-db-first-open`, on a new local branch `fix/global-db-first-open`, with its own cargo target folder. RED, GREEN, and mutation run there.
  - Why: your main tree has 74 uncommitted paths from the overhaul. `crates/ax-global-db/src/lib.rs` is unchanged there, so the fix can be built and tested apart and then applied.
- **Landing:** after GREEN and mutation pass in the worktree, apply the same `lib.rs` diff to the main tree and run the full gauntlet and the real run there. The final EVIDENCE numbers come from the main tree.
- **Release binary:** the real run uses `target-dev/debug/ax`. The release binary that the `ax` shim and the live server use is not rebuilt.
- **Commits:** none, including on the worktree branch, unless you ask. This spec is not committed either.
- **Cleanup:** the worktree is removed with `git worktree remove` at the end if it contains no `.ax` folder; otherwise it is left for you.
- **Files added or changed:**
  - `crates/ax-global-db/src/lib.rs` (the fix and two tests)
  - `scripts/global-db-first-open-gauntlet.sh`
  - `docs/specs/global-db-first-open/EVIDENCE.md`
- **Tools:** `cargo-mutants` 27.1.0 and `cargo-llvm-cov` 0.9.1, both already installed. Nothing new is installed.

## Out of scope

- Opening the global pool once per process in `web_auth` instead of on every request. That is a performance change, and this fix does not need it.
- Rebuilding the release binary or restarting the live server.
- Other code that calls `ax_db::connect_options`.
