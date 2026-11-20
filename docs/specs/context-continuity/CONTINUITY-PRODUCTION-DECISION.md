# Production decision

Ax does not provide long-chat equivalence.

The harness in `crates/ax-usage/src/continuity.rs` passes its twelve scenarios. `equivalence_claimed` is false. `live_agent_experiments` is 0. The agent in those scenarios is a fixture, not a model. Latency is the documented one-millisecond-per-item model, not a measured model call.

A continuity claim becomes available only after an independent experiment uses the same model, the same task, the same snapshot, and isolated worktrees, and records that Arm B passed because of Ax items that Arm C could not see. This repository does not contain that experiment.

The Logging panel can copy a test reference and a prompt. The prompt names the reference, the checkpoint, the mode, and the task. It does not include historical answers or a transcript.
