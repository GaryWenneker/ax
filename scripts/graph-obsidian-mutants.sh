#!/usr/bin/env bash
# Manual mutants for the Obsidian-style graph modules. Each mutant must change
# the file (proved by cmp) and must make the unit suite fail.
set -euo pipefail
cd "$(dirname "$0")/../crates/ax-web/web-ui"
lib=src/lib
killed=0
total=0

mutate() { # file, python-literal old, python-literal new
  local file="$1" old="$2" new="$3"
  total=$((total + 1))
  cp "$file" "$file.orig"
  python3 - "$file" "$old" "$new" <<'EOF'
import sys
p, old, new = sys.argv[1:]
s = open(p).read()
if old not in s:
    sys.exit(f"mutant pattern not found in {p}: {old}")
open(p, "w").write(s.replace(old, new, 1))
EOF
  if cmp -s "$file" "$file.orig"; then
    mv "$file.orig" "$file"; echo "NOT APPLIED: $old"; exit 1
  fi
  if node --test "$lib/graphObsidian.test.ts" >/dev/null 2>&1; then
    echo "SURVIVED: $file :: $old -> $new"
  else
    killed=$((killed + 1)); echo "killed: $file :: $old -> $new"
  fi
  mv "$file.orig" "$file"
}

mutate $lib/graphFilter.ts "if (!opts.orphans)" "if (opts.orphans)"
mutate $lib/graphFilter.ts "n.kind === 'doc'" "n.kind === 'function'"
mutate $lib/graphFilter.ts "!kept.has(e.source) ||" "!kept.has(e.source) &&"
mutate $lib/graphFilter.ts "if (!q) return false;" "if (!q) return true;"
mutate $lib/graphHover.ts "nodes.add(inc.node);" ""
mutate $lib/graphHover.ts "index[e.target]?.push({ node: e.source, edge: i });" ""
mutate $lib/graphHover.ts "alpha: 0.9 * (1 - t)" "alpha: 0.9 * t"
mutate $lib/graphHover.ts "(((timeMs % PULSE_MS) + PULSE_MS) % PULSE_MS)" "timeMs"
mutate $lib/graphForces.ts "r * Math.sqrt((i + 0.5)" "2 * r * Math.sqrt((i + 0.5)"
mutate $lib/graphForces.ts "charge: -8 * s.repelForce" "charge: 8 * s.repelForce"
mutate $lib/graphLabels.ts "if (hit) continue;" ""
mutate $lib/graphLabels.ts "return Math.min(LABEL_MAX_PX, grown);" "return grown;"
mutate $lib/graphLabels.ts "Math.max(1, scale), 0.25)" "Math.max(1, scale), 1)"
mutate $lib/graphLabels.ts "boxes[b].priority - boxes[a].priority" "boxes[a].priority - boxes[b].priority"
mutate $lib/graphLabels.ts "b.x < p.x + p.w" "b.x <= p.x + p.w"
mutate $lib/graphSettings.ts "...(parsed as Partial<GraphSettings>)" ""

node --test "$lib/graphObsidian.test.ts" >/dev/null
echo "mutants killed: $killed/$total"
[ "$killed" -eq "$total" ]
