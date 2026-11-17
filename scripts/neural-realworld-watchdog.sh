#!/usr/bin/env bash
# Keep the held-out inject agent matrix running until exit 0.
set -euo pipefail
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
cd "$ROOT"
export AX_BIN="${AX_BIN:-$ROOT/target-dev/debug/ax}"
export AX_RW_AGENT_TIMEOUT_SEC="${AX_RW_AGENT_TIMEOUT_SEC:-420}"
export PYTHONUNBUFFERED=1
LOG="${LOG:-/tmp/rw-agent-resume.log}"
PIDFILE="${PIDFILE:-/tmp/rw-agent-resume.pid}"

echo "{\"watchdog\":\"start\",\"log\":\"$LOG\"}" >>"$LOG"
echo $$ >"${PIDFILE}.watchdog"

while true; do
  echo "{\"watchdog\":\"launch\",\"ts\":\"$(date -u +%Y-%m-%dT%H:%M:%SZ)\"}" >>"$LOG"
  python3 -u scripts/neural-realworld-agent.py \
    --inject-heldout \
    --reps 1 \
    --seed 29 \
    --skip-done \
    --arms A_production_retrieval,B_neural_ranking,D_prediction,E_learning_train_only \
    >>"$LOG" 2>&1 &
  child=$!
  echo "$child" >"$PIDFILE"
  set +e
  wait "$child"
  code=$?
  set -e
  echo "{\"watchdog\":\"child_exit\",\"code\":$code,\"ts\":\"$(date -u +%Y-%m-%dT%H:%M:%SZ)\"}" >>"$LOG"
  # Exit 0 means matrix finished; otherwise restart after brief pause
  if [[ $code -eq 0 ]]; then
    echo "{\"watchdog\":\"complete\"}" >>"$LOG"
    rm -f "$PIDFILE" "${PIDFILE}.watchdog"
    exit 0
  fi
  sleep 5
done
