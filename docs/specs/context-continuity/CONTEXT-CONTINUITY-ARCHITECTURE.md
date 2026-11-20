# Context continuity

A new Ax chat reconstructs work from persistent project knowledge. It does not replay a transcript.

## What already existed

Ax already mints an `axs_` session id, records an external chat id when a hook sends one, and keeps working notes for the current chat. Those notes are not a checkpoint of the project knowledge, and they are not a cross-chat experiment.

## What this adds

`crates/ax-usage/src/continuity.rs` is the continuity harness.

- A session identity keeps provider-issued ids only when the provider sent them.
- A checkpoint is a hash-sealed snapshot of Ax knowledge at a cutoff. Transcript and handoff text are dropped before the seal.
- A later chat selects from that snapshot by tag overlap. That is the retrieval method. The checkpoint is the historical reference, not the prompt.
- Three arms share one snapshot, model, task, limits, and config. Each arm has its own worktree.
- Arm C cannot see historical, global, handoff, or transcript items.
- The scorecard compares functional tests. It does not compare answer text.
- The production decision stays negative until a live model experiment is recorded. This harness does not record one.

## What this does not do

It does not open a second IDE conversation, call a model, or declare that Ax matches a long chat. The fixture agent passes only when the required Ax items were delivered. That proves the gates. It does not prove a model.
