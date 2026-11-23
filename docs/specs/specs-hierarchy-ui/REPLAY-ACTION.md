# SPEC addendum — B5 Replay action

Continuation of Plan B5. Operator can start a replay from a sealed context manifest without inventing provider chat ids.

## Behaviors

1. `POST /api/context-authority/replay` with `{ "manifestId" }` loads session/turn/agent/provider from the stored manifest and inserts a `replay_runs` row via `start_replay_for_manifest`.
2. Automated stays `not_implemented`; score stays `not_tested` until Plan C measures a run. Response never invents a pass.
3. `GET /api/context-authority/replay/{id}` returns the stored receipt.
4. ManifestAuthorityPanel shows a Replay button (disabled when seal is broken). After start, status text uses `formatReplayNotice` (no 95% / confident green).
5. Plan C V2 pilot helper `prepare_v2_pilot_arms` creates G/A/B/C workspaces; `format_scorecard_lines` refuses to claim a release gate unless verdict is Pass.

## Must not

- Store transcripts in replay_runs.
- Invent provider chat ids.
- Claim 95% fidelity from this UI action.
- Migrate live project DB schemas beyond existing ensure_* helpers.
