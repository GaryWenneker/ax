#!/usr/bin/env bash
set -euo pipefail
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
fail() { echo "FAIL: $*" >&2; exit 1; }

echo "== wiring =="
rg -q "exclusive_to_database" "$ROOT/crates/ax-policy/src/migrate.rs" \
  || fail "missing exclusive_to_database"
rg -q "keep_files" "$ROOT/crates/ax-cli/src/main.rs" \
  || fail "missing --keep-files CLI flag"
rg -q "export_policy_to_files_filtered" "$ROOT/crates/ax-cli/src/commands/policy.rs" \
  || fail "files --yes must export via filtered helper"

echo "== negative control =="
if rg -q "exclusive_to_database_does_not_exist" "$ROOT/crates/ax-policy/src/migrate.rs"; then
  fail "negative control passed"
fi

echo "== tests =="
(cd "$ROOT" && cargo test -p ax-policy remove_ax_policy_files_leaves_cursor_bootstrap --offline -- --nocapture)

echo "gauntlet-policy-storage: ok"
