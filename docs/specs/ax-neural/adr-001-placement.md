# ADR 001 — Where Neural lives

## Problem

The proposal sketched nine crates, a service binary, an integrations tree, and a second vector database. The repository already has memory, graph, policy, MCP, SQLite, and sqlite-vec.

## Alternatives

1. Nine crates and a standalone service, as drawn in the proposal.
2. One crate, `ax-neural`, called from CLI, MCP, and Command Center. Vectors in new SQLite tables. No ML crate on the default build.
3. Fold the logic into `ax-memory` only.

## Decision

Alternative 2.

`ax-memory` owns hash-v1. Mixing a second model into `memories.embedding` would fight `reembed_all_foreign` and the fixed 256-dimension `memory_vec` table. A side table keyed by entity kind, entity id, and model id keeps that column untouched.

A service process is not required for local mode. In-process calls read config and keep the layer on.

## Consequences

- Default `cargo check -p ax-cli` does not compile Candle, Burn, ONNX Runtime, or Tract.
- `ax-reasoning` stays the LLM offload crate and is not a Neural dependency.
- LanceDB is not added.
- A second crate is allowed later only if a real runtime dependency would otherwise link into the default binary.

## Risks

- Schema 45 is additive. Old databases migrate on the next open. `neural.enabled = false` does not turn ranking off.
- Preflight is already over its token budget. Rank notes are attached because ranking stays on.
