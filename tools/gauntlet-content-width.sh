#!/usr/bin/env bash
set -euo pipefail
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
CSS="$ROOT/crates/ax-web/web-ui/src/index.css"
fail() { echo "FAIL: $*" >&2; exit 1; }

echo "== wiring =="
# Old laptop letterbox must be gone.
if grep -n 'max-width: calc(var(--layout-max) - var(--sidebar-w))' "$CSS"; then
  fail "laptop band still caps container at layout-max - sidebar"
fi
grep -A30 'Tablet: sidebar hidden' "$CSS" | grep -q 'max-width: none' \
  || fail "tablet container must max-width none"

echo "== negative control =="
if grep -q 'layout-max-does-not-exist' "$CSS"; then
  fail "negative control passed"
fi

echo "== tsc =="
(cd "$ROOT/crates/ax-web/web-ui" && npx tsc --noEmit)

echo "gauntlet-content-width: ok"
