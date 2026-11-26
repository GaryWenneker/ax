#!/bin/sh
# Process-level lock check: several ax processes on one project's .ax/ at once.
#
# usage: scripts/slim-concurrency.sh [AX_BINARY] PROJECT_DIR
#
# PROJECT_DIR must be an already initialized throwaway copy of a project with a
# few hundred source files (the script re-indexes it and adds/removes one file).
# Checks:
#   1. a reader (`ax status`) during an index leaves .ax/ax.lock in place;
#   2. a second writer (`ax sync`) waits for the indexer instead of running
#      next to it, and no command reports "database is locked";
#   3. readers keep answering while the index writes;
#   4. after `kill -9` of an indexer, the next writer starts at once (the
#      kernel released the lock; no 180 s wait, no manual `ax unlock`).
# Exits non-zero on the first failed check.
set -u

if [ $# -eq 2 ]; then AX=$1; DIR=$2; else AX=ax; DIR=${1:?usage: $0 [AX_BINARY] PROJECT_DIR}; fi
case $AX in /*) ;; */*) AX=$(cd "$(dirname "$AX")" && pwd)/$(basename "$AX") ;; esac
cd "$DIR" || exit 2
[ -d .ax ] || { echo "not initialized: $DIR (run ax init first)" >&2; exit 2; }
export AX_NO_UPDATE_CHECK=1
LOG=$(mktemp -d)
PROBE=ax_concurrency_probe.py
fail() { echo "FAIL: $*" >&2; echo "logs in $LOG" >&2; rm -f "$PROBE"; exit 1; }
now() { date +%s; }

echo "1-3. index while reading and syncing"
"$AX" index . --force --quiet >"$LOG/index.log" 2>&1 &
ipid=$!
sleep 2
kill -0 "$ipid" 2>/dev/null || fail "index finished within 2 s; use a bigger project"
[ -e .ax/ax.lock ] || fail "no .ax/ax.lock while indexing"
"$AX" status >"$LOG/status.log" 2>&1 || fail "ax status failed during index"
[ -e .ax/ax.lock ] || fail "ax status removed .ax/ax.lock while the index held it"
printf 'def probe():\n    return 1\n' >"$PROBE"
"$AX" sync >"$LOG/sync.log" 2>&1
sync_rc=$?
sync_end_while_index=$(kill -0 "$ipid" 2>/dev/null && echo yes || echo no)
readers=0
while kill -0 "$ipid" 2>/dev/null; do
  "$AX" status >>"$LOG/status.log" 2>&1 || fail "ax status failed during index"
  readers=$((readers + 1))
  sleep 1
done
wait "$ipid" || fail "index failed: $(tail -2 "$LOG/index.log")"
[ "$sync_rc" -eq 0 ] || fail "ax sync failed: $(tail -2 "$LOG/sync.log")"
[ "$sync_end_while_index" = no ] || fail "ax sync finished while the index was still running (two writers)"
if grep -qi "database is locked" "$LOG"/*.log; then fail "database is locked reported"; fi
echo "   ok: lock kept, sync waited for the index, $readers extra reads answered"

echo "4. kill -9 an indexer, then write again"
"$AX" index . --force --quiet >"$LOG/index2.log" 2>&1 &
ipid=$!
sleep 2
kill -9 "$ipid" 2>/dev/null
wait "$ipid" 2>/dev/null
rm -f "$PROBE"
start=$(now)
"$AX" sync >"$LOG/sync2.log" 2>&1 || fail "ax sync after kill -9 failed: $(tail -2 "$LOG/sync2.log")"
took=$(( $(now) - start ))
[ "$took" -lt 60 ] || fail "ax sync after kill -9 waited ${took}s for the lock"
if grep -qi "database is locked" "$LOG"/*.log; then fail "database is locked reported"; fi
"$AX" status >/dev/null 2>&1 || fail "ax status after recovery failed"
echo "   ok: next writer started at once (${took}s)"

rm -rf "$LOG"
echo "PASS"
