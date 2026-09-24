#!/usr/bin/env bash
# Every gauntlet layer for docs/specs/turn-memory-outcomes.md in one run. Any failure, crash or
# skipped item fails the whole run. Usage: bash scripts/gauntlet-turn-outcomes.sh [base-ref, default main]
# For negative controls only: GAUNTLET_LAYERS="3 4" runs a subset, GAUNTLET_ALLOW_DIRTY=1 skips the
# clean-tree check. A run with either set is not evidence.
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
cd "$ROOT"
BASE="${1:-main}"
CHANGED_RS=(
  crates/ax-cli/src/commands/turn_hook.rs
  crates/ax-cli/src/commands/memory.rs
  crates/ax-cli/src/main.rs
  crates/ax-memory/src/history.rs
  crates/ax-memory/src/turns.rs
  crates/ax-memory/src/store.rs
  crates/ax-memory/src/lib.rs
  crates/ax-memory/tests/history.rs
  crates/ax-memory/tests/turns.rs
  crates/ax-mcp/src/tools.rs
  crates/ax-mcp/src/tool_filter.rs
  crates/ax-mcp/src/query_pool.rs
  crates/ax-installer/src/hooks.rs
)
CRATES=(-p ax-cli -p ax-memory -p ax-installer)
# Known failures before the change. The first five come from the main checkout (per-turn memory
# spec; the last two of those are flaky). cycles_api_path_handlers_work opens the repo's own .ax/,
# which a fresh worktree does not have.
BASELINE_FAILURES="bootstrap::tests::legacy_prefix_from_workspace_folder
bootstrap::tests::resolves_placeholder_to_folder_name
savings::tests::cursor_transcript_path_filter
savings::tests::transcript_import_does_not_wipe_state_tokens
seed::tests::seeds_sonar_project_key_from_folder_name
cycles_api_path_handlers_work"
LOG="$(mktemp -d /tmp/ax-gauntlet-outcomes.XXXX)"
cargo_() { env -u CARGO_TARGET_DIR cargo "$@"; }
fail() { echo "GAUNTLET FAILED: $*" >&2; echo "logs: $LOG" >&2; exit 1; }
LAYERS="${GAUNTLET_LAYERS:-1 2 3 4 5 6 7}"
want() { [[ " $LAYERS " == *" $1 "* ]]; }

echo "source: $(git rev-parse HEAD) (base $BASE $(git rev-parse --short "$BASE"))"
echo "tools: $(cargo --version) | $(rustfmt --version) | node $(node --version) | sqlite3 $(sqlite3 --version | cut -d' ' -f1)"
if [ "${GAUNTLET_ALLOW_DIRTY:-0}" != 1 ] && [ -n "$(git status --porcelain -- crates scripts site/src Cargo.lock)" ]; then
  fail "uncommitted changes under crates/, scripts/, site/src/ or Cargo.lock; commit first so the run matches a SHA"
fi

if want 1; then
echo "== 1. targeted tests"
cargo_ test -q "${CRATES[@]}" >"$LOG/targeted.log" 2>&1 || fail "targeted tests"
cargo_ test -q -p ax-mcp --lib --test catalog_payload_size >>"$LOG/targeted.log" 2>&1 || fail "ax-mcp tests"
grep -E "^test result" "$LOG/targeted.log" | awk '{p+=$4; f+=$6} END {print "   passed="p" failed="f}'
for run in 1 2 3 4 5; do # suite health: these tests run git and SQLite in temp dirs
  cargo_ test -q -p ax-cli --bin ax turn_hook >"$LOG/repeat-hook-$run.log" 2>&1 || fail "turn hook tests failed on repeat $run"
  cargo_ test -q -p ax-memory --test history --test turns >"$LOG/repeat-memory-$run.log" 2>&1 ||
    fail "memory tests failed on repeat $run"
done
echo "   turn hook and memory tests green 5/5 repeats"
fi

if want 2; then
echo "== 2. workspace suite against the baseline"
set +e
cargo_ test --workspace --no-fail-fast >"$LOG/workspace.log" 2>&1
ws_exit=$?
set -e
grep -qE "^test result" "$LOG/workspace.log" || fail "workspace suite produced no results (exit $ws_exit)"
if grep -E "^error(\[E[0-9]+\])?: could not compile|^error\[E[0-9]+\]" "$LOG/workspace.log"; then
  fail "workspace suite did not compile"
fi
failed="$(grep -E "^test .* \.\.\. FAILED$" "$LOG/workspace.log" | sed -E 's/^test (.*) \.\.\. FAILED$/\1/' | sort -u)"
new_failures="$(comm -23 <(printf '%s\n' "$failed" | sed '/^$/d') <(printf '%s\n' "$BASELINE_FAILURES" | sort))"
grep -E "^test result" "$LOG/workspace.log" | awk '{p+=$4; f+=$6} END {print "   passed="p" failed="f}'
printf '%s\n' "$failed" | sed '/^$/d; s/^/   failed: /'
[ -z "$new_failures" ] || fail "new test failures: $new_failures"
if [ "$ws_exit" -ne 0 ] && [ -z "$failed" ]; then
  fail "workspace suite exited $ws_exit without a failing test"
fi
fi

changed_lines() { # file -> one changed line number per line, relative to BASE
  git diff -U0 "$BASE" -- "$1" | sed -nE 's/^@@ -[0-9,]+ \+([0-9]+)(,([0-9]+))? @@.*/\1 \3/p' |
    while read -r start count; do
      count="${count:-1}"
      for ((i = 0; i < count; i++)); do echo $((start + i)); done
    done
}

if want 3; then
echo "== 3. clippy: no warnings on changed lines"
cargo_ clippy "${CRATES[@]}" -p ax-mcp --all-targets --message-format short -- -A clippy::invalid_regex \
  >"$LOG/clippy.log" 2>&1 || fail "clippy did not finish"
hits=0
for f in "${CHANGED_RS[@]}"; do
  lines="$(changed_lines "$f")"
  while IFS=: read -r line msg; do
    [ -n "$line" ] || continue
    if grep -qx "$line" <<<"$lines"; then
      echo "   $f:$line:$msg"
      hits=$((hits + 1))
    fi
  done < <(grep -E "^$f:[0-9]+:[0-9]+: (warning|error)" "$LOG/clippy.log" | sed -E "s#^$f:([0-9]+):[0-9]+:#\1:#" || true)
done
[ "$hits" -eq 0 ] || fail "$hits clippy finding(s) on changed lines"
echo "   0 on changed lines"
fi

if want 4; then
echo "== 4. rustfmt: no drift beyond $BASE (new files: none at all)"
for f in "${CHANGED_RS[@]}"; do
  # By path rustfmt also checks every `mod` the file declares; count only this file's hunks.
  now="$(rustfmt --edition 2021 --check "$f" 2>&1 | grep -c "^Diff in $ROOT/$f:" || true)"
  if git cat-file -e "$BASE:$f" 2>/dev/null; then
    before="$(git show "$BASE:$f" | rustfmt --edition 2021 --check 2>&1 | grep -c '^Diff in' || true)"
  else
    before=0
  fi
  echo "   $f: $now diff hunks (was $before)"
  [ "$now" -le "$before" ] || fail "rustfmt drift grew in $f"
done
fi

if want 5 || want 7; then
echo "== 5. release build and CLI docs"
cargo_ build -q --release -p ax-cli >"$LOG/build.log" 2>&1 || fail "release build"
AX_BIN="$ROOT/target-dev/release/ax" node scripts/check-cli-docs.mjs "$ROOT/target-dev/release/ax" || fail "cli docs"
fi

if want 6; then
echo "== 6. mutants (this feature, then the per-turn memory invariants)"
bash scripts/mutants-turn-outcomes.sh || fail "mutants (turn outcomes)"
bash scripts/mutants-per-turn-memory.sh || fail "mutants (per-turn memory)"
fi

if want 7; then
echo "== 7. real execution with the fresh binary"
repo="$LOG/repo"
bin="$LOG/bin"
mkdir -p "$repo" "$bin" "$LOG/home/.cursor"
ln -s "$ROOT/target-dev/release/ax" "$bin/ax"
# Not `( … ) || fail`: bash ignores `set -e` inside a subshell whose status is tested.
set +e
(
  set -euo pipefail
  export PATH="$bin:$PATH"
  cd "$repo"
  git init -q
  git config user.email gauntlet@example.invalid
  git config user.name gauntlet
  echo 'fn main() {}' >main.rs
  git add main.rs
  git commit -qm init
  ax init >"$LOG/init.log" 2>&1

  sql() { sqlite3 .ax/ax.db "$1"; }
  cursor() { # event generation [text]
    node -e 'const [e,g,t,r]=process.argv.slice(1);process.stdout.write(JSON.stringify({conversation_id:"c1",generation_id:g,hook_event_name:e,prompt:e==="beforeSubmitPrompt"?"Add the helper":undefined,text:t||undefined,workspace_roots:[r]}))' "$1" "$2" "${3:-}" "$repo"
  }
  claude() { # event [last_assistant_message]
    node -e 'const [e,m,r]=process.argv.slice(1);process.stdout.write(JSON.stringify({session_id:"s1",cwd:r,hook_event_name:e,prompt:e==="UserPromptSubmit"?"Claude adds b.rs":undefined,last_assistant_message:m||undefined,stop_hook_active:false}))' "$1" "${2:-}" "$repo"
  }
  quiet() { # label, then the command reading stdin; fails on any output or a nonzero exit
    local label="$1"; shift
    "$@" >"$LOG/$label.out" 2>"$LOG/$label.err" || { echo "$label exited nonzero" >&2; exit 1; }
    if [ -s "$LOG/$label.out" ] || [ -s "$LOG/$label.err" ]; then
      echo "$label printed output:" >&2; cat "$LOG/$label.out" "$LOG/$label.err" >&2; exit 1
    fi
  }
  token="sk-abcdefghijklmnopqrstuvwx1234"

  # O1 + O4: Cursor turn; two replies, the second one with a token.
  cursor beforeSubmitPrompt g1 | quiet c-start ax turn-hook start
  cursor afterAgentResponse g1 "Looking first." | quiet c-reply-1 ax turn-hook response
  echo 'fn helper() {}' >>main.rs
  cursor afterAgentResponse g1 "Added helper() with token $token in main.rs." | quiet c-reply-2 ax turn-hook response
  grep -rq "$token" .ax/turns && { echo "O4: snapshot holds the token" >&2; exit 1; }
  cursor stop g1 | quiet c-end ax turn-hook end
  body="$(sql "select body from memories where kind = 'turn'")"
  grep -q "Outcome: Added helper() with token \[redacted\] in main.rs." <<<"$body" ||
    { echo "O1/O4: unexpected outcome: $body" >&2; exit 1; }
  grep -q "$token" <<<"$body" && { echo "O4: token stored" >&2; exit 1; }
  grep -q "Looking first" <<<"$body" && { echo "O1: an earlier reply was stored" >&2; exit 1; }

  # O6 + O2: Claude SubagentStop writes nothing, Stop writes the main agent's reply.
  claude UserPromptSubmit | quiet cl-start ax turn-hook start
  echo 'fn b() {}' >b.rs
  claude SubagentStop "Subagent report" | ax stop-hook >"$LOG/sub.out" 2>"$LOG/sub.err" || { echo "O6: stop-hook exited nonzero" >&2; exit 1; }
  [ "$(sql "select count(*) from memories where kind = 'turn'")" = 1 ] || { echo "O6: SubagentStop wrote a turn memory" >&2; exit 1; }
  claude Stop "Created b.rs with b()." | ax stop-hook >"$LOG/stop.out" 2>"$LOG/stop.err" || { echo "O2: stop-hook exited nonzero" >&2; exit 1; }
  [ "$(sql "select count(*) from memories where kind = 'turn' and body like '%Outcome: Created b.rs with b().'")" = 1 ] ||
    { echo "O2: Claude outcome missing: $(sql "select body from memories where kind = 'turn'")" >&2; exit 1; }

  # H1 + H4: ax history lists the turn and the commit; --id gives one turn in full.
  git add -A . ':!.ax' && git commit -qm "Add helper and b.rs"
  ax history main.rs >"$LOG/history.txt" 2>&1
  grep -q " turn turn-" "$LOG/history.txt" && grep -q "Outcome: Added helper()" "$LOG/history.txt" &&
    grep -q " commit .*: Add helper and b.rs" "$LOG/history.txt" ||
    { echo "H1: unexpected history:" >&2; cat "$LOG/history.txt" >&2; exit 1; }
  id="$(sql "select id from memories where kind = 'turn' and body like '%Added helper%'")"
  ax history --id "$id" >"$LOG/history-id.txt" 2>&1
  grep -q "$id" "$LOG/history-id.txt" || { echo "H4: --id did not show the turn" >&2; exit 1; }
  ax history main.rs --since 2999-01-01 >"$LOG/history-since.txt" 2>&1
  grep -q "No matching turns or commits." "$LOG/history-since.txt" || { echo "H3: --since kept entries" >&2; exit 1; }
  if ax history main.rs --since 24-09 >"$LOG/history-bad.txt" 2>&1; then
    echo "H3: a bad --since was accepted" >&2; exit 1
  fi

  # P1: a turn older than 90 days is backed up, then deleted, at the next turn end that writes.
  old_ms=$(node -e 'console.log(Date.now() - 91 * 86400000)')
  sql "update memories set created_at = $old_ms where id = '$id'"
  cursor beforeSubmitPrompt g2 | quiet c2-start ax turn-hook start
  echo 'fn c() {}' >>main.rs
  cursor stop g2 | quiet c2-end ax turn-hook end
  [ "$(sql "select count(*) from memories where id = '$id'")" = 0 ] || { echo "P1: old turn not pruned" >&2; exit 1; }
  grep -q "\"id\":\"$id\"" .ax/backups/turn-memories-*.jsonl || { echo "P1: old turn not in the backup" >&2; exit 1; }
  git status --porcelain --untracked-files=all | grep -q "\.ax/backups" && { echo "P1: backup is not ignored by git" >&2; exit 1; }

  # Installer: afterAgentResponse is wired, once, and removed again.
  mkdir -p "$LOG/installdir" && cd "$LOG/installdir"
  HOME="$LOG/home" ax install --yes --target cursor >"$LOG/install1.log" 2>&1
  HOME="$LOG/home" ax install --yes --target cursor >"$LOG/install2.log" 2>&1
  [ "$(grep -o 'turn-hook response' "$LOG/home/.cursor/hooks.json" | wc -l | tr -d ' ')" = 1 ] ||
    { echo "installer: afterAgentResponse entry missing or duplicated" >&2; exit 1; }
  node -e 'const c=require(process.argv[1]);if(!c.hooks.afterAgentResponse.some(h=>/turn-hook response$/.test(h.command)))process.exit(1)' "$LOG/home/.cursor/hooks.json" ||
    { echo "installer: turn-hook response is not under afterAgentResponse" >&2; exit 1; }
  HOME="$LOG/home" ax uninstall >"$LOG/uninstall.log" 2>&1
  grep -q "turn-hook" "$LOG/home/.cursor/hooks.json" && { echo "installer: uninstall left turn hooks" >&2; exit 1; }
  true
)
real_exit=$?
set -e
[ "$real_exit" -eq 0 ] || fail "real execution"
echo "   O1 O2 O4 O6 H1 H3 H4 P1 and the installer verified with the release binary"
fi

if [ "$LAYERS" != "1 2 3 4 5 6 7" ] || [ "${GAUNTLET_ALLOW_DIRTY:-0}" = 1 ]; then
  echo "PARTIAL RUN ($LAYERS, dirty=${GAUNTLET_ALLOW_DIRTY:-0}): not evidence"
  exit 0
fi
echo "GAUNTLET PASSED ($(git rev-parse --short HEAD))"
