#!/usr/bin/env bash
# Phase 3 embedding-runtime bake-off (SPEC-PHASE3-EMBED.md).
# Measures hash-v1 / lexical / cpu-blake3; reports onnx/candle as not_built until linked.
# Usage: scripts/neural-embed-bakeoff.sh [out.json]
set -euo pipefail
cd "$(dirname "$0")/.."
OUT="${1:-target-dev/neural-embed-bakeoff.json}"
mkdir -p "$(dirname "$OUT")"
export AX_SKIP_WEB_BUILD=1
BIN="${AX_BIN:-./target-dev/debug/ax}"
if [ ! -x "$BIN" ]; then
  cargo build -q -p ax-cli
  BIN=./target-dev/debug/ax
fi
TMP="$(mktemp -d)"
trap 'rm -rf "$TMP"' EXIT
"$BIN" init "$TMP/proj" >/dev/null
REPORT="$("$BIN" neural --path "$TMP/proj" embed-bakeoff)"
python3 - "$OUT" "$REPORT" <<'PY'
import json, sys
out, report_s = sys.argv[1], sys.argv[2]
report = json.loads(report_s)
assert report.get("kind") == "measured"
assert report.get("semanticWeight") == 0.0
assert report.get("productionDefaultsUnchanged") is True
assert report.get("neuralEmbeddingAdmitted") is False
ids = {c["id"] for c in report.get("candidates", [])}
assert {"hash-v1", "lexical-semantic-cpu", "onnx-runtime-cpu", "candle-cpu"} <= ids
for c in report["candidates"]:
    if c["id"] in ("onnx-runtime-cpu", "candle-cpu"):
        assert c["kind"] == "not_built"
        assert c["admitted"] is False
    if c.get("kind") == "measured":
        assert c.get("claimsNeuralInference") is False
json.dump(report, open(out, "w", encoding="utf-8"), indent=2)
print(out)
PY
