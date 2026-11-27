#!/usr/bin/env bash
# Gauntlet: Command Center macOS theme preset.
set -euo pipefail
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
cd "$ROOT"
source "$ROOT/scripts/lib/mutate.sh"
source "$ROOT/scripts/lib/negative-control.sh"

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

echo "== negative control: the wiring checks fail on a broken copy =="
negative_control statusbar-ink crates/ax-web/web-ui/src/lib/themes.ts 'statusbarInk(fill)' 'statusbarInk(bg)' \
  grep -q 'statusbarInk(fill)'
negative_control accent-on-fill crates/ax-web/web-ui/src/index.css 'var(--accent-on-fill' 'var(--accent-fill' \
  grep -q 'var(--accent-on-fill'

echo "== tsc =="
(cd crates/ax-web/web-ui && npx tsc --noEmit)

echo "== manual mutation =="
# Only a failing test kills a mutant; a syntax error is a run error (scripts/lib/mutate.sh).
THEMES="crates/ax-web/web-ui/src/lib/themes.ts"
theme_tests() { (cd crates/ax-web/web-ui && node --experimental-strip-types --test src/lib/themes.test.ts); }
ax_mutate theme_tests "$THEMES" "id: 'macos'" "id: 'mac-os'"
ax_mutate theme_tests "$THEMES" "accent: '#64d2ff'" "accent: '#0a84ff'"
ax_mutate theme_tests "$THEMES" 'statusbarInk(fill)' 'statusbarInk(theme.statusbarBg || theme.accent)'
ax_mutants_summary

echo "gauntlet-macos-theme: ok"
