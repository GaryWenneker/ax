#!/bin/sh
set -eu
cd "$(git rev-parse --show-toplevel)"
: "${AX_CHROME_EXECUTABLE:?Set AX_CHROME_EXECUTABLE}"
export AX_NO_UPDATE_CHECK=1 AX_SKIP_WEB_BUILD=1 AX_REVIEW_SCREENSHOTS=1
cargo_cmd="${CARGO:-cargo}"
rustfmt_cmd="${RUSTFMT:-rustfmt}"
python3 docs/specs/review-peer-compatibility/mutations.py
"$rustfmt_cmd" --check --edition 2021 --config skip_children=true crates/ax-cli/src/server_http.rs crates/ax-cli/src/server_replica.rs crates/ax-web/src/policy.rs crates/ax-web/src/replication_setup.rs
"$cargo_cmd" test -p ax-web --lib
"$cargo_cmd" test -p ax-cli --bin ax server_
"$cargo_cmd" clippy -p ax-web -p ax-cli --all-targets -- -D warnings
(cd crates/ax-web/web-ui && npm run build)
"$cargo_cmd" build -p ax-cli
AX_VERIFY_NAV=1 AX_VERIFY_BINARY="${CARGO_TARGET_DIR:-target}/debug/ax" python3 docs/specs/review-peer-compatibility/http-smoke.py
(cd site && npm test)
git diff --check
