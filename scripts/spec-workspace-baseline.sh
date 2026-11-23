#!/usr/bin/env bash
# Latency baseline for docs/specs/specification-workspace (Phase 0, acceptance criterion 13).
# Seeds a temporary project through the public CLI, then times the read routes of its own ax web.
# HOME and every AX_* database path point into the temporary folder, so the live project is untouched.
# The temporary folder is left in place because it contains a .ax folder.
set -euo pipefail

ROOT=$(cd "$(dirname "$0")/.." && pwd)
AX=${AX:-$ROOT/target-dev/release/ax}
SPECS=${SPECS:-300}
PLANS=${PLANS:-3}
RUNS=${RUNS:-20}
PORT=${PORT:-7392}

if lsof -nP -iTCP:"$PORT" -sTCP:LISTEN >/dev/null 2>&1; then
  echo "FAIL: port $PORT is already in use" >&2
  exit 1
fi

W=$(mktemp -d -t ax-spec-baseline)
mkdir -p "$W/proj" "$W/home"
export HOME="$W/home" AX_HOME_DIR="$W/home" AX_GLOBAL_DB="$W/home/global.db" AX_USAGE_DB="$W/home/usage.db"
cd "$W/proj"
git init -q
echo "# baseline" > README.md

echo "binary: $("$AX" --version)"
echo "folder: $W"
"$AX" init </dev/null >"$W/init.log" 2>&1
"$AX" spec-flag on >/dev/null

seed_start=$(date +%s)
for i in $(seq -w 1 "$SPECS"); do
  "$AX" spec-create "{\"projectId\":\"baseline\",\"specId\":\"spec-$i\",\"specTitle\":\"Spec $i\",\"specBody\":\"# Spec $i\n\n- [ ] Requirement $i.\",\"planId\":\"plan-$i-1\",\"planTitle\":\"Plan $i.1\",\"planBody\":\"1. Step one.\",\"sessionId\":\"session-$i\",\"turnId\":\"turn-$i-1\"}" >/dev/null
  for p in $(seq 2 "$PLANS"); do
    "$AX" spec-plan "{\"projectId\":\"baseline\",\"specId\":\"spec-$i\",\"planId\":\"plan-$i-$p\",\"planTitle\":\"Plan $i.$p\",\"planBody\":\"1. Step one.\",\"sessionId\":\"session-$i\",\"turnId\":\"turn-$i-$p\"}" >/dev/null
  done
done
seed_end=$(date +%s)
calls=$((SPECS * PLANS))
echo "seeded: $SPECS specs x $PLANS plans through $calls CLI calls in $((seed_end - seed_start)) s"

"$AX" web --port "$PORT" "$W/proj" >"$W/web.log" 2>&1 &
WEB=$!
trap 'kill "$WEB" 2>/dev/null || true' EXIT
for _ in $(seq 1 60); do
  if curl -s -o /dev/null "http://127.0.0.1:$PORT/api/specs"; then break; fi
  sleep 0.5
done

# time METHOD PATH [BODY]: RUNS timed requests; every answer must be 200.
time_route() {
  local method=$1 path=$2 body=${3:-}
  local out="$W/times.txt"
  : >"$out"
  for _ in $(seq 1 "$RUNS"); do
    local line
    if [ "$method" = GET ]; then
      line=$(curl -s -o "$W/body.json" -w '%{http_code} %{time_total}' "http://127.0.0.1:$PORT$path")
    else
      line=$(curl -s -o "$W/body.json" -w '%{http_code} %{time_total}' -X "$method" \
        -H 'content-type: application/json' -d "$body" "http://127.0.0.1:$PORT$path")
    fi
    if [ "${line%% *}" != 200 ]; then
      echo "FAIL: $method $path answered ${line%% *}" >&2
      cat "$W/body.json" >&2
      exit 1
    fi
    echo "${line##* }" >>"$out"
  done
  cp "$W/body.json" "$W/response$(echo "$path" | tr '/' '_').json"
  sort -n "$out" | awk -v route="$method $path" -v bytes="$(wc -c <"$W/body.json" | tr -d ' ')" '
    { t[NR] = $1 * 1000 }
    END {
      p50 = t[int(NR * 0.50 + 0.999999)]; p95 = t[int(NR * 0.95 + 0.999999)]
      printf "%-40s n=%d  p50 %.1f ms  p95 %.1f ms  max %.1f ms  %s bytes\n", route, NR, p50, p95, t[NR], bytes
    }'
}

time_route GET /api/specs
time_route GET "/api/specs/spec-$(printf '%0*d' "${#SPECS}" 1)/revisions"
time_route GET "/api/specs/plan/plan-$(printf '%0*d' "${#SPECS}" 1)-1/revisions"
echo "BASELINE OK"
