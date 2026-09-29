#!/usr/bin/env bash
# Gauntlet for the WebDAV vault (docs/specs/obsidian-webdav-vault.md).
# Runs every executable layer; any failure, crash, or new warning in the dav code exits 1.
set -euo pipefail
cd "$(dirname "$0")/.."

NEW_FILES=(
  crates/ax-web/src/dav/mod.rs
  crates/ax-web/src/dav/fs.rs
  crates/ax-web/src/dav/pages.rs
  crates/ax-web/tests/dav_vault.rs
  crates/ax-web/tests/dav_share.rs
)

echo "== source state"
git rev-parse HEAD
rustc --version
cargo --version

echo "== tests: ax-web, ax-db, ax-policy, ax-memory"
cargo test -q -p ax-web -p ax-db -p ax-policy -p ax-memory --no-fail-fast

echo "== suite health: dav tests 10x"
for i in $(seq 1 10); do
  cargo test -q -p ax-web --test dav_vault --test dav_share >/dev/null
done
echo "dav tests: 10/10 runs green"

echo "== clippy: zero warnings in dav code"
# clippy::invalid_regex is a pre-existing deny-level error in ax-context that stops clippy before ax-web.
clippy_out="$(cargo clippy -p ax-web --all-targets --message-format=short -- -A clippy::invalid_regex 2>&1)"
grep -q "Finished" <<<"$clippy_out" || { echo "$clippy_out"; echo "clippy did not finish"; exit 1; }
if grep -E '^crates/ax-web/(src/dav/|tests/dav_)' <<<"$clippy_out"; then
  echo "clippy: warnings in dav code"; exit 1
fi
echo "clippy: 0 warnings in dav code"

echo "== rustfmt on new files"
rustfmt --check --edition 2021 "${NEW_FILES[@]}"
echo "rustfmt: clean"

echo "== mutation"
scripts/dav-mutants.sh

echo "gauntlet: all layers green"
