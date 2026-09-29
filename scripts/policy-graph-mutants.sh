#!/usr/bin/env bash
# Manual mutants for the policy graph (server endpoint + client model). Each
# mutant must change the file (proved by cmp) and must make its suite fail.
set -euo pipefail
root="$(cd "$(dirname "$0")/.." && pwd)"
web="$root/crates/ax-web/web-ui"
killed=0
total=0

ts_suite() { (cd "$web" && node --test src/lib/policyGraph.test.ts src/lib/graphObsidian.test.ts src/lib/autoGroup.test.ts); }
mem_suite() { (cd "$root" && cargo test -q -p ax-web --lib memory && cargo test -q -p ax-web --test memory_files); }
turn_suite() { (cd "$root" && cargo test -q -p ax-cli --bin ax turn_hook); }
rs_suite() { (cd "$root" && cargo test -q -p ax-web --lib links_api && cargo test -q -p ax-web --test links_api); }

mutate() { # suite, file, python-literal old, python-literal new
  local suite="$1" file="$2" old="$3" new="$4"
  total=$((total + 1))
  cp "$file" "$file.orig"
  python3 - "$file" "$old" "$new" <<'PY'
import sys
p, old, new = sys.argv[1:]
s = open(p).read()
if old not in s:
    sys.exit(f"mutant pattern not found in {p}: {old}")
open(p, "w").write(s.replace(old, new, 1))
PY
  if cmp -s "$file" "$file.orig"; then
    mv "$file.orig" "$file"; echo "NOT APPLIED: $old"; exit 1
  fi
  if "$suite" >/dev/null 2>&1; then
    echo "SURVIVED: $file :: $old -> $new"
  else
    killed=$((killed + 1)); echo "killed: $file :: $old -> $new"
  fi
  mv "$file.orig" "$file"
}

lib="$web/src/lib"
mutate ts_suite "$lib/policyGraph.ts" "keys.has(e.source) && keys.has(e.target)" "keys.has(e.source)"
mutate ts_suite "$lib/policyGraph.ts" "degree.set(e.target, (degree.get(e.target) ?? 0) + 1);" ""
mutate ts_suite "$lib/policyGraph.ts" "global: n.origin === 'global'" "global: false"
mutate ts_suite "$lib/policyGraph.ts" "e.source === key).map((e) => e.target)" "e.target === key).map((e) => e.source)"
mutate ts_suite "$lib/policyGraph.ts" "if (!payload) return { nodes: [], edges: [] };" ""
mutate ts_suite "$lib/graphSettings.ts" "storage?.setItem(key," "storage?.setItem(GRAPH_SETTINGS_KEY,"
mutate ts_suite "$lib/graphLabels.ts" "(scale - start) / 0.4" "(scale - start) / 4"

api="$root/crates/ax-web/src/links_api.rs"
mutate rs_suite "$api" "if ti == si || entries[ti].turn || !seen.insert((si, ti))" "if entries[ti].turn || !seen.insert((si, ti))"
mutate rs_suite "$api" "if ti == si || entries[ti].turn || !seen.insert((si, ti))" "if ti == si || entries[ti].turn"
mutate rs_suite "$api" "entry.turn = m.kind == ax_memory::TURN_KIND;" ""
mutate rs_suite "$api" "        if e.turn {
            continue;
        }" ""
mutate rs_suite "$api" "            v[\"label\"] = json!(e.title);" ""

ts_suite >/dev/null
rs_suite >/dev/null
ag="$lib/autoGroup.ts"
mutate ts_suite "$ag" "const targets = groups.filter((g) => g.id !== UNGROUPED);" "const targets = groups;"
mutate ts_suite "$ag" "const loose = items.filter((i) => i.group === UNGROUPED || !docs.has(i.group));" "const loose = items;"
mutate ts_suite "$ag" "score >= AUTO_GROUP_THRESHOLD ? best.id : null" "best.id"
mutate ts_suite "$ag" "if (!best || score > best.score)" "if (!best || score < best.score)"
mutate ts_suite "$ag" "tokens([g.label, ...(g.aliases ?? [])].join(' '))" "[]"

mem="$root/crates/ax-web/src/memory.rs"
mutate mem_suite "$mem" "let unix = path.starts_with('/') && !path.starts_with(\"//\");" "let unix = true;"
hook="$root/crates/ax-cli/src/commands/turn_hook.rs"
mutate turn_suite "$hook" ".or_insert(ax_memory::FileChangeKind::Added);" ".or_insert(ax_memory::FileChangeKind::Modified);"
mutate turn_suite "$hook" "if wanted.contains(path.as_str()) {" "if true {"
mutate turn_suite "$hook" "body.push_str(\"\\n\\nChanges:\\n\");" "body.push_str(\"\\n\\nChanged:\\n\");"

echo "mutants killed: $killed/$total"
[ "$killed" -eq "$total" ]
