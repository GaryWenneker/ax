#!/usr/bin/env bash
# Fail-closed: one-line tables without dropping tags.
set -euo pipefail
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
cd "$ROOT"
source "$ROOT/scripts/lib/mutate.sh"
source "$ROOT/scripts/lib/negative-control.sh"
# The web UI styles are split over src/styles/*.css: the policy table tags live in
# 13-policy.css, the table sizing in 21-calm-lists.css.
POLICY_CSS="$ROOT/crates/ax-web/web-ui/src/styles/13-policy.css"
CSS="$ROOT/crates/ax-web/web-ui/src/styles/21-calm-lists.css"

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
grep -q 'flex-wrap: nowrap' "$POLICY_CSS" || fail "missing nowrap on policy tag row"
if grep -A8 '\.policy-table-tags \.policy-view-tags' "$POLICY_CSS" | grep -q 'flex-wrap: wrap'; then
  fail "policy-table-tags still wraps"
fi
grep -q 'padding: 2px 8px' "$POLICY_CSS" || fail "dense table padding not 2px 8px"
grep -q 'table.policy-table' "$CSS" || fail "missing table.policy-table crush fix"
grep -q 'width: max-content' "$CSS" || fail "policy-table must size to content"
grep -q 'flex-flow: row nowrap' "$CSS" || fail "missing flex-flow nowrap on table tags"

echo "== negative control: the wiring checks fail on a broken copy =="
negative_control row-nowrap "$CSS" 'flex-flow: row nowrap' 'flex-flow: row wrap' grep -q 'flex-flow: row nowrap'

echo "== tsc =="
(cd crates/ax-web/web-ui && npx tsc --noEmit)

echo "== manual mutation: restoring the compact helper must fail T1 =="
# Only a failing test kills a mutant; a syntax error is a run error (scripts/lib/mutate.sh).
UTIL="crates/ax-web/web-ui/src/components/ui/policyListUtils.ts"
oneline_tests() { (cd crates/ax-web/web-ui && node --experimental-strip-types --test src/tagOneline.test.ts); }
ax_mutate oneline_tests "$UTIL" \
  'const GIT_SHARED_SCOPES = ' \
  $'export function compactTagItems() { return { shown: [], extra: 0 }; }\nconst GIT_SHARED_SCOPES = '
ax_mutants_summary

echo "gauntlet-policy-table-oneline: ok"
