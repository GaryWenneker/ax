#!/usr/bin/env bash
set -euo pipefail
cd "$(dirname "$0")/.."
removal_base="${STE_REMOVAL_BASE:-7cd03a58f250988555fb8c2f88b92e1f6967a893}"
report_dir="${STE_REMOVAL_REPORT_DIR:-target-dev/ste-removal-evidence}"
export PYTHONDONTWRITEBYTECODE=1
if [[ -n "$(git status --porcelain --untracked-files=normal)" ]]; then
  printf 'Commit the implementation and verification files before running the gauntlet.\n' >&2
  exit 1
fi
mkdir -p "$report_dir"
git rev-parse HEAD > "$report_dir/source-sha.txt"
rustc --version > "$report_dir/toolchain.txt"
cargo --version >> "$report_dir/toolchain.txt"
python3 --version >> "$report_dir/toolchain.txt"
# Verify this reverses only the experiment, preserving the later OpenSSL fix.
for file in context_budget invoke lib server smart_output tools; do
  git show "b8d832b100678a8f2802fbdf400605a561f91a1b:crates/ax-mcp/src/$file.rs" | cmp - "crates/ax-mcp/src/$file.rs"
done
test ! -f crates/ax-mcp/src/text_style.rs
git diff --exit-code "$removal_base" -- Cargo.toml Cargo.lock 'crates/*/Cargo.toml'
python3 - <<'PY'
from pathlib import Path
for filename in ["README.md", "site/src/content/docs/guides/smart-output.md", "site/src/content/docs/reference/mcp-server.md"]:
    text = Path(filename).read_text(encoding="utf-8")
    if any(marker in text for marker in ("textStyle", "STE-inspired", "ste-inspired", "ASD-STE100")):
        raise SystemExit(f"current documentation still advertises withdrawn option: {filename}")
PY
cargo test -p ax-mcp -p ax-policy --locked --no-fail-fast -- --test-threads=1
cargo check --workspace --locked
cargo clippy --workspace --all-targets --locked -- -D warnings
rustfmt --edition 2021 --check crates/ax-mcp/tests/preflight_standard_contract.rs
python3 -m unittest discover -s tools -p 'test_mcp_removal_coverage.py' -v
cargo llvm-cov -p ax-mcp --locked --json --output-path "$report_dir/coverage.json" -- --test-threads=1
python3 tools/check_mcp_removal_coverage.py --report "$report_dir/coverage.json" --base "$removal_base" > "$report_dir/coverage-gate.json"
python3 tools/ste_removal_mutants.py > "$report_dir/mutants.json"
cargo test -p ax-mcp --locked --test preflight_standard_contract -- --test-threads=1
AX_MCP_FULL=1 cargo test -p ax-mcp --locked --test preflight_standard_contract -- --test-threads=1
(cd site && npm test)
(cd site && npm exec -- astro build)
git diff --check
printf 'STE removal gauntlet finished. Reports: %s\n' "$report_dir"
