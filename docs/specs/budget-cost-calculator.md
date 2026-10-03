# Budget cost calculator

Status: approved for phase 2 by the user ("ok") after the cost-architecture audit.
Spec approval: the audit decisions were confirmed; this file is the executable contract for the calculator.

## Decisions

- A missing model price is `unknown`. The savings counterfactual still uses the reference model and stays an estimate.
- Catalog token math is `estimated`. `actual` is reserved for a provider invoice amount, which Ax does not receive.
- Euro display needs an explicit FX rate in config. That rate is not part of this phase.
- Do not replace `ax savings` or `input_cost_usd`.

## Scenarios

- 1,000,000 input tokens at $3 / Mtok → input $3.00, confidence `estimated`.
- 1,000,000 output tokens at $15 / Mtok → output $15.00, confidence `estimated`.
- 1,000,000 cache-read tokens at $0.30 / Mtok → cache read $0.30, confidence `estimated`.
- 1,000,000 cache-write tokens at $3.75 / Mtok → cache write $3.75, confidence `estimated`.
- Positive tokens and a missing rate → that component `unknown`, and the total is `unknown`.
- Missing token count → that component `unknown`.
- Zero tokens → $0.00 `estimated`, even when the rate is missing.
- Negative tokens or a negative rate → `unknown`.
- Claude `cache_read_input_tokens` and `cache_creation_input_tokens` are stored separately and are not added to input tokens.

## Out of scope

- Monthly budget, CLI, context budgets, routing, and simulation.
- Blocking a provider HTTP call.
- Hardcoded live prices. Built-in savings defaults stay on the savings path only.
