#!/usr/bin/env bash
# Manual mutants for the policy link-origin fix. Each mutant must make the unit suite fail.
set -euo pipefail
cd "$(dirname "$0")/../crates/ax-web/web-ui"

run_suite() { node --experimental-strip-types --test src/lib/policySelection.test.ts src/lib/routes.test.ts >/dev/null 2>&1; }

run_suite || { echo "baseline suite is red"; exit 1; }

killed=0; total=0
mutant() {
  local file=$1 from=$2 to=$3
  total=$((total + 1))
  cp "$file" "$file.orig"
  python3 - "$file" "$from" "$to" <<'PY'
import sys
p, a, b = sys.argv[1:]
s = open(p).read()
if a not in s:
    sys.exit(f"mutant anchor missing: {a}")
open(p, "w").write(s.replace(a, b, 1))
PY
  if run_suite; then echo "SURVIVED: $file :: $from"; else echo "killed:   $file :: $from"; killed=$((killed + 1)); fi
  mv "$file.orig" "$file"
}

mutant src/lib/routes.ts "origin: state.origin ?? null," "origin: null,"
mutant src/lib/routes.ts "projectId: state.projectId ?? null," "projectId: null,"
mutant src/lib/policySelection.ts "projectId: routeProjectId ?? globalRow?.projectId" "projectId: globalRow?.projectId"
mutant src/lib/policySelection.ts "if (routeOrigin) return {};" "if (routeOrigin) return { origin: 'global' };"
mutant src/lib/policySelection.ts "return !hasLocal && globalRow" "return globalRow"

run_suite || { echo "suite red after restore"; exit 1; }
echo "$killed/$total killed"
[ "$killed" -eq "$total" ]
