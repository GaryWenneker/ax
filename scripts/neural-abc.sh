#!/usr/bin/env bash
# Phase 2 A/B/C retrieval benchmark (SPEC-PHASE2-BENCHMARK.md).
# Arms: A = BM25 (no Neural ranking), B = Ax locked fusion, C = Neural lexical when admitted.
# Does not disable production Neural ranking. Does not raise SEMANTIC_WEIGHT.
# Usage: scripts/neural-abc.sh [out.json]
set -euo pipefail
cd "$(dirname "$0")/.."
OUT="${1:-target-dev/neural-abc.json}"
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
REPORT="$("$BIN" neural --path "$TMP/proj" benchmark-abc)"
STATUS="$("$BIN" neural --path "$TMP/proj" status)"
python3 - "$OUT" "$REPORT" "$STATUS" <<'PY'
import json, sys
out, report_s, status_s = sys.argv[1], sys.argv[2], sys.argv[3]
report = json.loads(report_s)
status = json.loads(status_s)
caps = status.get("capabilities") or {}
payload = {
    "kind": report.get("kind", "measured"),
    "spec": "docs/specs/ax-neural/SPEC-PHASE2-BENCHMARK.md",
    "productionRankingUnchanged": report.get("productionRankingUnchanged"),
    "semanticWeight": report.get("semanticWeight"),
    "arms": report.get("arms"),
    "latency": report.get("latency"),
    "memory": report.get("memory"),
    "tokens": report.get("tokens"),
    "lockedFamilies": report.get("lockedFamilies"),
    "heldOutFamilyCount": report.get("heldOutFamilyCount"),
    "claimsNeuralInference": False,
    "neuralStatus": {
        "rankingActive": status.get("rankingActive"),
        "semanticActive": status.get("semanticActive"),
        "lexicalRetrieval": (caps.get("lexicalRetrieval") or {}).get("status"),
        "neuralEmbeddingInference": (caps.get("neuralEmbeddingInference") or {}).get("status"),
        "neuralRanking": (caps.get("neuralRanking") or {}).get("status"),
    },
}
json.dump(payload, open(out, "w", encoding="utf-8"), indent=2)
print(out)
# Fail closed if the report lies about production or weight.
assert payload["productionRankingUnchanged"] is True, "production ranking must stay unchanged"
assert payload["semanticWeight"] == 0.0, "SEMANTIC_WEIGHT must remain 0"
assert payload["claimsNeuralInference"] is False
assert "A_ax_without_neural_ranking" in (payload["arms"] or {})
assert "B_ax_original_ranking" in (payload["arms"] or {})
assert "C_ax_neural" in (payload["arms"] or {})
PY
