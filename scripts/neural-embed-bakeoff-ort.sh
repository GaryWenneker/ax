#!/usr/bin/env bash
# Phase 3 ONNX MiniLM bake-off (optional feature neural-ort).
# Requires ~/.ax/models/all-MiniLM-L6-v2.onnx + tokenizer.json.
# Does not enable neural-ort in the default ax binary.
# Usage: scripts/neural-embed-bakeoff-ort.sh [out.json]
set -euo pipefail
cd "$(dirname "$0")/.."
OUT="${1:-target-dev/neural-embed-bakeoff-ort.json}"
mkdir -p "$(dirname "$OUT")"
export AX_SKIP_WEB_BUILD=1
export AX_DUMP_EMBED_BAKEOFF="$(pwd)/$OUT"
cargo test -q -p ax-neural --features neural-ort --lib dump_embed_bakeoff_json_when_env_set -- --nocapture
python3 - "$OUT" <<'PY'
import json, sys
path = sys.argv[1]
report = json.load(open(path))
assert report["kind"] == "measured"
assert report["semanticWeight"] == 0.0
assert report["productionDefaultsUnchanged"] is True
assert report["neuralEmbeddingInferenceActive"] is False
onnx = next(c for c in report["candidates"] if c["id"] == "onnx-runtime-cpu")
assert onnx["kind"] in ("measured", "not_configured"), onnx
print(path)
print("winner", report.get("winner"), "admitted", report.get("neuralEmbeddingAdmitted"))
PY
