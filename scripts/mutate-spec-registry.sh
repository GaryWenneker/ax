#!/bin/bash
# Proves the spec-registry tests kill two real mutants, then restores the source.
set -euo pipefail
root="$(cd "$(dirname "$0")/.." && pwd)"
file="$root/crates/ax-usage/src/spec_registry.rs"
original="$(cat "$file")"
restore() { printf '%s' "$original" > "$file"; }
trap restore EXIT

apply() {
  local from="$1" to="$2" name="$3" filter="$4"
  python3 - "$file" "$from" "$to" <<'PY'
import pathlib, sys
path, source, target = sys.argv[1:]
file = pathlib.Path(path)
file.write_text(file.read_text().replace(source, target, 1))
PY
  set +e
  cargo test -p ax-usage --lib "$filter" -- --test-threads=1 >/tmp/ax-spec-mutant.out 2>&1
  status=$?
  set -e
  restore
  if [[ "$status" -eq 0 ]]; then
    echo "$name: survived"
    exit 1
  fi
  echo "$name: killed (exit $status)"
}

apply 'let kind = if plan_count > 0 { "plan" } else { "spec" };' \
  'let kind = if plan_count > 0 { "spec" } else { "spec" };' \
  'approved-plan-becomes-spec' 'stays_a_plan'

apply '        Ok(None) => match live_bound_spec(pool, root).await {
            Some(spec_id) => spec_id,
            None => return "none".into(),
        },' \
  '        Ok(None) => return "none".into(),' \
  'live-session-ignored' 'different_chat'

echo "killed 2/2"
