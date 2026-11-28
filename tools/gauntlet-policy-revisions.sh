#!/usr/bin/env bash
# Gauntlet: policy hash-on-change revisions (schema v20).
set -euo pipefail
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
cd "$ROOT"
source "$ROOT/scripts/lib/mutate.sh"
source "$ROOT/scripts/lib/negative-control.sh"
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

echo "== negative control: the wiring checks fail on a broken copy =="
negative_control save-revision crates/ax-policy/src/store.rs 'record_save_revision' 'record_revision' \
  grep -q 'record_save_revision'

echo "== tsc =="
(cd crates/ax-web/web-ui && npx tsc --noEmit)

echo "== manual mutation =="
# Only a failing test kills a mutant; a build error is a run error (scripts/lib/mutate.sh).
SRC="crates/ax-policy/src/revisions.rs"
revision_tests() { cargo test -p ax-policy revisions; }
ax_mutate revision_tests "$SRC" \
  '    if latest.as_deref() == Some(hash.as_str()) {' \
  '    if false && latest.as_deref() == Some(hash.as_str()) {'
ax_mutate revision_tests "$SRC" \
  'pub const POLICY_REVISION_CAP: i64 = 20;' 'pub const POLICY_REVISION_CAP: i64 = 21;'
ax_mutate revision_tests "$SRC" \
  'pub const SOURCE_RESTORE: &str = "restore";' 'pub const SOURCE_RESTORE: &str = "save";'
ax_mutants_summary

echo "== gauntlet ok =="
