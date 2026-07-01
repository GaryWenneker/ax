#!/usr/bin/env bash
# Upload dist/ax-* release archives to getax.wenneker.io (public CDN for private GitHub repo).
#
# Usage (after Release CI artifacts downloaded to ./dist):
#   bash scripts/publish-getax-releases.sh v2.0.0
#
# Requires: netlify-cli, linked site (cd site && netlify link)
set -euo pipefail

TAG="${1:?usage: publish-getax-releases.sh v2.0.0}"
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
CARGO_VER="$(grep -E '^version = ' "${ROOT}/crates/ax-cli/Cargo.toml" | head -1 | sed -E 's/version = "(.*)"/\1/')"
EXPECTED_TAG="v${CARGO_VER}"
if [ "${TAG}" != "${EXPECTED_TAG}" ]; then
  echo "Tag ${TAG} does not match Cargo.toml version ${EXPECTED_TAG}. Bump Cargo.toml or pass ${EXPECTED_TAG}." >&2
  exit 1
fi
DIST="${ROOT}/dist"
SITE="${ROOT}/site"
RELEASE_DIR="${SITE}/public/releases/${TAG}"

if ! compgen -G "${DIST}/ax-*" > /dev/null; then
  echo "No archives in ${DIST}/ — download Release CI artifacts first." >&2
  exit 1
fi

bash "$(dirname "$0")/verify-release-assets.sh" "${DIST}"

mkdir -p "${RELEASE_DIR}"
cp "${DIST}"/ax-* "${RELEASE_DIR}/"
cp "${DIST}/SHA256SUMS" "${RELEASE_DIR}/" 2>/dev/null || true
echo "${TAG}" > "${SITE}/public/releases/latest.txt"
cp "${ROOT}/install.sh" "${ROOT}/install.ps1" "${SITE}/public/"

echo "Staged $(ls -1 "${RELEASE_DIR}" | wc -l | tr -d ' ') files under site/public/releases/${TAG}/"

cd "${SITE}"
npm ci
npm run build
netlify deploy --prod --dir=dist --message="Release ${TAG} binaries"

echo "Published ${TAG} to https://getax.wenneker.io/releases/${TAG}/"
