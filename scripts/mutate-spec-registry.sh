#!/bin/bash
# Proves the spec-registry tests kill two real mutants, then restores the source.
# Each mutant's anchor must occur exactly once, so a moved anchor fails the run
# instead of testing unchanged source.
set -euo pipefail
root="$(cd "$(dirname "$0")/.." && pwd)"
source "$root/scripts/lib/restore-on-exit.sh"
file="$root/crates/ax-usage/src/spec_registry.rs"
backup="$(mktemp "${TMPDIR:-/tmp}/ax-spec-registry.XXXXXX")"
cp -p "$file" "$backup"
ax_guard_file "$file"
restore() { cp -p "$backup" "$file"; }

apply() {
  local from="$1" to="$2" name="$3" filter="$4"
  python3 - "$file" "$from" "$to" "$name" <<'PY'
import pathlib, sys
path, source, target, name = sys.argv[1:]
file = pathlib.Path(path)
text = file.read_text()
count = text.count(source)
if count != 1:
    sys.exit(f"{name}: anchor found {count} times, expected exactly once")
file.write_text(text.replace(source, target, 1))
PY
  set +e
  cargo test -p ax-usage --lib "$filter" -- --test-threads=1 >"${TMPDIR:-/tmp}/ax-spec-mutant.out" 2>&1
  status=$?
  set -e
  restore
  if [[ "$status" -eq 0 ]]; then
    echo "$name: survived"
    exit 1
  fi
  echo "$name: killed (exit $status)"
}

apply 'let detected = if plan_count > 0 { "plan" } else { "spec" };' \
  'let detected = if plan_count > 0 { "spec" } else { "spec" };' \
  'approved-plan-becomes-spec' 'stays_a_plan'

apply '        Ok(None) => match live_bound_spec(pool, root).await {
            Some(spec_id) => spec_id,
            None => return "none".into(),
        },' \
  '        Ok(None) => return "none".into(),' \
  'live-session-ignored' 'different_chat'

rm -f "$backup"
echo "killed 2/2"
