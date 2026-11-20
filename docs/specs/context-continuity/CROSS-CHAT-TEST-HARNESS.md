# Cross-chat test harness

Every scenario runs three arms on one `World`.

| Arm | Session | Transcript | Knowledge |
|---|---|---|---|
| A, long conversation | source session | visible as available context | tag overlap |
| B, new chat with Ax | new session | hidden | tag overlap |
| C, negative control | another new session | hidden | non-historical project tools only |

Shared across the arms: model name `fixture-model-1`, task, acceptance tests, token limit, config string, and repository snapshot id. The worktree path and the file map differ per arm. A write recorded on one arm is the string `A`, `B`, or `C` in that arm's map.

Arm B and Arm C do not receive the transcript or the forbidden historical answer. Arm C also drops historical items, global memory, and handoff text, including items that carry the task tag.

Modes are `historical` and `current`. A comparison of one historical result with one current result fails the independent-session gate. Scenario `conflicting_decisions` checks the two modes separately: the historical cutoff keeps the earlier storage decision, and current knowledge keeps the later one.

The twelve scenarios are `unfinished_feature`, `architectural_decision`, `diagnosed_bug`, `project_rule`, `early_memory`, `modified_file`, `daemon_restart`, `ide_restart`, `different_agent`, `replica`, `conflicting_decisions`, and `unrelated_history`.

The fixture agent cites a required id only when it was delivered, and it passes the acceptance list only when every required id was cited. Retrieval latency is one millisecond per retrieved item. Input tokens are UTF-8 bytes divided by four. Arm A counts the transcript in its available input, so its token count is higher than Arm B.
