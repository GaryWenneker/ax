---
name: savings-gauntlet
description: "Rerun the ax token-savings gauntlet (tasks without ax vs with ax over MCP), show the overview tables, and propose an improvement plan. Use when the user asks to rerun or check token savings, the savings gauntlet, or with-vs-without-ax numbers."
alwaysApply: false
triggers: ["savings","gauntlet","tokenbesparing","token savings","with vs without ax","rerun savings","savings gauntlet"]
tags: ["benchmark","tokens","savings"]
priority: 55
enabled: true
status: approved
scope: project
group: implementation-quality
---

# Savings gauntlet

Measures what an agent receives for ten tasks, first with plain file reads, then through the ax MCP server. Tokens are o200k. Net = without − with; a task only counts when its anchors pass.

## Workflow

1. **Status.** Call `ax_status`. If the index is stale, call `ax_sync` first. On macOS, rebuild with `CARGO_TARGET_DIR=$PWD/target-dev ./scripts/reinstall-cli.sh`, then `ax daemon restart`, so the MCP server runs the current code. A Cursor sandbox can redirect cargo's target dir; check that `target-dev/release/ax` has a fresh timestamp.
2. **Run.** `scripts/bench-agent-efficiency/run-savings-gauntlet.sh`. Exit code 1 means the gate is red. The live Claude arm is skipped when `claude` is missing or not logged in; that is expected.
3. **Tables.** From `scripts/bench-agent-efficiency/out/report.json` (or `out/summary.md`), show in chat:
   - per task: tier, feature, without, with ax, net, net %, previous net, what to change;
   - per feature: net and net %;
   - the gate result and every gate failure line;
   - **ALWAYS (absolute, user requirement): the full comparison table**, never only percentages or deltas. One row per task plus a **Total** row, with these columns: Task | Without | With ax (before) | With ax (now) | Net % (before) | Net % (now) | Change (points). "Before" is the last run before the current round of fixes, not merely the previous rerun. Take it from `out/report.prev.json`, or from the earlier summary when that file was overwritten, and say where the before-values came from.
4. **Improvement plan.** Write `docs/plans/savings-<YYYY-MM-DD>.md` (or a plan in Plan mode): for every task with the lowest net %, a failed anchor, or a drop against the previous net, name the product change, the file or tool it touches, and the expected token effect. Ask for approval. **Do not fix anything before the user approves the plan.**
5. **After an approved fix:** rerun step 2 and show the tables again, including the full comparison table from step 3.
6. Optional: `ax_remember` the totals (kind `note`, title `Savings gauntlet <date>`).

## Anti-gaming (absolute)

- Never edit a task's `without:` block or its anchors (`include`, `include_with`, `exclude_from_with`) to make a number green. They are hash-frozen in `without_hashes.json`; `--freeze` is only for an approved spec change.
- Fix the product, not the baseline. A test fixture that adds an anchor string to extra files inflates `without`; rename such fixtures.
- Never report a number from a run older than the last code change.
- A failed anchor counts as net 0 and fails the gate; do not hide it.

## Related

- [[macos-cursor-ax-mcp-binary]] — step 1 rebuild on this Mac