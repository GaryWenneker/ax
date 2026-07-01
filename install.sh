#!/bin/sh
#
# ax standalone installer — macOS, Linux, and WSL2 (Linux binary).
# Windows: install.ps1
#
#   curl -fsSL https://getax.wenneker.io/install.sh | sh
#
# Always installs the latest published release (highest semver with assets).
# Stops running ax processes and replaces any previous install under ~/.ax.
# Pin a version: AX_VERSION=v2.0.12 curl -fsSL ... | sh
# Upgrade: ax upgrade  (or re-run this script)
# Uninstall: curl -fsSL https://getax.wenneker.io/install.sh | sh -s -- --uninstall
set -eu

REPO="${AX_GITHUB_REPO:-GaryWenneker/ax}"
DOWNLOAD_BASE="${AX_DOWNLOAD_BASE:-https://getax.wenneker.io/releases}"
INSTALL_DIR="${AX_INSTALL_DIR:-$HOME/.ax}"
BIN_DIR="${AX_BIN_DIR:-$HOME/.local/bin}"

stop_ax_processes() {
  if command -v pkill >/dev/null 2>&1; then
    pkill -9 -x ax 2>/dev/null || true
    sleep 1
    pkill -9 -x ax 2>/dev/null || true
  fi
}

clean_install_tree() {
  stop_ax_processes
  rm -rf "$INSTALL_DIR/current" "$INSTALL_DIR/versions"
  find "$INSTALL_DIR" -maxdepth 1 -type d -name 'upgrade-staging-*' -exec rm -rf {} + 2>/dev/null || true
}

if [ "${1:-}" = "--uninstall" ]; then
  stop_ax_processes
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
  for url in \
    "https://github.com/$REPO/releases/download/$tag/ax-${target}.tar.gz" \
    "$DOWNLOAD_BASE/$tag/ax-${target}.tar.gz"
  do
    code="$(curl -fsSL -o /dev/null -w '%{http_code}' "$url" 2>/dev/null || true)"
    [ "$code" = "200" ] && return 0
  done
  return 1
}

resolve_version() {
  if [ -n "${AX_VERSION:-}" ]; then
    case "$AX_VERSION" in v*) tag="$AX_VERSION" ;; *) tag="v$AX_VERSION" ;; esac
    asset_url_ok "$tag" || {
      echo "ax: AX_VERSION $tag has no downloadable ax-${target}.tar.gz" >&2
      return 1
    }
    printf '%s\n' "$tag"
    return 0
  fi

  tmp="$(mktemp)"
  # GitHub first — getax latest.txt is a site pointer and may lag behind GitHub.
  curl -fsSL "https://api.github.com/repos/$REPO/releases/latest" 2>/dev/null \
    | sed -n 's/.*"tag_name": *"\(v[^"]*\)".*/\1/p' >>"$tmp" || true
  curl -fsSL "https://api.github.com/repos/$REPO/releases?per_page=30" 2>/dev/null \
    | sed -n 's/.*"tag_name": *"\(v[^"]*\)".*/\1/p' >>"$tmp" || true
  curl -fsSL "$DOWNLOAD_BASE/latest.txt" 2>/dev/null | tr -d '[:space:]\r' >>"$tmp" || true

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

ax_install_targets() {
  printf '%s\n' "$INSTALL_DIR/current/ax" "$BIN_DIR/ax"
  if [ "${AX_KEEP_CARGO_BIN:-}" != "1" ]; then
    printf '%s\n' "$HOME/.cargo/bin/ax"
  fi
}

sync_local_ax_instances() {
  src="$1"
  stop_ax_processes
  ax_install_targets | while IFS= read -r dest; do
    [ -n "$dest" ] || continue
    [ "$dest" = "$src" ] && continue
    mkdir -p "$(dirname "$dest")"
    rm -f "$dest"
    cp -f "$src" "$dest"
    chmod +x "$dest" 2>/dev/null || true
  done
}

confirm_ax_install() {
  expected="${1#v}"
  failed=0
  while IFS= read -r path; do
    [ -n "$path" ] || continue
    if [ ! -x "$path" ]; then
      echo "ax: install incomplete — missing $path" >&2
      failed=1
      continue
    fi
    ver="$("$path" version 2>/dev/null || true)"
    case "$ver" in
      *"$expected"*) ;;
      *)
        echo "ax: $path reports '$ver', expected $expected — stop ax MCP/web and re-run install" >&2
        failed=1
        ;;
    esac
  done <<EOF
$(ax_install_targets)
EOF
  [ "$failed" -eq 0 ] || return 1
}

update_session_path() {
  case ":${PATH}:" in
    *":$BIN_DIR:"*) PATH="$(echo "$PATH" | tr ':' '\n' | grep -vx "$BIN_DIR" | tr '\n' ':' | sed 's/:$//')" ;;
  esac
  export PATH="$BIN_DIR:$PATH"
}

# Kill stale ax and remove previous install before download.
clean_install_tree

version="$(resolve_version)" || {
  echo "ax: could not resolve a release with downloadable assets; set AX_VERSION." >&2
  exit 1
}

getax_url="$DOWNLOAD_BASE/$version/ax-${target}.tar.gz"
github_url="https://github.com/$REPO/releases/download/$version/ax-${target}.tar.gz"

echo "Installing ax $version ($target) — latest available..."
tmp="$(mktemp -d)"
trap 'rm -rf "$tmp"' EXIT

if curl -fsSL "$github_url" -o "$tmp/ax.tar.gz" 2>/dev/null; then
  echo "  downloaded from: $github_url"
elif curl -fsSL "$getax_url" -o "$tmp/ax.tar.gz" 2>/dev/null; then
  echo "  downloaded from: $getax_url"
else
  echo "ax: download failed." >&2
  echo "  tried: $github_url" >&2
  echo "  tried: $getax_url" >&2
  echo "  For a dev build: cargo install --git https://github.com/$REPO ax-cli" >&2
  exit 1
fi

stop_ax_processes

dest="$INSTALL_DIR/current"
mkdir -p "$dest"
tar -xzf "$tmp/ax.tar.gz" -C "$dest" --strip-components=1
chmod +x "$dest/ax" 2>/dev/null || true

mkdir -p "$BIN_DIR"
sync_local_ax_instances "$dest/ax"
update_session_path
confirm_ax_install "$version" || exit 1

installed_ver="$("$BIN_DIR/ax" version 2>/dev/null || true)"
echo "Installed to $dest (replaced previous install)"
if [ -n "$installed_ver" ]; then
  echo "Active: $installed_ver ($BIN_DIR/ax)"
else
  echo "Run: ax version"
fi
echo "Synced local instances:"
ax_install_targets | while IFS= read -r path; do
  [ -n "$path" ] && echo "  $path"
done
