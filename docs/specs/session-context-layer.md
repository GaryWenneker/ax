# Plan: session context layer

Status: approved on 2026-10-02 ("Approved as written: build parts 1-6 in the planned order"). Branch `feat/conversation-context-cache`, worktree `/Users/gary/io/ax-ctxcache`.
Builds on the conversation cache (`conversation-context-cache.md`) and the working context (`working-context.md`).

## 1. Validation of the proposal against ax today

| Proposal | ax today | Verdict |
|---|---|---|
| Static context (graph, rules, skills, memory), cached long-term | Preflight sends `<ax_index>` only when it changes; rules and skills via inject; memory titles | Exists. Add a visible version line (part 6) |
| Session context with a session id | `ax_session` notes and the graph reuse cache, keyed by a conversation id | Exists, but **the id is broken in practice** (section 2). Fix first |
| `ax session create` returns `axs_…` | No explicit create | Adopt in a lighter form: preflight mints the id, the agent passes it back (part 1) |
| Every MCP call carries `session_id` | Not possible from the IDE; Cursor sends no conversation id to MCP servers | Adopt as an optional `session` argument that the agent copies from preflight |
| Conversation summary after about 5 turns | `ax_session compact` exists, but nothing prompts it | Adopt as a nudge. The agent writes the summary; ax has no model to write it (part 3) |
| `ax_context` with init/get/add/update/compact/clear | `ax_context` already builds task context; `ax_session` has get/add/update/compact/clear | Reject the name: renaming would break `ax_context` callers and seeded rules. Reject `init`: the first write creates the snapshot |
| Large replies become `context_id` + summary, `ax_expand` on demand | The context cache does this already, with ids and a ledger | Exists |
| Content-addressed ids: SHA-256 of project + graph version + rules version + content | Graph reuse: key = conversation + tool + canonical args, checked against file hashes and the index fingerprint. Notes: hash of the content only, with a separate stale flag | Keep the current split. If the graph version were part of the notes id, every `ax_sync` would make the notes vanish, although most notes stay true after an edit. The proposal's "no invalidation logic needed" holds for graph replies (a version change is a miss, as revision 7 does) but not for notes |
| Raw context on disk, small working context in the prompt | Raw: context cache + reuse cache. Working: `ax_session`, capped at 800 tokens | Exists |
| Do not store every agent output | Turn memories exist (`ax turn-hook`); `ax_session` stores only what the agent records | Agree. No automatic transcript capture |
| Provider cached input tokens (OpenAI) | Not in ax's control: in Cursor and Claude Code the IDE builds the model request, not ax | Docs only. Cached input lowers price and latency for a repeated prefix, but the tokens still fill the context window. ax's job is to keep the window small. A "Context Builder → OpenAI API" layer applies only if ax ever runs its own agent, so it is rejected for now |

## 2. Finding that changes the priority: the conversation id never reaches the cache

- The cache keys on `read_active_cursor_session()`, the file `~/.ax/active-cursor-session`.
- On this Mac that file does not exist. The only writer is `ax session-hook`, reached through `~/.cursor/hooks/ax-session-model.sh`, which hides every error with `2>/dev/null || true`.
- Of 272 MCP log lines between 2026-09-22 and 2026-10-01, **0** carried `session=`.
- The fallback is a per-process id, and tool calls run in the **per-project daemon** (`call_tool_and_wrap`; the stdio proxy only forwards lines). So in real use, "per conversation" means "per daemon lifetime". **Every chat and every window on the project shares one cache and one set of `ax_session` notes.**
- Freshness checks still prevent stale answers. But a new chat receives `[ax cache hit]` stubs for replies it never saw, and it sees notes from another chat.
- The L4 to L6 tests and the scripted sessions passed because they write the session file themselves. None of them covers the case where nothing writes it.

## 3. Plan

### Part 1: reliable session identity (bug fix, first)

1. `ax_preflight` accepts an optional `session`. If it is absent, ax mints `axs_<16 hex>` (random) and prints `session=<id>` in the reply, together with one sentence: pass it to later ax calls in this chat.
2. Every tool that reads or writes session state (the 9 graph tools, `ax_session`, preflight) accepts an optional `session`.
3. The id is resolved in this order:
   1. the `session` argument;
   2. the active-session file, when the hook wrote it in the last 10 minutes;
   3. the session last minted on this MCP connection. That fallback is per Cursor window, not per daemon.
4. `ax turn-hook start` (installed with an absolute binary path) also writes the active-session file from `conversation_id`. This makes step 2 work without the fragile shell script.
5. If the agent forgets the id, the chat starts cold. That is safe: a miss, never a wrong hit.

### Part 2: send the working context once per change

- Preflight accepts `known_context: "<hash>"`.
- If the hash equals the stored snapshot's hash and the snapshot is not stale, preflight sends one line, `<ax_working_context hash=X unchanged/>`, instead of the block of up to 800 tokens.
- After the IDE summarizes the chat, the agent no longer has the hash, so the full block comes back automatically.

### Part 3: conversation summary as a nudge

- ax counts turns per session (the `start` hook and preflight calls).
- After 5 turns without an `ax_session` write, or when the notes are stale, preflight adds one line: run `ax_session compact` with the objective, facts, files, decisions and open questions.
- ax does not summarize the turns itself. Extracting facts from reply text without a model would invent facts.

### Part 4: bounds and fail-closed (open gaps from the last check)

- Keep at most 200 snapshots per project and evict by `updated_at`. Rows older than 30 days are deleted on write.
- If the index cannot be read, `ax_session` add/update/compact is rejected with "index unavailable, retry after ax_sync". It does not store an empty fingerprint that can never go stale.

### Part 5: test gaps from the plan check

- L4: assert that the miss counter goes up on a miss.
- L4: assert latency, as "a hit does not call the tool". This is measured by a counter, not by wall-clock time, because timing tests flake.

### Part 6: visible static-context version

- Preflight prints `graph=<fingerprint16>` next to the snapshot hash, so the agent and the logs can see which index a reply came from.
- No rules version: inject is already sent again only when it changes.

### Docs

- README and site `reference/mcp-server.md`: `session`, `known_context`, the nudge, and the limits.
- `guides/token-savings.md`: provider cached input versus ax caches.
- Seeds: one sentence in the templates (pass `session` from preflight), bump `seedVersion` to 3, sync the repo copies.

## 4. Scenarios, simple to complex

| Level | Scenario | Expected |
|---|---|---|
| L1 | Preflight without `session` | Reply contains `session=axs_` + 16 hex; two calls mint two different ids |
| L1 | Resolution order | Argument beats the file; a file older than 10 minutes is ignored; otherwise the connection's last session |
| L2 | `known_context` equals the hash and the notes are fresh | One `unchanged` line; no block |
| L2 | `known_context` equals the hash but the notes are stale, or the hash differs, or none is sent | Full block |
| L2 | 201st snapshot | The oldest is evicted; the count stays at 200 |
| L2 | Index unreadable | add/update/compact rejected; stored row unchanged |
| L3 | 5 turns without a write | Preflight shows the compact nudge; after a write it is gone |
| L4 | Two chats on one daemon, no session file, different minted ids | Chat B: 0 hits and no notes from chat A |
| L4 | Same `session` across a daemon restart | Graph hits and notes are reused (stored in usage.db) |
| L4 | Miss counter and "hit does not run the tool" | Both asserted |
| L5 | Scripted 8-turn session with `session` and `known_context` | Repeat hits as before; the working-context block is sent only on turns where it changed; tokens reported per turn |
| L6 | Edit + sync mid-session | Graph repeat misses; notes marked stale; full block sent once; compact confirms |
| L7 | Live agents (claude CLI) | Still blocked until `claude /login` |

Must not change:
- Existing tests, and the `ax_context` behavior.
- Calls without `session` keep working.
- No new runtime dependencies.

## 5. Setup plan

- Same branch and worktree, no new dependencies.
- Files: `crates/ax-usage/src/{reuse_cache.rs, working_context.rs, store.rs, cursor_state.rs}`, `crates/ax-mcp/src/{tools.rs, server.rs}`, `crates/ax-cli/src/commands/turn_hook.rs`, seed templates, docs, `scripts/bench-agent-efficiency/reuse_session.py`.
- Commit the spec at approval, then at each green checkpoint. No PR, no merge.
- Gauntlet: `scripts/context-cache-gauntlet.sh` (tests 3×, clippy, changed-line coverage, cargo-mutants on the changed modules, L5/L6, audit), then the review loop until a round has zero findings.

## Revisions during implementation

- **R1 (part 3):** turns are counted from preflight calls only, not from the start hook as well. The hook runs in a separate process, and counting both would count every turn twice. The count lives in daemon memory, so a daemon restart resets it; it only drives a nudge. The nudge shows on the 6th preflight after the last write, that is, after 5 turns without one.
- **R2 (part 1):** `session` minting lives in `ax-mcp` (`chat_session.rs`), because `ax-usage` has no `uuid` dependency and the spec allows no new dependencies.
- **R3 (part 1):** policy-body dedupe in preflight (which rules were already delivered) still keys on the connection and the hook file, not on the new session id. Changing it would resend every rule body whenever an agent forgets to pass `session`. That is out of scope here; it is listed as a known limit.

- **R4 (part 5):** the design has no miss counter; `mcp_reuse_cache` counts hits only. The L4 test instead asserts what a miss does: the tool runs again (a test-only run counter per project and tool goes up), and the stored row is replaced (`hits` back to 0, a new `cache_id`). "A hit does not run the tool" is asserted with the same counter.

- **R5 (part 6):** `graph=<16 hex>` is printed in the `<ax_chat>` line, not next to the snapshot hash. A chat without notes has no snapshot block, and the version should show on every preflight. It is the first 16 hex characters of the index fingerprint, the same value that marks notes stale.

## 6. Order and expected effect

1. Part 1 makes the existing cache correct in real chats. Without it, the measured savings exist only in tests that write the session file.
2. Part 4 closes the open gaps.
3. Part 2 saves up to 800 tokens per turn once notes exist.
4. Parts 3, 5 and 6 are small.
