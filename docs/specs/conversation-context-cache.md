# Spec: per-conversation context cache

Plan: `~/.cursor/plans/conversation_context_cache_257a0bc3.plan.md`. Tier 3 (a stale hit fails silently).

## Goal

Within one agent conversation, a read-only graph call that was already answered is not
answered again in full. ax returns a short reference to the earlier answer, and that answer
stays retrievable byte-for-byte. Preflight lists what the conversation already knows.

## Revisions against the plan (visible, not silent)

1. **A hit returns a reference, not the body.** The plan said "return the stored body". That
   saves no tokens, because the agent receives the same body again. A hit returns
   `[ax cache hit] ... id=<id>` (well under 100 tokens) and the full body via `ax_expand id`.
   Equivalence is proven on the expanded body, not on the hit text.
2. **Invalidation is a content-hash recheck on every lookup** instead of marking rows stale in
   `ax_sync`. Each entry records the files its reply cites, with their SHA-256. A lookup
   rehashes them, so a missed sync cannot fail open. No sync hook is needed.
3. **A new table `mcp_reuse_cache`** instead of more columns on `mcp_session_index`, so the
   existing ledger and catalog keep working unchanged.
4. **Turn number** = count of `ax_preflight` events recorded for the conversation.

### Revisions found during implementation (appended, approved spec unchanged above)

5. **Entries are scoped to conversation + project root.** Found by the L4 suite: the same call
   in two projects with identical files shared one entry. Regression test
   `same_call_in_another_project_misses_even_with_identical_files`.
6. **Freshness also checks the indexed `content_hash`** (project `files` table), not only the
   disk hash. Found by L4 `edit_then_sync_is_a_miss_with_the_new_code`: a call made after an edit
   but before `ax_sync` returns old indexed source, which was stored against the new disk hash
   and then served after the sync. A hit now requires disk hash and indexed hash both unchanged.

```gherkin
Scenario: reply produced from a lagging index is not served after sync
  Given F was edited on disk but not yet synced, and ax_node returned old indexed source
  When ax_sync runs and the same call is repeated
  Then it is a miss and the reply shows the new code
```

## Behavior (scenarios)

```gherkin
Scenario: repeat graph call in the same conversation is a hit
  Given conversation "c1" called ax_node {"symbol":"open_pool"} and got body B citing file F
  And F is unchanged
  When "c1" calls ax_node {"symbol":"open_pool"} again
  Then the reply starts with "[ax cache hit]" and names an id
  And the reply is shorter than B in tokens
  And ax_expand(id) returns exactly B

Scenario: argument order and whitespace do not matter
  When "c1" calls ax_node {"file":"a.rs","symbol":"x"} after {"symbol":"x","file":"a.rs"}
  Then it is a hit

Scenario: any changed argument is a miss
  When "c1" calls ax_node {"symbol":"y"} after {"symbol":"x"}
  Then it is a miss and the tool runs

Scenario: another conversation never hits
  When conversation "c2" makes the same call as "c1"
  Then it is a miss

Scenario: edited file invalidates
  Given the cached entry cites F
  When F's content changes (no ax_sync)
  Then the next identical call is a miss, the tool runs, and the new body is stored

Scenario: touched but unchanged file still hits
  When F's mtime changes but its content does not
  Then the next identical call is a hit

Scenario: deleted cited file invalidates
  When F is deleted
  Then the next identical call is a miss

Scenario: reply that cites no file is never cached
  Then the call is a miss on every repeat (fail closed: nothing to verify freshness against)

Scenario: fresh:true bypasses
  When "c1" calls ax_node {"symbol":"open_pool","fresh":true}
  Then the tool runs, and "fresh" is not part of the cache key

Scenario: only read-only graph tools are cached
  Then ax_explore, ax_search, ax_node, ax_callers, ax_callees, ax_impact, ax_path, ax_affected, ax_context are cacheable
  And ax_preflight, ax_guard, ax_sync, ax_remember, ax_policy_capture, ax_expand, ax_stash, ax_rules, ax_skill are never stored or served

Scenario: error replies are never cached

Scenario: cache off
  Given AX_CONTEXT_CACHE=off
  Then no entry is stored and no hit is served

Scenario: size cap
  Given a per-conversation cap of N bytes (AX_REUSE_CACHE_BYTES, default 2 MB)
  When stored bodies exceed N
  Then oldest entries are evicted first and the total never exceeds N

Scenario: corrupt or missing row
  When the stored row is unreadable or its files_json is invalid
  Then the call is a miss, the tool runs, and the call does not error

Scenario: concurrent identical calls
  When 16 identical store operations run in parallel for one key
  Then exactly one row exists for that key and its body is one of the stored bodies, intact

Scenario: no conversation id
  Given no active Cursor session file
  Then the key uses a per-process id, so caching works for one MCP connection only

Scenario: preflight digest
  Given "c1" has fresh entries
  When "c1" calls ax_preflight
  Then inject contains <ax_session_context> with one line per entry (tool, args summary, cited files, id)
  And stale entries are not listed, the block stays under 1,500 tokens, and other conversations' entries are absent

Scenario: telemetry
  Then each hit records tokens_avoided = tokens(B) - tokens(hit text), matching the tokenizer
```

## Must NOT

- change replies of non-cacheable tools, or of a first (miss) call
- break existing `context_cache` tests, the ledger, or the catalog
- add runtime dependencies
- serve a hit when any cited file differs, is missing, or unreadable

## Seeded policy

Update the seeded templates listed in the plan (rules `explore-before-grep`, `prefer-mcp-ops`,
`subagents`; skills `startup`, `subagents`; IDE templates for Cursor, Claude, Continue, Cline;
the generated AGENTS block; the MCP instruction text and the preflight cache line) with the
same key sentence: *"A repeated graph call in this conversation returns a short `[ax cache hit]`
reference; the answer is already in your context or in `ax_expand id`. Read
`<ax_session_context>` before searching again; pass `fresh: true` to force a new query."*
Bump `seedVersion` where templates carry one. Tests: every template contains the key sentence
(negative control: delete it from one), and `tools.rs` carries the same sentence (drift test).

## Test ladder

L1 pure functions, L2 store, L3 invalidation and robustness, L4 MCP-level gate, L5 scripted
multi-turn cache off vs on, L6 edit scenario, L7 live-agent benchmark (median input tokens
-15%, success not lower; reported `not performed` if no agent API is available).

## Setup plan

- Isolation: git worktree `../ax-ctxcache` on branch `feat/conversation-context-cache` from
  HEAD, sharing `target-dev` through `CARGO_TARGET_DIR` to avoid a cold build. Your current
  uncommitted files are not touched.
- Commits: spec at approval, then one per green checkpoint, on that branch only.
- New files: `crates/ax-usage/src/reuse_cache.rs`, `scripts/context-cache-gauntlet.sh`,
  `scripts/bench-agent-efficiency/reuse_session.py`, this spec, the EVIDENCE file.
- Dev tools: `cargo-mutants` (mutation testing), `cargo-llvm-cov` (changed-line coverage),
  `cargo-audit`, each installed via `cargo install` only if missing; versions recorded.
- No new crate dependencies (sha2, serde_json, sqlx, tokio are already present).
