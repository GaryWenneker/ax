# SPEC — Plan C pilot v1 registry

First frozen pilot task and arm layout. Does **not** claim 95% functional reproduction.

## Behaviors

1. `freeze_pilot_task_v1` freezes `pilot-ts-list-filter` in stratum `typescript-react` with two noncritical criteria and one critical (`no-secret-in-ui`).
2. `prepare_pilot_bundle(dir)` creates arms G/A/B/C, writes `frozen-task.json`, and lists adapters `fixture` and `cursor`.
3. A fixture run that fails the critical criterion reports `release gate: not claimed`.
4. CLI: `ax replay-pilot-prepare <dir>` prints JSON with `releaseGate: "not claimed"`.

## Must not

- Claim measured 95%/100% release gates.
- Place golden solution code inside arm workspaces.
- Invent provider chat ids.
