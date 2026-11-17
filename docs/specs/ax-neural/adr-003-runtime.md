# ADR 003 — CPU runtime behind `neural-runtime`

## Problem

Phase 1 acceptance requires CPU inference. Linking Candle or ONNX into the default binary contradicts ADR 001. The bake-off has not admitted a Candle/ONNX challenger (ADR 002).

## Alternatives

1. Link Candle/ONNX in every build.
2. Keep `neural-runtime` empty forever and claim hash-v1 is “inference”.
3. Ship a pure-Rust CPU path behind `neural-runtime`: blake3 smoke embedder for health checks, plus a **lexical-semantic** bake-off challenger that can beat hash-v1 paraphrase without ML crates; leave Candle/ONNX as future adapters behind the same feature once they beat the bake-off on their own.

## Decision

Alternative 3.

- Default binary: `runtime_built() == false`.
- `--features neural-runtime`: enables `runtime::CpuBackend` (blake3, not admissible) and the lexical-semantic challenger scored by the bake-off.
- `challenger_admitted()` becomes true only when the lexical (or a later) challenger satisfies ADR 002 against measured hash-v1 families on the coding fixture.
- Blake3 vectors must never be reported as beating hash-v1 on paraphrase.

## Consequences

- `cargo test -p ax-neural --features neural-runtime` exercises the CPU path and may admit `lexical-semantic-cpu`.
- Default `ax-cli` tree still has no candle/burn/ort/tract/onnx packages.
- A later Candle/ONNX adapter implements the same `Backend` trait and must re-run the bake-off before admission.

## Risks

- Lexical synonym maps are domain-scoped. They are not general semantic embeddings. SEMANTIC_WEIGHT stays `0.0` until a held-out retune.
