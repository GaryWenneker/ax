#!/usr/bin/env bash
# Compatibility entry point: dispatch the guarded main release, never a local production upload.
set -euo pipefail
TAG="${1:?usage: publish-getax-releases.sh v8.0.0}"
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
cd "$ROOT"
[ "$(git branch --show-current)" = main ] || { echo "Releases must run from main." >&2; exit 1; }
[ -z "$(git status --porcelain)" ] || { echo "Releases require a clean checkout." >&2; exit 1; }
git fetch origin main
[ "$(git rev-parse HEAD)" = "$(git rev-parse origin/main)" ] || { echo "Local main must equal origin/main." >&2; exit 1; }
gh workflow run release.yml --repo GaryWenneker/ax --ref main -f "tag=$TAG"
