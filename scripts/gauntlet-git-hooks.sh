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
# Known failures on this machine before the change (see the spec's baseline note). The last two are
# flaky: they pass alone and fail only sometimes in the full workspace run.
BASELINE_FAILURES="bootstrap::tests::legacy_prefix_from_workspace_folder
bootstrap::tests::resolves_placeholder_to_folder_name
savings::tests::cursor_transcript_path_filter
savings::tests::transcript_import_does_not_wipe_state_tokens
seed::tests::seeds_sonar_project_key_from_folder_name"
LOG="$(mktemp -d /tmp/ax-gauntlet.XXXX)"
cargo_() { env -u CARGO_TARGET_DIR cargo "$@"; }
fail() { echo "GAUNTLET FAILED: $*" >&2; echo "logs: $LOG" >&2; exit 1; }
LAYERS="${GAUNTLET_LAYERS:-1 2 3 4 5 6 7}"
want() { [[ " $LAYERS " == *" $1 "* ]]; }

echo "source: $(git rev-parse HEAD) (base $BASE $(git rev-parse --short "$BASE"))"
echo "tools: $(cargo --version) | $(rustfmt --version) | node $(node --version)"
if [ "${GAUNTLET_ALLOW_DIRTY:-0}" != 1 ] && [ -n "$(git status --porcelain -- crates scripts site/src)" ]; then
  fail "uncommitted changes under crates/, scripts/ or site/src/; commit first so the run matches a SHA"
fi

if want 1; then
echo "== 1. targeted tests"
cargo_ test -q -p ax-sync -p ax-mcp -p ax-cli >"$LOG/targeted.log" 2>&1 || fail "targeted tests"
grep -E "^test result" "$LOG/targeted.log" | awk '{p+=$4; f+=$6} END {print "   passed="p" failed="f}'
fi

if want 2; then
echo "== 2. workspace suite against the baseline"
set +e
cargo_ test --workspace --no-fail-fast >"$LOG/workspace.log" 2>&1
ws_exit=$?
set -e
grep -qE "^test result" "$LOG/workspace.log" || fail "workspace suite produced no results (exit $ws_exit)"
grep -E "^error(\[E[0-9]+\])?: could not compile|^error\[E[0-9]+\]" "$LOG/workspace.log" &&
  fail "workspace suite did not compile"
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
cargo_ clippy -p ax-sync -p ax-mcp -p ax-cli --all-targets --message-format short -- -A clippy::invalid_regex \
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
  done < <(grep -E "^$f:[0-9]+:[0-9]+: (warning|error)" "$LOG/clippy.log" | sed -E "s#^$f:([0-9]+):[0-9]+:#\1:#")
done
[ "$hits" -eq 0 ] || fail "$hits clippy finding(s) on changed lines"
echo "   0 on changed lines"
fi

if want 4; then
echo "== 4. rustfmt: no drift beyond $BASE"
for f in "${CHANGED_RS[@]}"; do
  # By path rustfmt also checks every `mod` the file declares; count only this file's hunks.
  now="$(rustfmt --edition 2021 --check "$f" 2>&1 | grep -c "^Diff in $ROOT/$f:" || true)"
  before="$(git show "$BASE:$f" | rustfmt --edition 2021 --check 2>&1 | grep -c '^Diff in' || true)"
  echo "   $f: $now diff hunks (was $before)"
  [ "$now" -le "$before" ] || fail "rustfmt drift grew in $f"
done
fi

if want 5 || want 7; then
echo "== 5. release build and CLI docs"
cargo_ build -q --release -p ax-cli >"$LOG/build.log" 2>&1 || fail "release build"
AX_BIN="$ROOT/target-dev/release/ax" node scripts/check-cli-docs.mjs || fail "cli docs"
fi

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

if [ "$LAYERS" != "1 2 3 4 5 6 7" ] || [ "${GAUNTLET_ALLOW_DIRTY:-0}" = 1 ]; then
  echo "PARTIAL RUN ($LAYERS, dirty=${GAUNTLET_ALLOW_DIRTY:-0}): not evidence"
  exit 0
fi
echo "GAUNTLET PASSED ($(git rev-parse --short HEAD))"
