#!/usr/bin/env bash
# Manual mutants for the IDE Command Center integration. Each must apply, turn its tests red,
# and be restored byte-for-byte. Any other outcome fails the script.
set -euo pipefail
root="$(cd "$(dirname "$0")/.." && pwd)"
cd "$root"

cc=crates/ax-installer/src/command_center.rs
ide=crates/ax-web/web-ui/src/ideInstall.ts
rust_test="cargo test -q -p ax-installer --lib command_center"
ts_test="node --test crates/ax-web/web-ui/src/ideInstall.test.ts"

run_mutant() {
  local name="$1" file="$2" from="$3" to="$4" test_cmd="$5"
  local backup before after
  backup="$(mktemp)"
  cp "$file" "$backup"
  before="$(shasum "$file" | cut -d' ' -f1)"
  FROM="$from" TO="$to" python3 - "$file" <<'PY'
import os, sys
p = sys.argv[1]
s = open(p).read()
f, t = os.environ["FROM"], os.environ["TO"]
if s.count(f) != 1:
    sys.exit(f"mutant pattern found {s.count(f)} times, expected 1")
open(p, "w").write(s.replace(f, t))
PY
  local status=0
  $test_cmd >/dev/null 2>&1 || status=$?
  cp "$backup" "$file"
  rm -f "$backup"
  after="$(shasum "$file" | cut -d' ' -f1)"
  [[ "$before" == "$after" ]] || { echo "RESTORE FAILED: $file" >&2; exit 1; }
  if [[ $status -eq 0 ]]; then
    echo "SURVIVED: $name" >&2
    exit 1
  fi
  echo "killed: $name"
}

run_mutant "zed merge not idempotent" "$cc" \
  'if zed_has_ax_task(existing) {
        return existing.to_string();' \
  'if false {
        return existing.to_string();' "$rust_test"
run_mutant "app bundle before PATH" "$cc" \
  'if probe.on_path(ide.cli) {' 'if false && probe.on_path(ide.cli) {' "$rust_test"
run_mutant "any old version counts as current" "$cc" \
  '.contains(&current)' '.iter().any(|n| n.starts_with(EXTENSION_ID))' "$rust_test"
run_mutant "any JetBrains folder is a product" "$cc" \
  '!v.is_empty() && v.starts_with(|c: char| c.is_ascii_digit()) && v.chars().all(|c| c.is_ascii_digit() || c == '"'"'.'"'"')' \
  'true || v.is_empty()' "$rust_test"
run_mutant "uninstall removes whole plugins folder" "$cc" \
  'std::fs::remove_file(&path)' 'std::fs::remove_dir_all(dir.join("plugins"))' "$rust_test"
run_mutant "loading claims all connected" "$ide" \
  "if (load === 'loading') return 'Loading IDEs…';" "if (load === 'loading') return 'Every IDE found here is connected.';" "$ts_test"
run_mutant "terminal agents get a panel badge" "$ide" \
  'if (t.panel == null) return null;' 'if (t.panel === undefined) return null;' "$ts_test"
echo "all mutants killed"
