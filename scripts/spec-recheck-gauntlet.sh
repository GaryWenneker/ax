#!/usr/bin/env bash
# Gauntlet for the spec recheck in docs/specs/spec-plan-context-overhaul/EVIDENCE-RECHECK.md.
# Every layer fails closed: the first failing layer stops the script with a nonzero exit.
# Needs no running server; real execution (browser walkthrough, CLI pilot smoke) is recorded in the evidence file.
#
# Usage:
#   scripts/spec-recheck-gauntlet.sh                  every layer; prints GAUNTLET OK only then
#   LAYERS="clippy" scripts/spec-recheck-gauntlet.sh  selected layers (usage, web, ui, tsc, coverage, clippy,
#                                                     rustfmt, mutants, controls); a partial run never prints GAUNTLET OK
set -euo pipefail
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
UI="$ROOT/crates/ax-web/web-ui"
LOG="$(mktemp -t spec-recheck-gauntlet.XXXXXX)"
cd "$ROOT"

RUST_CHANGED=(
  crates/ax-usage/src/context_authority.rs
  crates/ax-usage/src/lib.rs
  crates/ax-usage/src/replay_validation.rs
  crates/ax-usage/src/spec_hierarchy.rs
  crates/ax-usage/src/spec_registry.rs
  crates/ax-web/src/context_authority_api.rs
  crates/ax-web/src/specs.rs
  crates/ax-web/tests/context_authority_api.rs
)

want() { [ -z "${LAYERS+x}" ] || [[ " $LAYERS " == *" $1 "* ]]; }

code_state() {
  local scope=(. ':(exclude)docs' ':(exclude)crates/ax-web/web-ui/dist')
  {
    git diff HEAD --binary -- "${scope[@]}"
    git ls-files --others --exclude-standard -z -- "${scope[@]}" | xargs -0 shasum -a 256
  } | shasum -a 256 | cut -d' ' -f1
}

layer() {
  local name="$1"
  shift
  if ! "$@" >"$LOG" 2>&1; then
    echo "FAIL: $name"
    tail -40 "$LOG"
    exit 1
  fi
}

cargo_totals() {
  awk '/^test result:/ {
         for (i = 1; i < NF; i++) {
           if ($(i + 1) ~ /^passed;/) passed += $i
           if ($(i + 1) ~ /^failed;/) failed += $i
           if ($(i + 1) ~ /^ignored;/) ignored += $i
         }
         suites++
       }
       END { printf "%d suites, %d passed, %d failed, %d ignored", suites, passed, failed, ignored }' "$LOG"
}

control() {
  local id="$1" expect="$2"
  if CONTROLS=1 ONLY="$id" bash scripts/spec-recheck-mutants.sh >"$LOG" 2>&1; then
    echo "FAIL: negative control $id passed"
    cat "$LOG"
    exit 1
  fi
  if ! grep -qF -- "$expect" "$LOG"; then
    echo "FAIL: negative control $id failed for another reason"
    cat "$LOG"
    exit 1
  fi
  echo "control $id: exits nonzero with \"$expect\""
}

echo "HEAD $(git rev-parse HEAD)"
start="$(code_state)"
echo "code state (diff and untracked files; docs and web-ui/dist excluded): $start"
echo "$(cargo --version) | $(rustc --version) | node $(node --version) | tsc $(cd "$UI" && npx tsc --version)"

if want usage; then
  for run in 1 2 3; do
    layer "ax-usage lib, run $run" cargo test -q -p ax-usage --lib
    echo "ax-usage lib, run $run: $(cargo_totals)"
  done
fi

if want web; then
  # tests/changes_feed.rs has a pre-existing flake outside this recheck: concurrent first opens of a fresh
  # global.db race on the switch to WAL (EVIDENCE-RECHECK.md, "Baseline flake"). It still fails this layer.
  layer "ax-web tests" cargo test -q -p ax-web --no-fail-fast
  echo "ax-web lib, integration, and doc tests: $(cargo_totals)"
fi

if want ui; then
  layer "web-ui npm test" bash -c "cd '$UI' && npm test"
  echo "web-ui npm test: $(grep -E '^ℹ (tests|pass|fail) ' "$LOG" | paste -sd ' ' -)"
fi

if want tsc; then
  layer "web-ui tsc" bash -c "cd '$UI' && npx tsc --noEmit"
  echo "web-ui tsc --noEmit: no errors"
fi

if want coverage; then
  layer "manifestAuthority.ts coverage" bash -c "cd '$UI' && node --experimental-strip-types --test \
    --experimental-test-coverage --test-coverage-include=src/lib/manifestAuthority.ts \
    --test-coverage-lines=100 --test-coverage-branches=100 --test-coverage-functions=100 \
    src/lib/manifestAuthority.test.ts"
  # node leaves a file that was never loaded out of the report and still prints 100%, so the row must be present.
  if ! grep -Eq 'manifestAuthority\.ts +\| +100\.00 +\| +100\.00 +\| +100\.00' "$LOG"; then
    echo "FAIL: manifestAuthority.ts is missing from the coverage report or below 100%"
    cat "$LOG"
    exit 1
  fi
  echo "manifestAuthority.ts coverage: 100% lines, branches, functions"
  # Branches are not enforced: node sets thresholds per file, and the file's payload parser, which no rechecked
  # behavior covers, has untested defensive branches.
  layer "hierarchyList.ts coverage" bash -c "cd '$UI' && node --experimental-strip-types --test \
    --experimental-test-coverage --test-coverage-include=src/lib/hierarchyList.ts \
    --test-coverage-lines=100 --test-coverage-functions=100 src/lib/hierarchyList.test.ts"
  row='hierarchyList\.ts +\| +100\.00 +\| +[0-9.]+ +\| +100\.00'
  if ! grep -Eq "$row" "$LOG"; then
    echo "FAIL: hierarchyList.ts is missing from the coverage report or below 100% lines or functions"
    cat "$LOG"
    exit 1
  fi
  echo "hierarchyList.ts coverage: 100% lines and functions, $(grep -Eo "$row" "$LOG" | awk -F'|' '{gsub(/ /, "", $3); print $3}')% branches (not enforced)"
fi

if want clippy; then
  layer "clippy" cargo clippy -p ax-usage -p ax-web --all-targets
  locations=0
  changed_hits=0
  while IFS= read -r line; do
    [[ "$line" == *"--> "* ]] || continue
    locations=$((locations + 1))
    for file in "${RUST_CHANGED[@]}"; do
      if [[ "$line" == *"--> $file:"* ]]; then
        echo "clippy finding in a changed file: $line"
        changed_hits=$((changed_hits + 1))
      fi
    done
  done <"$LOG"
  if [ "$changed_hits" -ne 0 ]; then
    echo "FAIL: $changed_hits clippy findings in changed files"
    exit 1
  fi
  echo "clippy ax-usage + ax-web --all-targets: $locations finding locations in other files, 0 in changed files"
fi

if want rustfmt; then
  layer "rustfmt" rustfmt --check --edition 2021 "${RUST_CHANGED[@]}"
  echo "rustfmt --check on changed Rust files: clean"
fi

if want mutants; then
  layer "mutants" bash scripts/spec-recheck-mutants.sh
  cat "$LOG"
fi

if want controls; then
  control C1 "SURVIVED: C1"
  control C2 "anchor found 0 times"
  control C3 "ERROR: C3"
  control C4 "ERROR: C4"
fi

end="$(code_state)"
if [ "$start" != "$end" ]; then
  echo "FAIL: code changed during the gauntlet ($start -> $end)"
  exit 1
fi
echo "code state unchanged: $end"
if [ -n "${LAYERS+x}" ]; then
  echo "PARTIAL RUN (layers: ${LAYERS:-none}); not a gauntlet pass"
  exit 0
fi
echo "GAUNTLET OK"
