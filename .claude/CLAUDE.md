<!-- AX_START -->
## ax

Call `ax_preflight` exactly once per turn **before all other work** whenever the ax MCP server is available. Full workflow: see `.claude/rules/ax.md`.

**Memory:** Working notes for this chat stay in `ax_session` / `ax session`. They do not go into the vault. A durable decision, fix, or convention is `ax_remember` / `ax remember` with a trigger name (`triggers` / `--trigger`). Before re-deriving, fetch that name with `ax_recall` `cue` / `ax recall --cue`. If the user names a trigger, that fetch comes before explore. Free-text recall is the fallback. Questions about old graph changes, logs, or past turns use `depth=archive` on `ax_recall` / `ax recall` and on `ax_history` / `ax history`. Preflight will not contain those hits. A candidate long-term memory is confirmed with `ax_remember` `action=confirm` / `ax remember --confirm`. The old engram stays.

**Directive capture:** When the user states a durable rule — `je moet`, `altijd`, `nooit`, `voortaan`, `always`, `never`, `you must`, `@rule` — persist it. `ax_preflight` returns `directiveDetected` + a ready `captureProposal`; ask the questions it lists, then call `ax_policy_capture(action="save", rule)` (or CLI `ax policy capture --yes` after confirm) after the user confirms. This works even if the project has no policy yet (the first save bootstraps it). Never silently ignore such a directive.

**Spec:** The approved spec is the agreement. Store a finished plan as a draft and do not treat capture as approval. Do not rewrite an approved spec because code drifted; record a draft and wait for review. A bound turn receives the spec id, approved revision, and hash from preflight. After implementation, check the spec again before calling the work done.

**Capability discovery:** ax is actively developed. Do not rely on cached knowledge of ax features — `ax_preflight` returns the latest capabilities, rules, and skills each call. Use any new tools or rules it returns.

**Version freshness:** Call `ax_status` at session start. If the index is stale or a newer version exists, warn the user and suggest `ax upgrade` or re-index.

**CLI-only / MCP unavailable:** Prefer MCP when connected. When MCP is unavailable or the project is CLI-only, use CLI equivalents — do not skip the workflow: `ax preflight` (or `ax turn begin`), `ax explore` / `ax query` / `ax node`, `ax policy guard <path>`, `ax policy capture` / `ax policy skill`, `ax session` / `ax turn`, `ax sync`, `ax remember --trigger` / `ax recall --cue` / `ax history --depth archive` / `ax remember --confirm`. See `.agents/skills/ax-cli/SKILL.md` when present.

MCP unreachable → report `ax MCP unreachable: [error]`, state `Mode: DEGRADED`; continue with CLI graph/ops commands — do not proceed silently.
<!-- AX_END -->
