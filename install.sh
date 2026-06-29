#!/bin/sh
#
# ax standalone installer (GitHub Releases).
#
#   curl -fsSL https://raw.githubusercontent.com/GaryWenneker/ax/main/install.sh | sh
#
# Upgrade: ax upgrade  (or re-run this script)
# Uninstall: curl -fsSL .../install.sh | sh -s -- --uninstall
set -eu

REPO="${AX_GITHUB_REPO:-GaryWenneker/ax}"
INSTALL_DIR="${AX_INSTALL_DIR:-$HOME/.ax}"
BIN_DIR="${AX_BIN_DIR:-$HOME/.local/bin}"

if [ "${1:-}" = "--uninstall" ]; then
  rm -f "$BIN_DIR/ax"
  rm -rf "$INSTALL_DIR"
  echo "ax uninstalled (removed $INSTALL_DIR and $BIN_DIR/ax)."
  exit 0
fi

os="$(uname -s)"
arch="$(uname -m)"
case "$os" in
  Darwin) os="darwin" ;;
  Linux)  os="linux" ;;
  *) echo "ax: unsupported OS '$os'." >&2; exit 1 ;;
esac
case "$arch" in
  arm64|aarch64) arch="arm64" ;;
  x86_64|amd64)  arch="x64" ;;
  *) echo "ax: unsupported architecture '$arch'." >&2; exit 1 ;;
esac
target="${os}-${arch}"

version="${AX_VERSION:-}"
if [ -z "$version" ]; then
  version="$(curl -fsSLI -o /dev/null -w '%{url_effective}' "https://github.com/$REPO/releases/latest" \
    | sed -n 's#.*/releases/tag/##p')"
fi
if [ -z "$version" ]; then
  version="$(curl -fsSL "https://api.github.com/repos/$REPO/releases/latest" \
    | sed -n 's/.*"tag_name": *"\([^"]*\)".*/\1/p' | head -n1)"
fi
[ -n "$version" ] || { echo "ax: could not resolve latest version; set AX_VERSION." >&2; exit 1; }
case "$version" in v*) ;; *) version="v$version" ;; esac

url="https://github.com/$REPO/releases/download/$version/ax-${target}.tar.gz"
echo "Installing ax $version ($target)..."
tmp="$(mktemp -d)"
trap 'rm -rf "$tmp"' EXIT
curl -fsSL "$url" -o "$tmp/ax.tar.gz" || { echo "ax: download failed: $url" >&2; exit 1; }

dest="$INSTALL_DIR/versions/$version"
rm -rf "$dest"
mkdir -p "$dest"
tar -xzf "$tmp/ax.tar.gz" -C "$dest" --strip-components=1

mkdir -p "$BIN_DIR"
ln -sf "$dest/ax" "$BIN_DIR/ax"
ln -sfn "$dest" "$INSTALL_DIR/current"

echo "Installed ax $version to $dest"
echo "Run: ax version"
