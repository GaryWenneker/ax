#!/usr/bin/env bash
set -euo pipefail
cd "$(dirname "$0")/../.."
report_dir=${1:?Provide a fresh report directory}
if [ -e "$report_dir" ]; then
    echo "Report directory must be new: $report_dir" >&2
    exit 2
fi
mkdir -p "$report_dir"
python - "$report_dir/source-manifest.json" <<'PYTHON'
import hashlib,json,subprocess,sys
from pathlib import Path
files=subprocess.check_output(["git","ls-files","*.rs"],text=True).splitlines()
Path(sys.argv[1]).write_text(json.dumps({f:hashlib.sha256(Path(f).read_bytes()).hexdigest() for f in files},indent=2)+"\n")
PYTHON
export CARGO_INCREMENTAL=0
export CARGO_BUILD_JOBS=${CARGO_BUILD_JOBS:-2}
export ASTRO_TELEMETRY_DISABLED=1
./scripts/check-home-dir.sh
cargo build --workspace --locked
cargo test --workspace --locked --no-fail-fast
cargo clippy --workspace --all-targets --locked -- -D warnings
npm --prefix site test
npm --prefix site run build
cargo llvm-cov --workspace --locked --lcov --output-path "$report_dir/coverage.lcov"
python scripts/context-safety/changed_coverage.py "$report_dir/coverage.lcov" --manifest "$report_dir/source-manifest.json" --output "$report_dir/changed-coverage.json"
cargo mutants --no-config --file crates/ax-mcp/src/request_context.rs --file crates/ax-mcp/src/context_budget.rs --file crates/ax-mcp/src/policy_session.rs --re 'replace validate .*with Ok|request_context.rs.*replace !=|replace projection_tokens .*with 0|replace <= with > in finish|replace != with == in PolicySessions::observe_context' --timeout 90 --build-timeout 600 --output "$report_dir/mutations" -- --test project_context_isolation --test context_budget_contract
