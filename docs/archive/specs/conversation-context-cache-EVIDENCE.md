# EVIDENCE: per-conversation context cache

Spec: `docs/specs/conversation-context-cache.md` (approved revisions 1–4, revisions 5–10 appended
during implementation and review, each with the test that forced it).

- **Spec approval:** obtained ("Approved, build it") for revisions 1–4 and the setup plan.
  Revisions 5–10 were added afterwards and are **not yet approved**; they tighten behavior
  (fewer hits, never more) and are listed below for review.
- **Tier:** 2, plus Tier 3 layers for the staleness risk (mutation tool, real-binary sessions,
  negative controls).
- **Source state:** branch `feat/conversation-context-cache`, commit `a292ad1`, worktree
  `/Users/gary/io/ax-ctxcache`. Its crates are byte-identical to `c449888`
  (`git diff --quiet c449888 a292ad1 -- crates` exits 0). Later commits changed only
  `scripts/context-cache-gauntlet.sh`.
- **Entry point:** `scripts/context-cache-gauntlet.sh`. It reruns every layer and fails closed.
- **Landing tree differs:** the worktree has none of the main tree's untracked or ignored files
  (`.ax/` index, `Naamloos.canvas`, uncommitted IDE work). The gauntlet builds its own fixture
  projects in temp dirs, with an isolated `HOME` and `AX_USAGE_DB`, so those files do not affect it.

## Result in one line

On a scripted 5-turn session, repeated graph calls cost **44% fewer tokens** (3,331 → 1,850).
After the `<ax_session_context>` block (175 tokens), **reuse saves 1,306 tokens, which is 17.6%
of the whole session**. Every repeat hit, every hit expands byte-for-byte to the original, and
an edit followed by `ax_sync` is always a miss.

## Savings (L5/L6, real `ax` binary over MCP stdio, o200k_base tokens)

| Measure | Cache off | Cache on | Change |
|---|---|---|---|
| Graph replies, 10 calls of which 5 repeats | 3,331 | 1,850 | −1,481 (−44.5%) |
| `<ax_session_context>` blocks in preflight | 0 | 175 | +175 |
| Net reuse saving, as share of the session | | | **−1,306 (17.6%)** |
| Whole session including all preflight | 7,437 | 6,776 | −661 (−8.9%) |
| L6 edit session (repeat, edit, sync, repeat) | 2,905 | 2,026 | −879 |

The "whole session" row understates reuse. `AX_CONTEXT_CACHE=off` also turns off the older
context-cache section of preflight (instruction line, ledger, catalog). That section accounts for
about 645 of the 820-token preflight difference. Three runs gave 44.5%, 44.2% and 43.9% graph
savings, varying only by a few tokens of telemetry text.

## Scenario → test mapping

| Scenario / clause | Verified by |
|---|---|
| Repeat call is a short hit; expand is byte-identical | L4 `second_identical_node_call_is_a_short_hit_and_expand_is_byte_identical`; L5 hit-expands check (3/3 runs) |
| Argument order and whitespace ignored; `fresh` ignored in key | L1 `key_ignores_key_order_whitespace_and_fresh`, `canonical_args_is_compact_sorted_json` |
| Different args, tool, or conversation is a miss | L1 `key_differs_for_conversation_tool_and_any_argument`; L5 second conversation: 0 hits |
| Cited file edited, deleted, or touched is a miss | L3 edit/delete/touch tests; L4 `edit_then_sync_is_a_miss_with_the_new_code`; L6 |
| Lagging index is not served after sync (rev 6) | L3 `reindexed_cited_file_invalidates_even_when_disk_is_unchanged`; L4 `edit_then_sync…` |
| Any index change invalidates (rev 7) | L3 `any_index_change_outside_the_cited_files_is_a_miss`, `reindex_of_an_uncited_file_is_a_miss`; L4 `new_caller_in_an_uncited_file_is_a_miss_after_sync` |
| More than 64 cited files: not cached (rev 8) | L2 `reply_citing_more_files_than_tracked_is_not_cached`, `limits_are_inclusive_at_the_boundary` |
| 200 tokens or less: not cached (rev 9) | L2 `reply_too_small_to_save_tokens_is_not_cached` |
| Preflight lists each entry once per session (rev 10) | L4 `preflight_lists_each_known_entry_once_per_session` |
| `fresh: true` reruns | L4 `fresh_true_reruns_the_tool`; schema test `cacheable_tools_advertise_fresh_and_others_do_not` |
| Only the 9 graph tools are cacheable | L1 `cacheable_tools_are_the_read_only_graph_tools` |
| Error replies never cached | Not observable: no cacheable tool sets `isError`. The guard stays in `server.rs` as defence; the hand mutant for it was replaced (see Mutation) |
| Cache off (`AX_CONTEXT_CACHE=off`) | L1 `ax_context_cache_off_disables_reuse`; L5 off arm: 0 hits; session-layer mutant `reuse_enabled -> true` killed |
| Size cap, oldest evicted | L2 size-cap tests (rows > 0 asserted) |
| Projects are scoped (rev 5) | L2 `same_call_in_another_project_misses_even_with_identical_files` |
| Corrupt rows, missing body, closed pool are misses | L3 `corrupt_rows_are_a_miss_not_an_error`, `lookup_misses_when_the_stored_body_is_gone`, closed-pool test |
| 16 concurrent stores of one key | L3 concurrency test: 1 row, body intact |
| Hit telemetry (hits, tokens avoided) | L4 hit-counter assertion `(1, original - sent)` |
| Seeded rules, skills, IDE templates name the cache | `every_seeded_surface_names_the_conversation_cache` (11 surfaces); `server_text_and_seeds_share_the_conversation_cache_sentence`; `conversation_cache_templates_bump_seed_version`; `sync_fix_brings_pre_cache_projects…` |
| Must not change non-cacheable or first-call replies | L5 miss replies equal the cache-off replies (same lines; see Known limits) |
| Must not break existing tests | Full suite: 477/477, 3 runs |
| No new runtime dependencies | `Cargo.lock` and manifests unchanged since `daf5772`; audit layer |
| L7 live-agent benchmark | **Blocked.** Authorized, and the harness `scripts/bench-agent-efficiency/reuse_live.py` is committed, but every `claude -p` call returns `Not logged in · Please run /login` (`terminal_reason: api_error`, zero tokens), with or without `--setting-sources project` |

## Gauntlet (fresh run on `a292ad1`, except cargo-mutants on identical crates `c449888`)

| Layer | Command | Result |
|---|---|---|
| Tool versions | `versions` | rustc 1.98.0, clippy 0.1.98, cargo-llvm-cov 0.9.1, cargo-mutants 27.1.0, cargo-audit 0.22.2, Python 3.14.7, tiktoken 0.14.0 |
| Tests, 3 runs | `cargo test -p ax-usage -p ax-mcp -p ax-policy` | 477 passed, 0 failed, in each of 3 runs |
| Clippy | `cargo clippy … --all-targets -- -D warnings` | 0 warnings |
| Changed-line coverage, minimum 90% | `cargo llvm-cov` + diff against `daf5772` | **195/199 = 98.0%**. Uncovered: 4 closing braces of untaken branches (`server.rs:346-347`; `tools.rs:687,691`, the no-policy-session path) |
| Mutation (tool) | `cargo mutants` on `reuse_cache.rs`, tests of ax-usage + ax-mcp | **126 tested: 120 caught, 0 missed, 6 unviable** (2 h) |
| Mutation (manual) | 6 hand mutants in server gate, store, schema, seeds | **6/6 killed**, restore verified with `git diff` |
| Session-layer mutant | `reuse_enabled -> true`, rebuilt binary, L5 | killed by "AX_CONTEXT_CACHE=off still served a hit" |
| Real execution L5/L6 | `reuse_session.py` × 3 | 0 failures; 5/5 hits; numbers above |
| Negative control | `reuse_session.py --control-no-edit` | fails as required (stale hit detected) |
| Supply chain | `cargo audit` head vs base lockfile | base 28, head 28, **new 0**. The gate's own control against an empty base reported 28 new, exit 1 |
| Secrets / capability | diff review | no secrets. New capability: reads cited project files and the project `files` table, writes `~/.ax/usage.db` (already used) |
| rustfmt | skipped | the repo is not rustfmt-clean and CI does not run it; `cargo fmt --all` rewrote ~300 unrelated files, which were reverted |

Mutants excluded from cargo-mutants, each with a reason:
- `indexed_of` (2 mutants): equivalent, because `index_fingerprint` covers every indexed path.
- `lookup` gate `||` → `&&`: equivalent, because the store never writes rows for non-cacheable
  tools.
- `reuse_enabled -> true`: reads the environment, so the session layer kills it instead.

The first mutation run had 23 survivors. They were killed with new tests for:
- the canonical compact-JSON form;
- `../` escapes to a file that exists outside the project;
- `path:abc` and dotless names;
- a missing stored body;
- the inclusive cap, the 64-file and token-cap boundaries;
- the hit counter.

## Review rounds

| Round | Skills (status) | Findings | Fixed |
|---|---|---|---|
| 1 | `old-coder` usable, `rust-review` usable, `python-review` usable | 7: R1-1 major (64-file truncation could hide an edit), R1-2 **blocker** (answer depends on uncited files → stale "no callers"), R1-3 major (tiny replies cost more as a hit), R1-4 minor (silent `let _`), R1-5 minor (harness mutated `os.environ`), R1-6 major (preflight resent the whole session block every turn), R1-7 minor (L4 tag assertion vacuous, the instruction line names the tag) | all; revisions 7–10 |
| 2 | same | 2 minor: R2-1 `session_entries` too public, R2-2 unused re-exports | both |
| 3 | same | 1 minor: R3-1 dead per-path branch in `indexed_hashes` (found by coverage) | fixed |
| 4 | same | 0 | — |

`old-coder-api`: not applicable (no HTTP surface). Every behavioral finding started with a RED
test seen failing. Each new test that went green right away was proven non-vacuous with a
throwaway mutant: index fingerprint, once-per-session filter, hit counter.

## Failures met and how they were resolved

- `AX_CONTEXT_CACHE=off` did not disable lookups, although the docs said it did. The L5 harness
  found this. Fixed with a RED unit test first.
- The L5 on arm first reused the off arm's MCP daemon (one daemon per project). The harness now
  gives each arm its own project.
- A pre-existing bug in the `ax-mcp` test helper: `ax_callers` takes `symbol`, not `name`. The
  harness and L4 tests were corrected.
- The hit-counter assertion flaked: tests share a usage DB, and identical replies share a
  `cache_id`. The query is now scoped to the test's project. Then 5/5 runs and 3 suite runs were
  clean.
- A git checkout lost an uncommitted token-savings doc paragraph. It was rewritten.

## Known limits

- **One active-session file for all Cursor chats.** Two chats at the same moment share an id. A
  hit is still verified fresh, and the body is reachable through `ax_expand`.
- **Any index change clears every hit for the project** (revision 7). With a watcher sync during
  active editing, entries live only until the next sync. This is the price of never serving a
  stale graph answer.
- **Two index builds can list equal-score callees in a different order.** This is a pre-existing
  nondeterminism, not caused by the cache. L5 therefore compares the off arm by its set of lines,
  and the on arm's hits byte-for-byte against its own first reply.
- **L7 (live agents, median input tokens −15%) blocked: the Claude CLI is not logged in.**
  Whether real agents re-ask often enough to benefit is not yet measured. After `claude /login`,
  run `AX_BIN=<ax> python3 scripts/bench-agent-efficiency/reuse_live.py --runs 5`. Design:
  - **Conversations:** 3 turns joined with `--resume`; turn 3 re-asks turn 1.
  - **Arms:** `AX_CONTEXT_CACHE` off vs on, alternating. Each arm has its own copy of `crates/ax-usage` and `crates/ax-mcp`, and its own daemon.
  - **Isolation:** the ax server gets an isolated `AX_HOME_DIR` holding the conversation id, plus its own `AX_USAGE_DB`. User-level Claude settings are skipped, so global ax hooks don't touch the real usage database or active session.
  - **Exit code:** nonzero unless the median drop is at least 15% and correctness doesn't fall.
  - **Known confound:** `off` also disables oversized-reply stubbing, not only reuse.
- **Pre-existing, out of scope:** `verify_content` flags `prefer-mcp-ops` without "ax_preflight",
  so `--fix` overwrites hand edits there. The `ax_index` watcher also queues thousands of
  `target-dev` object files as pending sync.
