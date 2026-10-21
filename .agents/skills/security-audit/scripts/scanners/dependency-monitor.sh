#!/bin/bash
# SPDX-License-Identifier: MIT
# Matches .github/workflows/dependency-monitor.yml.
# A vulnerability fails the scan. Unmaintained and unsound warnings are printed
# and do not fail it, which is how cargo audit and the GitHub job behave.

set -euo pipefail

PROJECT_DIR="${1:-.}"
ERRORS=0

fail() {
    echo "ERROR: $*"
    ERRORS=$((ERRORS + 1))
}

if [[ -f "$PROJECT_DIR/Cargo.lock" ]]; then
    if ! cargo audit --version >/dev/null 2>&1; then
        fail "cargo-audit is not installed"
    else
        err=$(mktemp)
        set +e
        audit=$(cargo audit --json --file "$PROJECT_DIR/Cargo.lock" 2>"$err")
        code=$?
        set -e
        if ! printf '%s' "$audit" | jq -e '.vulnerabilities.count' >/dev/null 2>&1; then
            detail=$(tr '\n' ' ' <"$err")
            rm -f "$err"
            fail "cargo audit did not return a vulnerability count: ${detail}"
        else
            rm -f "$err"
            printf '%s' "$audit" | jq -r '
              .vulnerabilities.list[]?
              | "VULN \(.advisory.id) \(.advisory.package) \(.package.version) categories=\(.advisory.categories | join(",")) aliases=\(.advisory.aliases | join(","))"
            '
            count=$(printf '%s' "$audit" | jq -r '.vulnerabilities.count')
            echo "RustSec vulnerabilities: $count"
            if [[ "$count" != "0" || "$code" -ne 0 ]]; then
                fail "Cargo.lock has RustSec vulnerabilities"
            fi
        fi
    fi
fi

audit_npm() {
    local dir="$1"
    if [[ ! -f "$dir/package-lock.json" ]]; then
        return
    fi
    echo "npm audit $dir"
    set +e
    npm audit --package-lock-only --audit-level=moderate --prefix "$dir"
    local code=$?
    set -e
    if [[ "$code" -ne 0 ]]; then
        fail "npm audit failed in $dir"
    fi
}

audit_npm "$PROJECT_DIR/site"
audit_npm "$PROJECT_DIR/telemetry-worker"
audit_npm "$PROJECT_DIR/crates/ax-web/web-ui"

if [[ "$ERRORS" -gt 0 ]]; then
    echo "Dependency monitor scanner FAILED ($ERRORS)"
    exit 1
fi
echo "Dependency monitor scanner PASSED"
exit 0
