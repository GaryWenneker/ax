---
title: Memory vault
description: Durable project memory for agents — decisions, fixes, and conventions with hybrid recall and automatic preflight injection.
---

ax keeps a persistent memory vault per project: decisions, bug fixes, architecture notes, and conventions that agents (and humans) should never re-discover from scratch. Memories live in `.ax/ax.db` next to the code graph, work fully offline, and are automatically injected into agent context.

## How it works

- **Store** — via `ax remember`, the `ax_remember` MCP tool, the Command Center **Memory** page, or automatic git capture.
- **Recall** — hybrid search: SQLite FTS5 (BM25) fused with local vector embeddings via Reciprocal Rank Fusion. Typos and paraphrases still match; no cloud API or model download needed.
- **Inject** — `ax_preflight` recalls memories relevant to the user's prompt and injects the top matches into the agent's context every turn.
- **Decay** — confidence halves every 90 days unless a memory is touched again, so stale knowledge ranks lower instead of misleading agents.
- **Contradiction flagging** — storing a memory that is ≥80% similar to an existing one returns the near-duplicates, so agents can update instead of contradicting.

## Memory kinds

| Kind | Use for |
|---|---|
| `decision` | Why the team chose X over Y |
| `bug_fix` | Root cause + fix of a non-obvious bug |
| `architecture` | Structural knowledge (boundaries, data flow) |
| `convention` | Team rules that are not lintable |
| `note` | Everything else (default) |
| `git` | Auto-captured from commit history |

## CLI

```bash
ax remember "We use tiktoken o200k_base for token counts; chars/4 was 30% off" \
  --kind decision --tag tokenizer --file crates/ax-usage/src/tokenizer.rs

ax recall "why tiktoken"           # hybrid search, top 5
ax recall "tokenizer" --limit 10 --json

ax capture-git --limit 100         # mine recent commits into memories
```

`capture-git` skips merge commits, trivial messages (`wip`, `fix typo`), and commits already captured — safe to run repeatedly.

After `ax init`, **post-commit** and **post-merge** git hooks run `ax capture-git --quiet` automatically so commit messages land in the memory vault without manual steps.

## MCP tools

Agents get two tools from the ax MCP server:

- **`ax_remember`** — store a memory (`body`, optional `title`, `kind`, `tags`, `files`). The response includes similar existing memories so the agent can flag contradictions.
- **`ax_recall`** — free-text search returning scored matches.

Preflight injection means agents usually don't need to call `ax_recall` explicitly — relevant memories arrive with policy rules at turn start.

## Command Center

The **Memory** page in the Command Center (`ax ship` / `ax web`) lists all memories with live hybrid search, a composer for new memories, a one-click **Capture from git** action, and per-memory detail (kind, source, confidence, linked files).

## Storage

Everything is stored locally in the project's `.ax/ax.db` (SQLite): a `memories` table, an FTS5 index, and 256-dimension feature-hash embeddings as blobs. Nothing leaves your machine.

## Related

- [Policy Engine](/guides/policy-engine/) — rules and skills injected alongside memories
- [MCP server reference](/reference/mcp-server/)
- [`ax remember` CLI](/reference/cli/#ax-remember)
