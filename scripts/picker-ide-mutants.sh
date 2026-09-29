#!/usr/bin/env bash
# Manual mutants for the folder picker and IDE install (docs/specs/folder-picker-and-ide-install.md). Each
# mutant must change the file (proved by cmp) and must make its suite fail.
set -euo pipefail
root="$(cd "$(dirname "$0")/.." && pwd)"
killed=0
total=0

picker_suite() { (cd "$root" && cargo test -q -p ax-web --lib folder_picker && cargo test -q -p ax-web --test folder_picker); }
agent_suite() { (cd "$root" && cargo test -q -p ax-web --test agent_install_guard); }
ts_suite() { (cd "$root/crates/ax-web/web-ui" && node --test src/vaultMount.test.ts src/ideInstall.test.ts); }

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


pk="$root/crates/ax-web/src/folder_picker.rs"
mutate picker_suite "$pk" "    if !crate::dav::mount::allowed(&headers, hub.readonly) {" "    if false {"
mutate picker_suite "$pk" "    if OPEN.swap(true, Ordering::SeqCst) {" "    if false {"
mutate picker_suite "$pk" "        if !out.status.success() {" "        if false {"
mutate picker_suite "$pk" "            continue;
        };" "            return Ok(None);
        };"
mutate picker_suite "$pk" "    let is_root = line == \"/\" || (line.len() == 3 && line.ends_with(\":\\\\\"));" "    let is_root = line == \"/\";"
mutate picker_suite "$pk" "    let path = if is_root { line } else { line.trim_end_matches(['/', '\\\\']) };" "    let path = line;"
mutate picker_suite "$pk" "\"-STA\"," ""
mutate picker_suite "$pk" "            own(\"kdialog\", &[\"--getexistingdirectory\", \".\", \"--title\", PROMPT])," ""
ag="$root/crates/ax-web/src/agent.rs"
mutate agent_suite "$ag" "    } else if !crate::dav::mount::allowed(headers, false) {" "    } else if false {"
mutate agent_suite "$ag" "    let error = if hub.readonly {" "    let error = if false {"
mutate agent_suite "$ag" "            \"results\": reports.iter().map(report_json).collect::<Vec<_>>()," ""
ide="$root/crates/ax-web/web-ui/src/ideInstall.ts"
mutate ts_suite "$ide" "  if (t.configured) return 'connected';" ""
mutate ts_suite "$ide" "  return targets.filter((t) => ideState(t) === 'found').map((t) => t.id);" "  return targets.filter((t) => t.detected).map((t) => t.id);"
mutate ts_suite "$ide" "      const files = r.files.length ? r.files.join(', ') : 'already up to date';" "      const files = r.files.join(', ');"
vm="$root/crates/ax-web/web-ui/src/vaultMount.ts"
mutate ts_suite "$vm" "  if (/^[A-Za-z]:$/.test(last)) return '';" ""
mutate ts_suite "$vm" "  const name = last.replace(/[^A-Za-z0-9 _-]/g, '-').slice(0, 32).trim();" "  const name = last.slice(0, 32).trim();"

echo "mutants killed: $killed/$total"
[ "$killed" -eq "$total" ]
