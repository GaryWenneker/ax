---
title: Smart Output
description: Compact agent context, structured metadata, project-bound expansion and honest token measurements.
---

Ax Smart Output is the default MCP response architecture. Ax chooses the representation; there is no XML/JSON/Markdown selector. Settings shows **MCP Output: Smart Output** as information, without claiming runtime cache status or token savings it cannot verify.

The remote MCP HTTP connection reuses this same dispatcher and output contract. Authentication and project selection happen before the tool executes. See [Remote MCP and browser login](/guides/remote-mcp/) for running Ax on one computer and retrieving its context from ChatGPT or another IDE.

## Response contract

The existing MCP envelope stays unchanged:

```json
{
  "content": [{ "type": "text", "text": "# Ax context\n\nProject: `/work/app`\n..." }],
  "structuredContent": {
    "project": { "path": "/work/app" },
    "session": { "id": "axs_example" },
    "matchedRules": 2,
    "guardRequired": true,
    "contextBudget": {
      "included": ["required", "memories"],
      "omitted": ["block:index"],
      "limit": 5000,
      "overBudget": false,
      "tokens": 1240,
      "measurement": "o200k_base"
    }
  },
  "isError": false
}
```

This is an illustrative response, not a measurement. Real identifiers, counts and status come from the tool. No common schema with invented fields is forced onto unrelated tools. Existing per-tool metadata fields remain meaningful. Null optional fields are omitted from the wire.

- **Text:** compact Markdown or plain text, independently actionable in clients that only read `content.text`.
- **Metadata:** counts, locations, stable IDs, scope, pagination and diagnostics. Preflight, recall, expansion, stash, session, durable, report and history do not repeat full bodies in metadata by default.
- **Source:** existing graph tools keep their numbered source and file references.
- **Large replies:** existing size thresholds, expiry and character pagination remain in use. Failed cache storage leaves the full reply inline. Small results stay inline.
- **Errors:** readable text and `isError: true`; failed operations do not become successful storage confirmations.

Session markers such as `ax_chat` and `ax_working_context` remain available because agents and existing clients use their IDs and hashes. Rule bodies, including embedded markup and exceptions, are preserved rather than rewritten by a generic XML converter.

## Preflight selection and budgets

Preflight identifies the actual project and chat. It preserves mandatory rule bodies, required workflows and working context. CRITICAL contextual rules are not dropped when the policy character limit is exceeded. Existing on-demand skill-loading semantics are preserved.

Unrelated memory-title dumps and the global cache catalog/ledger are no longer added. Prompt-matched memory IDs, relevant history and project/chat-scoped graph reuse remain available. Index context is brief; source-store warnings and pending changes remain actionable. Use `ax_status` for the full statistics.

The existing `context.budgetTokens` in project `ax.json` or global config controls optional blocks. Selection uses the shared tokenizer and drops whole blocks. Required content can exceed the budget: preflight reports `contextBudget.overBudget` and a visible warning rather than cutting mandatory rules. Token measurement is explicitly `o200k_base` or `estimate`.

```json
{ "context": { "budgetTokens": 5000 } }
```

Only pass `known_context` when the snapshot is still in the model's context. After compaction, call `ax_rules`, `ax_skill`, `ax_recall` or `ax_expand` to recover missing context; graph queries support `fresh: true`. A new session or MCP initialize resets policy delivery. Earlier delivery alone is not proof the model still holds the information.

### A normal agent turn

1. Call `ax_preflight` with the current task prompt and the retained chat session, if any.
2. Apply the mandatory policy and inspect freshness/budget warnings. Obtain the detailed index status with `ax_status` when needed.
3. Query the relevant symbols using `ax_explore`, `ax_search` or `ax_node`, and retrieve full matched memories by ID only when their details matter.
4. Expand cached results when the answer contains a cache handle. Keep the returned project/session ownership; foreign IDs cannot be used to switch projects.
5. Record durable working context. After client-side compaction, recover omitted policy, memories or graph answers before relying on them again.

Preflight skipping and client prompt caching are separate mechanisms. Ax omitting an unchanged body does not establish that an LLM provider bills a cached input rate, or that the client's compressed context retained that body.

## Project isolation and migration

The MCP server's project is authoritative. A different or unresolved `projectPath` produces a readable error: connect a separate server to that project. An invalid explicit MCP launch path no longer falls back to an unrelated working-directory project.

Cache IDs include the canonical project scope and body. Expansion checks stored ownership and expiry. A handle copied from another project cannot retrieve its body. Symlinked paths resolve to the same canonical project identity.

The existing usage database receives a nullable cache `project` column and an `ax_durable_scope` mapping table. These are additive, repeatable migrations. Old data stays stored; old unscoped cache handles and durable transcripts are **not** assigned guessed owners. Rerun the original tool or explicitly recreate a scoped transcript. This invalidation is deliberate and also applies with `AX_MCP_FULL=1`.

Durable MCP conversations are scoped by project and session. Fork/handoff children stay in that project; task resume/checkpoint/finish check the conversation owner. Working snapshots and graph reuse retain their existing project/session checks.

## Memories on demand

Free-text `ax_recall` returns ranked IDs and bounded summaries, with file provenance. It marks shortened summaries and instructs the agent to fetch full content. Retrieve a complete memory, including exceptions at the end, by ID:

```json
{ "id": "memory-id-from-the-search" }
```

Missing, disabled or foreign-project IDs fail explicitly. The old `query` and `limit` arguments remain supported. `AX_MCP_FULL=1` preserves raw structured matches for clients that need full machine-readable memory bodies.

## Compatibility and release

All tools and the MCP envelope remain. `AX_MCP_FULL=1` remains an explicit legacy structured-payload escape hatch, rather than a format selector. It does not disable ownership checks or change the new agent-facing text. This change does not alter graph export formats in the CLI or Command Center.

Consumers that parse policy XML wrappers, depend on full default recall bodies, or resume unscoped durable records must migrate. The implementation report recommends a major release for these default-contract changes; no tag or deployment is performed by the change itself.

## Measurements

Measure `content.text` tokens, structured bytes and complete response bytes separately. Client-specific serialization determines whether metadata is also part of model input. Full-response token counts are a wire-projection measurement, not billed API usage.

The repository benchmark uses three controlled preflight fixtures and records cold and repeated turns. It does not call a model, so generated output tokens and task-quality scores are unavailable. Keeping required rules can increase a cold preflight; do not trade away correctness to claim savings. See the implementation report under `docs/audits/2026-10-10-smart-output/` for results and limitations.
