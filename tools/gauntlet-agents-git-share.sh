#!/usr/bin/env bash
# Gauntlet for git-shared .agents layout (docs/specs/agents-git-share.md).
set -euo pipefail
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
cd "$ROOT"

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

echo "== leak-gate negative control (disable check, expect test fail, restore) =="
SRC="crates/ax-policy/src/agents_share.rs"
backup="$(mktemp)"
cp "$SRC" "$backup"
python3 - <<'PY'
from pathlib import Path
p = Path("crates/ax-policy/src/agents_share.rs")
t = p.read_text()
old = "if !doc.frontmatter.enabled {"
new = "if false && !doc.frontmatter.enabled {"
if old not in t:
    raise SystemExit("FAIL: leak-gate branch missing")
p.write_text(t.replace(old, new, 1))
PY
if cargo test -p ax-policy --lib agents_share::tests::leak_gate_detects_disabled_and_private --quiet; then
  echo "FAIL: negative control did not fail after removing the disabled check" >&2
  cp "$backup" "$SRC"
  exit 1
fi
echo "negative-control: leak test went red with defence removed"
cp "$backup" "$SRC"

echo "== manual mutation (5) =="
HIER="crates/ax-policy/src/hierarchy.rs"
hbackup="$(mktemp)"
cp "$HIER" "$hbackup"
restore() { cp "$backup" "$SRC"; cp "$hbackup" "$HIER"; }

kill_src() {
  local name="$1"
  python3 - "$2" "$3" <<'PY'
import pathlib, sys
p = pathlib.Path("crates/ax-policy/src/agents_share.rs")
a, b = sys.argv[1], sys.argv[2]
t = p.read_text()
if a not in t:
    raise SystemExit(f"mutant setup miss: {a!r}")
p.write_text(t.replace(a, b, 1))
PY
  if cargo test -p ax-policy --lib agents_share --quiet; then
    echo "FAIL: mutant survived: $name" >&2
    restore
    exit 1
  fi
  echo "killed: $name"
  cp "$backup" "$SRC"
}

kill_src "export-ignores-enabled" "enabled && scope.is_packable()" "scope.is_packable()"
kill_src "leak-skip-disabled" "if !doc.frontmatter.enabled {" "if false && !doc.frontmatter.enabled {"
kill_src "inactive-never" "if !enabled && scope.is_packable()" "if false && !enabled && scope.is_packable()"
kill_src "agents-dir-typo" 'pub const AGENTS_DIR: &str = ".agents";' 'pub const AGENTS_DIR: &str = ".agentz";'

python3 - <<'PY'
from pathlib import Path
p = Path("crates/ax-policy/src/hierarchy.rs")
t = p.read_text()
old = "PolicyScope::Project => crate::agents_share::agents_dir(project_root),"
new = "PolicyScope::Project => project_root.join(\".ax\").join(\"policy\"),"
if old not in t:
    raise SystemExit("hierarchy mutant miss")
p.write_text(t.replace(old, new, 1))
PY
if cargo test -p ax-policy --lib agents_share::tests::project_write_path_is_agents --quiet; then
  echo "FAIL: mutant survived: project-dir" >&2
  restore
  exit 1
fi
echo "killed: project-dir-legacy"
restore
rm -f "$backup" "$hbackup"

echo "gauntlet-agents-git-share: ok"
