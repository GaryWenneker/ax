# Context export schema 1.0

`schemaVersion` is `1.0`. `exportType` is `ax-context-continuity`.

The exporter returns two JSON documents plus a coverage report. Both documents are built from the same `ExportInput`. The verifier checks the documents against that input. It does not call a model.

## Availability

Every section is an object with `state`:

| State | Meaning |
| --- | --- |
| `present` | The value was supplied and is included. |
| `referenced` | The full value stays in Ax. The export carries the id and a content hash. |
| `unavailable` | The field was expected and the source did not provide it. |
| `redacted` | A secret was removed. The export stays partial. |
| `truncated` | The writer cap cut the text. The export stays partial. |
| `not_captured` | Ax has no store for this section. The export stays partial. |

An empty `items` array with state `present` means the section was captured and was empty. It does not mean the section was never stored.

## Sections

1. Identity — agent provider, external chat id, Ax session id, project id, linked session ids.
2. Session history — turns in sequence, each with prompt, outcome, files, commits, tool-call refs, and memory refs.
3. Working context — objective, decisions, constraints, open tasks, known failures, rejected approaches.
4. Knowledge — memories, graph facts, rules, skills.
5. Execution evidence — repository commit and model.
6. State — carried inside working context (open tasks, known failures) and identity (linked sessions).
7. Provenance — `snapshotId`, `sourceHashes.turns`, `missingFields`, `redactedFields`, `truncatedFields`, `completeness`, `completenessBlockers`.

`snapshotId` is `snap-` plus the first 12 hex characters of the turn-set hash. Each turn hash is SHA-256 of `id`, `sequence`, `prompt`, `outcome`, `files`, and `commits`, separated by newlines. Missing files or commits use the marker `unavailable`.

## Two representations

`representation: full-evidence` keeps every supplied turn body.

`representation: agent-ready` is the document a new chat receives:

- Every turn is referenced by id, sequence, and hash.
- A non-last turn prompt is `referenced`. Its body is not copied.
- The last turn prompt is copied only when it is under 2,000 characters, not truncated, and not redacted.
- Decisions and rejected approaches are copied. They are not dropped to save space.
- Knowledge entries are ids with state `referenced` or `unavailable`, not full bodies.

If a future export exceeds a caller budget, the exporter must return this manifest of references. It must not silently summarize.

## Completeness

`provenance.completeness` is `complete` or `partial`.

`complete` is illegal unless the turn inventory is closed and every mandatory section is `present` or a captured empty list. The loader's inventory is closed only when the `agent_turns` query succeeds. A caller-built fixture is closed only when `inventory_closed` is true.

The coverage report is separate from the JSON. Its lines are turns, decisions, rules and skills, unresolved references, source integrity, context relevance, and cross-chat continuity. Context relevance stays `Not yet tested` and cross-chat continuity stays `Pending` until those experiments are run. They are not folded into one score.
