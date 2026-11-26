#!/usr/bin/env bash
# Tests install-admin.sh against a local release directory (no network).
# Usage: test-install-admin.sh <path-to-ax-admin-binary>
# Covers: install, checksum verification, upgrade, a tampered archive (must be
# refused), uninstall, and that an `ax` binary next to it is never touched.
set -euo pipefail

BIN="$(cd "$(dirname "${1:?path to the ax-admin binary}")" && pwd)/$(basename "$1")"
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
WORK="$(mktemp -d "${TMPDIR:-/tmp}/ax-admin-install.XXXXXX")"
trap 'rm -rf "$WORK"' EXIT

case "$(uname -s)" in Linux) OS=linux ;; Darwin) OS=darwin ;; *) echo "unsupported host" >&2; exit 1 ;; esac
case "$(uname -m)" in x86_64|amd64) ARCH=x64 ;; aarch64|arm64) ARCH=arm64 ;; *) echo "unsupported host" >&2; exit 1 ;; esac
ASSET="ax-admin-$OS-$ARCH.tar.gz"
TAG="v0.0.0-test"
RELEASES="$WORK/releases"
mkdir -p "$RELEASES/$TAG" "$WORK/bin"
sha() { if command -v sha256sum >/dev/null 2>&1; then sha256sum "$1" | cut -d' ' -f1; else shasum -a 256 "$1" | cut -d' ' -f1; fi; }

AX_DIST_DIR="$RELEASES/$TAG" bash "$ROOT/scripts/package-admin-release.sh" "$OS-$ARCH" "$BIN" >/dev/null
(cd "$RELEASES/$TAG" && printf '%s  %s\n' "$(sha "$ASSET")" "$ASSET" > SHA256SUMS)

# An `ax` that must survive every step untouched.
printf '#!/bin/sh\necho "the real ax"\n' > "$WORK/bin/ax"
chmod 0755 "$WORK/bin/ax"
AX_SUM="$(sha "$WORK/bin/ax")"

run() { # the GitHub API is unreachable on purpose: the pinned version is used as is
  AX_ADMIN_DOWNLOAD_BASE="file://$RELEASES" AX_GITHUB_API="file://$WORK/no-api" \
    GITHUB_TOKEN= GH_TOKEN= sh "$ROOT/install-admin.sh" --bin-dir "$WORK/bin" "$@"
}
fail() { echo "FAIL: $*" >&2; exit 1; }
ax_untouched() { [[ "$(sha "$WORK/bin/ax")" == "$AX_SUM" ]] || fail "ax was modified ($1)"; }

run --version "$TAG" 2>"$WORK/install.log" || { cat "$WORK/install.log" >&2; fail "install"; }
[[ -x "$WORK/bin/ax-admin" ]] || fail "ax-admin was not installed"
[[ "$("$WORK/bin/ax-admin" version)" == "$("$BIN" version)" ]] || fail "installed binary differs"
ax_untouched install
echo "ok install"

# Upgrade: replace a stale binary.
printf 'stale' > "$WORK/bin/ax-admin"
run --version "$TAG" 2>/dev/null || fail "upgrade"
[[ "$(sha "$WORK/bin/ax-admin")" == "$(sha "$BIN")" ]] || fail "upgrade left the old binary"
ax_untouched upgrade
echo "ok upgrade"

# Negative control: a tampered archive must be refused and nothing replaced.
cp "$RELEASES/$TAG/$ASSET" "$WORK/good.tar.gz"
printf 'x' >> "$RELEASES/$TAG/$ASSET"
BEFORE="$(sha "$WORK/bin/ax-admin")"
if run --version "$TAG" 2>"$WORK/tamper.log"; then fail "a tampered archive was installed"; fi
grep -q "checksum mismatch" "$WORK/tamper.log" || { cat "$WORK/tamper.log" >&2; fail "no checksum error"; }
[[ "$(sha "$WORK/bin/ax-admin")" == "$BEFORE" ]] || fail "binary changed after a refused download"
cp "$WORK/good.tar.gz" "$RELEASES/$TAG/$ASSET"
echo "ok tampered archive refused"

# A release without the asset in SHA256SUMS is refused.
: > "$RELEASES/$TAG/SHA256SUMS"
if run --version "$TAG" 2>"$WORK/nosum.log"; then fail "installed without a checksum entry"; fi
grep -q "has no entry" "$WORK/nosum.log" || fail "no missing-checksum error"
echo "ok missing checksum refused"

run --uninstall 2>/dev/null || fail "uninstall"
[[ ! -e "$WORK/bin/ax-admin" ]] || fail "ax-admin still present after uninstall"
ax_untouched uninstall
run --uninstall 2>/dev/null || fail "second uninstall should be a no-op"
if run --bogus 2>/dev/null; then fail "unknown option accepted"; fi
echo "ok uninstall"
echo "install-admin.sh: all checks passed"
