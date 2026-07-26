---
title: Memory Vault
description: Durable project memory for agents — decisions, fixes, and conventions with hybrid recall and automatic preflight injection.
---

![Memory vault — browse, search, and inspect stored memories with hybrid recall](/screenshots/cc-memory-vault.png)

ax keeps a persistent memory vault per project: decisions, bug fixes, architecture notes, and conventions that agents (and humans) should never re-discover from scratch. Memories live in `.ax/ax.db` next to the code graph, work fully offline, and are automatically injected into agent context.

## Why Memory matters

Without memory, every agent session starts from zero. An agent might re-derive a decision you already made, propose a pattern you already rejected, or miss context about why code was written a certain way. The memory vault closes that gap: it captures the **why** behind changes and replays it automatically when relevant.

## How it works — overview

```
INGEST (write path)                              RECALL (read path)
                                                  
git post-commit ──► ax capture-git ──┐            ax_preflight (every turn)
git post-merge  ──► ax capture-git ──┤              │
CLI: ax remember ────────────────────┤              ▼
MCP: ax_remember ────────────────────┤  embed()   recall_for_prompt(prompt, 3)
UI: "New memory" ────────────────────┼──────►       │
                                     │            ┌─┴─────────────────────┐
                                  ax.db           │ FTS5 BM25 leg         │
                                  memories +      │ Vector cosine leg     │
                                  memories_fts    │ Reciprocal Rank       │
                                  + embedding     │ Fusion + decay(90d)   │
                                                  └───────────────────────┘
                                                    │
                                                    ▼
                                                  <ax_memories> block
                                                  injected into agent context
```

### Three ingest paths

1. **Automatic — git commits.** Post-commit and post-merge hooks run `ax capture-git` after every commit. Non-trivial commit messages become `kind: git` memories linked to the files they touched. Trivial subjects (merges, bumps, WIPs, typos, formatting, anything under 12 characters) are skipped. IDs are derived from the commit hash (`git-{hash12}`) so re-running never duplicates.

2. **Manual — you or your agent.** Click "New memory" in the Command Center, run `ax remember` from the CLI, or let agents store knowledge by calling `ax_remember` via MCP. All three paths run duplicate detection after saving.

3. **Injected — every agent turn.** When an agent calls `ax_preflight` (mandatory at the start of every turn per ax policy), the user's prompt is matched against all memories using hybrid search. The top 3 matches are formatted as an `<ax_memories>` XML block and injected into the agent's context alongside policy rules. The agent sees them automatically — no manual recall needed.

## Memory kinds

| Kind | Use for |
|---|---|
| `decision` | Why the team chose X over Y |
| `bug_fix` | Root cause + fix of a non-obvious bug |
| `architecture` | Structural knowledge (boundaries, data flow) |
| `convention` | Team rules that are not lintable |
| `note` | Everything else (default) |
| `git` | Auto-captured from commit history |

## The recall algorithm

Recall uses **hybrid search** — two independent ranking methods fused together, then weighted by time decay:

### Step 1 — FTS5 full-text search (lexical leg)

The query is tokenized, stop words are removed (English and Dutch), and each remaining token (minimum 3 characters) is quoted and joined with `OR`. This runs against a [FTS5](https://www.sqlite.org/fts5.html) virtual table that auto-syncs via triggers on every INSERT, UPDATE, and DELETE. Results are ranked by BM25.

### Step 2 — Vector similarity (semantic leg)

The query is embedded locally into a fixed 256-dimensional vector. **Default:** feature hashing (FNV-1a tokens + character trigrams) — no API calls, no model download. **Optional:** build with `--features onnx` and place a MiniLM `.onnx` at `AX_ONNX_MODEL` or `~/.ax/models/all-MiniLM-L6-v2.onnx` for dense neural embeddings (projected to 256-d for storage compatibility). Stored embeddings are compared via cosine similarity. Matches below 0.1 similarity are discarded. Legs are fused with weighted RRF (vector 0.5 / FTS 0.3 / graph reserved 0.2).

### Step 3 — Reciprocal Rank Fusion (RRF)

Both ranked lists are fused using standard [Reciprocal Rank Fusion](https://plg.uwaterloo.ca/~gvcormac/cormacksigir09-rrf.pdf) with k=60:

```
score(memory) = 1/(k + rank_fts + 1) + 1/(k + rank_vector + 1)
```

A memory that ranks high in both legs gets a strong combined score. A memory that only matches one leg still surfaces, but with a lower score.

### Step 4 — Confidence decay

The fused score is multiplied by the memory's **effective confidence**, which decays over time:

```
effective = confidence * 0.5 ^ (age_days / 90)
```

Confidence starts at 1.0 when a memory is created or updated. It halves every 90 days. This means:
- A 1-day-old memory retains ~99% of its confidence
- A 90-day-old memory retains ~50%
- A 180-day-old memory retains ~25%

Updating or re-saving a memory resets its confidence to 1.0. This prevents stale, outdated knowledge from dominating agent context while keeping fresh memories highly ranked.

## Automatic injection via `ax_preflight`

This is the mechanism that makes memories useful without agent action:

1. Every agent turn starts with `ax_preflight(prompt, files)` (mandatory per ax policy).
2. Preflight calls `recall_for_prompt(prompt, 3)` — the full hybrid recall pipeline with a limit of 3.
3. Matches with score <= 0.0 are filtered out.
4. Remaining matches are formatted as an `<ax_memories>` XML block (max 6,000 characters).
5. The block is appended to the inject string alongside policy rules.

The agent receives something like:

```xml
<ax_memories note="Durable project memories matched for this prompt
— treat as established context.">
### [decision] We use tiktoken o200k_base for token counts

Switched from chars/4 because it was 30% off on real prompts.

Files: crates/ax-usage/src/tokenizer.rs

### [bug_fix] Sonar proxy strips Content-Encoding after decompression

The proxy was sending compressed bodies with Content-Encoding removed,
causing double-decompression in some clients.

Files: crates/ax-web/src/sonar_proxy.rs

</ax_memories>
```

The agent treats these as established context — it never has to ask "have we decided this before?" because the answer arrives automatically.

## Explicit recall via `ax_recall`

Besides automatic injection, agents can explicitly search memories when they need deeper history:

- **MCP tool:** `ax_recall` with a `query` string and optional `limit` (up to 25, default 5). Returns scored matches plus a formatted inject block (up to 12,000 characters — double the preflight budget).
- **CLI:** `ax recall "query"` for interactive use.
- **Command Center:** The search box in the Memory page runs the same hybrid search.

Use explicit recall when you want to check "have we solved this before?" with more results than preflight delivers.

## Contradiction detection

When a new memory is stored (via any path), ax runs `find_similar` with a cosine threshold of 0.80 against all existing memories. If near-duplicates are found:

- **MCP:** The response includes `similar[]` with the matching memories and a note asking the agent to check for contradictions.
- **CLI:** Prints "Similar existing memories (possible duplicate/contradiction)" with similarity percentages.
- **Command Center:** Shows a toast warning with the count of similar memories.

This prevents the vault from accumulating contradictory knowledge — when a decision changes, the agent can delete or update the old memory instead of creating a conflicting one.

## The embedding model

The embedding model is entirely local — no API calls, no model files to download, fully deterministic across platforms:

- **Feature hashing:** Each word token is hashed (FNV-1a) into one of 256 dimensions with weight 2.0. Character trigrams (3-char sliding windows) are hashed with weight 1.0 for typo and morphology tolerance.
- **Sign randomization:** A second hash bit determines the sign of each feature, reducing collision bias.
- **L2 normalization:** The final vector is normalized to unit length, so cosine similarity equals the dot product.

Feature-hash embeddings give deterministic, typo-tolerant similarity that fuses well with FTS5. Optional ONNX dense embeddings replace `embed_text()` without changing the storage format (the 256-dim blob column stays the same). Command Center **Settings → Embeddings** shows the live backend (`hash` / `onnx` / `onnx_unconfigured`), Cargo feature flag, and model / tokenizer paths (`GET /api/memory/embed-status`). See [`docs/ONNX.md`](https://github.com/GaryWenneker/ax/blob/main/docs/ONNX.md).

## Storage schema

Everything is stored locally in `.ax/ax.db` (SQLite):

| Table | Purpose |
|---|---|
| `memories` | Main table: id, kind, title, body, tags (JSON), files (JSON), confidence, source, timestamps, embedding (BLOB) |
| `memories_fts` | FTS5 virtual table indexing id, title, body, tags — auto-synced via triggers |

Triggers keep the FTS5 index in sync automatically on INSERT, UPDATE, and DELETE — no manual reindexing needed.

### Source field values

| Source | Created by |
|---|---|
| `manual` | CLI `ax remember` or Command Center "New memory" |
| `mcp` | Agent calling `ax_remember` via MCP |
| `git` | `ax capture-git` (hooks or manual) |

## Git auto-capture

After `ax init`, git hooks are installed that capture memories automatically:

| Hook | Command | Behavior |
|---|---|---|
| `post-commit` | `ax capture-git --limit 1 --quiet` | Captures the latest commit |
| `post-merge` | `ax capture-git --limit 20 --quiet` | Captures recent merged commits |

The capture process:
1. Runs `git log --no-merges` with the specified limit
2. Parses each commit's hash, subject, body, timestamp, and touched files (up to 20 per commit)
3. Skips trivial subjects: anything under 12 characters, or starting with "wip", "merge", "bump", "typo", "format", or "lint"
4. Uses `INSERT OR IGNORE` with deterministic IDs (`git-{first 12 chars of hash}`), so re-running is idempotent
5. Git memories start at confidence 0.8 (slightly lower than manual 1.0)

You can also run `ax capture-git` manually or click "Capture from git" in the Command Center to backfill history.

## CLI

```bash
# Store a decision
ax remember "We use tiktoken o200k_base for token counts; chars/4 was 30% off" \
  --kind decision --tag tokenizer --file crates/ax-usage/src/tokenizer.rs

# Search memories
ax recall "why tiktoken"
ax recall "tokenizer" --limit 10 --json

# Capture git history
ax capture-git --limit 100
ax capture-git --limit 1 --quiet    # same as the post-commit hook
```

See the [CLI reference](/reference/cli/#memory-vault) for all flags and options.

## MCP tools

Agents get two tools from the ax MCP server:

- **`ax_remember`** — store a memory (`body`, optional `title`, `kind`, `tags`, `files`). Source is set to `mcp`. The response includes similar existing memories so the agent can flag contradictions.
- **`ax_recall`** — free-text search returning scored matches and a formatted inject block.

Preflight injection means agents usually don't need to call `ax_recall` explicitly — relevant memories arrive with policy rules at turn start.

The MCP server's `initialize` response includes guidance for agents:

> *Memory vault: when you make a durable decision, fix a tricky bug, or establish a convention, store it with `ax_remember`. Use `ax_recall` to search past decisions before re-deriving them. Relevant memories are auto-injected via `ax_preflight`.*

## Command Center

The **Memory** page in the Command Center (`ax ship --watch` or `ax web`) provides:

- **Stats strip** — total memories, git-captured count, manual/agent count, search mode indicator
- **Memory vault list** — all memories sorted by recency, with kind icons, age, tags, and source badges
- **Live hybrid search** — the search box runs the same recall algorithm agents use; scores shown per result
- **Detail panel** — click a memory to see full content, kind, source, confidence percentage, linked files, and a delete button
- **New memory modal** — composer with title, body, kind selector, and duplicate warning on save
- **Capture from git** — one-click button to mine recent git history

## Best practices

- **Store decisions, not facts.** "We use X because Y" is a good memory. "X is a library" is not — the agent can find that in code.
- **Include the why.** A memory saying "use tiktoken" is less useful than "use tiktoken o200k_base because chars/4 was 30% off on real prompts."
- **Link files.** Memories with file paths are more likely to surface when agents work on those files.
- **Clean up contradictions.** When a decision changes, delete or update the old memory. The duplicate warning helps, but periodic review keeps the vault sharp.
- **Let git hooks capture automatically.** After `ax init`, commit messages flow into the vault without manual steps. Write meaningful commit messages and they become agent context for free.
- **Use kinds.** Categorizing memories as `decision`, `bug_fix`, `architecture`, or `convention` helps both humans and recall ranking.

## Related

- [Policy Engine](/guides/policy-engine/) — rules and skills injected alongside memories
- [MCP server reference](/reference/mcp-server/) — `ax_remember` and `ax_recall` tool schemas
- [CLI reference](/reference/cli/#memory-vault) — `ax remember`, `ax recall`, `ax capture-git`
