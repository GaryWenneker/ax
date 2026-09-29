#!/usr/bin/env bash
# Manual mutants for the link picker search (docs/specs/link-picker-search.md).
# Unit mutants run the node suite; UI mutants run the e2e spec against a Vite dev
# server on :5199 (start it first: `npx vite --port 5199` in crates/ax-web/web-ui,
# with `ax web` on :7070); the server mutant runs the Rust links_api test.
set -euo pipefail
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
UI="$ROOT/crates/ax-web/web-ui"

unit() { (cd "$UI" && node --experimental-strip-types --test src/lib/linkPicker.test.ts >/dev/null 2>&1); }
ui() {
  (cd "$UI" && AX_WEB_URL=http://127.0.0.1:5199 env -u PLAYWRIGHT_BROWSERS_PATH \
    npx playwright test e2e/link-picker-search.spec.ts --project=system-chrome >/dev/null 2>&1)
}
rust() { (cd "$ROOT" && env -u CARGO_TARGET_DIR cargo test -q -p ax-web --test links_api >/dev/null 2>&1); }

killed=0; total=0
mutant() {
  local suite=$1 file=$2 from=$3 to=$4
  total=$((total + 1))
  cp "$file" "$file.orig"
  if ! python3 -c '
import sys
p, a, b = sys.argv[1:]
s = open(p).read()
if a not in s:
    sys.exit(f"mutant anchor missing: {a}")
open(p, "w").write(s.replace(a, b, 1))
' "$file" "$from" "$to"; then
    mv "$file.orig" "$file"
    exit 1
  fi
  [ "$suite" = ui ] && sleep 2
  if "$suite"; then echo "SURVIVED [$suite] $file :: $from"; else echo "killed   [$suite] $file :: $from"; killed=$((killed + 1)); fi
  mv "$file.orig" "$file"
  touch "$file"
}

unit || { echo "baseline unit suite is red"; exit 1; }
curl -sf -o /dev/null http://127.0.0.1:5199/ || { echo "no Vite dev server on :5199"; exit 1; }
ui || { echo "baseline e2e is red"; exit 1; }

LIB="$UI/src/lib/linkPicker.ts"
mutant unit "$LIB" "if (term.length > 1) tags.push(term.slice(1));" "if (term.length > 1) words.push(term.slice(1));"
mutant unit "$LIB" "if (prefix && prefixKind) {" "if (false) {"
mutant unit "$LIB" "q.tags.every((t) => tags.some((x) => x.startsWith(t)))" "q.tags.every((t) => tags.some((x) => x === t))"
mutant unit "$LIB" "q.words.every((w) =>" "q.words.some((w) =>"
mutant unit "$LIB" "if (names.some((n) => n.startsWith(first))) return 1;" ""
mutant unit "$LIB" "const shown = capAcrossKinds(hits, LINK_PICKER_MAX);" "const shown = hits.slice(0, LINK_PICKER_MAX);"
mutant unit "$LIB" "item.key !== opts.selfKey &&" ""
mutant unit "$LIB" "return /^https?:\\/\\/\\S+\$/i.test(query.trim());" "return /^https?:/i.test(query.trim());"
mutant unit "$LIB" "return clean ? \`\${item.target}|\${clean}\` : item.target;" "return item.target;"

PICK="$UI/src/components/LinkPicker.tsx"
EDIT="$UI/src/components/WysiwygEditor.tsx"
mutant ui "$PICK" "const targets = useLinkTargets(true);" "const targets = useLinkTargets(active);"
mutant ui "$PICK" "              setKind(c.kind);" "              setKind(null);"
mutant ui "$EDIT" "    editor?.commands.focus();" "    void 0;"

mutant rust "$ROOT/crates/ax-web/src/links_api.rs" 'v["tags"] = json!(e.tags);' ''

unit || { echo "unit suite red after restore"; exit 1; }
echo "$killed/$total killed"
[ "$killed" -eq "$total" ]
