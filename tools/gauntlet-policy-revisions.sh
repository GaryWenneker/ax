#!/usr/bin/env bash
# Gauntlet: policy hash-on-change revisions (schema v20).
set -euo pipefail
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
cd "$ROOT"
unset CARGO_TARGET_DIR

echo "== rust =="
cargo test -p ax-db --test migration_v20
cargo test -p ax-policy revisions

echo "== web helpers =="
(cd crates/ax-web/web-ui && node --experimental-strip-types --test src/policyRevisions.test.ts)

echo "== wiring =="
grep -q 'policy_revisions' crates/ax-db/src/migrations.rs \
  || { echo "FAIL: missing policy_revisions migration" >&2; exit 1; }
grep -q 'CURRENT_SCHEMA_VERSION: i32 = 20' crates/ax-db/src/migrations.rs \
  || { echo "FAIL: schema version must be 20" >&2; exit 1; }
grep -q 'record_save_revision' crates/ax-policy/src/store.rs \
  || { echo "FAIL: save_rule/save_skill must record revisions" >&2; exit 1; }
grep -q 'record_restore_writes' crates/ax-web/src/policy.rs \
  || { echo "FAIL: zip restore must record revisions" >&2; exit 1; }
grep -q 'record_restore_writes' crates/ax-cli/src/commands/policy.rs \
  || { echo "FAIL: CLI restore must record revisions" >&2; exit 1; }
grep -q 'revisions/{revId}/restore' crates/ax-web/src/policy.rs \
  || { echo "FAIL: missing restore revision route" >&2; exit 1; }
grep -q 'PolicyRevisionHistory' crates/ax-web/web-ui/src/components/PolicyRuleInlineWorkspace.tsx \
  || { echo "FAIL: rule workspace missing History" >&2; exit 1; }
grep -q 'PolicyRevisionHistory' crates/ax-web/web-ui/src/components/PolicySkillInlineWorkspace.tsx \
  || { echo "FAIL: skill workspace missing History" >&2; exit 1; }
grep -q 'Local revision history' site/src/content/docs/guides/policy-engine.md \
  || { echo "FAIL: policy-engine.md missing revision docs" >&2; exit 1; }
grep -q 'History' site/src/content/docs/guides/command-center.md \
  || { echo "FAIL: command-center.md missing History" >&2; exit 1; }

echo "== negative control =="
if grep -q 'policy-revisions-table-does-not-exist' crates/ax-db/src/migrations.rs; then
  echo "FAIL: negative control passed (vacuous grep)" >&2
  exit 1
fi

echo "== tsc =="
(cd crates/ax-web/web-ui && npx tsc --noEmit)

echo "== manual mutation =="
SRC="$ROOT/crates/ax-policy/src/revisions.rs"
ORIG="$(mktemp)"
cp "$SRC" "$ORIG"
kill_mutant() {
  local label="$1"
  if cargo test -p ax-policy revisions --quiet >/dev/null 2>&1; then
    echo "FAIL: mutant survived: $label" >&2
    cp "$ORIG" "$SRC"
    exit 1
  fi
  echo "killed: $label"
  cp "$ORIG" "$SRC"
}

# Mutant 1: never skip identical hashes
python3 - <<'PY'
from pathlib import Path
p = Path("crates/ax-policy/src/revisions.rs")
t = p.read_text()
old = """    if latest.as_deref() == Some(hash.as_str()) {
        return Ok(false);
    }"""
new = """    if false && latest.as_deref() == Some(hash.as_str()) {
        return Ok(false);
    }"""
if old not in t:
    raise SystemExit("mutant 1: pattern missing")
p.write_text(t.replace(old, new, 1))
PY
kill_mutant "skip-identical"

# Mutant 2: keep 21 instead of 20
python3 - <<'PY'
from pathlib import Path
p = Path("crates/ax-policy/src/revisions.rs")
t = p.read_text()
old = "pub const POLICY_REVISION_CAP: i64 = 20;"
new = "pub const POLICY_REVISION_CAP: i64 = 21;"
if old not in t:
    raise SystemExit("mutant 2: pattern missing")
p.write_text(t.replace(old, new, 1))
PY
kill_mutant "cap-21"

# Mutant 3: restore source stored as save
python3 - <<'PY'
from pathlib import Path
p = Path("crates/ax-policy/src/revisions.rs")
t = p.read_text()
old = 'pub const SOURCE_RESTORE: &str = "restore";'
new = 'pub const SOURCE_RESTORE: &str = "save";'
if old not in t:
    raise SystemExit("mutant 3: pattern missing")
p.write_text(t.replace(old, new, 1))
PY
kill_mutant "restore-as-save"

cp "$ORIG" "$SRC"
rm -f "$ORIG"

echo "== gauntlet ok =="
