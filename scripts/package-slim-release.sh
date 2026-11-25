#!/usr/bin/env bash
# Package the headless (slim) Linux binary for GitHub Releases.
# Usage: package-slim-release.sh <x64|arm64> <path-to-ax-binary>
# Creates dist/ax-slim-linux-<arch>.tar.gz containing ax-slim-linux-<arch>/ax,
# the layout install-slim.sh expects.
set -euo pipefail

ARCH="${1:?arch (x64 or arm64)}"
BIN="${2:?path to the slim ax binary}"
case "${ARCH}" in x64|arm64) ;; *) echo "unsupported arch '${ARCH}'" >&2; exit 1 ;; esac
[[ -f "${BIN}" ]] || { echo "binary not found: ${BIN}" >&2; exit 1; }

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
OUT_DIR="${ROOT}/dist"
NAME="ax-slim-linux-${ARCH}"
STAGE="${OUT_DIR}/${NAME}"

rm -rf "${STAGE}"
mkdir -p "${STAGE}"
cp "${BIN}" "${STAGE}/ax"
chmod 0755 "${STAGE}/ax"
(cd "${OUT_DIR}" && tar -czf "${NAME}.tar.gz" "${NAME}")
rm -rf "${STAGE}"
echo "Created ${OUT_DIR}/${NAME}.tar.gz"
