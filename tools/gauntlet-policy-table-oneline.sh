#!/usr/bin/env bash
# Fail-closed: one-line tables without dropping tags.
set -euo pipefail
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
cd "$ROOT"
CSS="$ROOT/crates/ax-web/web-ui/src/index.css"

fail() { echo "FAIL: $*" >&2; exit 1; }

echo "== unit =="
(cd crates/ax-web/web-ui && node --experimental-strip-types --test src/tagOneline.test.ts)

echo "== wiring =="
grep -q 'compactTagItems' crates/ax-web/web-ui/src/components/ui/policyListUtils.ts \
  && fail "compactTagItems must stay removed"
grep -q 'compact items=' crates/ax-web/web-ui/src/pages/PolicyRules.tsx \
  && fail "Rules table must not use compact TagList"
grep -q 'compact items=' crates/ax-web/web-ui/src/pages/PolicySkills.tsx \
  && fail "Skills table must not use compact TagList"
grep -q 'flex-wrap: nowrap' "$CSS" || fail "missing nowrap on policy tag row"
if grep -A8 '\.policy-table-tags \.policy-view-tags' "$CSS" | grep -q 'flex-wrap: wrap'; then
  fail "policy-table-tags still wraps"
fi
grep -q 'padding: 2px 8px' "$CSS" || fail "dense table padding not 2px 8px"
grep -q 'table.policy-table' "$CSS" || fail "missing table.policy-table crush fix"
grep -q 'width: max-content' "$CSS" || fail "policy-table must size to content"
grep -q 'flex-flow: row nowrap' "$CSS" || fail "missing flex-flow nowrap on table tags"

echo "== negative control =="
if grep -q 'flex-wrap: wrap-never-valid' "$CSS"; then
  fail "negative control passed (vacuous grep)"
fi

echo "== tsc =="
(cd crates/ax-web/web-ui && npx tsc --noEmit)

echo "== mutant: restore compact helper must fail T1 =="
UTIL="$ROOT/crates/ax-web/web-ui/src/components/ui/policyListUtils.ts"
ORIG="$(mktemp)"
cp "$UTIL" "$ORIG"
printf '\nexport function compactTagItems() { return { shown: [], extra: 0 }; }\n' >>"$UTIL"
if (cd crates/ax-web/web-ui && node --experimental-strip-types --test src/tagOneline.test.ts) >/tmp/tag-oneline-mutant.out 2>&1; then
  echo "FAIL: mutant survived" >&2
  cat /tmp/tag-oneline-mutant.out >&2
  cp "$ORIG" "$UTIL"
  exit 1
fi
echo "killed compactTagItems-export"
cp "$ORIG" "$UTIL"
rm -f "$ORIG"

echo "gauntlet-policy-table-oneline: ok"
