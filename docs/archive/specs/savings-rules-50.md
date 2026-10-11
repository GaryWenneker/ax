# SPEC: every savings task above 50% net

Revision of `turn_savings_positive` plan. Changed: CRITICAL rule bodies may be sent in compact form on first delivery (user choice "compact_critical", 2026-09-28). Tier 2.

## Behaviours

1. **Compact rule form.** An always-apply rule body above `AX_POLICY_RULE_INLINE_TOKENS` (default 250) is sent as: its `> **ABSOLUTE**` line, plus the sections whose heading is one of Rules, Required, Required workflow, Required on write, Required mapping, Hard rules, Forbidden, Scope, Thresholds, Correct shape (case-insensitive, `##` level), plus the line "Full rule: `ax_rules`". Everything else (Why, examples, explanation tables, other sections) is dropped.
   - `format_inject_block_with(... rule_inline_chars: Some(n))` on a 2,000-char rule with `## Why` and `## Forbidden` → output contains the Forbidden items and "ax_rules", not the Why text.
   - A rule with none of those headings keeps its first 3 non-empty lines plus the pointer.
   - Rules under the limit are unchanged.
   - Legacy `format_inject_block` (CLI) output stays byte-identical (existing test).
   - The delivered hash stays the hash of the full body, so a changed rule is resent.
2. **WARNING/INFO always-apply rules** follow the same compact form.
3. **Compact index block**: one line `Graph: N nodes, M edges, F code files; docs D`.
4. **Memory titles** default 200 tokens (`AX_PREFLIGHT_MEMORY_TITLE_TOKENS`).
5. **Gate**: the gauntlet fails when any passed task has net % below 50 (was: below 0).

## Must not

- Change any `without:` block or anchor (hash check stays on).
- Drop a rule entirely: every always-apply rule id still appears in the inject.
- Change `ax_rules` output: it stays the full body.

## Setup

No new dependencies. Files: `crates/ax-policy/src/format.rs`, `crates/ax-mcp/src/tools.rs`, `crates/ax-core` index formatter, `scripts/bench-agent-efficiency/gauntlet.py`, docs `site/src/content/docs/guides/token-savings.md`. Isolation: working tree (same as the rest of this uncommitted work).
