#!/usr/bin/env bash
# Local opt-in pilot checks. No live model/provider evaluation is dispatched.
set -euo pipefail
cd "$(dirname "$0")/.."
baseline="${STE_BASE_SHA:-94975f9cfe39f164f14db254973fc860d848eb96}"
report_dir="${STE_REPORT_DIR:-target-dev/ste-output-evidence}"
export PYTHONDONTWRITEBYTECODE=1
if [[ -n "$(git status --porcelain --untracked-files=normal)" ]]; then
  printf 'Commit the implementation before running the evidence gauntlet.\n' >&2
  exit 1
fi
mkdir -p "$report_dir"
rm -f "$report_dir"/coverage.json "$report_dir"/coverage-gate.json "$report_dir"/mutants.json "$report_dir"/measurements.jsonl "$report_dir"/evaluation.json
git rev-parse HEAD > "$report_dir/source-sha.txt"
git diff --binary "$baseline" > "$report_dir/source-diff.patch"
rustc --version > "$report_dir/toolchain.txt"
cargo --version >> "$report_dir/toolchain.txt"
python3 --version >> "$report_dir/toolchain.txt"
# The repository-wide suite includes unrelated CLI startup price fetches.
# This scoped substitute runs the complete affected MCP and policy suites;
# record the full-workspace runtime layer as blocked, never as passed.
cargo test -p ax-mcp -p ax-policy --locked --no-fail-fast -- --test-threads=1
cargo check --workspace --locked
cargo clippy --workspace --all-targets --locked -- -D warnings
rustfmt --edition 2021 --check crates/ax-mcp/src/text_style.rs crates/ax-mcp/tests/ste_output_contract.rs crates/ax-mcp/examples/evaluate_ste_output.rs
python3 -m unittest discover -s tools -p 'test_*ste_output*.py' -v
cargo llvm-cov -p ax-mcp --locked --json --output-path "$report_dir/coverage.json" -- --test-threads=1
python3 tools/check_ste_output_coverage.py --report "$report_dir/coverage.json" --base "$baseline" > "$report_dir/coverage-gate.json"
python3 tools/ste_output_mutants.py --output "$report_dir/mutants.json"
# The mutation runner restored the source; verify its green state again.
cargo test -p ax-mcp --locked --test ste_output_contract -- --test-threads=1
cargo run -p ax-mcp --example evaluate_ste_output --locked > "$report_dir/measurements.jsonl"
python3 tools/evaluate-ste-output.py --measurements "$report_dir/measurements.jsonl" > "$report_dir/evaluation.json"
git diff --check
printf 'STE output gauntlet finished. Reports: %s\n' "$report_dir"
