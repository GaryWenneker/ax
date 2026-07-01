#!/bin/sh
#
# ax standalone installer — macOS, Linux, and WSL2 (Linux binary).
# Windows: install.ps1
#
#   curl -fsSL https://getax.wenneker.io/install.sh | sh
#
# Upgrade: ax upgrade  (or re-run this script)
# Uninstall: curl -fsSL https://getax.wenneker.io/install.sh | sh -s -- --uninstall
set -eu

REPO="${AX_GITHUB_REPO:-GaryWenneker/ax}"
DOWNLOAD_BASE="${AX_DOWNLOAD_BASE:-https://getax.wenneker.io/releases}"
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

asset_url_ok() {
  tag="$1"
  url="https://github.com/$REPO/releases/download/$tag/ax-${target}.tar.gz"
  code="$(curl -fsSL -o /dev/null -w '%{http_code}' "$url" 2>/dev/null || true)"
  [ "$code" = "200" ]
}

resolve_version() {
  if [ -n "${AX_VERSION:-}" ]; then
    case "$AX_VERSION" in v*) printf '%s\n' "$AX_VERSION" ;; *) printf 'v%s\n' "$AX_VERSION" ;; esac
    return 0
  fi

  tmp="$(mktemp)"
  curl -fsSL "$DOWNLOAD_BASE/latest.txt" 2>/dev/null | tr -d '[:space:]\r' >>"$tmp" || true
  curl -fsSL "https://api.github.com/repos/$REPO/releases?per_page=30" 2>/dev/null \
    | sed -n 's/.*"tag_name": *"\(v[^"]*\)".*/\1/p' >>"$tmp" || true

  best=""
  while IFS= read -r cand; do
    [ -n "$cand" ] || continue
    case "$cand" in v*) tag="$cand" ;; *) tag="v$cand" ;; esac
    asset_url_ok "$tag" || continue
    num="${tag#v}"
    if [ -z "$best" ] || [ "$(printf '%s\n' "$num" "${best#v}" | sort -V | tail -n1)" = "$num" ]; then
      best="$tag"
    fi
  done <<EOF
$(sort -u "$tmp")
EOF
  rm -f "$tmp"

  if [ -n "$best" ]; then
    printf '%s\n' "$best"
    return 0
  fi
  return 1
}

version="$(resolve_version)" || {
  echo "ax: could not resolve a release with downloadable assets; set AX_VERSION." >&2
  exit 1
}

getax_url="$DOWNLOAD_BASE/$version/ax-${target}.tar.gz"
github_url="https://github.com/$REPO/releases/download/$version/ax-${target}.tar.gz"

echo "Installing ax $version ($target)..."
tmp="$(mktemp -d)"
trap 'rm -rf "$tmp"' EXIT

if curl -fsSL "$github_url" -o "$tmp/ax.tar.gz" 2>/dev/null; then
  :
elif curl -fsSL "$getax_url" -o "$tmp/ax.tar.gz" 2>/dev/null; then
  :
else
  echo "ax: download failed." >&2
  echo "  tried: https://github.com/${REPO}/releases/download/${version}/ax-${target}.tar.gz" >&2
  echo "  tried: https://getax.wenneker.io/releases/${version}/ax-${target}.tar.gz" >&2
  echo "  For a dev build: cargo install --git https://github.com/$REPO ax-cli" >&2
  exit 1
fi

# Stop running ax (web, mcp, daemon) so binaries can be replaced.
if command -v pkill >/dev/null 2>&1; then
  pkill -x ax 2>/dev/null || true
  sleep 0.5
fi

dest="$INSTALL_DIR/versions/$version"
rm -rf "$dest"
mkdir -p "$dest"
tar -xzf "$tmp/ax.tar.gz" -C "$dest" --strip-components=1

mkdir -p "$BIN_DIR"
ln -sf "$dest/ax" "$BIN_DIR/ax"
ln -sfn "$dest" "$INSTALL_DIR/current"

# Prepend ~/.local/bin on PATH for this shell (pipe installs do not reload profile).
case ":${PATH}:" in
  *":$BIN_DIR:"*) PATH="$(echo "$PATH" | tr ':' '\n' | grep -vx "$BIN_DIR" | tr '\n' ':' | sed 's/:$//')" ;;
esac
export PATH="$BIN_DIR:$PATH"

cargo_ax="$HOME/.cargo/bin/ax"
if [ -x "$cargo_ax" ] && [ "${AX_KEEP_CARGO_BIN:-}" != "1" ]; then
  old_ver="$("$cargo_ax" version 2>/dev/null || true)"
  cp -f "$dest/ax" "$cargo_ax"
  chmod +x "$cargo_ax"
  if [ -n "$old_ver" ]; then
    echo "Updated $cargo_ax (was: $old_ver)"
  fi
fi

installed_ver="$("$BIN_DIR/ax" version 2>/dev/null || true)"
echo "Installed ax $version to $dest"
if [ -n "$installed_ver" ]; then
  echo "Active: $installed_ver ($BIN_DIR/ax)"
else
  echo "Run: ax version"
fi
