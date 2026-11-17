# Proposed amendment to memory-hash-only

Status: proposed. Not applied. `docs/specs/memory-hash-only.md` is unchanged.

## Proposed wording

Hash-v1 remains the embedding stored on `memories.embedding` in every build. `embed_labeled` still returns model `hash-v1`. Settings → Embeddings still shows that one memory model and does not offer a download.

A second model is allowed only in `neural_embedding`, only when `ax.json` has `neural.enabled = true`, and only after the bake-off in `adr-002-embedding-bakeoff.md` admits it. `reembed_all_foreign` does not read `neural_embedding`.

## Why it is not applied

The bake-off did not admit a challenger. The current code matches the existing hash-only spec: there is still one memory embedding model.
