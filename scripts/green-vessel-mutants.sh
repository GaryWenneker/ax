#!/usr/bin/env bash
# Manual mutants for the Green Vessel intro. Each mutant must change the file
# and must make src/greenVesselIntro.test.ts fail. Restores the original after.
set -euo pipefail
cd "$(dirname "$0")/../crates/ax-web/web-ui"
file=src/greenVesselIntro.ts
css=src/greenVesselIntro.css
killed=0
total=0

mutate() {
  local target="$1" old="$2" new="$3"
  total=$((total + 1))
  cp "$target" "$target.orig"
  python3 - "$target" "$old" "$new" <<'EOF'
import sys
p, old, new = sys.argv[1:]
s = open(p).read()
if old not in s:
    sys.exit(f"mutant pattern not found in {p}: {old}")
open(p, "w").write(s.replace(old, new, 1))
EOF
  if cmp -s "$target" "$target.orig"; then
    mv "$target.orig" "$target"
    echo "NOT APPLIED: $old"
    exit 1
  fi
  if node --experimental-strip-types --test src/greenVesselIntro.test.ts >/dev/null 2>&1; then
    echo "SURVIVED: $target :: $old -> $new"
  else
    killed=$((killed + 1))
    echo "killed: $target :: $old -> $new"
  fi
  mv "$target.orig" "$target"
}

mutate "$file" "el.classList.add('is-playing');" ""
mutate "$file" "armUnlock(doc, play)" "undefined"
mutate "$file" "el.addEventListener('animationend', remove, { once: true });" ""
mutate "$file" "if (el.dataset.axVessel === 'started') return;" ""
mutate "$file" "el.dataset.axVessel = 'started';" "el.dataset.axVessel = 'started'; doc.defaultView?.localStorage.setItem('skip', '1');"
mutate "$css" "animation: ax-green-vessel 3.6s ease-in-out forwards;" "animation: ax-green-vessel 9s ease-in-out forwards;"

node --experimental-strip-types --test src/greenVesselIntro.test.ts >/dev/null
echo "mutants killed: $killed/$total"
[ "$killed" -eq "$total" ]
