#!/bin/sh
#
# ax standalone installer — macOS, Linux, and WSL2 (Linux binary).
# Windows: install.ps1
#
#   curl -fsSL https://getax.wenneker.io/install.sh | sh
#
# Always installs the latest published release (highest semver with assets).
# Self-heals before download: stops ax, clears stuck upgrade-status/staging,
# drops invalid AX_VERSION pins, and retries GitHub without a bad token.
# Pin a version: AX_VERSION=v2.0.14 curl -fsSL ... | sh
# Upgrade: ax upgrade  (or re-run this script)
# Uninstall: curl -fsSL https://getax.wenneker.io/install.sh | sh -s -- --uninstall
#   Removes the program and the PATH lines; keeps ~/.ax data (database, policy,
#   memories, sign-ins). Add --purge (and --yes when not interactive) to delete
#   that data as well.
set -eu

REPO="${AX_GITHUB_REPO:-GaryWenneker/ax}"
DOWNLOAD_BASE="${AX_DOWNLOAD_BASE:-https://getax.wenneker.io/releases}"
INSTALL_DIR="${AX_INSTALL_DIR:-$HOME/.ax}"
BIN_DIR="${AX_BIN_DIR:-$HOME/.local/bin}"

# PATH components and the install-target list cannot contain these separators.
case "$BIN_DIR" in
  *:*|*'
'*) echo "ax: unsupported AX_BIN_DIR (colon or newline); choose another binary directory." >&2; exit 1 ;;
esac
case "$BIN_DIR" in /*) ;; *) BIN_DIR="$(pwd)/$BIN_DIR" ;; esac

stop_ax_processes() {
  if command -v pkill >/dev/null 2>&1; then
    pkill -9 -x ax 2>/dev/null || true
    sleep 1
    pkill -9 -x ax 2>/dev/null || true
  fi
}

clear_pending_upgrade_state() {
  [ -d "$INSTALL_DIR" ] || return 0
  if [ -f "$INSTALL_DIR/upgrade-status.txt" ]; then
    rm -f "$INSTALL_DIR/upgrade-status.txt"
    echo "ax: cleared stuck upgrade-status.txt" >&2
  fi
  find "$INSTALL_DIR" -maxdepth 1 -type d -name 'upgrade-staging-*' -exec rm -rf {} + 2>/dev/null || true
  if [ -d "$INSTALL_DIR/current.old" ]; then
    rm -rf "$INSTALL_DIR/current.old"
    echo "ax: removed parked current.old" >&2
  fi
}

disable_stale_github_tokens() {
  clear_user="${1:-}"
  for name in GITHUB_TOKEN GH_TOKEN; do
    eval "val=\${$name-}"
    if [ -n "$val" ]; then
      unset "$name"
      echo "ax: ignoring process $name for this install (auth failed)." >&2
    fi
  done
  if [ "$clear_user" = "user" ] && command -v powershell.exe >/dev/null 2>&1; then
    for name in GITHUB_TOKEN GH_TOKEN; do
      stored="$(powershell.exe -NoProfile -Command "[Environment]::GetEnvironmentVariable('$name','User')" 2>/dev/null || true)"
      if [ -n "$stored" ]; then
        powershell.exe -NoProfile -Command "[Environment]::SetEnvironmentVariable('$name', \$null, 'User')" 2>/dev/null || true
        echo "ax: cleared stale $name from User environment." >&2
      fi
    done
  fi
}

clean_install_tree() {
  stop_ax_processes
  clear_pending_upgrade_state
  rm -rf "$INSTALL_DIR/current" "$INSTALL_DIR/versions"
}

ax_uninstall=0
ax_purge=0
ax_yes=0
for ax_arg in "$@"; do
  case "$ax_arg" in
    --uninstall) ax_uninstall=1 ;;
    --purge) ax_purge=1 ;;
    --yes|-y) ax_yes=1 ;;
    *) echo "ax: unknown option '$ax_arg' (supported: --uninstall [--purge [--yes]])." >&2; exit 1 ;;
  esac
done
if [ "$ax_purge" = 1 ] && [ "$ax_uninstall" != 1 ]; then
  echo "ax: --purge only works together with --uninstall." >&2
  exit 1
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
  asset="ax-${target}.tar.gz"
  json="$(curl -fsSL -H "Accept: application/vnd.github+json" -H "User-Agent: ax-install" \
    "https://api.github.com/repos/$REPO/releases/tags/$tag" 2>/dev/null)" || json=""
  if [ -n "$json" ] && printf '%s' "$json" | grep -q "\"name\": *\"$asset\""; then
    return 0
  fi
  for url in \
    "https://github.com/$REPO/releases/download/$tag/$asset" \
    "$DOWNLOAD_BASE/$tag/$asset"
  do
    # Fallback when API is unreachable — 1-byte Range GET (GitHub S3 rejects HEAD).
    code="$(curl -fsSL -r 0-0 -o /dev/null -w '%{http_code}' "$url" 2>/dev/null || true)"
    [ "$code" = "200" ] || [ "$code" = "206" ] && return 0
  done
  return 1
}

parse_latest_installable_tag() {
  asset="$1"
  python3 -c '
import json, re, sys
asset = sys.argv[1]
data = json.load(sys.stdin)
tags = []
for r in data:
    if r.get("draft") or r.get("prerelease"):
        continue
    if any(a.get("name") == asset for a in r.get("assets", [])):
        tags.append(r["tag_name"])
def key(t):
    return tuple(int(x) for x in re.sub(r"^v", "", t).split("."))
if tags:
    print(max(tags, key=key))
' "$asset" 2>/dev/null
}

fetch_github_releases_json() {
  # Args: output_path [bearer_token]
  out="$1"
  token="${2:-}"
  if [ -n "$token" ]; then
    curl -sS -o "$out" -w '%{http_code}' \
      -H "Accept: application/vnd.github+json" \
      -H "User-Agent: ax-install" \
      -H "Authorization: Bearer $token" \
      "https://api.github.com/repos/$REPO/releases?per_page=30" 2>/dev/null || printf '000'
  else
    curl -sS -o "$out" -w '%{http_code}' \
      -H "Accept: application/vnd.github+json" \
      -H "User-Agent: ax-install" \
      "https://api.github.com/repos/$REPO/releases?per_page=30" 2>/dev/null || printf '000'
  fi
}

resolve_version_from_api() {
  asset="ax-${target}.tar.gz"
  tmp="$(mktemp)"
  token="${GITHUB_TOKEN:-${GH_TOKEN:-}}"
  # First try with token when present; always fall back to anonymous.
  for attempt in with_token no_token; do
    if [ "$attempt" = "with_token" ] && [ -z "$token" ]; then
      continue
    fi
    if [ "$attempt" = "with_token" ]; then
      code="$(fetch_github_releases_json "$tmp" "$token")"
    else
      code="$(fetch_github_releases_json "$tmp")"
    fi
    case "$code" in
      200)
        tag="$(parse_latest_installable_tag "$asset" <"$tmp" || true)"
        rm -f "$tmp"
        if [ -n "$tag" ]; then
          printf '%s\n' "$tag"
          return 0
        fi
        echo "ax: GitHub API returned releases but none include $asset." >&2
        return 1
        ;;
      401|403)
        echo "ax: GitHub API $attempt failed: HTTP $code" >&2
        disable_stale_github_tokens user
        token=""
        continue
        ;;
      *)
        if [ "$attempt" = "with_token" ]; then
          echo "ax: GitHub API with token failed (HTTP $code); retrying without token." >&2
          disable_stale_github_tokens
          token=""
          continue
        fi
        echo "ax: GitHub API unreachable: HTTP $code" >&2
        rm -f "$tmp"
        return 1
        ;;
    esac
  done
  rm -f "$tmp"
  return 1
}

clear_stale_ax_version_pin() {
  invalid="$1"
  case "$invalid" in v*) tag="$invalid" ;; *) tag="v$invalid" ;; esac
  if [ -n "${AX_VERSION:-}" ]; then
    case "$AX_VERSION" in v*) cur="$AX_VERSION" ;; *) cur="v$AX_VERSION" ;; esac
    if [ "$cur" = "$tag" ]; then
      unset AX_VERSION
      echo "ax: cleared stale AX_VERSION from current shell." >&2
    fi
  fi
  if command -v powershell.exe >/dev/null 2>&1; then
    stored="$(powershell.exe -NoProfile -Command "[Environment]::GetEnvironmentVariable('AX_VERSION','User')" 2>/dev/null || true)"
    [ -n "$stored" ] || return 0
    case "$stored" in v*) sn="$stored" ;; *) sn="v$stored" ;; esac
    if [ "$sn" = "$tag" ]; then
      powershell.exe -NoProfile -Command '[Environment]::SetEnvironmentVariable("AX_VERSION", $null, "User")' 2>/dev/null || true
      echo "ax: cleared stale AX_VERSION=$stored from User environment." >&2
    fi
  fi
}

resolve_version() {
  if [ -n "${AX_VERSION:-}" ]; then
    case "$AX_VERSION" in v*) tag="$AX_VERSION" ;; *) tag="v$AX_VERSION" ;; esac
    if ! asset_url_ok "$tag"; then
      best="$(resolve_version_from_api || true)"
      clear_stale_ax_version_pin "$tag"
      if [ -n "$best" ]; then
        echo "ax: AX_VERSION $tag is not published (no ax-${target}.tar.gz). Installing latest: $best" >&2
        printf '%s\n' "$best"
        return 0
      fi
      echo "ax: AX_VERSION $tag has no downloadable ax-${target}.tar.gz" >&2
      echo "ax: unset pinning: unset AX_VERSION  (or remove from shell profile / User env)" >&2
      return 1
    fi
    printf '%s\n' "$tag"
    return 0
  fi

  best="$(resolve_version_from_api || true)"
  if [ -n "$best" ]; then
    printf '%s\n' "$best"
    return 0
  fi

  tmp="$(mktemp)"
  # Fallback when API asset list unavailable — semver tags + Range probe, then latest.txt.
  curl -fsSL -H "Accept: application/vnd.github+json" -H "User-Agent: ax-install" \
    "https://api.github.com/repos/$REPO/releases/latest" 2>/dev/null \
    | sed -n 's/.*"tag_name": *"\(v[^"]*\)".*/\1/p' >>"$tmp" || true
  curl -fsSL -H "Accept: application/vnd.github+json" -H "User-Agent: ax-install" \
    "https://api.github.com/repos/$REPO/releases?per_page=30" 2>/dev/null \
    | sed -n 's/.*"tag_name": *"\(v[^"]*\)".*/\1/p' >>"$tmp" || true
  curl -fsSL "$DOWNLOAD_BASE/latest.txt" 2>/dev/null | tr -d '[:space:]\r' >>"$tmp" || true

  best=""
  while IFS= read -r cand; do
    [ -n "$cand" ] || continue
    case "$cand" in v*) tag="$cand" ;; *) tag="v$cand" ;; esac
    asset_url_ok "$tag" || continue
    num="${tag#v}"
    if [ -z "$best" ] || [ "$(printf '%s\n' "$num" "${best#v}" | sort -t . -k1,1n -k2,2n -k3,3n | tail -n1)" = "$num" ]; then
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
    *":$BIN_DIR:"*) PATH="$(printf '%s\n' "$PATH" | tr ':' '\n' | grep -Fvx "$BIN_DIR" | tr '\n' ':' | sed 's/:$//')" ;;
  esac
  export PATH="$BIN_DIR:$PATH"
}

quote_shell_path() {
  printf "'%s'" "$(printf '%s' "$1" | sed "s/'/'\\\\''/g")"
}

profile_path_line() {
  ax_quoted_bin="$(quote_shell_path "$BIN_DIR")"
  printf '%s\n' "case \"\$PATH:\" in $ax_quoted_bin:*|*:$ax_quoted_bin:*) ;; *) export PATH=$ax_quoted_bin:\"\$PATH\" ;; esac"
}

append_path_profile() {
  ax_profile="$1"
  ax_path_line="$(profile_path_line)"
  if [ -f "$ax_profile" ] && grep -Fqx "$ax_path_line" "$ax_profile"; then
    echo "PATH configured: $ax_profile (already present)"
    return 0
  fi
  if ! mkdir -p "$(dirname "$ax_profile")" || ! printf '\n# ax PATH\n%s\n' "$ax_path_line" >> "$ax_profile"; then
    echo "ax: cannot update $ax_profile; manual PATH setup is required." >&2
    return 1
  fi
  echo "PATH configured: $ax_profile"
}

persist_shell_path() {
  ax_login_shell="${SHELL:-}"
  ax_login_shell="${ax_login_shell##*/}"
  if [ -z "$ax_login_shell" ]; then
    if [ "$os" = darwin ]; then ax_login_shell=zsh; else ax_login_shell=sh; fi
  fi
  case "$ax_login_shell" in
    zsh) append_path_profile "${ZDOTDIR:-$HOME}/.zshrc" ;;
    bash)
      append_path_profile "$HOME/.bashrc" || return 1
      if [ -f "$HOME/.bash_profile" ]; then
        append_path_profile "$HOME/.bash_profile"
      elif [ -f "$HOME/.bash_login" ]; then
        append_path_profile "$HOME/.bash_login"
      else
        append_path_profile "$HOME/.profile"
      fi
      ;;
    sh|dash) append_path_profile "$HOME/.profile" ;;
    *) echo "ax: shell $ax_login_shell needs manual PATH setup; no startup file was changed." >&2; return 1 ;;
  esac
}

show_session_path() {
  echo "Open a new terminal, or for sh/bash/zsh in this terminal run:"
  printf '  export PATH=%s:"$PATH"\n' "$(quote_shell_path "$BIN_DIR")"
}

remove_path_profile() {
  # Drops the block append_path_profile wrote; every other line stays as it was.
  ax_profile="$1"
  [ -f "$ax_profile" ] || return 0
  ax_path_line="$(profile_path_line)"
  grep -Fqx "$ax_path_line" "$ax_profile" || return 0
  ax_profile_tmp="$(mktemp)"
  if AX_PATH_LINE="$ax_path_line" awk '
    { line[NR] = $0 }
    END {
      target = ENVIRON["AX_PATH_LINE"]
      out = 0
      for (i = 1; i <= NR; i++) {
        if (line[i] == "# ax PATH" && line[i + 1] == target) {
          if (out > 0 && kept[out] == "") out--
          i++
          continue
        }
        if (line[i] == target) continue
        kept[++out] = line[i]
      }
      for (i = 1; i <= out; i++) print kept[i]
    }' "$ax_profile" >"$ax_profile_tmp" && cat "$ax_profile_tmp" >"$ax_profile"; then
    echo "PATH removed: $ax_profile"
  else
    echo "ax: could not update $ax_profile; remove the '# ax PATH' lines by hand." >&2
  fi
  rm -f "$ax_profile_tmp"
}

purge_target_is_safe() {
  ax_dir="${INSTALL_DIR%/}"
  [ -n "$ax_dir" ] || return 1
  [ "$ax_dir" != "${HOME%/}" ] || return 1
  case "${ax_dir##*/}" in
    .ax|ax|.ax[-_.]*|ax[-_.]*|*[-_.]ax) return 0 ;;
  esac
  return 1
}

confirm_purge() {
  [ "$ax_yes" = 1 ] && return 0
  if [ -t 2 ] && { : </dev/tty; } 2>/dev/null; then
    printf 'Delete %s with all ax data in it (database, policy, memories, sign-ins)? [y/N] ' "$INSTALL_DIR" >/dev/tty
    ax_answer=""
    read -r ax_answer </dev/tty || ax_answer=""
    case "$ax_answer" in y|Y|yes|YES|Yes) return 0 ;; esac
    return 1
  fi
  echo "ax: --purge deletes $INSTALL_DIR with all ax data in it; re-run with --uninstall --purge --yes to confirm." >&2
  return 1
}

uninstall_ax() {
  if [ "$ax_purge" = 1 ]; then
    if ! purge_target_is_safe; then
      echo "ax: refusing to purge '$INSTALL_DIR': it is not an ax-specific directory. Remove ax data by hand." >&2
      exit 1
    fi
    confirm_purge || { echo "ax: nothing was removed." >&2; exit 1; }
  fi
  stop_ax_processes
  rm -f "$BIN_DIR/ax"
  if [ -d "$INSTALL_DIR" ]; then
    clear_pending_upgrade_state
    rm -rf "$INSTALL_DIR/current" "$INSTALL_DIR/versions"
  fi
  for ax_profile in "${ZDOTDIR:-$HOME}/.zshrc" "$HOME/.zshrc" "$HOME/.bashrc" "$HOME/.bash_profile" "$HOME/.bash_login" "$HOME/.profile"; do
    remove_path_profile "$ax_profile"
  done
  if [ "$ax_purge" = 1 ]; then
    rm -rf "$INSTALL_DIR"
    echo "ax uninstalled and purged (removed $BIN_DIR/ax and $INSTALL_DIR)."
  else
    rmdir "$INSTALL_DIR" 2>/dev/null || true
    echo "ax uninstalled (removed $BIN_DIR/ax and the program in $INSTALL_DIR)."
    if [ -d "$INSTALL_DIR" ]; then
      echo "Kept ax data in $INSTALL_DIR. To delete it too: sh -s -- --uninstall --purge"
    fi
  fi
}

if [ "$ax_uninstall" = 1 ]; then
  uninstall_ax
  exit 0
fi

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
if ! persist_shell_path; then
  echo "ax: the binary is installed; complete the manual PATH setup shown below." >&2
fi
show_session_path

if [ "${AX_SKIP_PROJECT_INIT:-}" = "1" ]; then
  echo "Skipping project discovery (AX_SKIP_PROJECT_INIT=1)."
else
  echo "Discovering projects and running ax init..."
  "$BIN_DIR/ax" init --all || echo "ax: project discovery reported errors. Re-run: ax init --all" >&2
fi

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
