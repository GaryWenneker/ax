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

find_ax_bin() {
  local candidates=(
    "${CARGO_TARGET_DIR:-}/${RUST_TARGET}/release/ax"
    "${ROOT}/target-dev/${RUST_TARGET}/release/ax"
    "${ROOT}/target/${RUST_TARGET}/release/ax"
  )
  local c
  for c in "${candidates[@]}"; do
    if [[ -f "${c}" ]]; then
      echo "${c}"
      return 0
    fi
  done
  return 1
}

AX_BIN="$(find_ax_bin)" || {
  echo "Binary not found. Run: cargo build --release -p ax-cli --target ${RUST_TARGET}" >&2
  exit 1
}

rm -rf "${STAGE}"
mkdir -p "${STAGE}"

case "${NAME}" in
  win32-*)
    cp "${AX_BIN}.exe" "${STAGE}/ax.exe" 2>/dev/null || cp "${AX_BIN}" "${STAGE}/ax.exe"
    (cd "${OUT_DIR}" && zip -q -r "ax-${NAME}.zip" "ax-${NAME}")
    echo "Created ${OUT_DIR}/ax-${NAME}.zip"
    ;;
  *)
    cp "${AX_BIN}" "${STAGE}/ax"
    chmod +x "${STAGE}/ax"
    (cd "${OUT_DIR}" && tar -czf "ax-${NAME}.tar.gz" "ax-${NAME}")
    echo "Created ${OUT_DIR}/ax-${NAME}.tar.gz"
    ;;
esac

rm -rf "${STAGE}"
