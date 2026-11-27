#!/usr/bin/env bash
# Gauntlet for git-shared .agents layout (docs/specs/agents-git-share.md).
set -euo pipefail
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
cd "$ROOT"
source "$ROOT/scripts/lib/mutate.sh"

echo "== cargo test -p ax-policy --lib agents_share + related =="
cargo test -p ax-policy --lib agents_share -- --nocapture
cargo test -p ax-policy --lib ide_seed::tests::upserts_agents -- --nocapture
cargo test -p ax-policy --lib builtin_packs::tests::installs_without_overwrite -- --nocapture

echo "== cargo test -p ax-policy --lib (full crate) =="
cargo test -p ax-policy --lib

echo "== bootstrap pointer grep (fail closed) =="
grep -q '\.agents/rules/' crates/ax-policy/src/ide_seed.rs \
  || { echo "FAIL: ide_seed missing .agents/rules/" >&2; exit 1; }
grep -q '\.agents/skills/' crates/ax-policy/templates/ide/cursor/ax.mdc \
  || { echo "FAIL: cursor template missing .agents/skills/" >&2; exit 1; }
grep -q '\.agents/rules/' crates/ax-policy/templates/ide/claude/ax.md \
  || { echo "FAIL: claude template missing .agents/rules/" >&2; exit 1; }
grep -q 'skip rule' crates/ax-policy/src/index.rs \
  || { echo "FAIL: index must skip unparseable .agents rules" >&2; exit 1; }
if grep -q 'parse_rule_file(path, &raw).map_err' crates/ax-policy/src/index.rs; then
  echo "FAIL: stale check must skip unparseable rules, not map_err validation_failed" >&2
  exit 1
fi
grep -q 'INDEXED_DOT_DIRS' crates/ax-extraction/src/orchestrator.rs \
  || { echo "FAIL: indexer must whitelist .scripts" >&2; exit 1; }

echo "== manual mutation =="
# Only a failing test kills a mutant; a build error is a run error (scripts/lib/mutate.sh).
SRC="crates/ax-policy/src/agents_share.rs"
HIER="crates/ax-policy/src/hierarchy.rs"
agents_share_tests() { cargo test -p ax-policy --lib agents_share; }
leak_gate_test() { cargo test -p ax-policy --lib agents_share::tests::leak_gate_detects_disabled_and_private; }
project_dir_test() { cargo test -p ax-policy --lib agents_share::tests::project_write_path_is_agents; }

# The leak-gate test itself must go red when the disabled check is removed.
ax_mutate leak_gate_test "$SRC" 'if !doc.frontmatter.enabled {' 'if false && !doc.frontmatter.enabled {'
ax_mutate agents_share_tests "$SRC" 'enabled && scope.is_packable()' 'scope.is_packable()'
ax_mutate agents_share_tests "$SRC" 'if !doc.frontmatter.enabled {' 'if false && !doc.frontmatter.enabled {'
ax_mutate agents_share_tests "$SRC" \
  'if !enabled && scope.is_packable()' 'if false && !enabled && scope.is_packable()'
ax_mutate agents_share_tests "$SRC" \
  'pub const AGENTS_DIR: &str = ".agents";' 'pub const AGENTS_DIR: &str = ".agentz";'
ax_mutate project_dir_test "$HIER" \
  'PolicyScope::Project => crate::agents_share::agents_dir(project_root),' \
  'PolicyScope::Project => project_root.join(".ax").join("policy"),'
ax_mutants_summary

echo "gauntlet-agents-git-share: ok"
