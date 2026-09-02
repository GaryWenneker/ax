#!/usr/bin/env bash
# Gauntlet: git-share indicator in Command Center policy lists.
set -euo pipefail
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
cd "$ROOT"

echo "== isGitShared unit tests =="
(cd crates/ax-web/web-ui && node --experimental-strip-types --test src/gitShare.test.ts)

echo "== UI wiring =="
grep -q 'GitShareDot' crates/ax-web/web-ui/src/pages/PolicyRules.tsx \
  || { echo "FAIL: Rules missing GitShareDot" >&2; exit 1; }
grep -q 'GitShareDot' crates/ax-web/web-ui/src/pages/PolicySkills.tsx \
  || { echo "FAIL: Skills missing GitShareDot" >&2; exit 1; }
grep -q 'GitShareStatus' crates/ax-web/web-ui/src/components/PolicyMetaView.tsx \
  || { echo "FAIL: meta missing GitShareStatus" >&2; exit 1; }
grep -q 'git-share-dot' crates/ax-web/web-ui/src/index.css \
  || { echo "FAIL: missing git-share-dot CSS" >&2; exit 1; }

echo "== tsc =="
(cd crates/ax-web/web-ui && npx tsc --noEmit)

echo "== negative control (grep gate must fail closed) =="
if grep -q 'GitShareDot_NOT_A_REAL_MARKER' crates/ax-web/web-ui/src/pages/PolicyRules.tsx; then
  echo "FAIL: negative control passed (vacuous grep)" >&2
  exit 1
fi

echo "== manual mutation of isGitShared =="
UTIL="$ROOT/crates/ax-web/web-ui/src/components/ui/policyListUtils.ts"
ORIG="$(mktemp)"
cp "$UTIL" "$ORIG"
kill_mutant() {
  local name="$1"
  if (cd crates/ax-web/web-ui && node --experimental-strip-types --test src/gitShare.test.ts) >/tmp/gitshare-mutant.out 2>&1; then
    echo "FAIL: mutant $name survived" >&2
    cat /tmp/gitshare-mutant.out >&2
    cp "$ORIG" "$UTIL"
    exit 1
  fi
  echo "killed $name"
}

perl -i -pe 's/if \(enabled === false\) return false;/if (enabled === false) return true;/' "$UTIL"
kill_mutant "disabled-returns-true"
cp "$ORIG" "$UTIL"

perl -i -pe "s/const GIT_SHARED_SCOPES = new Set\(\['project', 'workspace'\]\);/const GIT_SHARED_SCOPES = new Set(['project']);/" "$UTIL"
kill_mutant "drop-workspace"
cp "$ORIG" "$UTIL"

perl -i -pe "s/const GIT_SHARED_SCOPES = new Set\(\['project', 'workspace'\]\);/const GIT_SHARED_SCOPES = new Set(['project', 'workspace', 'private']);/" "$UTIL"
kill_mutant "private-is-shared"
cp "$ORIG" "$UTIL"

perl -i -pe 's/return GIT_SHARED_SCOPES.has\(normalizePolicyScope\(scope\)\);/return true;/' "$UTIL"
kill_mutant "always-true"
cp "$ORIG" "$UTIL"

diff -q "$ORIG" "$UTIL" >/dev/null
rm -f "$ORIG"

echo "gauntlet-policy-git-share-indicator: ok"
