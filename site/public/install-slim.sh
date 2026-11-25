#!/bin/sh
#
# ax headless (slim) installer — Linux x86_64 and arm64.
# For Vercel Sandbox, Azure Container Apps images, CI runners and other Linux hosts.
# The full desktop/web ax uses install.sh instead.
#
#   curl -fsSL https://getax.wenneker.io/install-slim.sh | sh
#   wget -qO- https://getax.wenneker.io/install-slim.sh | sh
#
# Options (or the matching environment variables):
#   --version vX.Y.Z   AX_VERSION     install this release instead of the newest one
#   --bin-dir DIR      AX_BIN_DIR     default /usr/local/bin as root, else ~/.local/bin
#   --force                           replace an existing full ax installation
#   --uninstall                       remove the slim binary from the bin directory
#
# Installs one static binary. No agent configuration, no ~/.ax install tree and no
# self-update: re-run this script to upgrade.
set -eu

REPO="${AX_GITHUB_REPO:-GaryWenneker/ax}"
DOWNLOAD_BASE="${AX_SLIM_DOWNLOAD_BASE:-https://github.com/$REPO/releases/download}"
API_BASE="${AX_GITHUB_API:-https://api.github.com}"
VERSION="${AX_VERSION:-}"
FORCE=0
UNINSTALL=0
if [ -n "${AX_BIN_DIR:-}" ]; then
  BIN_DIR="$AX_BIN_DIR"
elif [ "$(id -u)" = "0" ]; then
  BIN_DIR="/usr/local/bin"
else
  BIN_DIR="${HOME:-/tmp}/.local/bin"
fi

usage() {
  cat <<'USAGE'
ax headless (slim) installer for Linux x86_64 and arm64.

  curl -fsSL https://getax.wenneker.io/install-slim.sh | sh
  curl -fsSL https://getax.wenneker.io/install-slim.sh | sh -s -- --version v10.1.1

Options (or environment variables):
  --version vX.Y.Z   AX_VERSION   install this release instead of the newest one
  --bin-dir DIR      AX_BIN_DIR   default /usr/local/bin as root, else ~/.local/bin
  --force                         replace an existing full ax installation
  --uninstall                     remove the slim binary from the bin directory
USAGE
}

say() { printf 'ax: %s\n' "$*" >&2; }
die() { say "$*"; exit 1; }

while [ $# -gt 0 ]; do
  case "$1" in
    --version) [ $# -ge 2 ] || die "--version needs a value"; VERSION="$2"; shift 2 ;;
    --version=*) VERSION="${1#*=}"; shift ;;
    --bin-dir) [ $# -ge 2 ] || die "--bin-dir needs a value"; BIN_DIR="$2"; shift 2 ;;
    --bin-dir=*) BIN_DIR="${1#*=}"; shift ;;
    --force) FORCE=1; shift ;;
    --uninstall) UNINSTALL=1; shift ;;
    -h|--help) usage; exit 0 ;;
    *) die "unknown option '$1' (see --help)" ;;
  esac
done
case "$BIN_DIR" in /*) ;; *) BIN_DIR="$(pwd)/$BIN_DIR" ;; esac
TARGET_BIN="$BIN_DIR/ax"

# A full ax build has `ax server`; the headless build does not.
is_full_ax() { [ -x "$1" ] && "$1" server --help >/dev/null 2>&1; }

if [ "$UNINSTALL" = 1 ]; then
  if [ ! -e "$TARGET_BIN" ]; then say "nothing to remove at $TARGET_BIN"; exit 0; fi
  if is_full_ax "$TARGET_BIN" && [ "$FORCE" != 1 ]; then
    die "$TARGET_BIN is the full ax, not the headless build; use install.sh --uninstall or pass --force"
  fi
  rm -f "$TARGET_BIN"
  say "removed $TARGET_BIN"
  exit 0
fi

[ "$(uname -s)" = "Linux" ] || die "the headless build is Linux-only; on $(uname -s) use https://getax.wenneker.io/install.sh"
case "$(uname -m)" in
  x86_64|amd64) ARCH="x64" ;;
  aarch64|arm64) ARCH="arm64" ;;
  *) die "unsupported architecture '$(uname -m)' (x86_64 and arm64 only)" ;;
esac
ASSET="ax-slim-linux-$ARCH.tar.gz"

if command -v curl >/dev/null 2>&1; then
  fetch() {
    if [ -n "${2:-}" ]; then
      curl -fsSL --retry 3 -H "User-Agent: ax-install-slim" -H "Authorization: Bearer $2" "$1"
    else
      curl -fsSL --retry 3 -H "User-Agent: ax-install-slim" "$1"
    fi
  }
  fetch_to() { curl -fsSL --retry 3 -H "User-Agent: ax-install-slim" -o "$2" "$1"; }
elif command -v wget >/dev/null 2>&1; then
  fetch() {
    if [ -n "${2:-}" ]; then
      wget -qO- --header="User-Agent: ax-install-slim" --header="Authorization: Bearer $2" "$1"
    else
      wget -qO- --header="User-Agent: ax-install-slim" "$1"
    fi
  }
  fetch_to() { wget -qO "$2" --header="User-Agent: ax-install-slim" "$1"; }
else
  die "curl or wget is required"
fi
if command -v sha256sum >/dev/null 2>&1; then
  sha256() { sha256sum "$1" | cut -d' ' -f1; }
elif command -v shasum >/dev/null 2>&1; then
  sha256() { shasum -a 256 "$1" | cut -d' ' -f1; }
else
  die "sha256sum or shasum is required to verify the download"
fi
command -v tar >/dev/null 2>&1 || die "tar is required"

# GitHub API GET; a stale GITHUB_TOKEN/GH_TOKEN must not block a public download.
api_get() {
  token="${GITHUB_TOKEN:-${GH_TOKEN:-}}"
  if [ -n "$token" ] && fetch "$API_BASE/repos/$REPO/$1" "$token" 2>/dev/null; then return 0; fi
  fetch "$API_BASE/repos/$REPO/$1" 2>/dev/null
}

# Print "tag" for every non-draft, non-prerelease release in the JSON on stdin that
# lists $ASSET. GitHub puts tag_name, draft and prerelease before the assets array.
tags_with_asset() {
  # shellcheck disable=SC2020  # one newline per JSON separator, on purpose
  tr ',{}[]' '\n\n\n\n\n' | sed 's/^[[:space:]]*//' | awk -v asset="$ASSET" '
    /^"tag_name":/   { tag = $0; sub(/^"tag_name": *"/, "", tag); sub(/".*$/, "", tag); draft = 0; pre = 0; next }
    /^"draft": *true/      { draft = 1; next }
    /^"prerelease": *true/ { pre = 1; next }
    /^"name":/ {
      n = $0; sub(/^"name": *"/, "", n); sub(/".*$/, "", n)
      if (n == asset && tag != "" && !draft && !pre && !(tag in seen)) { seen[tag] = 1; print tag }
    }'
}

# Highest vMAJOR.MINOR.PATCH from stdin.
highest_tag() {
  awk '{ t = $0; v = t; sub(/^v/, "", v); n = split(v, p, ".");
         k = sprintf("%09d%09d%09d", p[1] + 0, p[2] + 0, p[3] + 0);
         if (k > best) { best = k; bt = t } } END { if (bt != "") print bt }'
}

resolve_version() {
  if [ -n "$VERSION" ]; then
    case "$VERSION" in v*) ;; *) VERSION="v$VERSION" ;; esac
    json="$(api_get "releases/tags/$VERSION" || true)"
    if [ -z "$json" ]; then
      # API unreachable: trust the pin; the download and checksum still verify it.
      printf '%s\n' "$VERSION"; return 0
    fi
    if printf '%s' "$json" | tags_with_asset | grep -qx "$VERSION"; then
      printf '%s\n' "$VERSION"; return 0
    fi
    say "release $VERSION has no $ASSET; installing the newest release that has one"
  fi
  json="$(api_get "releases/latest" || true)"
  tag="$(printf '%s' "$json" | tags_with_asset | head -n 1)"
  if [ -z "$tag" ]; then
    json="$(api_get "releases?per_page=30" || true)"
    tag="$(printf '%s' "$json" | tags_with_asset | highest_tag)"
  fi
  [ -n "$tag" ] || die "no published release of $REPO contains $ASSET yet (or the GitHub API is unreachable; pin one with --version)"
  printf '%s\n' "$tag"
}

if [ -e "$TARGET_BIN" ] && is_full_ax "$TARGET_BIN" && [ "$FORCE" != 1 ]; then
  die "$TARGET_BIN is the full ax; pass --force to replace it, or --bin-dir to install elsewhere"
fi

TAG="$(resolve_version)"
say "installing headless ax $TAG (linux-$ARCH) to $TARGET_BIN"

TMP="$(mktemp -d 2>/dev/null || mktemp -d -t ax-slim)"
trap 'rm -rf "$TMP"' EXIT INT TERM
fetch_to "$DOWNLOAD_BASE/$TAG/$ASSET" "$TMP/$ASSET" || die "download failed: $DOWNLOAD_BASE/$TAG/$ASSET"
fetch_to "$DOWNLOAD_BASE/$TAG/SHA256SUMS" "$TMP/SHA256SUMS" || die "download failed: $DOWNLOAD_BASE/$TAG/SHA256SUMS"
expected="$(awk -v a="$ASSET" '{ f = $2; sub(/^\*/, "", f) } f == a { print $1; exit }' "$TMP/SHA256SUMS")"
[ -n "$expected" ] || die "SHA256SUMS of $TAG has no entry for $ASSET"
actual="$(sha256 "$TMP/$ASSET")"
[ "$expected" = "$actual" ] || die "checksum mismatch for $ASSET (expected $expected, got $actual)"

tar -xzf "$TMP/$ASSET" -C "$TMP"
SRC="$TMP/ax-slim-linux-$ARCH/ax"
[ -f "$SRC" ] || die "archive $ASSET does not contain ax-slim-linux-$ARCH/ax"

mkdir -p "$BIN_DIR" 2>/dev/null || die "cannot create $BIN_DIR; run as root or pass --bin-dir"
[ -w "$BIN_DIR" ] || die "$BIN_DIR is not writable; run as root (sudo) or pass --bin-dir"
cp "$SRC" "$TARGET_BIN.new"
chmod 0755 "$TARGET_BIN.new"
mv -f "$TARGET_BIN.new" "$TARGET_BIN"

"$TARGET_BIN" version >/dev/null 2>&1 || die "installed $TARGET_BIN but it does not run on this host"
say "installed $("$TARGET_BIN" version 2>/dev/null | sed 's/\x1b\[[0-9;]*m//g')"
case ":${PATH:-}:" in
  *":$BIN_DIR:"*) ;;
  *) say "add $BIN_DIR to PATH: export PATH=\"$BIN_DIR:\$PATH\"" ;;
esac
