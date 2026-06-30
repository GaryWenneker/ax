#!/usr/bin/env bash
# Verify all six ax release archives exist before GitHub upload or getax deploy.
#
# Usage:
#   bash scripts/verify-release-assets.sh           # checks ./dist
#   bash scripts/verify-release-assets.sh /path/to/dist
set -euo pipefail

DIST="${1:-dist}"
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
if [[ "$DIST" != /* ]]; then
  DIST="${ROOT}/${DIST}"
fi

REQUIRED=(
  ax-win32-x64.zip
  ax-win32-arm64.zip
  ax-linux-x64.tar.gz
  ax-linux-arm64.tar.gz
  ax-darwin-x64.tar.gz
  ax-darwin-arm64.tar.gz
)

missing=()
for name in "${REQUIRED[@]}"; do
  if [[ ! -f "${DIST}/${name}" ]]; then
    missing+=("$name")
  fi
done

if ((${#missing[@]} > 0)); then
  echo "ax: incomplete release — missing ${#missing[@]}/6 required asset(s) in ${DIST}:" >&2
  for name in "${missing[@]}"; do
    echo "  - ${name}" >&2
  done
  echo >&2
  echo "Required for Windows, macOS (Intel + Apple Silicon), Linux, and WSL2 (linux-*)." >&2
  echo "Run Release CI for tag v* or build all targets locally before publish." >&2
  exit 1
fi

echo "OK: all 6 release assets present in ${DIST}"
