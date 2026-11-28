#!/usr/bin/env bash
set -euo pipefail
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
fail() { echo "FAIL: $*" >&2; exit 1; }
source "$ROOT/scripts/lib/negative-control.sh"

echo "== wiring =="
rg -q "exclusive_to_database" "$ROOT/crates/ax-policy/src/migrate.rs" \
  || fail "missing exclusive_to_database"
rg -q "keep_files" "$ROOT/crates/ax-cli/src/main.rs" \
  || fail "missing --keep-files CLI flag"
rg -q "export_policy_to_files_filtered" "$ROOT/crates/ax-cli/src/commands/policy.rs" \
  || fail "files --yes must export via filtered helper"

echo "== negative control: the wiring checks fail on a broken copy =="
negative_control filtered-export "$ROOT/crates/ax-cli/src/commands/policy.rs" 'export_policy_to_files_filtered' \
  'export_policy_to_files' rg -q "export_policy_to_files_filtered"

echo "== tests =="
(cd "$ROOT" && cargo test -p ax-policy remove_ax_policy_files_leaves_cursor_bootstrap --offline -- --nocapture)

echo "gauntlet-policy-storage: ok"
