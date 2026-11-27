#!/usr/bin/env bash
# Every gauntlet layer for docs/specs/git-hook-exec.md in one run. Any failure, crash or skipped
# item fails the whole run. Usage: bash scripts/gauntlet-git-hooks.sh [base-ref, default main]
# For negative controls only: GAUNTLET_LAYERS="3 4" runs a subset, GAUNTLET_ALLOW_DIRTY=1 skips the
# clean-tree check. A run with either set is not evidence.
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
cd "$ROOT"
BASE="${1:-main}"
CHANGED_RS=(
  crates/ax-sync/src/git_hooks.rs
  crates/ax-mcp/src/server.rs
  crates/ax-cli/src/commands/ship.rs
  crates/ax-cli/src/main.rs
)
source "$ROOT/scripts/lib/gauntlet.sh"
gauntlet_init git-hooks
gauntlet_load_baseline scripts/lib/baseline-git-hooks.txt
gauntlet_header

if want 1; then
echo "== 1. targeted tests"
cargo_ test -q -p ax-sync -p ax-mcp -p ax-cli >"$LOG/targeted.log" 2>&1 || fail "targeted tests"
grep -E "^test result" "$LOG/targeted.log" | awk '{p+=$4; f+=$6} END {print "   passed="p" failed="f}'
fi

if want 2; then gauntlet_workspace_against_baseline; fi
if want 3; then gauntlet_clippy_changed_lines -p ax-sync -p ax-mcp -p ax-cli; fi
if want 4; then gauntlet_rustfmt_drift; fi
if want 5 || want 7; then gauntlet_build_and_check_docs; fi

if want 6; then
echo "== 6. mutants"
bash scripts/mutants-git-hooks.sh || fail "mutants"
fi

if want 7; then
echo "== 7. real execution: commit in a temp repo with the fresh binary"
repo="$LOG/repo"
bin="$LOG/bin"
mkdir -p "$repo" "$bin" "$LOG/home"
ln -s "$ROOT/target-dev/release/ax" "$bin/ax"
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
  for h in post-commit post-merge post-checkout; do
    [ -x ".git/hooks/$h" ] || { echo "hook $h not executable" >&2; exit 1; }
    [ "$(head -1 ".git/hooks/$h")" = "#!/bin/sh" ] || { echo "hook $h has no shebang" >&2; exit 1; }
    grep -qx "ax ship --evaluate --quiet" ".git/hooks/$h" || { echo "hook $h has no quiet ship line" >&2; exit 1; }
  done
  echo 'fn a() {}' >>main.rs
  git add main.rs
  git commit -m "feat: gauntlet quiet hook" >"$LOG/commit.out" 2>"$LOG/commit.err"
  if [ -s "$LOG/commit.err" ]; then
    echo "commit printed on stderr:" >&2
    cat "$LOG/commit.err" >&2
    exit 1
  fi
  extra="$(grep -vE '^\[|^ [0-9]+ files? changed' "$LOG/commit.out" || true)"
  [ -z "$extra" ] || { echo "commit printed more than git's summary: $extra" >&2; exit 1; }
  ax recall "gauntlet quiet hook" >"$LOG/recall.log" 2>&1
  grep -q "\[git\] feat: gauntlet quiet hook" "$LOG/recall.log" || { echo "no git memory for the commit" >&2; exit 1; }
)
real_exit=$?
set -e
agent_files_after="$(user_agent_files)"
[ "$agent_files_after" = "$agent_files_before" ] || fail "real execution changed the user's agent configs"
[ "$real_exit" -eq 0 ] || fail "real execution"
echo "   hooks executable with shebang and quiet ship line; commit printed only git's summary; git memory captured"
fi

gauntlet_finish
