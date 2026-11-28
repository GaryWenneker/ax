#!/usr/bin/env bash
# Manual mutants for the WYSIWYG body editor and the [[ link picker
# (docs/specs/policy-body-wysiwyg.md). Each mutant must change the file (proved by cmp)
# and must make its suite fail. Browser mutants run against a Vite dev server that
# proxies /api to a running `ax web` on :7070.
set -euo pipefail
root="$(cd "$(dirname "$0")/.." && pwd)"
source "$root/scripts/lib/mutate.sh"
ui="$root/crates/ax-web/web-ui"
port=5199

curl -sf -o /dev/null http://127.0.0.1:7070/ || { echo "ax web is not running on :7070"; exit 1; }
(cd "$ui" && npx vite --port "$port" --strictPort >/tmp/wysiwyg-mutants-vite.log 2>&1) &
vite_pid=$!
ax_guard_cleanup() { pkill -P "$vite_pid" 2>/dev/null || true; kill "$vite_pid" 2>/dev/null || true; }
for _ in $(seq 1 60); do curl -sf -o /dev/null "http://127.0.0.1:$port/" && break; sleep 0.5; done
curl -sf -o /dev/null "http://127.0.0.1:$port/" || { echo "vite did not start"; exit 1; }

node_suite() { (cd "$ui" && node --experimental-strip-types --test src/lib/linkPicker.test.ts src/lib/policyBodyView.test.ts); }
rust_suite() { (cd "$root" && env -u CARGO_TARGET_DIR cargo test -q -p ax-policy --lib w7_link_target); }
# The sleep lets Vite pick up the mutated file before the browser loads it.
e2e() { sleep 1; (cd "$ui" && AX_WEB_URL="http://127.0.0.1:$port" env -u PLAYWRIGHT_BROWSERS_PATH npx playwright test e2e/policy-wysiwyg.spec.ts --project=system-chrome -g "$1"); }
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

lp="$ui/src/lib/linkPicker.ts"
ax_mutate node_suite "$lp" "item.key !== opts.selfKey && " ""
ax_mutate node_suite "$lp" "const shown = capAcrossKinds(hits, LINK_PICKER_MAX);" "const shown = hits;"
ax_mutate node_suite "$lp" "q.words.every((w) => names.some((n) => n.includes(w)) || tags.some((x) => x.includes(w)))" "q.words.every((w) => tags.some((x) => x.includes(w)))"
ax_mutate node_suite "$lp" "  if (/[\\]\\n[]/.test(query)) return null;" ""
ax_mutate node_suite "$lp" "  return \`[[\${item.target}]]\`;" "  return \`[[\${item.label}]]\`;"
bv="$ui/src/lib/policyBodyView.ts"
ax_mutate node_suite "$bv" "stored === 'markdown' || stored === 'wysiwyg'" "stored === 'markdown'"
lr="$root/crates/ax-policy/src/links.rs"
ax_mutate rust_suite "$lr" "        if item.kind != LinkKind::Memory && self.resolve(&item.page) == Some(item) {" "        if item.kind != LinkKind::Memory {"
ax_mutate rust_suite "$lr" "        if item.kind != LinkKind::Memory && self.resolve(&item.page) == Some(item) {" "        if self.resolve(&item.page) == Some(item) {"
we="$ui/src/components/WysiwygEditor.tsx"
wn="$ui/src/lib/wysiwygNodes.ts"
ax_mutate e2e_roundtrip "$wn" "  renderMarkdown: (node) => \`[[\${node.attrs?.raw ?? ''}]]\`," "  renderMarkdown: (node) => String(node.attrs?.raw ?? ''),"
ax_mutate e2e_roundtrip "$wn" "  renderMarkdown: (node) => String(node.attrs?.raw ?? '')," "  renderMarkdown: () => '',"
ax_mutate e2e_norewrite "$we" "    content: value," "    onCreate: ({ editor: e }) => onChangeRef.current(e.getMarkdown()),
    content: value,"
ax_mutate e2e_restore "$we" "    content: value," "    content: '',"
ax_mutate e2e_rich_pick "$we" "    if (key === 'Enter' && rows[index]) {" "    if (false) {"
me="$ui/src/components/MarkdownEditor.tsx"
ax_mutate e2e_md_pick "$me" "          'aria-activedescendant': open && items[index] ? optionId(listId, index) : undefined," ""
ax_mutate node_suite "$lp" "    o.target !== ''" "    true"
ax_mutate e2e_esc "$me" "            if (e.key === 'Escape') return;" ""

ax_mutants_summary
