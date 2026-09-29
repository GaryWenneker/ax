#!/usr/bin/env bash
# Manual mutants for the WYSIWYG body editor and the [[ link picker
# (docs/specs/policy-body-wysiwyg.md). Each mutant must change the file (proved by cmp)
# and must make its suite fail. Browser mutants run against a Vite dev server that
# proxies /api to a running `ax web` on :7070.
set -euo pipefail
root="$(cd "$(dirname "$0")/.." && pwd)"
ui="$root/crates/ax-web/web-ui"
killed=0
total=0
port=5199

curl -sf -o /dev/null http://127.0.0.1:7070/ || { echo "ax web is not running on :7070"; exit 1; }
(cd "$ui" && npx vite --port "$port" --strictPort >/tmp/wysiwyg-mutants-vite.log 2>&1) &
vite_pid=$!
trap 'pkill -P "$vite_pid" 2>/dev/null || true; kill "$vite_pid" 2>/dev/null || true' EXIT
for _ in $(seq 1 60); do curl -sf -o /dev/null "http://127.0.0.1:$port/" && break; sleep 0.5; done
curl -sf -o /dev/null "http://127.0.0.1:$port/" || { echo "vite did not start"; exit 1; }

node_suite() { (cd "$ui" && node --experimental-strip-types --test src/lib/linkPicker.test.ts src/lib/policyBodyView.test.ts); }
rust_suite() { (cd "$root" && env -u CARGO_TARGET_DIR cargo test -q -p ax-policy --lib w7_link_target); }
e2e() { (cd "$ui" && AX_WEB_URL="http://127.0.0.1:$port" env -u PLAYWRIGHT_BROWSERS_PATH npx playwright test e2e/policy-wysiwyg.spec.ts --project=system-chrome -g "$1"); }
e2e_roundtrip() { e2e "W2/W4"; }
e2e_norewrite() { e2e "W3|W5"; }
e2e_esc() { e2e "Esc closes"; }
e2e_restore() { e2e "restored revision"; }
e2e_md_pick() { e2e "Markdown view inserts"; }
e2e_rich_pick() { e2e "WYSIWYG view inserts"; }

# Baseline: every suite must pass unmutated, or a "kill" proves nothing.
for s in node_suite rust_suite e2e_roundtrip e2e_norewrite e2e_esc e2e_rich_pick e2e_restore e2e_md_pick; do
  "$s" >/dev/null 2>&1 || { echo "BASELINE FAILS: $s"; exit 1; }
done

mutate() { # suite, file, old, new
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
  sleep 1
  if "$suite" >/dev/null 2>&1; then
    echo "SURVIVED: $file :: $old -> $new"
  else
    killed=$((killed + 1)); echo "killed: $file :: $old -> $new"
  fi
  mv "$file.orig" "$file"
  touch "$file" # the restored copy's mtime predates the mutant build; cargo would keep the mutant
}

lp="$ui/src/lib/linkPicker.ts"
mutate node_suite "$lp" "    .filter((i) => i.key !== selfKey)" ""
mutate node_suite "$lp" "    .slice(0, LINK_PICKER_MAX);" ";"
mutate node_suite "$lp" "i.label.toLowerCase().includes(q)" "false"
mutate node_suite "$lp" "  if (/[\\]\\n[]/.test(query)) return null;" ""
mutate node_suite "$lp" "  return \`[[\${item.target}]]\`;" "  return \`[[\${item.label}]]\`;"
bv="$ui/src/lib/policyBodyView.ts"
mutate node_suite "$bv" "stored === 'markdown' || stored === 'wysiwyg'" "stored === 'markdown'"
lr="$root/crates/ax-policy/src/links.rs"
mutate rust_suite "$lr" "        if item.kind != LinkKind::Memory && self.resolve(&item.page) == Some(item) {" "        if item.kind != LinkKind::Memory {"
mutate rust_suite "$lr" "        if item.kind != LinkKind::Memory && self.resolve(&item.page) == Some(item) {" "        if self.resolve(&item.page) == Some(item) {"
we="$ui/src/components/WysiwygEditor.tsx"
wn="$ui/src/lib/wysiwygNodes.ts"
mutate e2e_roundtrip "$wn" "  renderMarkdown: (node) => \`[[\${node.attrs?.raw ?? ''}]]\`," "  renderMarkdown: (node) => String(node.attrs?.raw ?? ''),"
mutate e2e_roundtrip "$wn" "  renderMarkdown: (node) => String(node.attrs?.raw ?? '')," "  renderMarkdown: () => '',"
mutate e2e_norewrite "$we" "    content: value," "    onCreate: ({ editor: e }) => onChangeRef.current(e.getMarkdown()),
    content: value,"
mutate e2e_restore "$we" "    content: value," "    content: '',"
mutate e2e_rich_pick "$we" "        if ((e.key === 'Enter' || e.key === 'Tab') && items[index]) {" "        if (e.key === 'Tab' && items[index]) {"
me="$ui/src/components/MarkdownEditor.tsx"
mutate e2e_md_pick "$me" "          'aria-activedescendant': open && items[index] ? optionId(listId, index) : undefined," ""
mutate node_suite "$lp" "    o.target !== ''" "    true"
mutate e2e_esc "$me" "            if (e.key === 'Escape') return;" ""

echo "mutants killed: $killed/$total"
[ "$killed" -eq "$total" ]
