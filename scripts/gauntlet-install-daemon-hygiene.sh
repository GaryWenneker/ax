#!/usr/bin/env bash
# Every gauntlet layer for docs/specs/install-daemon-hygiene.md in one run. Any failure, crash or
# skipped item fails the whole run. Usage: bash scripts/gauntlet-install-daemon-hygiene.sh [base-ref, default main]
# For negative controls only: GAUNTLET_LAYERS="3 4" runs a subset, GAUNTLET_ALLOW_DIRTY=1 skips the
# clean-tree check. A run with either set is not evidence.
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
cd "$ROOT"
BASE="${1:-main}"
CHANGED_RS=(
  crates/ax-cli/src/commands/init.rs
  crates/ax-cli/src/commands/mod.rs
  crates/ax-cli/src/commands/ship.rs
  crates/ax-cli/src/commands/sync.rs
  crates/ax-cli/tests/daemon_lifecycle.rs
  crates/ax-cli/tests/hooks_without_ax.rs
  crates/ax-cli/tests/init_gitignore.rs
  crates/ax-installer/src/ax_command.rs
  crates/ax-installer/src/lib.rs
  crates/ax-installer/src/targets.rs
  crates/ax-mcp/src/daemon.rs
  crates/ax-mcp/src/exe_identity.rs
  crates/ax-mcp/src/lib.rs
  crates/ax-mcp/src/proxy.rs
  crates/ax-mcp/src/proxy_pump.rs
  crates/ax-policy/src/agents_share.rs
)
source "$ROOT/scripts/lib/gauntlet.sh"
gauntlet_init hygiene
gauntlet_load_baseline scripts/lib/baseline-install-daemon-hygiene.txt
gauntlet_header "git $(git --version | cut -d' ' -f3)"

if want 1; then
echo "== 1. targeted tests"
{
  cargo_ test -q -p ax-installer &&
    cargo_ test -q -p ax-policy --lib agents_share &&
    cargo_ test -q -p ax-mcp --lib &&
    cargo_ test -q -p ax-cli --test hooks_without_ax --test init_gitignore --test daemon_lifecycle
} >"$LOG/targeted.log" 2>&1 || fail "targeted tests"
grep -E "^test result" "$LOG/targeted.log" | awk '{p+=$4; f+=$6} END {print "   passed="p" failed="f}'
for run in 1 2 3 4 5; do # suite health: the reconnect tests race real processes and sockets
  cargo_ test -q -p ax-mcp --lib proxy_pump >"$LOG/repeat-pump-$run.log" 2>&1 || fail "pump tests failed on repeat $run"
  cargo_ test -q -p ax-cli --test daemon_lifecycle >"$LOG/repeat-daemon-$run.log" 2>&1 ||
    fail "daemon lifecycle tests failed on repeat $run"
done
echo "   pump and daemon lifecycle tests green 5/5 repeats"
fi

if want 2; then gauntlet_workspace_against_baseline; fi
if want 3; then gauntlet_clippy_changed_lines -p ax-cli -p ax-installer -p ax-mcp -p ax-policy; fi
if want 4; then gauntlet_rustfmt_drift; fi
if want 5 || want 7; then gauntlet_build_and_check_docs; fi

if want 6; then
echo "== 6. mutants"
bash scripts/mutants-install-daemon-hygiene.sh || fail "mutants"
fi

if want 7; then
echo "== 7. real execution with the fresh binary"
REL="$ROOT/target-dev/release/ax"
. scripts/lib/user-agent-files.sh
agent_files_before="$(user_agent_files)"
# Not `( … ) || fail`: bash ignores `set -e` inside a subshell whose status is tested.
set +e
(
  set -euo pipefail
  export HOME="$LOG/home" AX_NO_UPDATE_CHECK=1 NO_COLOR=1
  mkdir -p "$HOME/.cursor" "$HOME/.claude" "$LOG/shim" "$LOG/copy"
  # C1: agents must get the PATH shim, not the binary that runs the install.
  printf '#!/bin/sh\nexec "%s" "$@"\n' "$REL" >"$LOG/shim/ax"
  chmod +x "$LOG/shim/ax"
  cp "$REL" "$LOG/copy/ax"
  export PATH="$LOG/shim:$PATH"
  [ "$(command -v ax)" = "$LOG/shim/ax" ] || { echo "setup: the shim is not the first ax on PATH" >&2; exit 1; }

  # A5 + B1: an initialized repo, a fresh worktree, a checkout inside it.
  repo="$LOG/repo"
  mkdir -p "$repo" && cd "$repo"
  git init -q
  git config user.email gauntlet@example.invalid
  git config user.name gauntlet
  echo 'fn main() {}' >main.rs
  git add main.rs
  git commit -qm init
  ax init >"$LOG/init.log" 2>&1
  stray="$(git status --porcelain --untracked-files=all -- .ax | sed 's/^?? //' |
    grep -v -e '^\.ax/\.gitignore$' -e '^\.ax/policy/' || true)"
  [ -z "$stray" ] || { echo "B1: untracked under .ax/: $stray" >&2; exit 1; }
  git worktree add -q "$LOG/wt" -b wt-branch >"$LOG/worktree.log" 2>&1 ||
    { echo "A5: git worktree add failed:" >&2; cat "$LOG/worktree.log" >&2; exit 1; }
  cd "$LOG/wt"
  git checkout -q -b wt-second >"$LOG/checkout.log" 2>&1 ||
    { echo "A5: checkout in the worktree failed:" >&2; cat "$LOG/checkout.log" >&2; exit 1; }
  if grep -qi "ax\|not initialized" "$LOG/worktree.log" "$LOG/checkout.log"; then
    echo "A5: ax printed output during worktree add or checkout:" >&2
    cat "$LOG/worktree.log" "$LOG/checkout.log" >&2
    exit 1
  fi
  [ ! -e "$LOG/wt/.ax" ] || { echo "A5: the worktree got a .ax/" >&2; exit 1; }
  grep -rq "ax sync --quiet" "$repo/.git/hooks" ||
    { echo "A5: ax init installed no hook that runs ax sync --quiet; the check proved nothing" >&2; exit 1; }

  # C1 + C3 + C6: install from the copy; configs get the shim, the report names both.
  cd "$repo"
  "$LOG/copy/ax" install --yes --target cursor >"$LOG/install-cursor.log" 2>&1
  node -e 'const c=require(process.argv[1]);if(c.mcpServers.ax.command!==process.argv[2])process.exit(1)' \
    "$HOME/.cursor/mcp.json" "$LOG/shim/ax" ||
    { echo "C1/C6: mcp.json does not run the shim:" >&2; cat "$HOME/.cursor/mcp.json" >&2; exit 1; }
  grep -q "Agents run ax from PATH: $LOG/shim/ax (this binary: $LOG/copy/ax)" "$LOG/install-cursor.log" ||
    { echo "C3: the report does not name both paths:" >&2; cat "$LOG/install-cursor.log" >&2; exit 1; }

  # C4: a stale Claude hook path is replaced in place; the user's own hook stays.
  node -e 'require("fs").writeFileSync(process.argv[1],JSON.stringify({hooks:{Stop:[
    {hooks:[{type:"command",command:"/tmp/ax-gone/bin/ax stop-hook"}]},
    {hooks:[{type:"command",command:"user-notify --done"}]}]}},null,2))' "$HOME/.claude/settings.json"
  "$LOG/copy/ax" install --yes --target claude >"$LOG/install-claude.log" 2>&1
  node -e 'const s=require(process.argv[1]).hooks.Stop;const want=process.argv[2]+" stop-hook";
    if(s.length!==2||s[0].hooks[0].command!==want||s[1].hooks[0].command!=="user-notify --done")process.exit(1)' \
    "$HOME/.claude/settings.json" "$LOG/shim/ax" ||
    { echo "C4: Claude Stop hooks not repaired in place:" >&2; cat "$HOME/.claude/settings.json" >&2; exit 1; }

  # E5 with the release binary: replacing the file stops its daemon and removes daemon.json.
  cp "$REL" "$LOG/copy/ax-daemon"
  AX_DAEMON_EXE_CHECK_MS=200 "$LOG/copy/ax-daemon" serve --mcp --daemon --path "$repo" \
    </dev/null >/dev/null 2>"$LOG/daemon.err" &
  daemon_pid=$!
  for _ in $(seq 1 200); do [ -f .ax/daemon.json ] && break; sleep 0.1; done
  [ -f .ax/daemon.json ] || { echo "E5: the daemon never wrote daemon.json" >&2; kill "$daemon_pid"; exit 1; }
  sleep 0.6
  kill -0 "$daemon_pid" 2>/dev/null || { echo "E5: the daemon stopped with its binary untouched" >&2; exit 1; }
  cp "$REL" "$LOG/copy/ax-daemon.new" && printf '\0' >>"$LOG/copy/ax-daemon.new"
  mv "$LOG/copy/ax-daemon.new" "$LOG/copy/ax-daemon"
  for _ in $(seq 1 100); do kill -0 "$daemon_pid" 2>/dev/null || break; sleep 0.1; done
  if kill -0 "$daemon_pid" 2>/dev/null; then
    kill "$daemon_pid"; echo "E5: the daemon kept running on a replaced binary" >&2; exit 1
  fi
  wait "$daemon_pid" || { echo "E5: the daemon exited nonzero" >&2; exit 1; }
  [ ! -f .ax/daemon.json ] || { echo "E5: daemon.json left behind" >&2; exit 1; }
  true
)
real_exit=$?
set -e
agent_files_after="$(user_agent_files)"
[ "$agent_files_after" = "$agent_files_before" ] || fail "real execution changed the user's agent configs"
[ "$real_exit" -eq 0 ] || fail "real execution"
echo "   A5 B1 C1 C3 C4 C6 E5 verified with the release binary; D7 E3 E4 E5 run as real processes in layer 1"
fi

gauntlet_finish
