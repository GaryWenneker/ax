# Live chase light and cache status

Tier 2. Isolation: this working tree. The release the user asked for tags this checkout, so a separate worktree would not be the tree that ships.

Spec approval: the user asked to implement this behavior and, when it is done, bump, build, package, deploy, update docs, and release the site (2026-10-03). The written spec was not approved in a separate message.

## Behaviors

1. A row marked new (logging, memory, and the other lists that use `live-new`) does not paint a background or box-shadow, and does not replace the title color. A copy of the title and subtitle (`::after` with `content: attr(data-chase)`) shows a narrow light moving across those glyphs (`mask-image`, keyframes `live-new-chase`). Table cells are not clipped. `prefers-reduced-motion` shows the original text and does not animate.
2. `ax_cache_status` can be called at any time. It returns two single-line records and no stored body, file path, or file contents:
   - `cache group=<key> lane=context ...` with enabled, threshold, live rows, stored tokens, expired rows, and, when a session id is known, that session's row and token counts.
   - `cache group=<key> lane=token ...` with file-token cache entries, capacity, hits, misses, evictions, and whether the tokenizer is available.
3. The group key is the session id with only ASCII letters, digits, `_`, and `-` kept (max 64). An empty result is `global`. Both lines of one call share that key.
4. When verbose MCP logging is on, those two lines are written to the same trace as other tool lines. A newly stored context-cache body also writes `cache group=<key> lane=context event=store id=cc_<16 hex> original_tokens=… sent_tokens=… removed_tokens=…` and does not write the body. A bad id is not logged.
5. On the Logging page, two or more visible lines that share a `cache group=` key get the same node hue. Adjacent ones are joined by a rail. A single line, or a line that is not a cache status line, gets no rail.
6. A context-database failure still returns the token line. The context line says `error=unavailable` and does not include the database error text. A poisoned token-cache lock says `error=lock`.
7. `ax_cache_status` is never replaced by a context-cache stub.

## Must not

- Do not return or log a cached body, a file path, or file bytes from status or the store line.
- Do not paint the old `live-new-glow` background on new rows.
- Do not join unrelated log lines with the cache rail.

## Setup

No new dependencies. Tests: `cargo test -p ax-usage --lib cache_status`, `cargo test -p ax-usage --lib tokenizer`, `cargo test -p ax-usage --lib context_cache`, `cargo test -p ax-mcp --lib tool_filter`, `node --test` for `traceGroups.test.ts` and `liveChase.test.ts`. Web production build: `npm run build` in `crates/ax-web/web-ui`.
