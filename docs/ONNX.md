# ONNX dense memory embeddings

By default, memory hybrid recall uses **feature-hash** vectors (256-d) fused with
FTS5 via weighted RRF (`0.5` vector / `0.3` FTS / `0.2` graph reserved).

## Enable ONNX

```powershell
cargo build -p ax-cli --release --features onnx
# or after install from a build that enabled the feature
```

Model discovery:

1. `AX_ONNX_MODEL` — path to a `.onnx` file
2. `~/.ax/models/all-MiniLM-L6-v2.onnx`

Tokenizer discovery (HuggingFace `tokenizer.json` WordPiece vocab):

1. `AX_ONNX_TOKENIZER` — path to `tokenizer.json`
2. `tokenizer.json` beside the model file

When a model loads successfully, `embed_text` prefers ONNX output (mean-pooled,
projected to 256-d for storage compatibility). If load/run fails, feature-hash
is used automatically.

Without a tokenizer file, ax falls back to a hashed token-id stand-in so the
session path can be exercised. For production MiniLM recall, place the real
`tokenizer.json` next to the model (or set `AX_ONNX_TOKENIZER`).

Command Center: **Settings → Embeddings** shows backend, Cargo feature flag, model
path, and tokenizer path.
`GET /api/memory/embed-status` returns
`{ backend, tokenizer, feature, modelPath, tokenizerPath }`
(`backend`: `hash` | `onnx` | `onnx_unconfigured`).
