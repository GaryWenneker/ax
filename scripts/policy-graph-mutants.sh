#!/usr/bin/env bash
# Manual mutants for the policy graph (server endpoint + client model). Each
# mutant must change the file and must make a test in its suite fail
# (scripts/lib/mutate.sh).
set -euo pipefail
root="$(cd "$(dirname "$0")/.." && pwd)"
source "$root/scripts/lib/mutate.sh"
web="$root/crates/ax-web/web-ui"

ts_suite() { (cd "$web" && node --test src/lib/policyGraph.test.ts src/lib/graphObsidian.test.ts src/lib/autoGroup.test.ts); }
mem_suite() { (cd "$root" && cargo test -q -p ax-web --lib memory && cargo test -q -p ax-web --test memory_files); }
turn_suite() { (cd "$root" && cargo test -q -p ax-cli --bin ax turn_hook); }
rs_suite() { (cd "$root" && cargo test -q -p ax-web --lib links_api && cargo test -q -p ax-web --test links_api); }

lib="$web/src/lib"
ax_mutate ts_suite "$lib/policyGraph.ts" "keys.has(e.source) && keys.has(e.target)" "keys.has(e.source)"
ax_mutate ts_suite "$lib/policyGraph.ts" "degree.set(e.target, (degree.get(e.target) ?? 0) + 1);" ""
ax_mutate ts_suite "$lib/policyGraph.ts" "global: n.origin === 'global'" "global: false"
ax_mutate ts_suite "$lib/policyGraph.ts" "e.source === key).map((e) => e.target)" "e.target === key).map((e) => e.source)"
ax_mutate ts_suite "$lib/policyGraph.ts" "if (!payload) return { nodes: [], edges: [] };" ""
ax_mutate ts_suite "$lib/graphSettings.ts" "storage?.setItem(key," "storage?.setItem(GRAPH_SETTINGS_KEY,"
ax_mutate ts_suite "$lib/graphLabels.ts" "(scale - start) / 0.4" "(scale - start) / 4"

api="$root/crates/ax-web/src/links_api.rs"
ax_mutate rs_suite "$api" "if ti == si || entries[ti].turn || !seen.insert((si, ti))" "if entries[ti].turn || !seen.insert((si, ti))"
ax_mutate rs_suite "$api" "if ti == si || entries[ti].turn || !seen.insert((si, ti))" "if ti == si || entries[ti].turn"
ax_mutate rs_suite "$api" "entry.turn = m.kind == ax_memory::TURN_KIND || m.kind == \"policy\";" ""
ax_mutate rs_suite "$api" "        if e.turn {
            continue;
        }" ""
ax_mutate rs_suite "$api" "            v[\"label\"] = json!(e.title);" ""

ts_suite >/dev/null
rs_suite >/dev/null
ag="$lib/autoGroup.ts"
ax_mutate ts_suite "$ag" "const targets = groups.filter((g) => g.id !== UNGROUPED);" "const targets = groups;"
ax_mutate ts_suite "$ag" "const loose = items.filter((i) => i.group === UNGROUPED || !docs.has(i.group));" "const loose = items;"
ax_mutate ts_suite "$ag" "score >= AUTO_GROUP_THRESHOLD ? best.id : null" "best.id"
ax_mutate ts_suite "$ag" "if (!best || score > best.score)" "if (!best || score < best.score)"
ax_mutate ts_suite "$ag" "tokens([g.label, ...(g.aliases ?? [])].join(' '))" "[]"

mem="$root/crates/ax-web/src/memory.rs"
ax_mutate mem_suite "$mem" "let unix = path.starts_with('/') && !path.starts_with(\"//\");" "let unix = true;"
hook="$root/crates/ax-cli/src/commands/turn_hook.rs"
ax_mutate turn_suite "$hook" ".or_insert(ax_memory::FileChangeKind::Added);" ".or_insert(ax_memory::FileChangeKind::Modified);"
ax_mutate turn_suite "$hook" "if wanted.contains(path.as_str()) {" "if true {"
ax_mutate turn_suite "$hook" "body.push_str(\"\\n\\nChanges:\\n\");" "body.push_str(\"\\n\\nChanged:\\n\");"

ax_mutants_summary
