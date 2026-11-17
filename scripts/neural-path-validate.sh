#!/usr/bin/env bash
# §2 ONNX production-path validation (arms A–D). Does not change production defaults.
# Usage: scripts/neural-path-validate.sh [out.json]
# With model: builds ax-neural --features neural-ort and dumps JSON.
set -euo pipefail
cd "$(dirname "$0")/.."
OUT="${1:-target-dev/neural-path-validate.json}"
mkdir -p "$(dirname "$OUT")"
export AX_SKIP_WEB_BUILD=1
export AX_DUMP_PATH_VALIDATE="$(pwd)/$OUT"
cargo test -q -p ax-neural --features neural-ort --lib dump_path_validate_json_when_env_set -- --nocapture
python3 - "$OUT" <<'PY'
import json, sys
path = sys.argv[1]
report = json.load(open(path))
assert report["kind"] == "measured"
assert report["semanticWeight"] == 0.0
assert report["productionDefaultsUnchanged"] is True
assert report["neuralEmbeddingInferenceActive"] is False
assert "A_hash_v1" in report["arms"]
assert "B_lexical_semantic" in report["arms"]
assert "C_onnx_embedding" in report["arms"]
assert "D_combined_rrf" in report["arms"]
stages = report["stages"]
for key in (
    "modelLoading",
    "embeddingInference",
    "vectorPersistence",
    "queryEmbedding",
    "similarity",
    "retrieval",
    "rankingFusion",
    "contextDelivery",
):
    assert key in stages, key
print(path)
print("C", report["arms"]["C_onnx_embedding"].get("kind"))
print("D method", report["arms"]["D_combined_rrf"].get("method"))
for name, arm in report["arms"].items():
    loc = arm.get("locked") or {}
    print(name, "P@3", loc.get("precisionAt3"), "R@3", loc.get("recallAt3"), "MRR", loc.get("mrr"))
PY
