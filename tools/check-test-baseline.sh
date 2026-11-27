#!/usr/bin/env bash
# Compare a cargo test log with the tests that are known to fail.
#
#   tools/check-test-baseline.sh LOG BASELINE
#
# BASELINE lists one test name per line. Fails when the log has a failure that is
# not in BASELINE, and when a BASELINE test did not fail (it passed, or it no longer
# ran). The baseline cannot then hide a later regression in a test that was fixed:
# remove the test from BASELINE once it passes.
set -euo pipefail
LOG="$1"
BASELINE="$2"
FAILED="$(grep -E '^test .* FAILED$' "$LOG" | sed -E 's/^test (.*) \.\.\. FAILED$/\1/' | sort -u || true)"
[[ -n "$FAILED" ]] && printf '%s\n' "$FAILED"
UNEXPECTED="$(printf '%s\n' "$FAILED" | grep -v '^$' | grep -vxFf "$BASELINE" || true)"
[[ -z "$UNEXPECTED" ]] || { echo "NEW failures:"; echo "$UNEXPECTED"; exit 1; }
STALE="$(grep -v '^$' "$BASELINE" | grep -vxF -- "$FAILED" || true)"
if [[ -z "$FAILED" ]]; then STALE="$(grep -v '^$' "$BASELINE" || true)"; fi
[[ -z "$STALE" ]] || {
  echo "Baseline tests that did not fail (they pass now or no longer run); remove them from the baseline:"
  echo "$STALE"
  exit 1
}
echo "$(printf '%s\n' "$FAILED" | grep -c . || true) failed, all in the baseline"
