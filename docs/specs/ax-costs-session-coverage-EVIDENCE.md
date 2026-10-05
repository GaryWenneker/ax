# Evidence: ax costs session coverage

Spec: `docs/specs/ax-costs-session-coverage.md`
Spec approval: not obtained as a separate sign-off. The request was the bug report.

## Mapping

| Behavior | Test |
|---|---|
| Empty usage events take models, agents, input tokens, and spend from sessions | `cost_report::tests::imported_sessions_fill_empty_cost_report` |
| Output and cache stay unknown | same |
| Known cycles stay 0 | same |
| Usage events are not overwritten by sessions | `cost_report::tests::usage_events_are_not_replaced_by_sessions` |
| Event rows still group by agent | same (`claude`) |
| Rollup sums input, groups one agent, keeps a dated spend | `savings::tests::session_cost_coverage_groups_models_and_agents` |
| Zero-length sessions contribute 0 minutes | same (`minutes == 0`) |
| A 90s span rounds up to 2 minutes; two 30s spans sum to 1 minute; a missing end or an end before the start adds 0 | `savings::tests::session_cost_coverage_rounds_spans_up_to_minutes` |
| Text report starts with the AX banner, then period, spend, model, and minutes | `cost_report::tests::session_report_leads_with_ax_banner_and_minutes` |
| Tokens, context efficiency, and cycle efficiency omit missing counts and show `Time` when minutes were measured | same |
| Cost/min and Input/min are printed; Output is not labeled with minutes | same |
| A non-zero cost per minute below half a cent keeps four decimals | `cost_report::tests::tiny_cost_per_minute_keeps_four_decimals` |
| Cyan is applied only to the six art lines, and only when color is requested | `cost_report::tests::color_cost_banner_paints_only_the_six_art_lines` |
| A report that is not session-sourced still prints `unknown` | `cost_report::tests::known_and_unknown_events_stay_separate` |

## Gauntlet

Final run after the last edit (`pub(crate)`, docs on the new structs, agent assertion):

```
cargo test -p ax-usage --lib cost_
```

5 passed, 0 failed (`pricing::tests::cost_math`, the three cost-report tests, the session coverage test).

Earlier, before that last assertion, the full crate suite was:

```
cargo test -p ax-usage --lib
```

191 passed, 0 failed. That run does not include the final assertion.

PATH binary after `scripts/reinstall-cli.sh` (POSIX shim `~/.local/bin/ax` → `target-dev/release/ax`, ax 7.0.0), against `~/.ax/usage.db`:

```
ax costs
```

Exit 0. Spent €3.23 (the session table's US$ 3.48 at the configured 1.08 USD per EUR). Models: grok-4.7 €2.80, claude-opus-5-5 €0.42, grok-4.7-fast €0.00. Agent: cursor €3.23. Input tokens: 1604804. Output and cache: unknown. Known cycles: 0. Tokens avoided: 36262874. Estimated savings: $54.39.

## Review rounds

| Round | Skills | Findings | Fixed |
|---|---|---|---|
| 1 | old-coder usable; rust-review missing from `ax_skill`, full text read from `.cursor/skills/rust-review/SKILL.md` | 1 minor: `session_cost_coverage` was `pub` | made `pub(crate)` |
| 2 | same | 0 | — |

## Import before the report

`ax costs` calls `import_agent_logs(true, true)` before `collect_report`.

RED: `cargo test -p ax-cli --bin ax run_imports_a_cursor_transcript_before_the_report -- --test-threads=1` failed with `models: []`.

GREEN: the same command passed (`1 passed`).

Live `ax costs today` after `scripts/reinstall-cli.sh` (10s, including the import): Spent €0.53, model grok-4.7, agent cursor, input tokens 285452. Output, cache, and known cycles stay unknown/0.

A fresh usage database also failed the savings totals query: `COALESCE(AVG(duration_ms), 0)` is an integer when no rows exist, and the query reads that column as `f64`. The fallback is now `0.0`. The RED run above is after that fix and before the import call.

## Limits

- A session with spend but no `started_at` counts in the total and not on a day, so the month projection can be lower than Spent.
- Output and cache columns on `agent_session_log` are not selected by the savings query. They stay unknown as token counts. When session minutes were measured, the text report shows that time instead of the word unknown.
- Mutation testing was not run.

## Banner and minutes

RED: `session_report_leads_with_ax_banner_and_minutes` failed because the report started with `AX COST SUMMARY` and the token, context, and cycle sections still said `unknown`. `tiny_cost_per_minute_keeps_four_decimals` then failed with `Cost/min $0.00` for a $0.50 spend over 201 minutes.

Final run after the last edit:

```
env -u CARGO_TARGET_DIR cargo test -p ax-usage --lib -- --test-threads=1
```

195 passed, 0 failed.

The Cursor import test `run_imports_a_cursor_transcript_before_the_report` passed (`1 passed`) before the four-decimal rate change. That change is inside `format_summary`, which the JSON path of that test does not print. The release build of `ax-cli` after the change succeeded.

`scripts/reinstall-cli.sh` installed a POSIX shim (`file` reports `POSIX shell script text executable, ASCII text`) at `~/.local/bin/ax` pointing at `target-dev/release/ax`, ax 7.0.0.

Live `ax costs today` after that install:

```
Today  ·  €0.58  ·  grok-4.7  ·  204 min
Input                  314200
Time                   204 min
Cost/min               €0.0029
Input/min              1540
Known cycles           0
```

The three sections do not contain the word unknown. Output and cache token counts are still not invented. A TTY gets the six art lines in bright cyan (`ESC[96m`); a pipe stays plain.

## Review rounds (banner)

| Round | Skills | Findings | Fixed |
|---|---|---|---|
| 1 | rust-review from `.cursor/skills/rust-review/SKILL.md` | Coloring inside `format_summary` depended on whether stdout was a TTY, so the banner test could flake. Cost/min at two decimals rounded a real rate to 0.00. | Color is `color_cost_banner(text, bool)`. Rates under 0.005 keep four decimals. |
| 2 | same | 0 | — |
