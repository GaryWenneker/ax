#!/usr/bin/env bash
# Entry point for skill-group gauntlet layers.
set -euo pipefail
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
cd "$ROOT"

echo "== cargo test -p ax-policy skill_groups + parse =="
cargo test -p ax-policy --lib skill_groups -- --nocapture
cargo test -p ax-policy --lib parse::tests::parse_skill_group_roundtrip -- --nocapture
cargo test -p ax-policy --lib parse::tests::parse_rule_group_roundtrip -- --nocapture

echo "== cargo test -p ax-db migration_v18 + migration_v19 =="
cargo test -p ax-db --test migration_v18 -- --nocapture
cargo test -p ax-db --test migration_v19 -- --nocapture

echo "== group filter + collapse helpers =="
(cd crates/ax-web/web-ui && node --experimental-strip-types --test src/skillGroupFilter.test.ts)

echo "== UI wiring =="
grep -q 'PolicyGroupListControls' crates/ax-web/web-ui/src/pages/PolicySkills.tsx \
  || { echo "FAIL: Skills missing group controls" >&2; exit 1; }
grep -q 'PolicyGroupListControls' crates/ax-web/web-ui/src/pages/PolicyRules.tsx \
  || { echo "FAIL: Rules missing group controls" >&2; exit 1; }
grep -q 'Collapse all' crates/ax-web/web-ui/src/components/ui/PolicyGroupListControls.tsx \
  || { echo "FAIL: missing Collapse all" >&2; exit 1; }
grep -q 'Expand all' crates/ax-web/web-ui/src/components/ui/PolicyGroupListControls.tsx \
  || { echo "FAIL: missing Expand all" >&2; exit 1; }
grep -q 'type="checkbox"' crates/ax-web/web-ui/src/components/ui/PolicyGroupListControls.tsx \
  || { echo "FAIL: missing group checkboxes" >&2; exit 1; }
(cd crates/ax-web/web-ui && npx tsc --noEmit)

echo "== catalog copies match =="
diff -q crates/ax-policy/data/skill-groups.json crates/ax-web/web-ui/src/skill-groups.json

echo "gauntlet-skill-groups: ok"
