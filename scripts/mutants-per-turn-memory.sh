#!/usr/bin/env bash
# Manual mutation run for docs/specs/per-turn-memory.md (turn hooks, turn memories, installer).
# The one-line call from `ax stop-hook` into the turn end has no unit test; the real-execution
# layer of scripts/gauntlet-per-turn-memory.sh covers it.
# Each mutant must apply exactly once (else the run fails) and must make its crate's tests fail.
# Files are restored from the git index after each mutant; stage the change under test first.
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
cd "$ROOT"

TURN=crates/ax-cli/src/commands/turn_hook.rs
TURNS=crates/ax-memory/src/turns.rs
MEMLIB=crates/ax-memory/src/lib.rs
SYNC=crates/ax-memory/src/sync.rs
TOOLS=crates/ax-mcp/src/tools.rs
HOOKS=crates/ax-installer/src/hooks.rs
TARGETS=crates/ax-installer/src/targets.rs
MAIN=crates/ax-cli/src/main.rs

for f in "$TURN" "$TURNS" "$MEMLIB" "$SYNC" "$TOOLS" "$HOOKS" "$TARGETS" "$MAIN"; do
  if ! git diff --quiet -- "$f"; then
    echo "mutants: $f has unstaged changes; stage them first" >&2
    exit 1
  fi
done

test_crate() { # crate -> exit status of its tests (ax-cli: the binary's unit tests)
  if [ "$1" = ax-cli ]; then
    env -u CARGO_TARGET_DIR cargo test -q -p ax-cli --bin ax >/dev/null 2>&1
  else
    env -u CARGO_TARGET_DIR cargo test -q -p "$1" >/dev/null 2>&1
  fi
}

for crate in ax-cli ax-memory ax-mcp ax-installer; do
  if ! test_crate "$crate"; then
    echo "mutants: baseline tests of $crate fail on the unmutated tree; a kill would mean nothing" >&2
    exit 1
  fi
done

killed=0
total=0

# mutant <id> <crate> <file> <perl-substitution>; the substitution must change the file.
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
  if test_crate "$crate"; then
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

# Turn hook (ax-cli)
mutant T1  ax-cli "$TURN" 's/\.filter\(\|path\| snapshot\.dirty\.get\(\*path\) != now_dirty\.get\(\*path\)\)/.filter(|_| true)/'
mutant T2  ax-cli "$TURN" 's/if files\.is_empty\(\) && commits\.is_empty\(\) \{/if files.is_empty() || commits.is_empty() {/'
mutant T3  ax-cli "$TURN" 's/"--untracked-files=all"/"--untracked-files=normal"/'
mutant T4  ax-cli "$TURN" 's/let id_source = format!\("\{conversation\}\\n\{\}", snapshot\.turn\);/let id_source = conversation.to_string();/'
mutant T5  ax-cli "$TURN" 's/map_or\(1, \|s\| s\.counter \+ 1\)/map_or(1, |s| s.counter)/'
mutant T6  ax-cli "$TURN" 's/prompt: clip\(&ax_memory::redact_secrets\(&input\.prompt\), PROMPT_CHARS\)/prompt: clip(&input.prompt, PROMPT_CHARS)/'
mutant T7  ax-cli "$TURN" 's/\.and_then\(\|v\| v\.as_bool\(\)\) != Some\(false\)/.and_then(|v| v.as_bool()) != Some(true)/'
mutant T8  ax-cli "$TURN" 's/    if !per_turn_enabled\(root\) \{\n        return None;\n    \}\n    let record/    let record/'
mutant T9  ax-cli "$TURN" 's/    let _ = ax_memory::prune_turns\([^\n]*\n//'
mutant T10 ax-cli "$TURN" 's/    value == Some\("1"\)/    value.is_some()/'
mutant T11 ax-cli "$TURN" 's/\.and_then\(\|r\| r\.get\(0\)\)/.and_then(|r| r.get(1))/'
mutant T12 ax-cli "$TURN" 's/    std::fs::write\(dir\.join\("\.gitignore"\), "\*\\n"\)\.ok\(\)\?;\n//'
mutant T13 ax-cli "$TURN" 's/Some\(line\) => clip\(line, TITLE_CHARS\)/Some(line) => line.to_string()/'
mutant T14 ax-cli "$TURN" 's/\|s\| format!\("\{s\}\.\.HEAD"\)/|_| "HEAD".to_string()/'
mutant T15 ax-cli "$TURN" 's/files: lines\.filter\(\|l\| !l\.trim\(\)\.is_empty\(\)\)\.map\(str::to_string\)\.collect\(\)/files: Vec::new()/'
mutant T16 ax-cli "$TURN" 's/    if !ax_context::directory::get_ax_dir\(root\)\.is_dir\(\) \|\| !per_turn_enabled\(root\) \{/    if !per_turn_enabled(root) {/'
mutant T17 ax-cli "$TURN" 's/prompt: text\("prompt"\)\.unwrap_or_default\(\)/prompt: String::new()/'
mutant T18 ax-cli "$MAIN" 's/args\.get\(1\)\.is_some_and\(\|a\| a == "turn-hook"\) \|\| //'
mutant T19 ax-cli "$MAIN" 's/args\.get\(1\)\.is_some_and\(\|a\| a == "turn-hook"\)/args.iter().any(|a| a == "turn-hook")/'

# Turn memories (ax-memory)
mutant M1 ax-memory "$MEMLIB" 's/m\.score > 0\.0 && m\.memory\.kind != TURN_KIND/m.score > 0.0/'
mutant M2 ax-memory "$MEMLIB" 's/recall\(pool, prompt, \(limit \* 4\)\.max\(20\)\)/recall(pool, prompt, limit)/'
mutant M3 ax-memory "$SYNC"   's/\s*\.filter\(\|m\| m\.kind != crate::turns::TURN_KIND\)//'
mutant M4 ax-memory "$TURNS"  's/DELETE FROM memories WHERE kind = \? AND created_at < \?/DELETE FROM memories WHERE (kind = ? OR 1) AND created_at < ?/'
mutant M5 ax-memory "$TURNS"  's/let cutoff = now_ms - max_age_days \* 86_400_000;/let cutoff = now_ms;/'
mutant M6 ax-memory "$TURNS"  's/token\.starts_with\("sk-"\) && len >= 20/token.starts_with("sk-") \&\& len >= 200/'
mutant M7 ax-memory "$TURNS"  's/INSERT OR IGNORE INTO memories \(id, kind/INSERT OR REPLACE INTO memories (id, kind/'

# Preflight titles (ax-mcp)
mutant P1 ax-mcp "$TOOLS" 's/if !row\.enabled \|\| row\.kind == ax_memory::TURN_KIND \{/if !row.enabled {/'

# Installer (ax-installer)
mutant I1 ax-installer "$TARGETS" 's/\("UserPromptSubmit", "turn-hook start"\),/("UserPromptSubmit", "prompt-hook"),/'
mutant I2 ax-installer "$HOOKS"   's/fn is_turn_hook\(entry: &Value\) -> bool \{\n    command_contains\(entry, TURN_MARKER\)/fn is_turn_hook(entry: \&Value) -> bool {\n    command_contains(entry, MARKER)/'
mutant I3 ax-installer "$HOOKS"   's/\("stop", "end"\)/("Stop", "end")/'
mutant I4 ax-installer "$HOOKS"   's/    upsert_cursor_turn_hooks\(&mut after, bin\)\?;\n//'

echo "mutants: $killed/$total killed"
[ "$killed" -eq "$total" ]
