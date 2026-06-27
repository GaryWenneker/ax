#!/usr/bin/env bash
# Reinstall ax CLI to ~/.cargo/bin after a local build.
set -euo pipefail
root="$(cd "$(dirname "$0")/.." && pwd)"
cd "$root"

echo "Stopping ax daemon (if running)..."
ax daemon stop 2>/dev/null || true

echo "Installing ax-cli to cargo bin..."
cargo install --path crates/ax-cli --force

bin="$(command -v ax)"
echo "$(ax --version)"
echo "Installed: $bin"
