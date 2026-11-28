#!/usr/bin/env bash
# Gauntlet: Command Center theme contrast (the azure-light and azure-dark
# presets). The wcag-contrast rule names this script as its enforcement.
set -euo pipefail
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
cd "$ROOT"
UI="crates/ax-web/web-ui"
THEMES="$ROOT/$UI/src/lib/themes.ts"

# require <description> <grep args...>: fail unless the pattern is found.
require() {
  local what="$1"
  shift
  grep -q "$@" || { echo "FAIL: $what" >&2; exit 1; }
}

echo "== theme unit tests =="
(cd "$UI" && node --experimental-strip-types --test src/lib/themes.test.ts)

echo "== wiring =="
require "missing azure-light preset" "id: 'azure-light'" "$THEMES"
require "missing azure-dark preset" "id: 'azure-dark'" "$THEMES"
require "applyTheme missing --accent-text" "setProperty('--accent-text'" "$THEMES"
require "applyTheme missing --accent-on-fill" "setProperty('--accent-on-fill', onFill)" "$THEMES"
require "accent-on-fill ink must come from the painted fill" "const onFill = statusbarInk(fill).fg" "$THEMES"
require "status bar ink must come from the status bar surface" "const ink = statusbarInk(theme.bgSide)" "$THEMES"
require "no stylesheet uses --accent-on-fill" -r "var(--accent-on-fill" "$UI/src/styles"
require "missing CRITICAL wcag-contrast rule" "id: wcag-contrast" .agents/rules/wcag-contrast.mdc

echo "== negative control (grep gate) =="
if (require "expected" "id: 'theme-that-does-not-exist'" "$THEMES") 2>/dev/null; then
  echo "FAIL: negative control passed (vacuous grep)" >&2
  exit 1
fi

echo "== tsc =="
(cd "$UI" && npx tsc --noEmit)

echo "== manual mutation =="
ORIG="$(mktemp)"
OUT="$(mktemp)"
cp "$THEMES" "$ORIG"
trap 'cp "$ORIG" "$THEMES"; rm -f "$ORIG" "$OUT"' EXIT
kill_mutant() {
  local name="$1"
  if (cd "$UI" && node --experimental-strip-types --test src/lib/themes.test.ts) >"$OUT" 2>&1; then
    echo "FAIL: mutant $name survived" >&2
    cat "$OUT" >&2
    exit 1
  fi
  echo "killed $name"
  cp "$ORIG" "$THEMES"
}

perl -i -pe "s/id: 'azure-dark'/id: 'azure-night'/" "$THEMES"
kill_mutant "rename-dark-id"

perl -i -pe "s/const ink = statusbarInk\(theme.bgSide\)/const ink = statusbarInk(fill)/" "$THEMES"
kill_mutant "statusbar-ink-from-accent"

perl -i -pe "s/light:'#386900'/light:'#b9ff6a'/" "$THEMES"
kill_mutant "light-accent-unreadable"

diff -q "$ORIG" "$THEMES" >/dev/null

echo "gauntlet-macos-theme: ok"
