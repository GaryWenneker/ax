#!/usr/bin/env bash
# A/B context metrics for Neural (SPEC-FULL F6). Measures coding-fixture bake-off
# families with Neural status present; does not claim agent task success.
# Usage: scripts/neural-ab.sh [out.json]
set -euo pipefail
cd "$(dirname "$0")/.."
OUT="${1:-target-dev/neural-ab.json}"
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
REPORT="$("$BIN" neural --path "$TMP/proj" benchmark)"
STATUS="$("$BIN" neural --path "$TMP/proj" status)"
python3 - "$OUT" "$REPORT" "$STATUS" <<'PY'
import json, sys
out, report_s, status_s = sys.argv[1], sys.argv[2], sys.argv[3]
report = json.loads(report_s)
status = json.loads(status_s)
families = report.get("families", {})
def hits(name):
    f = families.get(name, {})
    return {"scored": f.get("scored"), "hits": f.get("hits")}
without = report.get("withoutAx") or {}
without_families = without.get("families") or {}
def without_hits(name):
    f = without_families.get(name, {})
    return {"scored": f.get("scored"), "hits": f.get("hits"), "measured": f.get("measured")}
payload = {
    "kind": "measured",
    "comparisons": {
        "A_without_ax": {
            "status": "measured",
            "method": without.get("method"),
            "families": {k: without_hits(k) for k in ("named_trigger", "paraphrase", "abstention", "delivery_budget")},
        },
        "B_ax_without_neural_ranking": {"status": "not_applicable", "reason": "Neural ranking is required and cannot be disabled"},
        "C_ax_with_neural": {
            "status": "measured",
            "bakeoffWinner": report.get("winner"),
            "challengerAdmitted": report.get("challengerAdmitted"),
            "families": {k: hits(k) for k in ("named_trigger", "paraphrase", "abstention", "delivery_budget")},
            "queryLatency": report.get("queryLatency"),
            "neuralStatus": {
                "enabled": status.get("enabled"),
                "rankingActive": status.get("rankingActive"),
                "runtimeBuilt": status.get("runtimeBuilt"),
                "model": status.get("model"),
            },
        },
    },
    "claims": [],
}
json.dump(payload, open(out, "w", encoding="utf-8"), indent=2)
print(out)
PY
