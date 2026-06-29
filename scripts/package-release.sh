#!/usr/bin/env bash
# Package ax CLI binary for GitHub Releases.
# Usage: package-release.sh <bundle-name> <rust-target>
# Example: package-release.sh win32-x64 x86_64-pc-windows-msvc
set -euo pipefail

NAME="${1:?bundle name}"
RUST_TARGET="${2:?rust target}"
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
OUT_DIR="${ROOT}/dist"
STAGE="${OUT_DIR}/ax-${NAME}"
BIN_DIR="${ROOT}/target/${RUST_TARGET}/release"

rm -rf "${STAGE}"
mkdir -p "${STAGE}"

case "${NAME}" in
  win32-*)
    cp "${BIN_DIR}/ax.exe" "${STAGE}/ax.exe"
    (cd "${OUT_DIR}" && zip -q -r "ax-${NAME}.zip" "ax-${NAME}")
    echo "Created ${OUT_DIR}/ax-${NAME}.zip"
    ;;
  *)
    cp "${BIN_DIR}/ax" "${STAGE}/ax"
    chmod +x "${STAGE}/ax"
    (cd "${OUT_DIR}" && tar -czf "ax-${NAME}.tar.gz" "ax-${NAME}")
    echo "Created ${OUT_DIR}/ax-${NAME}.tar.gz"
    ;;
esac

rm -rf "${STAGE}"
