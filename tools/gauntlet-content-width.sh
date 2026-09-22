#!/usr/bin/env bash
set -euo pipefail
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
CSS="$ROOT/crates/ax-web/web-ui/src/index.css"
fail() { echo "FAIL: $*" >&2; exit 1; }

echo "== wiring =="
grep -q -- '--layout-max: 1100px' "$CSS" \
  || fail "expected --layout-max: 1100px default"
grep -q -- '--stage-w: calc(var(--sidebar-w) + 4px + var(--layout-max))' "$CSS" \
  || fail "expected --stage-w"
grep -A20 '^\.app {' "$CSS" | grep -q 'width: 100%' \
  || fail ".app must be full viewport width"
grep -A20 '^\.titlebar-inner {' "$CSS" | grep -q 'var(--stage-w)' \
  || fail "titlebar-inner must cap at stage-w"
grep -A12 '^\.statusbar-inner {' "$CSS" | grep -q 'var(--stage-w)' \
  || fail "statusbar-inner must cap at stage-w"
grep -q -- '--layout-max: 1320px' "$CSS" \
  || fail "expected 1320px at 1920+"
grep -q -- '--layout-max: 1480px' "$CSS" \
  || fail "expected 1480px at 2560+"

if grep -n 'max-width: calc(var(--layout-max) - var(--sidebar-w))' "$CSS"; then
  fail "must not use layout-max minus sidebar"
fi

echo "== negative control =="
if grep -q 'layout-max-does-not-exist' "$CSS"; then
  fail "negative control passed"
fi

echo "== tsc =="
(cd "$ROOT/crates/ax-web/web-ui" && npx tsc --noEmit)

echo "gauntlet-content-width: ok"
