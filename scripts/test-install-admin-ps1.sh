#!/usr/bin/env bash
# Exercises install-admin.ps1 with PowerShell 7 against a local HTTP server.
# Usage: test-install-admin-ps1.sh <path-to-an-ax-admin-binary-for-this-host>
# On macOS/Linux this checks download, checksum, extraction, upgrade, the
# tampered-archive refusal and uninstall. The user PATH change is Windows-only
# and is not covered here.
set -euo pipefail

BIN="$(cd "$(dirname "${1:?path to the ax-admin binary}")" && pwd)/$(basename "$1")"
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
command -v pwsh >/dev/null 2>&1 || { echo "pwsh is required" >&2; exit 1; }
WORK="$(mktemp -d "${TMPDIR:-/tmp}/ax-admin-ps1.XXXXXX")"
SERVER=""
trap '[[ -n "$SERVER" ]] && kill "$SERVER" 2>/dev/null; rm -rf "$WORK"' EXIT
TAG="v0.0.0-test"
REL="$WORK/rel/$TAG"
mkdir -p "$REL"
for arch in x64 arm64; do
  AX_DIST_DIR="$REL" bash "$ROOT/scripts/package-admin-release.sh" "win32-$arch" "$BIN" >/dev/null
done
(cd "$REL" && shasum -a 256 ax-admin-*.zip > SHA256SUMS)

PORT="$(python3 -c 'import socket; s=socket.socket(); s.bind(("127.0.0.1",0)); print(s.getsockname()[1])')"
(cd "$WORK/rel" && exec python3 -m http.server "$PORT" --bind 127.0.0.1 >/dev/null 2>&1) &
SERVER=$!
for _ in $(seq 1 50); do curl -fs "http://127.0.0.1:$PORT/$TAG/SHA256SUMS" >/dev/null 2>&1 && break; sleep 0.1; done

run() {
  LOCALAPPDATA="$WORK/local" AX_ADMIN_INSTALL_DIR="$WORK/inst" \
    AX_ADMIN_DOWNLOAD_BASE="http://127.0.0.1:$PORT" AX_GITHUB_API="http://127.0.0.1:$PORT/no-api" \
    pwsh -NoProfile -NonInteractive -File "$ROOT/install-admin.ps1" "$@"
}
fail() { echo "FAIL: $*" >&2; exit 1; }
EXE="$WORK/inst/bin/ax-admin.exe"

run -Version "$TAG" >"$WORK/install.log" 2>&1 || { cat "$WORK/install.log" >&2; fail "install"; }
[[ -f "$EXE" ]] || fail "ax-admin.exe was not installed"
cmp -s "$EXE" "$BIN" || fail "installed binary differs"
echo "ok install"

printf 'stale' > "$EXE"
run -Version "${TAG#v}" >/dev/null 2>&1 || fail "upgrade (version without the v prefix)"
cmp -s "$EXE" "$BIN" || fail "upgrade left the old binary"
echo "ok upgrade"

for zip in "$REL"/ax-admin-*.zip; do printf 'x' >> "$zip"; done
if run -Version "$TAG" >"$WORK/tamper.log" 2>&1; then fail "a tampered archive was installed"; fi
grep -q "checksum mismatch" "$WORK/tamper.log" || { cat "$WORK/tamper.log" >&2; fail "no checksum error"; }
cmp -s "$EXE" "$BIN" || fail "binary changed after a refused download"
echo "ok tampered archive refused"

# A download that does not run here leaves the working program in place.
for arch in x64 arm64; do
  rm -rf "$WORK/broken" && mkdir -p "$WORK/broken/ax-admin-win32-$arch"
  printf '#!/bin/sh\nexit 3\n' > "$WORK/broken/ax-admin-win32-$arch/ax-admin.exe"
  chmod 0755 "$WORK/broken/ax-admin-win32-$arch/ax-admin.exe"
  rm -f "$REL/ax-admin-win32-$arch.zip"
  (cd "$WORK/broken" && zip -q -r "$REL/ax-admin-win32-$arch.zip" "ax-admin-win32-$arch")
done
(cd "$REL" && shasum -a 256 ax-admin-*.zip > SHA256SUMS)
if run -Version "$TAG" >"$WORK/broken.log" 2>&1; then fail "a program that does not run was installed"; fi
grep -q "does not run on this computer" "$WORK/broken.log" || { cat "$WORK/broken.log" >&2; fail "no does-not-run error"; }
cmp -s "$EXE" "$BIN" || fail "the working program was replaced by a broken one"
[[ ! -e "$EXE.new" ]] || fail "a staged file was left behind"
echo "ok unusable download refused"

run -Uninstall >/dev/null 2>&1 || fail "uninstall"
[[ ! -e "$EXE" ]] || fail "ax-admin.exe still present after uninstall"
echo "ok uninstall"
echo "install-admin.ps1: all checks passed"
