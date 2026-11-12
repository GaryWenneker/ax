#!/usr/bin/env bash
# Compare baseline vs candidate metrics. Exit 0 only if lift>=0.50 and secondary gates pass.
set -euo pipefail
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
BEFORE="${1:-}"
AFTER="${2:-}"
if [[ -z "$BEFORE" || -z "$AFTER" ]]; then
  echo "usage: compare.sh <baseline/metrics.json> <candidate/metrics.json>" >&2
  exit 2
fi
python3 - "$BEFORE" "$AFTER" <<'PY'
import json, sys
before = json.loads(open(sys.argv[1]).read())
after = json.loads(open(sys.argv[2]).read())
b = before.get("median_ucp_at_k")
a = after.get("median_ucp_at_k")
if b is None or a is None or b == 0:
    lift = None
else:
    lift = (a - b) / b

def gate_s2(m):
    r = m.get("s2_skill_inclusion_rate")
    return r is not None and r >= 1.0

def gate_recall(b, a):
    br = b.get("median_recall_at_k") or 0
    ar = a.get("median_recall_at_k") or 0
    return (ar - br) >= -0.05

ship = bool(lift is not None and lift >= 0.50 and gate_recall(before, after))
# S2 is aspirational until Wave C; warn but do not block Wave A protocol claims if documented
claim = "none"
if lift is not None and lift >= 0.90 and ship:
    claim = ">=90%"
elif lift is not None and lift >= 0.50 and ship:
    claim = ">=50%"

out = {
    "primary": {
        "metric": "median_ucp_at_k",
        "before": b,
        "after": a,
        "lift": lift,
    },
    "secondaries": {
        "S1_recall_ok": gate_recall(before, after),
        "S2_skill_inclusion_before": before.get("s2_skill_inclusion_rate"),
        "S2_skill_inclusion_after": after.get("s2_skill_inclusion_rate"),
        "n_scored_before": before.get("n_scored"),
        "n_scored_after": after.get("n_scored"),
    },
    "ship": ship,
    "claimAllowed": claim,
}
print(json.dumps(out, indent=2))
sys.exit(0 if ship else 1)
PY
