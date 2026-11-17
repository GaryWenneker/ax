# EVIDENCE — Phase 3 embedding-runtime bake-off

**Absolute path:** `/Users/gary/io/ax/docs/specs/ax-neural/EVIDENCE-PHASE3.md`  
**Spec:** `/Users/gary/io/ax/docs/specs/ax-neural/SPEC-PHASE3-EMBED.md`  
**Spec approval:** not obtained (autonomous goal run)  
**Source:** `docs/specs/ax-neural/embed-bakeoff-results.json` via `scripts/neural-embed-bakeoff-ort.sh` (feature `neural-ort`)  
**Tests:** `cargo test -p ax-neural --lib` → **123 passed** (default); ort suite previously **124 passed** with feature

## Spec → evidence

| Scenario | Result |
|---|---|
| Default binary: neural embedding not active | `neuralEmbeddingInferenceActive: false`; production defaults unchanged |
| ONNX with local MiniLM | `onnx-runtime-cpu` → `kind: measured`, paraphrase R@3 **1.0**, held-out R@3 ≈ **0.833** ≥ hash ≈ **0.667** → **admitted** |
| Candle without link | `candle-cpu` → `kind: not_built` |
| Hash / lexical measured; no false neural claim | both `claimsNeuralInference: false`; paraphrase R@3 hash **0.0**, lexical **1.0** |
| cpu-blake3 | measured when `neural-runtime` on; paraphrase R@3 **0.0**; not admissible as semantic |
| SEMANTIC_WEIGHT / memory column | `0.0` / `hash-v1`; `productionDefaultsUnchanged: true` |
| Selection | winner `onnx-runtime-cpu`; do **not** enable production inference or raise `SEMANTIC_WEIGHT` without promotion gate |

## Gauntlet

| Layer | Result |
|---|---|
| Unit | embed_bakeoff + onnx_embed + dispatch in suite |
| Real execution | ort bake-off script → JSON with `neuralEmbeddingAdmitted: true` |
| RSS | `not_measured` (stated reason) |
