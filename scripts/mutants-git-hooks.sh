#!/usr/bin/env bash
# Manual mutation run for the git hook fix (shebang, execute bit, repair at MCP startup).
# Each mutant must apply exactly once (else the run fails) and must make its crate's tests fail.
# Files are restored from the git index after each mutant; stage the change under test first.
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
cd "$ROOT"

HOOKS=crates/ax-sync/src/git_hooks.rs
SERVER=crates/ax-mcp/src/server.rs

for f in "$HOOKS" "$SERVER"; do
  if ! git diff --quiet -- "$f"; then
    echo "mutants: $f has unstaged changes; stage them first" >&2
    exit 1
  fi
done

for crate in ax-sync ax-mcp; do
  if ! env -u CARGO_TARGET_DIR cargo test -q -p "$crate" >/dev/null 2>&1; then
    echo "mutants: baseline tests of $crate fail on the unmutated tree; a kill would mean nothing" >&2
    exit 1
  fi
done

killed=0
total=0

# mutant <id> <crate> <file> <perl-substitution>
mutant() {
  local id="$1" crate="$2" file="$3" expr="$4"
  total=$((total + 1))
  local before after
  before="$(shasum "$file" | cut -d' ' -f1)"
  perl -0pi -e "$expr" "$file"
  after="$(shasum "$file" | cut -d' ' -f1)"
  if [ "$before" = "$after" ]; then
    git checkout -q -- "$file"
    echo "mutants: $id did not apply to $file" >&2
    exit 1
  fi
  if env -u CARGO_TARGET_DIR cargo test -q -p "$crate" >/dev/null 2>&1; then
    echo "SURVIVED $id ($file)"
  else
    echo "killed   $id"
    killed=$((killed + 1))
  fi
  git checkout -q -- "$file"
  if [ -n "$(git diff -- "$file")" ]; then
    echo "mutants: $file not restored" >&2
    exit 1
  fi
}

mutant M1 ax-sync "$HOOKS"  's/format!\("\{SHEBANG\}\\n\{content\}"\)/content.to_string()/'
mutant M2 ax-sync "$HOOKS"  's/perms\.set_mode\(mode \| 0o755\);/perms.set_mode(mode);/'
mutant M3 ax-sync "$HOOKS"  's/if !existing\.lines\(\)\.any\(is_ax_line\) \{/if false {/'
mutant M4 ax-sync "$HOOKS"  's/if existing != Some\(content\) \{/if true {/'
mutant M5 ax-sync "$HOOKS"  's/=> with_shebang\(e\),/=> e.to_string(),/'
mutant M6 ax-sync "$HOOKS"  's/if content\.starts_with\("#!"\) \{/if content.starts_with("#!\/bin\/sh") {/'
mutant M7 ax-sync "$HOOKS"  's/\.filter\(\|l\| !is_ax_line\(l\)\)/.filter(|l| !l.contains("ax sync"))/'
mutant M8 ax-mcp  "$SERVER" 's/    if let Err\(e\) = ax_sync::repair_git_hooks\(root\) \{\n        eprintln!\("ax: git hook repair skipped: \{e\}"\);\n    \}/    let _ = root;/'

echo "mutants: $killed/$total killed"
[ "$killed" -eq "$total" ]
