#!/usr/bin/env bash
# Gauntlet for docs/specs/macro-calls-history-subject.md. Run from the repository root.
# Every layer must pass; the script stops at the first failure.
set -euo pipefail
export CARGO_TARGET_DIR="${CARGO_TARGET_DIR:-target-dev}"
export CARGO_INCREMENTAL=0
# Test binaries run against an empty, throwaway HOME (see gauntlet-preflight-quality.sh).
export CARGO_HOME="${CARGO_HOME:-$HOME/.cargo}"
export RUSTUP_HOME="${RUSTUP_HOME:-$HOME/.rustup}"
GAUNTLET_HOME="$(mktemp -d)"
trap 'rm -rf "$GAUNTLET_HOME"' EXIT
CRATES=(-p ax-extraction -p ax-memory -p ax-core -p ax-mcp -p ax-cli)

echo "== source state: $(git rev-parse HEAD) ($(git status --porcelain | wc -l | tr -d ' ') uncommitted files)"
echo "== toolchain: $(rustc --version); $(python3 --version)"

echo "== 1/7 tests"
cargo test "${CRATES[@]}" --no-run
HOME="$GAUNTLET_HOME" AX_HOME_DIR="$GAUNTLET_HOME" AX_SKIP_WEB_BUILD=1 cargo test "${CRATES[@]}" 2>&1 | tee "$CARGO_TARGET_DIR/gauntlet-tests.log" | grep -E "^test result|FAILED|panicked" || true
if grep -qE "test result: FAILED|^error" "$CARGO_TARGET_DIR/gauntlet-tests.log"; then echo "tests failed"; exit 1; fi
grep -c "^test result: ok" "$CARGO_TARGET_DIR/gauntlet-tests.log" | sed 's/^/suites ok: /'
awk '/^test result: ok/ {p+=$4} END {print "tests passed: " p}' "$CARGO_TARGET_DIR/gauntlet-tests.log"

echo "== 2/7 clippy"
cargo clippy "${CRATES[@]}" --all-targets -- -D warnings

echo "== 3/7 format (the Rust files this spec changed)"
rustfmt --edition 2021 --check \
  crates/ax-extraction/src/languages/rust.rs crates/ax-extraction/src/languages/refs.rs \
  crates/ax-extraction/src/extraction_version.rs \
  crates/ax-memory/src/history.rs crates/ax-memory/tests/history.rs

echo "== 4/7 mutants"
python3 tools/graph_history_mutants.py

echo "== 5/7 release build"
cargo build --release -p ax-cli

echo "== 6/7 end-to-end, every MCP tool, as a Claude client and as a lean client"
python3 tools/ax_agent_e2e.py "$CARGO_TARGET_DIR/release/ax"

echo "== 7/7 slim (headless) build: same end-to-end run"
cargo build --profile release-slim -p ax-cli --no-default-features --features "minimal-grammars mimalloc"
python3 tools/ax_agent_e2e.py "$CARGO_TARGET_DIR/release-slim/ax"

echo "GAUNTLET PASSED"
