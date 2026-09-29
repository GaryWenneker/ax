#!/usr/bin/env bash
# Gauntlet for docs/specs/calm-list-style.md. Needs `ax web` on :7070 built from this tree.
set -euo pipefail
cd "$(dirname "$0")/../crates/ax-web/web-ui"

echo "== node:test (web-ui unit suite)"
node --test src/*.test.ts src/lib/*.test.ts

echo "== tsc --noEmit"
npx tsc --noEmit

echo "== production build"
npm run build

echo "== served bundle matches dist"
want=$(grep -o 'assets/index-[^"]*\.js' dist/index.html)
got=$(curl -fsS http://127.0.0.1:7070/ | grep -o 'assets/index-[^"]*\.js')
if [ "$want" != "$got" ]; then
  echo "FAIL: ax web serves $got, dist has $want (run scripts/reinstall-cli.sh and restart ax web)"
  exit 1
fi

echo "== playwright (system Chrome)"
npx playwright test e2e/calm-lists.spec.ts e2e/logging-text.spec.ts e2e/nodes-scroll.spec.ts \
  --project=system-chrome --reporter=line

echo "GAUNTLET OK"
