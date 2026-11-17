#!/usr/bin/env bash
# Phase 6 fixture agent-eval harness. Live LLM metrics stay not_measured.
set -euo pipefail
cd "$(dirname "$0")/.."
OUT="${1:-target-dev/neural-agent-eval.json}"
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
REPORT="$("$BIN" neural --path "$TMP/proj" agent-eval)"
python3 - "$OUT" "$REPORT" <<'PY'
import json, sys
out, report_s = sys.argv[1], sys.argv[2]
report = json.loads(report_s)
assert report["kind"] == "measured"
assert report["liveLlmTaskSuccess"]["kind"] == "not_measured"
assert report["semanticWeight"] == 0.0
for arm in ("A_ax_without_neural_ranking","B_ax_original_ranking","C_ax_neural_ranking","D_ax_neural_prediction"):
    assert arm in report["arms"]
json.dump(report, open(out,"w"), indent=2)
print(out)
PY
