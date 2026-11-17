#!/usr/bin/env bash
# Reruns the database-integrity gauntlet. Mutants are restored before the suite.
set -euo pipefail

root="$(cd "$(dirname "$0")/.." && pwd)"
cd "$root"

src="crates/ax-db/src/integrity.rs"
backup="$(mktemp)"
cp "$src" "$backup"
restore() { cp "$backup" "$src"; }
trap restore EXIT

echo "HEAD $(git rev-parse HEAD)"
shasum -a 256 \
  "$src" \
  crates/ax-db/tests/integrity.rs \
  crates/ax-cli/src/commands/db.rs \
  crates/ax-cli/src/main.rs \
  crates/ax-memory/tests/vector_repair.rs \
  docs/specs/memory-db-integrity.md
start="$(shasum -a 256 "$src" | awk '{print $1}')"

apply() {
  python3 - "$src" "$1" "$2" <<'PY'
import pathlib, sys
path, old, new = sys.argv[1:]
file = pathlib.Path(path)
text = file.read_text()
count = text.count(old)
if count != 1:
    raise SystemExit(f"mutant target found {count} times")
file.write_text(text.replace(old, new, 1))
PY
}

expect_kill() {
  local name="$1"
  local needle="$2"
  local log
  log="$(mktemp)"
  set +e
  cargo test -p ax-db --test integrity "$name" --offline -- --exact --test-threads=1 >"$log" 2>&1
  local code=$?
  set -e
  cat "$log"
  if [[ "$code" -eq 0 ]]; then
    echo "mutant survived: $name"
    exit 1
  fi
  if ! grep -F -q "$needle" "$log"; then
    echo "mutant did not reach the expected failure: $name"
    exit 1
  fi
  rm -f "$log"
  restore
}

echo "MUTANT orphan-delete"
apply \
  'sqlx::query("DELETE FROM memory_cues WHERE memory_id NOT IN (SELECT id FROM memories)")' \
  'sqlx::query("SELECT 1")'
expect_kill "an_orphan_cue_is_removed_and_the_memory_and_backup_stay" "orphan_cues"

echo "MUTANT fts-rebuild"
apply \
  "let sql = format!(\"INSERT INTO \\\"{name}\\\"(\\\"{name}\\\") VALUES ('rebuild')\");" \
  'let sql = "SELECT 1".to_string();'
expect_kill "a_broken_full_text_index_is_rebuilt" "database disk image is malformed"

echo "MUTANT missing-file"
apply \
  'return report(
            path,
            vec![finding("missing", "", "database file is missing")],
        );' \
  'return report(path, vec![]);'
expect_kill "a_missing_database_is_not_created" 'right: ["missing"]'

restore
now="$(shasum -a 256 "$src" | awk '{print $1}')"
if [[ "$now" != "$start" ]]; then
  echo "restore mismatch: $now != $start"
  exit 1
fi
trap - EXIT
rm -f "$backup"

echo "SUITE ax-db"
cargo test -p ax-db --offline -- --test-threads=8
echo "SUITE vector_repair"
cargo test -p ax-memory --test vector_repair --offline -- --test-threads=4
echo "CLIPPY ax-db"
cargo clippy -p ax-db --tests --offline -- -W clippy::all
echo "FORMAT"
rustfmt --edition 2021 --check \
  "$src" \
  crates/ax-db/tests/integrity.rs \
  crates/ax-cli/src/commands/db.rs \
  crates/ax-memory/tests/vector_repair.rs

echo "BUILD ax"
cargo build -p ax-cli --offline
bin="$root/target-dev/debug/ax"
if [[ ! -x "$bin" ]]; then
  echo "binary missing: $bin"
  exit 1
fi

echo "CLI help"
"$bin" db check --help
"$bin" db repair --help

demo="$(mktemp -d)"
mkdir -p "$demo/.ax"
echo "CLI missing database"
set +e
"$bin" db check --path "$demo" --json >"$demo/check.json"
check_code=$?
"$bin" db repair --path "$demo" --dry-run --json >"$demo/repair.json"
repair_code=$?
"$bin" db check --path "$demo" --db nope >"$demo/bad.txt" 2>&1
bad_code=$?
set -e
cat "$demo/check.json"
echo "CHECK_EXIT:$check_code"
cat "$demo/repair.json"
echo "REPAIR_EXIT:$repair_code"
echo "BAD_DB_EXIT:$bad_code"
if [[ "$check_code" -eq 0 || "$repair_code" -eq 0 || "$bad_code" -eq 0 ]]; then
  echo "cli accepted a broken or invalid database"
  exit 1
fi
grep -q '"code": "missing"' "$demo/check.json"
grep -q '"dry_run": true' "$demo/repair.json"
grep -q 'db must be project, usage, global, or all' "$demo/bad.txt"
python3 - "$demo/.ax/integrity.json" <<'PY'
import json, sys
body = json.load(open(sys.argv[1]))
assert body["repair"] == "ax db repair"
assert body["ok"] is False
assert body["findings"][0]["code"] == "missing"
PY
if find "$demo/.ax/backups" -type f 2>/dev/null | grep -q .; then
  echo "dry-run wrote a backup"
  exit 1
fi
rm -rf "$demo"
echo "GAUNTLET_OK"
