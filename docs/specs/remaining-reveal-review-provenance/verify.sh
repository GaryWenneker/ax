#!/bin/sh
set -eu
cd "$(git rev-parse --show-toplevel)"
: "${AX_CHROME_EXECUTABLE:?Set AX_CHROME_EXECUTABLE to an installed headless Chrome}"
cargo_cmd="${CARGO:-cargo}"
rustfmt_cmd="${RUSTFMT:-rustfmt}"
export AX_SKIP_WEB_BUILD=1 AX_NO_UPDATE_CHECK=1
python3 docs/specs/remaining-reveal-review-provenance/mutations.py
"$rustfmt_cmd" --check --edition 2021 --config skip_children=true \
  crates/ax-policy/src/provenance.rs crates/ax-policy/src/pack.rs \
  crates/ax-policy/src/review.rs crates/ax-policy/src/lib.rs \
  crates/ax-cli/src/server_apply.rs crates/ax-cli/src/server_exchange.rs \
  crates/ax-share/src/sync.rs crates/ax-web/src/policy.rs
# Full affected library suites. The unrelated WebDAV baseline failure is
# reproduced and identified in EVIDENCE rather than hidden by a test skip.
"$cargo_cmd" test -p ax-policy -p ax-share -p ax-web --lib
"$cargo_cmd" test -p ax-cli --bin ax server_
"$cargo_cmd" clippy -p ax-policy -p ax-share -p ax-web -p ax-cli --all-targets -- -D warnings
(cd crates/ax-web/web-ui && npm run build)
node --experimental-strip-types --test crates/ax-web/web-ui/src/lib/navigation-retirement.test.ts crates/ax-web/web-ui/src/lib/themes.test.ts
"$cargo_cmd" build -p ax-cli
AX_VERIFY_NAV=1 AX_VERIFY_BINARY="${CARGO_TARGET_DIR:-target}/debug/ax" python3 docs/specs/remaining-reveal-review-provenance/http-smoke.py
(cd site && npm test && node --experimental-strip-types --experimental-test-coverage --test-coverage-include='**/src/scripts/scroll-reveal.js' --test-coverage-lines=100 --test-coverage-functions=100 --test-coverage-branches=90 --test tests/scroll-reveal.test.ts && node scripts/run-local-motion-check.mjs scripts/check-install-docs.mjs && node scripts/run-local-motion-check.mjs scripts/check-scroll-reveal.mjs && node scripts/run-local-motion-check.mjs)
git diff --check
