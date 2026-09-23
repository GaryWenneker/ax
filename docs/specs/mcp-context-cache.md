# MCP context cache

Approved by the user request to implement the attached plan. Tier 2. Isolation: the existing checkout (the change is the product). No new dependencies beyond `sha2`, already used by `ax-mcp`.

## Behaviors

1. When an MCP tool reply's o200k token count is greater than or equal to `AX_CONTEXT_CACHE_TOKENS` (default 3000), ax stores the full UTF-8 body in `usage.db` table `mcp_context_cache` and the model-facing text is a stub.
2. The stub names the tool, the cache id (`cc_` + 16 hex chars of SHA-256 of the body), `original_tokens`, `sent_tokens`, `removed_tokens`, a one-line summary (first line, at most 160 chars), and tells the agent to call `ax_expand`.
3. `sent_tokens` is the token count of that stub. `removed_tokens = original_tokens - sent_tokens`. If the stub is not smaller, the original text is returned.
4. `ax_preflight`, `ax_guard`, `ax_policy_capture`, and `ax_expand` are never replaced by a stub.
5. `AX_CONTEXT_CACHE_TOKENS=0` or `AX_CONTEXT_CACHE=off` disables the cache. Replies under the threshold are unchanged.
6. `ax_expand` with `id` returns the stored body. `offset` and `limit` are character indexes. Default `limit` is 8000. `limit` is clamped to 12000. Unknown or expired ids return an error string and do not invent a body.
7. Rows expire 7 days after insert. A repeat store of the same body refreshes expiry.
8. A database error leaves the original reply in place.
9. When a reply is stubbed, the savings log records the stub size as `response_tokens_est` and adds `removed_tokens` to `tokens_saved_est` for savings-eligible tools.
10. When the cache is enabled, `ax_preflight` injects one line telling the agent that oversized replies are recoverable with `ax_expand`.

## Revision 2026-09-23: graph reads keep their head

Agents fell back to Read/Grep when an `ax_explore` answer became a bare stub (and in clients whose tool list lacked `ax_expand`). Behaviors 1–2 and 4 are revised:

11. Graph read tools (`ax_explore`, `ax_node`, `ax_search`, `ax_callers`, `ax_callees`, `ax_impact`, `ax_path`, `ax_cycles`, `ax_api`, `ax_context`, `ax_affected`, `ax_insights`, `ax_report`) use threshold `max(AX_CONTEXT_CACHE_TOKENS, AX_GRAPH_INLINE_TOKENS)`; `AX_GRAPH_INLINE_TOKENS` defaults to 12000.
12. Above it, the model sees a verbatim prefix of whole lines (a char prefix when the first line alone is too long) plus an `[ax context cache]` footer. The whole text is at most the threshold in tokens. The footer names the id, `shown_lines`, the exact `ax_expand` offset of the next line, `ax_node`, and says not to Read or Grep the files.
13. `ax_rules` and `ax_skill` (policy delivery) are never stubbed, alongside behavior 4's list.

## Must not

- Do not route provider API traffic through a proxy.
- Do not write file dumps into the memory vault.
- Do not drop preflight or guard text.
