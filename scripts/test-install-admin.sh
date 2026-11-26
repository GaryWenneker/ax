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

# A download that does not run here leaves the working binary in place.
BROKEN="$WORK/broken"
mkdir -p "$BROKEN/ax-admin-$OS-$ARCH"
printf '#!/bin/sh\nexit 3\n' > "$BROKEN/ax-admin-$OS-$ARCH/ax-admin"
chmod 0755 "$BROKEN/ax-admin-$OS-$ARCH/ax-admin"
(cd "$BROKEN" && tar -czf "$RELEASES/$TAG/$ASSET" "ax-admin-$OS-$ARCH")
(cd "$RELEASES/$TAG" && printf '%s  %s\n' "$(sha "$ASSET")" "$ASSET" > SHA256SUMS)
if run --version "$TAG" 2>"$WORK/broken.log"; then fail "a binary that does not run was installed"; fi
grep -q "does not run on this host" "$WORK/broken.log" || { cat "$WORK/broken.log" >&2; fail "no does-not-run error"; }
[[ "$(sha "$WORK/bin/ax-admin")" == "$BEFORE" ]] || fail "working binary was replaced by a broken one"
[[ ! -e "$WORK/bin/ax-admin.new" ]] || fail "a staged file was left behind"
cp "$WORK/good.tar.gz" "$RELEASES/$TAG/$ASSET"
(cd "$RELEASES/$TAG" && printf '%s  %s\n' "$(sha "$ASSET")" "$ASSET" > SHA256SUMS)
echo "ok unusable download refused"

# Without --version the newest release that has the asset is found through the
# GitHub API (a local stand-in here): drafts, prereleases and releases without
# the asset are skipped.
release() { # tag draft prerelease asset
  printf '{"tag_name": "%s", "draft": %s, "prerelease": %s, "assets": [{"name": "%s", "size": 1}]}' "$1" "$2" "$3" "$4"
}
API_DIR="$WORK/api"
mkdir -p "$API_DIR"
release v9.9.9 false false ax-linux-x64.tar.gz > "$API_DIR/latest.json"
release v9.9.9 false false ax-linux-x64.tar.gz > "$API_DIR/tag-v9.9.9.json"
printf '[%s,%s,%s,%s,%s]' \
  "$(release v9.9.9 false false ax-linux-x64.tar.gz)" \
  "$(release v9.9.8 true false "$ASSET")" \
  "$(release v9.9.7 false true "$ASSET")" \
  "$(release v0.0.2 false false "$ASSET")" \
  "$(release "$TAG" false false "$ASSET")" > "$API_DIR/list.json"
API_PORT="$(python3 -c 'import socket; s=socket.socket(); s.bind(("127.0.0.1",0)); print(s.getsockname()[1])')"
python3 - "$API_DIR" "$API_PORT" <<'PY' &
import http.server, sys, pathlib
root, port = pathlib.Path(sys.argv[1]), int(sys.argv[2])
routes = {
    "/repos/GaryWenneker/ax/releases/latest": "latest.json",
    "/repos/GaryWenneker/ax/releases": "list.json",
    "/repos/GaryWenneker/ax/releases/tags/v9.9.9": "tag-v9.9.9.json",
}
class Api(http.server.BaseHTTPRequestHandler):
    def do_GET(self):
        name = routes.get(self.path.split("?")[0])
        if name is None:
            self.send_error(404)
            return
        body = (root / name).read_bytes()
        self.send_response(200)
        self.send_header("Content-Type", "application/json")
        self.send_header("Content-Length", str(len(body)))
        self.end_headers()
        self.wfile.write(body)
    def log_message(self, *args):
        pass
http.server.HTTPServer(("127.0.0.1", port), Api).serve_forever()
PY
API_PID=$!
trap 'kill "$API_PID" 2>/dev/null; rm -rf "$WORK"' EXIT
for _ in $(seq 1 50); do curl -fs "http://127.0.0.1:$API_PORT/repos/GaryWenneker/ax/releases/latest" >/dev/null 2>&1 && break; sleep 0.1; done
with_api() {
  AX_ADMIN_DOWNLOAD_BASE="file://$RELEASES" AX_GITHUB_API="http://127.0.0.1:$API_PORT" GITHUB_TOKEN= GH_TOKEN= \
    sh "$ROOT/install-admin.sh" --bin-dir "$WORK/bin" "$@"
}
mkdir -p "$RELEASES/v0.0.2"
cp "$RELEASES/$TAG/$ASSET" "$RELEASES/$TAG/SHA256SUMS" "$RELEASES/v0.0.2/"
rm -f "$WORK/bin/ax-admin"
with_api 2>"$WORK/latest.log" || { cat "$WORK/latest.log" >&2; fail "install of the newest release"; }
grep -q "installing ax-admin v0.0.2 " "$WORK/latest.log" || { cat "$WORK/latest.log" >&2; fail "did not choose v0.0.2"; }
# A pinned release that exists but has no ax-admin asset is an error, not a silent switch.
if with_api --version v9.9.9 2>"$WORK/pin.log"; then fail "a release without the asset was accepted"; fi
grep -q "has no $ASSET" "$WORK/pin.log" || { cat "$WORK/pin.log" >&2; fail "no missing-asset error"; }
BEFORE="$(sha "$WORK/bin/ax-admin")"
echo "ok newest release resolved, pinned release without the asset refused"

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
