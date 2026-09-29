#!/usr/bin/env bash
# Manual mutants for docs/specs/calm-list-style.md.
# UI mutants run against a Vite dev server (proxying /api to ax web on :7070), so no ax rebuild is needed.
set -euo pipefail
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
UI="$ROOT/crates/ax-web/web-ui"
PORT=5199
cd "$UI"

npx vite --port "$PORT" --strictPort >/tmp/calm-mutants-vite.log 2>&1 &
VITE_PID=$!
BACKUP=""
TARGET=""
cleanup() {
  if [ -n "$BACKUP" ] && [ -f "$BACKUP" ]; then cp "$BACKUP" "$TARGET"; rm -f "$BACKUP"; fi
  kill "$VITE_PID" 2>/dev/null || true
}
trap cleanup EXIT
for _ in $(seq 1 40); do curl -fsS -o /dev/null "http://127.0.0.1:$PORT/" && break; sleep 0.5; done
curl -fsS -o /dev/null "http://127.0.0.1:$PORT/" || { echo "FAIL: vite did not start"; exit 1; }

run_ui() { AX_WEB_URL="http://127.0.0.1:$PORT" npx playwright test e2e/calm-lists.spec.ts e2e/app-feel.spec.ts e2e/logging-blade.spec.ts --project=system-chrome --reporter=dot >/tmp/calm-mutant-run.log 2>&1; }
run_unit() { node --test src/lib/calmRows.test.ts >/tmp/calm-mutant-run.log 2>&1; }
run_stable() { AX_WEB_URL="http://127.0.0.1:$PORT" npx playwright test e2e/memory-stable.spec.ts e2e/blade-dismiss.spec.ts -g "does not move|deselected row|kind category|open row again" --project=system-chrome --reporter=dot >/tmp/calm-mutant-run.log 2>&1; }
run_sel_unit() { node --test src/lib/policySelection.test.ts src/memoryCategory.test.ts >/tmp/calm-mutant-run.log 2>&1; }

echo "== control: clean tree must pass"
run_ui || { echo "FAIL: clean UI run failed"; tail -20 /tmp/calm-mutant-run.log; exit 1; }
run_unit || { echo "FAIL: clean unit run failed"; exit 1; }

killed=0
total=0
mutant() {
  local name="$1" file="$2" from="$3" to="$4" runner="$5"
  if [[ -n "${ONLY:-}" && " $ONLY " != *" ${name%% *} "* ]]; then return; fi
  total=$((total + 1))
  TARGET="$UI/$file"
  BACKUP="$(mktemp)"
  cp "$TARGET" "$BACKUP"
  FROM="$from" TO="$to" node -e '
    const fs = require("fs"); const p = process.argv[1]; const s = fs.readFileSync(p, "utf8");
    if (!s.includes(process.env.FROM)) { console.error("mutant anchor not found"); process.exit(2); }
    fs.writeFileSync(p, s.replace(process.env.FROM, process.env.TO));' "$TARGET"
  if cmp -s "$TARGET" "$BACKUP"; then echo "FAIL: $name did not change $file"; exit 1; fi
  sleep 1.5
  if "$runner"; then
    echo "SURVIVED: $name"
  else
    echo "killed:   $name"
    killed=$((killed + 1))
  fi
  cp "$BACKUP" "$TARGET"; rm -f "$BACKUP"; BACKUP=""
}

FILES=(src/components/ui/PolicyCalmList.tsx src/components/ui/PageLayout.tsx src/index.css src/lib/calmRows.ts src/components/McpTraceLive.tsx src/lib/contextMenuGuard.ts src/pages/Memory.tsx src/memoryCategory.ts src/lib/policySelection.ts)
before=$(shasum "${FILES[@]}")

mutant "M1 rows lose graph variant" src/components/ui/PolicyCalmList.tsx 'variant="graph"' 'variant={undefined}' run_ui
mutant "M2 selected class dropped" src/components/ui/PageLayout.tsx "\${selected ? ' page-item--selected' : ''}" "" run_ui
mutant "M3 group collapse ignored" src/components/ui/PolicyCalmList.tsx '{open && (' '{(' run_ui
mutant "M4 tool badge width reset removed" src/index.css '.mcp-trace-calm .mcp-col-tool { width: auto; min-width: 0; }' '' run_ui
mutant "M5 plural always" src/lib/calmRows.ts "n === 1 ? '' : 's'" "'s'" run_unit
# docs/specs/logging-look.md
mutant "M6 milliseconds kept" src/lib/calmRows.ts "time.replace(/\\.\\d+\$/, '')" "time" run_unit
mutant "M7 prompt in shown as returned" src/lib/calmRows.ts "if (kind === 'inbound') return 'in';" "if (kind === 'inbound') return 'out';" run_ui
mutant "M8 one color for every kind" src/components/McpTraceLive.tsx 'mcp-kind-badge mcp-kind-badge--${e.kind}' 'mcp-kind-badge' run_ui
# docs/specs/app-feel.md
mutant "M10 right-click guard does nothing" src/lib/contextMenuGuard.ts 'if (!keepsBrowserMenu(e.target)) e.preventDefault();' '' run_ui
mutant "M11 text fields lose their menu" src/lib/contextMenuGuard.ts 'target.closest(EDITABLE) !== null' 'false' run_ui
mutant "M12 icon group spread out" src/index.css '.calm-list .calm-row-aside { gap: 2px; }' '.calm-list .calm-row-aside { gap: 6px; }' run_ui
mutant "M13 toggles not dimmed" src/index.css '.calm-list .settings-toggle { opacity: 0.5;' '.calm-list .settings-toggle { opacity: 1;' run_ui
mutant "M14 row hover leaves toggle dim" src/index.css '.calm-list .page-item--graph:is(:hover, :focus-within) .settings-toggle { opacity: 1; }' '' run_ui
mutant "M15 global rows lose their empty slots" src/components/ui/PolicyCalmList.tsx '<span className="calm-row-slot calm-row-slot--toggle" aria-hidden="true" />' 'null' run_ui
# docs/specs/selected-row-text.md
mutant "M16 selected title not brighter" src/index.css '.page-item--selected .page-item-title {
  color: #ffffff;' '.page-item--selected .page-item-title {
  color: #cccccc;' run_ui
mutant "M17 lists bounce and chain scroll" src/index.css ':is(.memory-vault-list, .mcp-trace-scroller) { overscroll-behavior: none; }' '' run_ui
mutant "M18 inner Logging list traps the wheel" src/index.css '.mcp-trace-scroller .page-item-list.mcp-trace-calm { overflow: visible; overscroll-behavior: auto; }' '' run_ui
mutant "M18 inner Logging list traps the wheel" src/index.css '.mcp-trace-scroller .page-item-list.mcp-trace-calm { overflow: visible; overscroll-behavior: auto; }' '' run_ui
# docs/specs/logging-blade.md
mutant "M19 detail opens without the blade class" src/components/McpTraceLive.tsx 'className={`memory-blade mcp-blade mcp-blade--${selected.kind}`}' 'className={`mcp-blade mcp-blade--${selected.kind}`}' run_ui
mutant "M20 step badges lose kind colors" src/index.css ':is(html, html[data-ax-theme]) .mcp-blade-steps .page-item-badge.mcp-kind-badge {' '.no-such-steps .page-item-badge.mcp-kind-badge {' run_ui
mutant "M21 blade keeps the offline blur" src/index.css '.workspace:has(.mcp-blade) .mcp-trace-shell--offline .mcp-trace-scroller { filter: none; opacity: 1; }' '' run_ui
mutant "M9 page card keeps its own background" src/index.css ':is(html, html[data-ax-theme]) body .settings-card {' '.no-such-card {' run_ui
mutant "M22 blade narrows the whole page again" src/index.css '.workspace:has(.memory-blade) > .container { padding-right: 1.6rem; }' '' run_stable
mutant "M23 mouse focus keeps the toggle bright" src/index.css ':is(.memory-vault-list, .calm-list) .page-item--graph:is(:focus-within):not(:hover):not(:has(:focus-visible)) > .settings-toggle.on { opacity: 0; }' '' run_stable
mutant "M24 memory rows lose their category class" src/pages/Memory.tsx 'className={`memory-cat--${memoryCategory(m.kind)}${' 'className={`${' run_stable
mutant "M25 git memories fall into Note" src/memoryCategory.ts "  git: 'commit'," '' run_sel_unit
mutant "M26 same row no longer deselects" src/lib/policySelection.ts 'if (g.selected.size === 1 && g.selected.has(id)) {' 'if (false) {' run_sel_unit
mutant "M27 Logging row click no longer toggles" src/components/McpTraceLive.tsx 'setSelectedId((open) => (open === id ? null : id));' 'setSelectedId(id);' run_stable

after=$(shasum "${FILES[@]}")
if [ "$before" != "$after" ]; then echo "FAIL: sources not restored after mutants"; exit 1; fi
echo "sources restored: yes"
echo "mutants killed: $killed/$total"
[ "$killed" -eq "$total" ]
