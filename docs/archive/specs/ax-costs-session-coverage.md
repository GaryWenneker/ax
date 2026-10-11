# ax costs shows imported session models and agents

## Problem

`ax costs` rolls up `agent_usage_event`. That table can be empty while Command Center already lists imported Cursor and Claude sessions from `agent_session_log` (agent, model, input tokens, catalog cost).

## Behavior

1. When the selected period has no quoted usage events, and imported sessions exist, the cost summary lists each model and each agent with catalog spend.
2. Input tokens are the sum of session input counts that are present. Output and cache stay unknown when the session row has no count.
3. Spend and daily spend for the month projection use those session costs. Known cycles stay 0. A cycle is still one quoted usage event.
4. When the period has any quoted usage event, session totals are not added on top of event spend, models, or agents.
5. `ax costs session` lists agents. `ax costs model` lists models. JSON includes `models`, `agents`, and `sessionSourced`.
6. `ax costs` imports local Cursor transcripts, Claude transcripts, and Cursor composer state before it builds or prints the report. A transcript written since the last import is included in that report.
7. The text report starts with a block-letter AX banner, then one line with the period, catalog spend, the first model when one exists, and the recorded session minutes when the report is session-sourced.
8. In Tokens, Context efficiency, and Cycle efficiency, a missing count is left out when session minutes were measured, and each section shows `Time` as that minute total. Cycle efficiency also shows `Cost/min` (spend divided by those minutes) and `Input/min` (input tokens divided by those minutes, whole tokens) when the total is greater than zero and input tokens are known. A non-zero cost per minute that would display as 0.00 at two decimals is shown with four decimals. A report with no measured session minutes still prints `unknown` for a missing count. Known cycles stay the quoted-event count. Session minutes are the sum of `(ended_at - started_at)` for rows whose end is at or after the start, rounded up to the next minute. A zero span, a missing end, or an end before the start adds nothing.

## Must not

- Invent output or cache tokens.
- Count an imported session as a known cycle.
- Double-count session spend when usage events exist.
- Print a minute count on the Output, Cache read, or Cache write rows.
