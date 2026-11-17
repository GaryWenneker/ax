# Hash vectors in every build

Spec approval: not obtained (autonomous run). The approved plan plus "continue" authorized the phase. Isolation: the working tree, same as the rest of this plan.

Tier 2. The live project database is not opened and is not re-embedded.

## Scenarios

1. `embed_labeled` returns the model name `hash-v1` and a vector of length 256. `embed_text` returns that same vector. There is no second model name.
2. `reembed_all_foreign` rewrites every row whose model is not `hash-v1`, including a disabled row, in one call. A row already on `hash-v1` is not counted and keeps a null embedding. Each rewritten blob is 1024 bytes. The call is not made from `Database::open`.
3. Embed status reports backend `hash` and model `hash-v1`. Settings → Embeddings shows that one model. It does not offer an ONNX feature, a model path, or a tokenizer.
4. The `onnx` Cargo feature and `crates/ax-memory/src/onnx.rs` are gone. Local build scripts do not pass `--features onnx`. `~/.ax/models` is not deleted by this code.

## Evidence

Recorded after the last code edit. Spec approval: not obtained (autonomous run).

- `cargo test -p ax-memory --offline --lib --tests`: 127 passed, 0 failed, 2 ignored (the 100,000-row bench and `write_coding_baseline`).
- `cargo clippy -p ax-memory -p ax-web --offline --all-targets -- -D warnings`: exit 0.
- `cargo check -p ax-cli --offline --features onnx`: exit 101, `the package 'ax-cli' does not contain this feature: onnx`.
- `npm run build` in `crates/ax-web/web-ui`: `tsc` and vite exit 0. The built bundle contains `hash-v1` and `Nothing is downloaded`, and does not contain `Re-probe` or `--features onnx`.
- Manual mutants, each restored before the suite above: status backend `onnx` failed `the_only_embedding_model_is_hash_v1` (`left: "onnx" right: "hash"`); an `enabled = 1` filter failed the one-pass test (`left: 1 right: 2`); writing model `minilm` failed it (`left: "minilm" right: "hash-v1"`).
- The settings page was not opened in a browser. `ax web` opens the project database, and this work does not.

## Out of scope

- Running the single pass against the live vault.
- Old builds registering a build id and skipping writers.
- Flagging Blossom notes.
- Suggested triggers.
