# Context continuity results

These figures come from the in-process harness in `crates/ax-usage/src/continuity.rs`.
The agent is a fixture that passes a task only when every required Ax item was delivered.
Retrieval latency is one millisecond per retrieved item. Input tokens are UTF-8 bytes divided by four.
Equivalence claimed: false. Live agent experiments: 0.

| Scenario | Recall | Precision | Decisions | Rules | Pass | Equivalence | Token overhead | Control pass |
|---|---:|---:|---:|---:|---:|---:|---:|---:|
| unfinished_feature | 1.000 | 1.000 | 1.000 | 1.000 | 1.000 | 1.000 | 0.056 | 0.000 |
| architectural_decision | 1.000 | 1.000 | 1.000 | 1.000 | 1.000 | 1.000 | 0.054 | 0.000 |
| diagnosed_bug | 1.000 | 1.000 | 1.000 | 1.000 | 1.000 | 1.000 | 0.075 | 0.000 |
| project_rule | 1.000 | 1.000 | 1.000 | 1.000 | 1.000 | 1.000 | 0.058 | 0.000 |
| early_memory | 1.000 | 1.000 | 1.000 | 1.000 | 1.000 | 1.000 | 0.063 | 0.000 |
| modified_file | 1.000 | 1.000 | 1.000 | 1.000 | 1.000 | 1.000 | 0.054 | 0.000 |
| daemon_restart | 1.000 | 1.000 | 1.000 | 1.000 | 1.000 | 1.000 | 0.056 | 0.000 |
| ide_restart | 1.000 | 1.000 | 1.000 | 1.000 | 1.000 | 1.000 | 0.056 | 0.000 |
| different_agent | 1.000 | 1.000 | 1.000 | 1.000 | 1.000 | 1.000 | 0.056 | 0.000 |
| replica | 1.000 | 1.000 | 1.000 | 1.000 | 1.000 | 1.000 | 0.056 | 0.000 |
| conflicting_decisions | 1.000 | 1.000 | 1.000 | 1.000 | 1.000 | 1.000 | 0.054 | 0.000 |
| unrelated_history | 1.000 | 1.000 | 1.000 | 1.000 | 1.000 | 1.000 | 0.056 | 0.000 |

Ax does not provide long-chat equivalence. No live model was compared.
