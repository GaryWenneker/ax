#!/usr/bin/env bash
# Gauntlet for the Neural plan audit (docs/specs/ax-neural/SPEC.md, EVIDENCE.md).
# Runs every executable layer; a failed check, a crash, a new failing test, a new warning in Neural
# code, or a change to the source tree during the run exits 1. Every home-grown check is first run
# against a known-bad input and must fail on it.
# Cargo runs with AX_SKIP_WEB_BUILD=1: the web-ui build needs the npm configuration of the real home
# directory. The web layer runs node and tsc directly; scripts/reinstall-cli.sh does the real build.
# Tests run with an empty HOME: global rules in the real home change preflight replies.
# Failures that exist without the Neural change are listed in scripts/neural-baseline-failures.txt.
#   scripts/neural-gauntlet.sh   # every layer, about an hour
set -euo pipefail
cd "$(dirname "$0")/.."

WORK="$(mktemp -d)"
trap 'rm -rf "$WORK"' EXIT
EMPTY_HOME="$WORK/home"
mkdir -p "$EMPTY_HOME"
export AX_SKIP_WEB_BUILD=1
export CARGO_HOME="${CARGO_HOME:-$HOME/.cargo}"
export RUSTUP_HOME="${RUSTUP_HOME:-$HOME/.rustup}"
unset ONLY
BASELINE=scripts/neural-baseline-failures.txt
ACCEPTED=scripts/neural-accepted-mutants.txt
NEURAL_FILES='^crates/(ax-neural/|ax-mcp/src/neural_tool\.rs|ax-mcp/tests/neural_preflight\.rs|ax-cli/src/commands/neural\.rs|ax-web/src/neural\.rs|ax-db/tests/migration_v4[56]\.rs)'

hermetic() { HOME="$EMPTY_HOME" "$@"; }

# Exit 1 unless the command fails; used for negative controls.
must_fail() {
  local what="$1"; shift
  if "$@" >"$WORK/control.out" 2>&1; then
    cat "$WORK/control.out"; echo "CONTROL PASSED, so the check cannot fail: $what"; exit 1
  fi
  echo "control ok: $what"
}

# EVIDENCE.md is left out: it is written from this run's output, after the run.
state_hash() {
  local skip=':(exclude)docs/specs/ax-neural/EVIDENCE.md'
  { git diff HEAD --binary -- . "$skip"; git ls-files --others --exclude-standard -z -- . "$skip" | xargs -0 shasum -a 256; } \
    | shasum -a 256 | cut -d' ' -f1
}

echo "== source state"
echo "HEAD $(git rev-parse HEAD)"
echo "changed tracked files: $(git diff HEAD --name-only | wc -l | tr -d ' '); untracked files: $(git ls-files --others --exclude-standard | wc -l | tr -d ' ')"
START_STATE="$(state_hash)"
echo "source-tree hash: $START_STATE"
rustc --version
cargo --version
node --version
echo "npm $(npm --version)"
python3 --version
cargo mutants --version
cargo llvm-cov --version
cargo audit --version

echo "== spec map: every test named in SPEC.md exists"
python3 scripts/neural-spec-map.py docs/specs/ax-neural/SPEC.md
cp docs/specs/ax-neural/SPEC.md "$WORK/spec.md"
printf '| control | `dispatch::tests::no_such_test_exists_in_this_crate` (then an escaped pipe: `\\|`) |\n' >>"$WORK/spec.md"
must_fail "spec map with a missing test" python3 scripts/neural-spec-map.py "$WORK/spec.md"

echo "== invariants (each gate runs its own negative control first)"
python3 scripts/neural-invariants.py

echo "== rustfmt"
rustfmt --check --edition 2021 crates/ax-neural/src/lib.rs crates/ax-mcp/src/neural_tool.rs \
  crates/ax-mcp/src/tools.rs crates/ax-mcp/src/context_budget.rs crates/ax-mcp/tests/neural_preflight.rs \
  crates/ax-cli/src/main.rs crates/ax-web/src/neural.rs crates/ax-db/tests/migration_v45.rs \
  crates/ax-db/tests/migration_v46.rs
echo "rustfmt: clean (crates/ax-cli/src/main.rs pulls in all of ax-cli)"

echo "== supply chain: no known vulnerability in the Neural dependency tree"
cargo tree -p ax-neural --all-features -e normal,build --prefix none >"$WORK/tree.txt"
set +e
cargo audit --json >"$WORK/audit.json" 2>"$WORK/audit.err"
audit_status=$?
set -e
[ "$audit_status" -le 1 ] || { cat "$WORK/audit.err"; echo "cargo audit failed to run (exit $audit_status)"; exit 1; }
audit_verdict() {  # <audit json> <cargo tree output>
  python3 - "$1" "$2" <<'PY'
import json, re, sys
report = json.load(open(sys.argv[1], encoding="utf-8"))
tree = set(re.findall(r"^(\S+) v(\S+)", open(sys.argv[2], encoding="utf-8").read(), re.M))
if not tree:
    sys.exit("the dependency tree is empty")
found = report["vulnerabilities"]["list"]
neural = [v for v in found if (v["package"]["name"], v["package"]["version"]) in tree]
for v in neural:
    print(f"VULNERABLE {v['package']['name']} {v['package']['version']}: {v['advisory']['id']}")
warnings = report.get("warnings", {})
for kind in sorted(warnings):
    names = sorted({f"{w['package']['name']} {w['package']['version']}" for w in warnings[kind]})
    inside = sorted(n for n in names if tuple(n.split(" ")) in tree)
    print(f"warning {kind}: {len(names)} in the workspace; in the Neural tree: {', '.join(inside) or 'none'}")
print(f"vulnerabilities: {len(found)} in the workspace, {len(neural)} in the Neural tree")
sys.exit(1 if neural else 0)
PY
}
audit_verdict "$WORK/audit.json" "$WORK/tree.txt"
python3 - "$WORK/audit.json" "$WORK/tree.txt" "$WORK/audit-control.json" <<'PY'
import json, re, sys
report = json.load(open(sys.argv[1], encoding="utf-8"))
name, version = re.search(r"^(\S+) v(\S+)", open(sys.argv[2], encoding="utf-8").read(), re.M).groups()
report["vulnerabilities"]["list"].append(
    {"package": {"name": name, "version": version}, "advisory": {"id": "RUSTSEC-CONTROL"}})
json.dump(report, open(sys.argv[3], "w", encoding="utf-8"))
PY
must_fail "audit with a vulnerable crate in the Neural tree" audit_verdict "$WORK/audit-control.json" "$WORK/tree.txt"

echo "== web-ui: node tests, tsc, and full coverage of neuralMetrics.ts"
web_coverage() {  # extra node arguments
  (cd crates/ax-web/web-ui && node --experimental-strip-types --test --experimental-test-coverage \
    --test-coverage-include=src/components/neuralMetrics.ts --test-coverage-lines=100 \
    --test-coverage-branches=100 --test-coverage-functions=100 "$@" src/components/neuralMetrics.test.ts)
}
(cd crates/ax-web/web-ui && npm test 2>&1 | grep -E "^ℹ (tests|pass|fail)")
(cd crates/ax-web/web-ui && npx tsc --noEmit -p .)
web_coverage >"$WORK/web-coverage.txt" 2>&1 || { cat "$WORK/web-coverage.txt"; echo "web coverage below 100%"; exit 1; }
grep -E "neuralMetrics\.ts" "$WORK/web-coverage.txt"
must_fail "web coverage from one test" web_coverage --test-name-pattern="shows rank latency per preflight"
echo "web-ui: node tests green, tsc clean, neuralMetrics.ts 100% lines, branches, and functions"

echo "== clippy: no warnings in Neural files"
clippy_hits() { grep -E "$NEURAL_FILES" || true; }
clippy_check() {  # <label> <cargo clippy arguments>
  local label="$1" out; shift
  out="$(cargo clippy "$@" --message-format=short -- -A clippy::invalid_regex 2>&1)" || true
  grep -q "Finished" <<<"$out" || { echo "$out" | tail -40; echo "clippy did not finish: $label"; exit 1; }
  local hits; hits="$(clippy_hits <<<"$out")"
  [ -z "$hits" ] || { echo "$hits"; echo "clippy: warnings in Neural files ($label)"; exit 1; }
  echo "clippy $label: 0 warnings in Neural files"
}
[ -n "$(clippy_hits <<<'crates/ax-neural/src/rank.rs:1:1: warning: control')" ] \
  || { echo "CONTROL PASSED, so the clippy filter cannot fail"; exit 1; }
echo "control ok: clippy filter sees a warning in a Neural file"
clippy_check "workspace crates" -p ax-neural -p ax-mcp -p ax-cli -p ax-web -p ax-db --all-targets
clippy_check "ax-neural all features" -p ax-neural --all-targets --all-features

echo "== tests (empty HOME, --no-fail-fast)"
verdict() { python3 scripts/neural-test-verdict.py "$@"; }
sample_ok='     Running unittests src/lib.rs (x)
test a::b ... ok
test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.01s'
sample_new_failure='     Running unittests src/lib.rs (x)
test a::b ... FAILED
test result: FAILED. 0 passed; 1 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.01s'
sample_crash="$sample_new_failure
     Running tests/other.rs (y)
thread 't' has overflowed its stack"
printf 'control a::b\n' >"$WORK/control-baseline.txt"
verdict control "$WORK/control-baseline.txt" 0 <<<"$sample_ok" >/dev/null
must_fail "verdict on a new failing test" verdict control "$BASELINE" 101 <<<"$sample_new_failure"
must_fail "verdict on a compile error" verdict control "$BASELINE" 101 <<<"error: could not compile \`x\`"
must_fail "verdict on a run without results" verdict control "$BASELINE" 101 <<<"error: no tests"
must_fail "verdict on a crash behind a baseline failure" \
  verdict control "$WORK/control-baseline.txt" 101 <<<"$sample_crash"
suite() {  # <label> <cargo test arguments>
  local label="$1" out status; shift
  set +e
  out="$(hermetic cargo test "$@" --no-fail-fast 2>&1)"
  status=$?
  set -e
  verdict "$label" "$BASELINE" "$status" <<<"$out"
}
suite ax-neural -p ax-neural
suite ax-neural-all-features -p ax-neural --all-features
suite ax-db -p ax-db
suite ax-memory -p ax-memory
suite ax-mcp -p ax-mcp
suite ax-web -p ax-web
suite ax-cli -p ax-cli

echo "== suite health: Neural tests at 1, 3, and 16 threads, three rounds"
ran_green() {  # cargo test -q output: a result line with at least one test, and no failure
  grep -qE "^test result: ok\. [1-9][0-9]* passed" <<<"$1" && ! grep -q "FAILED" <<<"$1"
}
must_fail "health check on a run of zero tests" ran_green "test result: ok. 0 passed; 0 failed"
health() {  # <threads> <cargo test arguments>
  local threads="$1" out; shift
  out="$(hermetic cargo test -q "$@" -- --test-threads="$threads" 2>&1)" || true
  ran_green "$out" || { echo "$out" | tail -40; echo "health run failed: $* at $threads threads"; exit 1; }
}
for round in 1 2 3; do
  for threads in 1 3 16; do
    health "$threads" -p ax-neural
    health "$threads" -p ax-mcp --test neural_preflight
    health "$threads" -p ax-mcp --lib neural
    health "$threads" -p ax-mcp --lib context_budget
    health "$threads" -p ax-cli --bin ax commands::neural
    health "$threads" -p ax-cli --bin ax neural_request_tests
    health "$threads" -p ax-web --lib neural
    health "$threads" -p ax-db --test migration_v45 --test migration_v46
  done
  out="$(cd crates/ax-web/web-ui && node --experimental-strip-types --test src/components/neuralMetrics.test.ts 2>&1)" \
    || { echo "$out" | tail -20; echo "web health run failed"; exit 1; }
  echo "round $round: green at 1, 3, and 16 threads"
done

echo "== coverage: Neural files and the Neural regions of shared files"
covered() {
  local out
  out="$(hermetic cargo llvm-cov --no-report "$@" 2>&1)" || { echo "$out" | tail -40; echo "coverage run failed: $*"; exit 1; }
  ran_green "$out" || { echo "$out" | tail -40; echo "coverage run ran no tests: $*"; exit 1; }
}
hermetic cargo llvm-cov clean --workspace
# cargo llvm-cov takes no test-name argument; filters go to the test binary after --.
covered -p ax-neural
covered -p ax-mcp --test neural_preflight
covered -p ax-mcp --lib -- neural context_budget
covered -p ax-cli --bin ax -- commands::neural neural_request_tests
covered -p ax-web --lib -- neural
hermetic cargo llvm-cov report --lcov --output-path "$WORK/lcov.info"
python3 scripts/neural-coverage.py "$WORK/lcov.info"
sed -E 's/^DA:([0-9]+),[0-9]+/DA:\1,0/' "$WORK/lcov.info" >"$WORK/zero.info"
must_fail "coverage gate on a report where nothing ran" python3 scripts/neural-coverage.py "$WORK/zero.info"
# Only predict.rs loses its hits; the regions still pass, so only its floor can catch this.
awk '/^SF:.*crates\/ax-neural\/src\/predict\.rs$/ { zero = 1 }
     /^end_of_record$/ { zero = 0 }
     zero && /^DA:/ { split(substr($0, 4), f, ","); $0 = "DA:" f[1] ",0" }
     { print }' "$WORK/lcov.info" >"$WORK/one-file-zero.info"
must_fail "coverage floor when one Neural file lost its tests" python3 scripts/neural-coverage.py "$WORK/one-file-zero.info"
must_fail "coverage gate without a report" python3 scripts/neural-coverage.py "$WORK/missing.info"

echo "== real execution: the built binary in a temporary project"
cargo build -q -p ax-cli
python3 scripts/neural-real-run.py target-dev/debug/ax
printf '#!/bin/sh\necho "{}"\n' >"$WORK/fake-ax"
chmod +x "$WORK/fake-ax"
must_fail "real run against a binary that only prints {}" python3 scripts/neural-real-run.py "$WORK/fake-ax"

echo "== mutation: hand-written mutants"
set +e
hermetic scripts/neural-mutants.sh --control >"$WORK/mutants-control.txt" 2>&1
control_status=$?
set -e
if [ "$control_status" -eq 0 ] || ! grep -q "SURVIVED control-comment" "$WORK/mutants-control.txt"; then
  cat "$WORK/mutants-control.txt"; echo "the mutation runner did not report the comment-only mutant"; exit 1
fi
echo "control ok: a comment-only mutant is reported as surviving and fails the run"
hermetic scripts/neural-mutants.sh

echo "== mutation: cargo-mutants over the whole ax-neural crate"
compare_missed() {  # <missed.txt> <accepted list>
  python3 - "$1" "$2" <<'PY'
import pathlib, sys
missed = {line.strip() for line in pathlib.Path(sys.argv[1]).read_text(encoding="utf-8").splitlines() if line.strip()}
accepted = {
    line.strip()
    for line in pathlib.Path(sys.argv[2]).read_text(encoding="utf-8").splitlines()
    if line.strip() and not line.lstrip().startswith("#")
}
for line in sorted(missed - accepted):
    print(f"NEW survivor: {line}")
for line in sorted(accepted - missed):
    print(f"STALE accepted entry, no longer missed: {line}")
print(f"missed {len(missed)}, accepted {len(accepted)}")
sys.exit(1 if missed != accepted else 0)
PY
}
grep -v '^#' "$ACCEPTED" | grep . >"$WORK/missed-control.txt"
echo "crates/ax-neural/src/control.rs:1:1: replace control" >>"$WORK/missed-control.txt"
must_fail "accepted-mutant check with an unlisted survivor" compare_missed "$WORK/missed-control.txt" "$ACCEPTED"
grep -v '^#' "$ACCEPTED" | grep . | tail -n +2 >"$WORK/missed-control.txt"
must_fail "accepted-mutant check with a stale entry" compare_missed "$WORK/missed-control.txt" "$ACCEPTED"
rm -rf target-dev/neural-mutants
set +e
# Without --gitignore the copy takes ignored output too: target-dev and .ax, hundreds of GB.
hermetic cargo mutants --package ax-neural --gitignore true -j 4 --output target-dev/neural-mutants >"$WORK/cargo-mutants.txt" 2>&1
mutants_status=$?
set -e
tail -1 "$WORK/cargo-mutants.txt"
case "$mutants_status" in
  0 | 2) ;;
  *) tail -40 "$WORK/cargo-mutants.txt"; echo "cargo mutants exit $mutants_status (3 timeout, 4 baseline failed)"; exit 1 ;;
esac
OUT=target-dev/neural-mutants/mutants.out
[ -s "$OUT/caught.txt" ] || { echo "cargo mutants caught nothing"; exit 1; }
[ ! -s "$OUT/timeout.txt" ] || { cat "$OUT/timeout.txt"; echo "cargo mutants timeouts"; exit 1; }
echo "caught $(wc -l <"$OUT/caught.txt" | tr -d ' '), missed $(wc -l <"$OUT/missed.txt" | tr -d ' '), unviable $(wc -l <"$OUT/unviable.txt" | tr -d ' ')"
compare_missed "$OUT/missed.txt" "$ACCEPTED"

echo "== source state unchanged"
END_STATE="$(state_hash)"
[ "$END_STATE" = "$START_STATE" ] || { echo "the source tree changed during the run: $START_STATE -> $END_STATE"; exit 1; }
echo "source-tree hash still $END_STATE"

echo "gauntlet: all layers green"
