#!/usr/bin/env bash
# Gauntlet for links between rules, skills and memories (docs/specs/obsidian-links.md).
# Runs every executable layer; any failure, crash, or new warning in the changed code exits 1.
# Known baseline failure outside this change: ax-mcp new_tools_smoke::cycles_api_path_handlers_work
# ("project not initialized"); it is allowed by name and nothing else.
set -euo pipefail
cd "$(dirname "$0")/.."

NEW_FILES=(
  crates/ax-policy/src/links.rs
  crates/ax-mcp/src/links.rs
  crates/ax-web/src/global_policy.rs
  crates/ax-web/src/links_api.rs
  crates/ax-web/tests/dav_links.rs
  crates/ax-web/tests/links_api.rs
)
BASELINE_FAILURE="cycles_api_path_handlers_work"

echo "== source state"
git rev-parse HEAD
rustc --version
cargo --version
node --version

echo "== tests: ax-web, ax-policy, ax-memory, ax-mcp, ax-db, ax-global-db, ax-core"
set +e
out="$(cargo test -q -p ax-web -p ax-policy -p ax-memory -p ax-mcp -p ax-db -p ax-global-db -p ax-core --no-fail-fast 2>&1)"
status=$?
set -e
grep -E "^test result:" <<<"$out" | sort | uniq -c
grep -q "test result:" <<<"$out" || { echo "$out" | tail -40; echo "tests did not run"; exit 1; }
failed="$(grep -E "^    [a-z0-9_:]+$" <<<"$out" | sort -u || true)"
if [ "$status" -ne 0 ]; then
  if [ "$(echo "$failed" | xargs)" != "$BASELINE_FAILURE" ]; then
    echo "$out" | grep -E "panicked|FAILED|^error" | head -40
    echo "failing tests beyond the baseline: $failed"
    exit 1
  fi
  echo "only the baseline failure: $BASELINE_FAILURE"
fi

echo "== web-ui: node tests and tsc"
(cd crates/ax-web/web-ui && node --test src/*.test.ts src/lib/*.test.ts 2>&1 | grep -E "^ℹ (pass|fail)")
(cd crates/ax-web/web-ui && node --test src/*.test.ts src/lib/*.test.ts >/dev/null 2>&1)
(cd crates/ax-web/web-ui && npx tsc --noEmit -p .)
echo "web-ui: node tests green, tsc clean"

echo "== suite health: new tests 10x"
quiet() { local out; out="$("$@" 2>&1)" || { echo "$out" | tail -40; echo "run $i failed: $*"; exit 1; }; }
for i in $(seq 1 10); do
  quiet cargo test -q -p ax-web --test dav_links --test links_api
  quiet cargo test -q -p ax-policy --lib links::
  quiet cargo test -q -p ax-mcp --lib links::
  (cd crates/ax-web/web-ui && quiet node --test src/wikilinks.test.ts src/lib/policyBladeMotion.test.ts)
done
echo "new tests: 10/10 runs green"

echo "== clippy: zero warnings in the new and changed files"
# clippy::invalid_regex is a pre-existing deny-level error in ax-context that stops clippy before ax-web.
clippy_out="$(cargo clippy -p ax-web -p ax-mcp -p ax-policy --all-targets --message-format=short -- -A clippy::invalid_regex 2>&1)"
grep -q "Finished" <<<"$clippy_out" || { echo "$clippy_out" | tail -40; echo "clippy did not finish"; exit 1; }
if grep -E '^crates/(ax-policy/src/links\.rs|ax-mcp/src/links\.rs|ax-web/src/(global_policy|links_api)\.rs|ax-web/src/dav/|ax-web/tests/(dav_links|links_api)\.rs)' <<<"$clippy_out"; then
  echo "clippy: warnings in changed code"; exit 1
fi
echo "clippy: 0 warnings in changed code"

echo "== rustfmt on new files"
rustfmt --check --edition 2021 "${NEW_FILES[@]}"
echo "rustfmt: clean"

echo "== mutation"
scripts/links-mutants.sh

echo "gauntlet: all layers green"
