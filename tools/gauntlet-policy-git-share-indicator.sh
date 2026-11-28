#!/usr/bin/env bash
# Gauntlet: git-share indicator in Command Center policy lists.
set -euo pipefail
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
cd "$ROOT"
source "$ROOT/scripts/lib/mutate.sh"
source "$ROOT/scripts/lib/negative-control.sh"

echo "== isGitShared unit tests =="
(cd crates/ax-web/web-ui && node --experimental-strip-types --test src/gitShare.test.ts)

echo "== UI wiring =="
grep -q 'GitShareDot' crates/ax-web/web-ui/src/pages/PolicyRules.tsx \
  || { echo "FAIL: Rules missing GitShareDot" >&2; exit 1; }
grep -q 'GitShareDot' crates/ax-web/web-ui/src/pages/PolicySkills.tsx \
  || { echo "FAIL: Skills missing GitShareDot" >&2; exit 1; }
grep -q 'GitShareStatus' crates/ax-web/web-ui/src/components/PolicyMetaView.tsx \
  || { echo "FAIL: meta missing GitShareStatus" >&2; exit 1; }
grep -q 'git-share-dot' crates/ax-web/web-ui/src/styles/13-policy.css \
  || { echo "FAIL: missing git-share-dot CSS" >&2; exit 1; }

echo "== tsc =="
(cd crates/ax-web/web-ui && npx tsc --noEmit)

echo "== negative control: the wiring checks fail on a broken copy =="
negative_control git-share-css crates/ax-web/web-ui/src/styles/13-policy.css 'git-share-dot' 'git-share-mark' \
  grep -q 'git-share-dot'
negative_control meta-status crates/ax-web/web-ui/src/components/PolicyMetaView.tsx 'GitShareStatus' 'GitShareState' \
  grep -q 'GitShareStatus'

echo "== manual mutation of isGitShared =="
# Only a failing test kills a mutant; a syntax error is a run error (scripts/lib/mutate.sh).
UTIL="crates/ax-web/web-ui/src/components/ui/policyListUtils.ts"
git_share_tests() { (cd crates/ax-web/web-ui && node --experimental-strip-types --test src/gitShare.test.ts); }
ax_mutate git_share_tests "$UTIL" \
  'if (enabled === false) return false;' 'if (enabled === false) return true;'
ax_mutate git_share_tests "$UTIL" \
  "const GIT_SHARED_SCOPES = new Set(['project', 'workspace']);" \
  "const GIT_SHARED_SCOPES = new Set(['project']);"
ax_mutate git_share_tests "$UTIL" \
  "const GIT_SHARED_SCOPES = new Set(['project', 'workspace']);" \
  "const GIT_SHARED_SCOPES = new Set(['project', 'workspace', 'private']);"
ax_mutate git_share_tests "$UTIL" \
  'return GIT_SHARED_SCOPES.has(normalizePolicyScope(scope));' 'return true;'
ax_mutants_summary

echo "gauntlet-policy-git-share-indicator: ok"
