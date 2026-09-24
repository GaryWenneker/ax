#!/usr/bin/env bash
# Manual mutation run for the git hook fix (shebang, execute bit, repair at MCP startup, quiet hooks).
# The `if quiet` branch in ship::run has no unit test; the real-execution check covers it.
# Each mutant must apply exactly once (else the run fails) and must make its crate's tests fail.
# Files are restored from the git index after each mutant; stage the change under test first.
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
cd "$ROOT"

HOOKS=crates/ax-sync/src/git_hooks.rs
SERVER=crates/ax-mcp/src/server.rs
SHIP=crates/ax-cli/src/commands/ship.rs
MAIN=crates/ax-cli/src/main.rs

for f in "$HOOKS" "$SERVER" "$SHIP" "$MAIN"; do
  if ! git diff --quiet -- "$f"; then
    echo "mutants: $f has unstaged changes; stage them first" >&2
    exit 1
  fi
done

for crate in ax-sync ax-mcp ax-cli; do
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
mutant M9  ax-sync "$HOOKS" 's/if l\.trim\(\) == LEGACY_SHIP_LINE \{/if l.contains(LEGACY_SHIP_LINE) {/'
mutant M10 ax-sync "$HOOKS" 's/if content\.ends_with\(.\\n.\) \{/if false {/'
mutant M11 ax-sync "$HOOKS" 's/SHIP_LINE: &str = "ax ship --evaluate --quiet"/SHIP_LINE: &str = "ax ship --evaluate"/'
mutant M12 ax-cli  "$SHIP"  's/\.filter\(\|s\| s\.status == "failed"\)/.filter(|s| s.status != "passed")/'
mutant M13 ax-cli  "$SHIP"  's/if passed \{\n        return None;/if false {\n        return None;/'
mutant M14 ax-cli  "$SHIP"  's/if failed\.is_empty\(\) \{/if true {/'
mutant M15 ax-cli  "$MAIN"  's/\.any\(\|a\| a == "--quiet"\)/.any(|a| a.contains("--quiet"))/'
mutant M16 ax-cli  "$MAIN"  's/\{\n        "ax=warn"/{\n        "ax=info"/'

echo "mutants: $killed/$total killed"
[ "$killed" -eq "$total" ]
