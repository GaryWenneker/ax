#!/usr/bin/env bash
# Scores the coding-memory fixture and writes benchmarks/memory/results/<commit>.json.
# Real LongMemEval-S and LoCoMo files are used only when AX_LONGMEMEVAL_PATH or
# AX_LOCOMO_PATH is set. This script does not download them.
set -euo pipefail

root="$(cd "$(dirname "$0")/.." && pwd)"
cd "$root"

commit="$(git rev-parse HEAD)"
dirty=false
if [ -n "$(git status --porcelain)" ]; then
  dirty=true
fi
mkdir -p benchmarks/memory/results
out="$root/benchmarks/memory/results/${commit}.json"

AX_MEMORY_BENCH_COMMIT="$commit" \
AX_MEMORY_BENCH_DIRTY="$dirty" \
AX_MEMORY_BENCH_OUT="$out" \
  cargo test -p ax-memory --test memory_bench --offline -- --ignored --test-threads=1 write_coding_baseline

echo "wrote $out"
