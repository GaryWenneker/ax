#!/usr/bin/env bash
# Gauntlet for docs/specs/preflight-quality.md. Run from the repository root.
# Every layer must pass; the script stops at the first failure.
set -euo pipefail
export CARGO_TARGET_DIR="${CARGO_TARGET_DIR:-target-dev}"
export CARGO_INCREMENTAL=0
# Tests read ~/.ax (global policy, global.db). On a machine with a large global
# policy the required rules alone exceed the 12,000-token test budget and
# ice_preflight fails, so every layer runs against an empty, throwaway HOME.
export CARGO_HOME="${CARGO_HOME:-$HOME/.cargo}"
export RUSTUP_HOME="${RUSTUP_HOME:-$HOME/.rustup}"
GAUNTLET_HOME="$(mktemp -d)"
trap 'rm -rf "$GAUNTLET_HOME"' EXIT
export HOME="$GAUNTLET_HOME" AX_HOME_DIR="$GAUNTLET_HOME"
CRATES=(-p ax-policy -p ax-context -p ax-core -p ax-mcp -p ax-cli)

echo "== source state: $(git rev-parse HEAD) ($(git status --porcelain | wc -l | tr -d ' ') uncommitted files)"
echo "== toolchain: $(rustc --version); $(python3 --version)"

echo "== 1/6 tests"
cargo test "${CRATES[@]}" 2>&1 | tee "$CARGO_TARGET_DIR/gauntlet-tests.log" | grep -E "^test result|FAILED|panicked" || true
if grep -qE "test result: FAILED|^error" "$CARGO_TARGET_DIR/gauntlet-tests.log"; then echo "tests failed"; exit 1; fi
grep -c "^test result: ok" "$CARGO_TARGET_DIR/gauntlet-tests.log" | sed 's/^/suites ok: /'
awk '/^test result: ok/ {p+=$4} END {print "tests passed: " p}' "$CARGO_TARGET_DIR/gauntlet-tests.log"

echo "== 2/6 clippy"
cargo clippy "${CRATES[@]}" --all-targets -- -D warnings

echo "== 3/6 format (the Rust files this spec changed)"
rustfmt --edition 2021 --check \
  crates/ax-policy/src/matcher.rs crates/ax-policy/src/capture.rs \
  crates/ax-context/src/directory.rs crates/ax-cli/src/commands/prompt_hook.rs \
  crates/ax-cli/tests/prompt_hook_noise.rs crates/ax-core/src/stats_format.rs \
  crates/ax-mcp/src/server.rs crates/ax-mcp/src/policy_session.rs

echo "== 4/6 mutants"
python3 tools/preflight_quality_mutants.py

echo "== 5/6 release build"
cargo build --release -p ax-cli

echo "== 6/6 end-to-end, every MCP tool, as a Claude client and as a lean client"
python3 tools/ax_agent_e2e.py "$CARGO_TARGET_DIR/release/ax"

echo "GAUNTLET PASSED"
