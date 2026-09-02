#!/usr/bin/env bash
# Gauntlet: Command Center macOS theme preset.
set -euo pipefail
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
cd "$ROOT"

echo "== theme unit tests =="
(cd crates/ax-web/web-ui && node --experimental-strip-types --test src/lib/themes.test.ts)

echo "== wiring =="
grep -q "id: 'macos'" crates/ax-web/web-ui/src/lib/themes.ts \
  || { echo "FAIL: missing macos preset" >&2; exit 1; }
grep -q "setProperty('--accent-text'" crates/ax-web/web-ui/src/lib/themes.ts \
  || { echo "FAIL: applyTheme missing --accent-text" >&2; exit 1; }
grep -q "setProperty('--accent-on-fill'" crates/ax-web/web-ui/src/lib/themes.ts \
  || { echo "FAIL: applyTheme missing --accent-on-fill" >&2; exit 1; }
grep -q 'accent: '\''#64d2ff'\''' crates/ax-web/web-ui/src/lib/themes.ts \
  || { echo "FAIL: macos accent must be #64d2ff" >&2; exit 1; }
grep -q 'var(--accent-on-fill' crates/ax-web/web-ui/src/index.css \
  || { echo "FAIL: macos nav must use --accent-on-fill" >&2; exit 1; }
grep -q 'policy-table thead th:first-child' crates/ax-web/web-ui/src/index.css \
  || { echo "FAIL: missing grouped-table header indent" >&2; exit 1; }
grep -q 'flex-direction: row' crates/ax-web/web-ui/src/index.css \
  || { echo "FAIL: missing tag row wrap" >&2; exit 1; }
grep -q 'statusbarInk(fill)' crates/ax-web/web-ui/src/lib/themes.ts \
  || { echo "FAIL: statusbar ink must use painted fill" >&2; exit 1; }
grep -q 'id: wcag-contrast' .agents/rules/wcag-contrast.mdc \
  || { echo "FAIL: missing CRITICAL wcag-contrast rule" >&2; exit 1; }

echo "== negative control (grep gate) =="
if grep -q 'data-ax-theme="macos-does-not-exist"' crates/ax-web/web-ui/src/index.css; then
  echo "FAIL: negative control passed (vacuous grep)" >&2
  exit 1
fi

echo "== tsc =="
(cd crates/ax-web/web-ui && npx tsc --noEmit)

echo "== manual mutation =="
THEMES="$ROOT/crates/ax-web/web-ui/src/lib/themes.ts"
ORIG="$(mktemp)"
cp "$THEMES" "$ORIG"
kill_mutant() {
  local name="$1"
  if (cd crates/ax-web/web-ui && node --experimental-strip-types --test src/lib/themes.test.ts) >/tmp/macos-theme-mutant.out 2>&1; then
    echo "FAIL: mutant $name survived" >&2
    cat /tmp/macos-theme-mutant.out >&2
    cp "$ORIG" "$THEMES"
    exit 1
  fi
  echo "killed $name"
}

perl -i -pe "s/id: 'macos'/id: 'mac-os'/" "$THEMES"
kill_mutant "rename-id"
cp "$ORIG" "$THEMES"

perl -i -pe "s/accent: '#64d2ff'/accent: '#0a84ff'/" "$THEMES"
kill_mutant "wrong-accent"
cp "$ORIG" "$THEMES"

perl -i -pe "s/statusbarInk\(fill\)/statusbarInk(theme.statusbarBg || theme.accent)/" "$THEMES"
kill_mutant "ink-from-statusbarBg"
cp "$ORIG" "$THEMES"

diff -q "$ORIG" "$THEMES" >/dev/null
rm -f "$ORIG"

echo "gauntlet-macos-theme: ok"
