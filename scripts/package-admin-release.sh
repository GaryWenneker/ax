#!/usr/bin/env bash
# Package the ax-admin binary for GitHub Releases.
# Usage: package-admin-release.sh <bundle> <path-to-ax-admin-binary>
#   bundle: linux-x64 | linux-arm64 | darwin-x64 | darwin-arm64 | win32-x64 | win32-arm64
# Creates dist/ax-admin-<bundle>.tar.gz (or .zip for win32) containing
# ax-admin-<bundle>/ax-admin[.exe], the layout install-admin.sh and
# install-admin.ps1 expect.
set -euo pipefail

BUNDLE="${1:?bundle, for example linux-x64}"
BIN="${2:?path to the ax-admin binary}"
case "${BUNDLE}" in
  linux-x64|linux-arm64|darwin-x64|darwin-arm64|win32-x64|win32-arm64) ;;
  *) echo "unsupported bundle '${BUNDLE}'" >&2; exit 1 ;;
esac
[[ -f "${BIN}" ]] || { echo "binary not found: ${BIN}" >&2; exit 1; }

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
OUT_DIR="${AX_DIST_DIR:-${ROOT}/dist}"
NAME="ax-admin-${BUNDLE}"
STAGE="${OUT_DIR}/${NAME}"

rm -rf "${STAGE}"
mkdir -p "${STAGE}"
case "${BUNDLE}" in
  win32-*)
    cp "${BIN}" "${STAGE}/ax-admin.exe"
    rm -f "${OUT_DIR}/${NAME}.zip"
    if command -v zip >/dev/null 2>&1; then
      (cd "${OUT_DIR}" && zip -q -r "${NAME}.zip" "${NAME}")
    else
      # Git Bash on a Windows runner has no zip.
      (cd "${OUT_DIR}" && powershell -NoProfile -Command "Compress-Archive -Path '${NAME}' -DestinationPath '${NAME}.zip' -Force")
    fi
    ARCHIVE="${NAME}.zip"
    ;;
  *)
    cp "${BIN}" "${STAGE}/ax-admin"
    chmod 0755 "${STAGE}/ax-admin"
    (cd "${OUT_DIR}" && tar -czf "${NAME}.tar.gz" "${NAME}")
    ARCHIVE="${NAME}.tar.gz"
    ;;
esac
rm -rf "${STAGE}"
[[ -s "${OUT_DIR}/${ARCHIVE}" ]] || { echo "archive was not created" >&2; exit 1; }
echo "Created ${OUT_DIR}/${ARCHIVE}"
