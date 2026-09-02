#!/usr/bin/env bash
# Placement gates for Verbose MCP logging (Settings vs Logging).
set -euo pipefail
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
cd "$ROOT"

fail() { echo "FAIL: $*" >&2; exit 1; }

# Negative control: this script must fail when the Logging switch still exists.
if [[ "${GAUNTLET_NEGATIVE:-}" == "1" ]]; then
  echo "== negative control (expect fail) =="
fi

echo "== S1 Settings has Verbose MCP logging toggle =="
grep -q 'title="Verbose MCP logging"' crates/ax-web/web-ui/src/pages/Settings.tsx \
  || fail "Settings.tsx missing SettingRow title Verbose MCP logging"
grep -q 'label="Verbose MCP logging"' crates/ax-web/web-ui/src/pages/Settings.tsx \
  || fail "Settings.tsx missing Toggle label"
grep -q 'setUiVerboseMcp' crates/ax-web/web-ui/src/pages/Settings.tsx \
  || fail "Settings.tsx missing setUiVerboseMcp"
grep -q 'verbose_mcp' crates/ax-web/web-ui/src/pages/Settings.tsx \
  || fail "Settings.tsx missing verbose_mcp persist"

echo "== S2 Logging has no switch =="
if grep -n 'aria-label="Verbose MCP logging"' crates/ax-web/web-ui/src/pages/Logging.tsx; then
  fail "Logging.tsx still has Verbose MCP logging switch"
fi
if grep -n 'role="switch"' crates/ax-web/web-ui/src/pages/Logging.tsx; then
  fail "Logging.tsx still has a role=switch"
fi
grep -q "navigateRoute({ page: 'settings' })" crates/ax-web/web-ui/src/pages/Logging.tsx \
  || fail "Logging.tsx missing Settings navigation hint"

echo "== S3 docs =="
if grep -R --include='*.md' -n 'Logging → Verbose MCP logging' README.md site/src/content/docs; then
  fail "docs still say Logging → Verbose MCP logging"
fi
grep -q 'Settings → Interface → Verbose MCP logging' README.md \
  || fail "README missing Settings path"

echo "== Command Center (Ship) has no verbose toggle =="
if grep -n 'verbose_mcp\|Verbose MCP logging' crates/ax-web/web-ui/src/pages/Ship.tsx; then
  fail "Ship.tsx must not host Verbose MCP logging"
fi

echo "== web-ui tsc =="
(cd crates/ax-web/web-ui && npx tsc --noEmit)

echo "gauntlet-verbose-mcp-settings: ok"
