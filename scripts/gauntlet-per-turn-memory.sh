#!/usr/bin/env bash
# Every gauntlet layer for docs/specs/per-turn-memory.md in one run. Any failure, crash or skipped
# item fails the whole run. Usage: bash scripts/gauntlet-per-turn-memory.sh [base-ref, default main]
# For negative controls only: GAUNTLET_LAYERS="3 4" runs a subset, GAUNTLET_ALLOW_DIRTY=1 skips the
# clean-tree check. A run with either set is not evidence.
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
cd "$ROOT"
BASE="${1:-main}"
CHANGED_RS=(
  crates/ax-cli/src/commands/turn_hook.rs
  crates/ax-cli/src/commands/stop_hook.rs
  crates/ax-cli/src/commands/mod.rs
  crates/ax-cli/src/main.rs
  crates/ax-memory/src/turns.rs
  crates/ax-memory/src/lib.rs
  crates/ax-memory/src/sync.rs
  crates/ax-memory/src/types.rs
  crates/ax-memory/tests/turns.rs
  crates/ax-mcp/src/tools.rs
  crates/ax-installer/src/hooks.rs
  crates/ax-installer/src/targets.rs
)
CRATES=(-p ax-cli -p ax-memory -p ax-mcp -p ax-installer)
source "$ROOT/scripts/lib/gauntlet.sh"
gauntlet_init turn
gauntlet_load_baseline scripts/lib/baseline-per-turn-memory.txt
gauntlet_header "sqlite3 $(sqlite3 --version | cut -d' ' -f1)"

if want 1; then
echo "== 1. targeted tests"
cargo_ test -q "${CRATES[@]}" >"$LOG/targeted.log" 2>&1 || fail "targeted tests"
grep -E "^test result" "$LOG/targeted.log" | awk '{p+=$4; f+=$6} END {print "   passed="p" failed="f}'
for run in 1 2 3 4 5; do # suite health: the turn hook tests run git and SQLite in temp dirs
  cargo_ test -q -p ax-cli --bin ax turn_hook >"$LOG/repeat-$run.log" 2>&1 || fail "turn hook tests failed on repeat $run"
done
echo "   turn hook tests green 5/5 repeats"
fi

if want 2; then gauntlet_workspace_against_baseline; fi
if want 3; then gauntlet_clippy_changed_lines "${CRATES[@]}"; fi
if want 4; then gauntlet_rustfmt_drift; fi
if want 5 || want 7; then gauntlet_build_and_check_docs; fi

if want 6; then
echo "== 6. mutants"
bash scripts/mutants-per-turn-memory.sh || fail "mutants"
fi

if want 7; then
echo "== 7. real execution with the fresh binary"
repo="$LOG/repo"
bin="$LOG/bin"
mkdir -p "$repo" "$bin" "$LOG/plain" "$LOG/home/.cursor" "$LOG/home/.claude" "$LOG/installdir"
ln -s "$ROOT/target-dev/release/ax" "$bin/ax"
# `ax init` writes the user's global agent configs; they must stay untouched.
. scripts/lib/user-agent-files.sh
agent_files_before="$(user_agent_files)"
# Not `( … ) || fail`: bash ignores `set -e` inside a subshell whose status is tested.
set +e
(
  set -euo pipefail
  export HOME="$LOG/home"
  export PATH="$bin:$PATH"
  cd "$repo"
  git init -q
  git config user.email gauntlet@example.invalid
  git config user.name gauntlet
  echo 'fn main() {}' >main.rs
  git add main.rs
  git commit -qm init
  ax init >"$LOG/init.log" 2>&1

  turns() { sqlite3 .ax/ax.db "select count(*) from memories where kind = 'turn'"; }
  cursor() { printf '{"conversation_id":"c1","generation_id":"%s","prompt":"%s","workspace_roots":["%s"]}' "$1" "$2" "$repo"; }
  claude() { printf '{"session_id":"s1","cwd":"%s","prompt":"%s","stop_hook_active":false}' "$repo" "$1"; }
  quiet() { # label, then the command reading stdin; fails on any output or a nonzero exit
    local label="$1"; shift
    "$@" >"$LOG/$label.out" 2>"$LOG/$label.err" || { echo "$label exited nonzero" >&2; exit 1; }
    if [ -s "$LOG/$label.out" ] || [ -s "$LOG/$label.err" ]; then
      echo "$label printed output:" >&2; cat "$LOG/$label.out" "$LOG/$label.err" >&2; exit 1
    fi
  }

  # T1 + T5: Cursor turn with an edit, end delivered twice.
  cursor g1 "Add the gauntlet helper sk-abcdefghijklmnopqrstuvwx1234" | quiet cursor-start ax turn-hook start
  grep -rq "sk-abcdefghij" .ax/turns && { echo "snapshot holds the secret" >&2; exit 1; }
  echo 'fn helper() {}' >>main.rs
  start_ms=$(node -e 'console.log(Date.now())')
  cursor g1 "" | quiet cursor-end ax turn-hook end
  end_ms=$(node -e 'console.log(Date.now())')
  cursor g1 "" | quiet cursor-end-retry ax turn-hook end
  [ "$(turns)" = 1 ] || { echo "T1/T5: expected 1 turn memory, got $(turns)" >&2; exit 1; }
  body="$(sqlite3 .ax/ax.db "select title || '|' || body || '|' || files from memories where kind = 'turn'")"
  grep -q "Add the gauntlet helper" <<<"$body" && grep -q "main.rs" <<<"$body" && grep -q "\[redacted\]" <<<"$body" ||
    { echo "T1/T12: unexpected turn memory: $body" >&2; exit 1; }
  grep -q "sk-abcdefghij" <<<"$body" && { echo "T12: secret stored" >&2; exit 1; }

  # T10: recall finds it.
  ax recall "gauntlet helper" >"$LOG/recall.log" 2>&1
  grep -q "\[turn\]" "$LOG/recall.log" || { echo "T10: recall did not return the turn memory" >&2; exit 1; }

  # T11: even tagged `shared`, a turn memory is never exported.
  sqlite3 .ax/ax.db "update memories set tags = '[\"shared\"]' where kind = 'turn'"
  ax memory export --out "$LOG/export.jsonl" --quiet >/dev/null 2>&1
  grep -q '"turn"' "$LOG/export.jsonl" 2>/dev/null && { echo "T11: turn memory exported" >&2; exit 1; }

  # T2: a turn without changes writes nothing.
  cursor g2 "Explain main.rs" | quiet cursor-start-2 ax turn-hook start
  cursor g2 "" | quiet cursor-end-2 ax turn-hook end
  [ "$(turns)" = 1 ] || { echo "T2: a turn without changes wrote a memory" >&2; exit 1; }

  # T15: Claude turn end runs inside `ax stop-hook`.
  claude "Claude adds b.rs" | quiet claude-start ax turn-hook start
  echo 'fn b() {}' >b.rs
  claude "" | ax stop-hook >"$LOG/stop-hook.out" 2>"$LOG/stop-hook.err" || { echo "T15: stop-hook exited nonzero" >&2; exit 1; }
  [ "$(turns)" = 2 ] || { echo "T15: stop-hook did not write the turn memory ($(turns))" >&2; exit 1; }

  # T7: ax.json memory.perTurn false, and AX_NO_STOP_HOOK=1.
  cp ax.json "$LOG/ax.json.bak" 2>/dev/null || echo '{}' >"$LOG/ax.json.bak"
  node -e 'const f="ax.json",fs=require("fs");const c=fs.existsSync(f)?JSON.parse(fs.readFileSync(f)):{};c.memory={perTurn:false};fs.writeFileSync(f,JSON.stringify(c))'
  cursor g3 "Off by config" | quiet off-start ax turn-hook start
  echo 'fn c() {}' >>main.rs
  cursor g3 "" | quiet off-end ax turn-hook end
  [ "$(turns)" = 2 ] || { echo "T7: memory.perTurn false still wrote a memory" >&2; exit 1; }
  cp "$LOG/ax.json.bak" ax.json
  cursor g4 "Off by env" | quiet env-start env AX_NO_STOP_HOOK=1 ax turn-hook start
  echo 'fn d() {}' >>main.rs
  cursor g4 "" | quiet env-end env AX_NO_STOP_HOOK=1 ax turn-hook end
  [ "$(turns)" = 2 ] || { echo "T7: AX_NO_STOP_HOOK=1 still wrote a memory" >&2; exit 1; }

  # T14: no git, no ax project, garbage input: exit 0 and silent.
  cd "$LOG/plain"
  printf '{"conversation_id":"x","workspace_roots":["%s"]}' "$LOG/plain" | quiet plain-start ax turn-hook start
  printf 'not json' | quiet garbage-end ax turn-hook end
  quiet empty-end ax turn-hook end </dev/null
  [ -z "$(ls -A "$LOG/plain")" ] || { echo "T14: files written outside an ax project" >&2; exit 1; }

  # T13: install twice, uninstall once; user hook entries survive.
  cd "$LOG/installdir"
  echo '{"version":1,"hooks":{"stop":[{"command":"./notify.sh"}]}}' >"$LOG/home/.cursor/hooks.json"
  HOME="$LOG/home" ax install --yes --target cursor --target claude >"$LOG/install1.log" 2>&1
  cp "$LOG/home/.cursor/hooks.json" "$LOG/hooks-after-1.json"
  HOME="$LOG/home" ax install --yes --target cursor --target claude >"$LOG/install2.log" 2>&1
  cmp -s "$LOG/hooks-after-1.json" "$LOG/home/.cursor/hooks.json" || { echo "T13: reinstall changed hooks.json" >&2; exit 1; }
  count_in() { grep -o "$1" "$2" | wc -l | tr -d ' '; }
  [ "$(count_in 'turn-hook start' "$LOG/home/.cursor/hooks.json")" = 1 ] &&
    [ "$(count_in 'turn-hook end' "$LOG/home/.cursor/hooks.json")" = 1 ] &&
    [ "$(count_in 'turn-hook start' "$LOG/home/.claude/settings.json")" = 1 ] ||
    { echo "T13: turn hook entries missing or duplicated" >&2; exit 1; }
  HOME="$LOG/home" ax uninstall >"$LOG/uninstall.log" 2>&1
  grep -q "turn-hook" "$LOG/home/.cursor/hooks.json" "$LOG/home/.claude/settings.json" &&
    { echo "T13: uninstall left turn hook entries" >&2; exit 1; }
  grep -q "./notify.sh" "$LOG/home/.cursor/hooks.json" || { echo "T13: uninstall removed a user hook" >&2; exit 1; }

  echo "   turn end took $((end_ms - start_ms)) ms"
)
real_exit=$?
set -e
agent_files_after="$(user_agent_files)"
[ "$agent_files_after" = "$agent_files_before" ] || fail "real execution changed the user's agent configs"
[ "$real_exit" -eq 0 ] || fail "real execution"
echo "   T1 T2 T5 T7 T10 T11 T12 T13 T14 T15 verified with the release binary"
fi

gauntlet_finish
