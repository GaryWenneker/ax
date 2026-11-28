# Layers shared by the spec gauntlets (scripts/gauntlet-*.sh).
#
# The caller sets ROOT (repository root, already the working directory), BASE (the base ref) and
# CHANGED_RS (the Rust files the change touches), then sources this file and calls gauntlet_init.
# A gauntlet keeps only what is its own: targeted tests, mutants and real execution.
#
# For negative controls only: GAUNTLET_LAYERS="3 4" runs a subset and GAUNTLET_ALLOW_DIRTY=1 skips
# the clean-tree check. gauntlet_finish reports such a run as partial, never as evidence.

cargo_() { env -u CARGO_TARGET_DIR cargo "$@"; }
fail() { echo "GAUNTLET FAILED: $*" >&2; echo "logs: ${LOG:-none}" >&2; exit 1; }
want() { [[ " $LAYERS " == *" $1 "* ]]; }

# gauntlet_init <name>: log directory and the layer selection.
gauntlet_init() {
  LOG="$(mktemp -d "${TMPDIR:-/tmp}/ax-gauntlet-$1.XXXX")"
  LAYERS="${GAUNTLET_LAYERS:-1 2 3 4 5 6 7}"
  BASELINE_FAILURES=""
}

# gauntlet_load_baseline <file>: workspace tests already failing on the base, one per line.
gauntlet_load_baseline() {
  [ -f "$1" ] || fail "missing $1"
  BASELINE_FAILURES="$(sed '/^#/d; /^$/d' "$1")"
}

# gauntlet_header [extra tool versions]: print the source and tools; refuse a dirty tree.
gauntlet_header() {
  echo "source: $(git rev-parse HEAD) (base $BASE $(git rev-parse --short "$BASE"))"
  echo "tools: $(cargo --version) | $(rustfmt --version) | node $(node --version)${1:+ | $1}"
  if [ "${GAUNTLET_ALLOW_DIRTY:-0}" != 1 ] && [ -n "$(git status --porcelain -- crates scripts site/src Cargo.lock)" ]; then
    fail "uncommitted changes under crates/, scripts/, site/src/ or Cargo.lock; commit first so the run matches a SHA"
  fi
}

# Layer 2: the whole workspace suite may fail only on tests in the baseline.
gauntlet_workspace_against_baseline() {
  echo "== 2. workspace suite against the baseline"
  local ws_exit failed new_failures
  set +e
  cargo_ test --workspace --no-fail-fast >"$LOG/workspace.log" 2>&1
  ws_exit=$?
  set -e
  grep -qE "^test result" "$LOG/workspace.log" || fail "workspace suite produced no results (exit $ws_exit)"
  if grep -E "^error(\[E[0-9]+\])?: could not compile|^error\[E[0-9]+\]" "$LOG/workspace.log"; then
    fail "workspace suite did not compile"
  fi
  failed="$(grep -E "^test .* \.\.\. FAILED$" "$LOG/workspace.log" | sed -E 's/^test (.*) \.\.\. FAILED$/\1/' | sort -u || true)"
  new_failures="$(comm -23 <(printf '%s\n' "$failed" | sed '/^$/d') <(printf '%s\n' "$BASELINE_FAILURES" | sed '/^$/d' | sort))"
  grep -E "^test result" "$LOG/workspace.log" | awk '{p+=$4; f+=$6} END {print "   passed="p" failed="f}'
  printf '%s\n' "$failed" | sed '/^$/d; s/^/   failed: /'
  [ -z "$new_failures" ] || fail "new test failures: $new_failures"
  if [ "$ws_exit" -ne 0 ] && [ -z "$failed" ]; then
    fail "workspace suite exited $ws_exit without a failing test"
  fi
}

# gauntlet_changed_lines <file>: one changed line number per line, relative to BASE.
gauntlet_changed_lines() {
  git diff -U0 "$BASE" -- "$1" | sed -nE 's/^@@ -[0-9,]+ \+([0-9]+)(,([0-9]+))? @@.*/\1 \3/p' |
    while read -r start count; do
      count="${count:-1}"
      for ((i = 0; i < count; i++)); do echo $((start + i)); done
    done
}

# Layer 3: gauntlet_clippy_changed_lines <crate selection...>: no clippy finding on a changed line.
gauntlet_clippy_changed_lines() {
  echo "== 3. clippy: no warnings on changed lines"
  cargo_ clippy "$@" --all-targets --message-format short -- -A clippy::invalid_regex \
    >"$LOG/clippy.log" 2>&1 || fail "clippy did not finish"
  local hits=0 f lines line msg
  for f in "${CHANGED_RS[@]}"; do
    [ -f "$f" ] || fail "changed file list is stale: $f"
    lines="$(gauntlet_changed_lines "$f")"
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
}

# Layer 4: rustfmt drift in a changed file may not grow beyond BASE; a new file may have none.
gauntlet_rustfmt_drift() {
  echo "== 4. rustfmt: no drift beyond $BASE (new files: none at all)"
  local f now before
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
}

# Layer 5: release build, then every CLI command must be documented.
gauntlet_build_and_check_docs() {
  echo "== 5. release build and CLI docs"
  cargo_ build -q --release -p ax-cli >"$LOG/build.log" 2>&1 || fail "release build"
  node scripts/check-cli-docs.mjs "$ROOT/target-dev/release/ax" || fail "cli docs"
}

# The last line of a gauntlet: a subset or a dirty tree is reported as partial, not as evidence.
gauntlet_finish() {
  if [ "$LAYERS" != "1 2 3 4 5 6 7" ] || [ "${GAUNTLET_ALLOW_DIRTY:-0}" = 1 ]; then
    echo "PARTIAL RUN ($LAYERS, dirty=${GAUNTLET_ALLOW_DIRTY:-0}): not evidence"
    exit 0
  fi
  echo "GAUNTLET PASSED ($(git rev-parse --short HEAD))"
}
