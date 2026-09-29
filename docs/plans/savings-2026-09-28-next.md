# Savings gauntlet: new tests and leaner context (plan, awaiting approval)

Measured on 2026-09-28: the first `ax_preflight` for "Which CRITICAL rules apply before writing crates/ax-mcp/src/tools.rs?" is 5,045 tokens; `ax_guard` for the same question is 9 tokens.

## Where the 5,045 tokens go

| Part | Tokens | Problem |
|---|---:|---|
| 14 CRITICAL always-apply rules (compacted) | ~3,100 | Rules for UI (wcag-contrast, outline-badges, frontend-production-build) and PR comments are sent for a Rust file write |
| Matched contextual rules | ~460 | codegraph-parity (457) plus c-errors and c-memory match a `.rs` file |
| Context catalog | 594 | First call of a new connection lists cache entries from earlier sessions, including other chats' prompts |
| old-coder skill summary | 252 | Fine; could shrink to description plus pointer |
| Memory titles | 187 | Fine; not needed when the prompt is about rules |
| Index snapshot | 114 | Mostly static, could be one line |
| Cache note, token-budget banner, review-language line | ~120 | Repeated boilerplate; review language duplicates the dutch-pr-comments rule |

## Logic changes (product, not baseline)

1. **Scope always-apply rules by file type.** A rule with `globs` or a `Scope` section that excludes the touched files is listed by id only ("not relevant to these files: …"). Expected: −1,000 to −1,500 tokens on Rust-file prompts.
2. **Tighter contextual matching.** A contextual rule needs a glob match on a touched file or at least two prompt terms, not one. C rules must not match `.rs`. Expected: −400.
3. **Session-scoped catalog only.** A new connection starts with an empty catalog; entries from other connections are never listed. Expected: −594 on first call.
4. **One-line index and no banner in preflight.** Index as `Graph: N nodes, M edges, F files`; drop the token-budget banner in preflight (it is the only reply the agent cannot shrink). Expected: −130.
5. **Intent-gated extras.** Memory titles only when the prompt is not a pure rules/guard question; review-language line only when the dutch-pr-comments rule is not already sent. Expected: −200.
6. **Guard-first answer for "which rules apply" prompts.** Preflight notices the guard intent and returns the ids and ABSOLUTE lines only. Expected: easy-rules and medium-guard from ~57% to ~75%.

## New gauntlet tasks (frozen `without:` blocks, anchors decided up front)

| Id | Feature | Question | Checks (anchors) |
|---|---|---|---|
| rules-frontend-file | rules | Rules before editing `web-ui/src/pages/Ship.tsx` | includes wcag-contrast and frontend-production-build; excludes c-errors |
| rules-precision-negative | rules | Rules before editing a `.rs` file | excludes wcag-contrast, outline-badges, c-errors, c-memory bodies |
| rules-changed-midsession | session | Two preflights with one rule body changed in between | second reply resends only the changed rule |
| skill-reload | skills | `ax_skill("old-coder")` twice in one session | second reply is a short "unchanged" notice |
| graph-path | graph | How does `serve_session` reach `format_inject_block_with`? | `ax_path` returns the chain; without = the files on that chain |
| graph-search | graph | Where is `content_hash` defined? | `ax_search` one line with path and lines |
| graph-ts | graph | Callers of a TypeScript function in web-ui | ax covers non-Rust code |
| memory-miss | memory | Recall a topic with no memory | reply under 40 tokens, no wrapper |
| memory-by-id | memory | Recall one memory by id | body only |
| session-10turns-filehop | session | Ten turns across Rust, TSX and docs files | total with-arm stays under 35% of without |
| live-replay | session | Last 20 real preflight prompts from the ax MCP call log, replayed | every reply's anchors pass; net % reported separately from the synthetic tasks |

`live-replay` is the "live data" arm: prompts come from real sessions, `without` is the full `.agents/` rules and skills an IDE would load, and the prompt set is frozen by hash when first captured so it cannot drift.

## Gate

The per-task and per-feature floor stays at 50%. New tasks start under the same floor; a new task below 50% is a finding for this plan, not a reason to lower the floor.

## Order

1. Add the new tasks, freeze their `without:` hashes, run once to record the baseline (expect several below 50%).
2. Fix 3 and 4 (smallest, safest), rerun.
3. Fix 1, 2 and 5, rerun.
4. Fix 6, rerun, and show the full comparison table.
