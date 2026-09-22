# Evidence: MCP context cache

Spec: `docs/specs/mcp-context-cache.md`. Approval: user asked to implement the attached plan.

## Mapping

| Behavior | Check |
|---|---|
| Exempt preflight, guard, capture, expand | `exempt_tools_are_the_turn_contract` |
| Stub smaller, names `ax_expand`, sent count matches | `stub_is_smaller_and_names_expand` |
| Summary capped at 160 | `summary_is_first_line_capped_at_160` |
| offset/limit and past-end | `page_respects_offset_and_clamps_limit` |
| Store, reload, expiry | `store_roundtrip_and_expiry` |
| `ax_expand` advertised by default | `tool_filter` default catalog tests |

## Gauntlet

Command (after the last edit):

```
cargo test -p ax-usage --lib context_cache -- --test-threads=1
cargo test -p ax-mcp --lib tool_filter -- --test-threads=1
```

Result: ax-usage context_cache 5 passed, 0 failed. ax-mcp tool_filter 6 passed, 0 failed. `ax-mcp` lib compiled, so the server gate type-checks.

Skipped: full workspace suite (scope is these crates), mutation tool (none wired), coverage tool (none wired for this crate).

Known limit: the gate that swaps the live MCP reply is not executed inside a running MCP server in this run. It is the `cache_oversized_reply` match in `crates/ax-mcp/src/server.rs`, compiled with `ax-mcp`.
