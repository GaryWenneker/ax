# Restore files that a mutation script rewrites in place, however the script ends.
#
# Source this file, then call `ax_guard_file <path>` before the first in-place
# rewrite of <path>. On exit (success, a failing command under `set -e`, Ctrl+C,
# SIGTERM or SIGHUP) every guarded file gets back the copy taken when it was
# first guarded. Guarding a file again keeps that first copy. A relative path
# is resolved against the working directory at guard time, so the script may
# change directory afterwards.
#
# AX_GUARD_TMP is a scratch directory for the script's own backups; it is
# removed on exit, so nothing such as a `*.orig` file is left in the tree.
#
# A restored file gets a fresh mtime, so cargo rebuilds it instead of reusing
# a build of the mutant.
#
# This file sets the EXIT, INT, TERM and HUP traps. A script that sources it
# must not replace them; it may define a function `ax_guard_cleanup`, which
# runs on exit before the files are restored (for example to stop a server).

_AX_GUARD_DIR="$(mktemp -d "${TMPDIR:-/tmp}/ax-guard.XXXXXX")"
_AX_GUARD_COUNT=0
AX_GUARD_TMP="$_AX_GUARD_DIR/scratch"
mkdir "$AX_GUARD_TMP"

ax_guard_file() {
  local path="$1" i=0
  case "$path" in
    /*) ;;
    *) path="$PWD/$path" ;;
  esac
  while [ "$i" -lt "$_AX_GUARD_COUNT" ]; do
    if [ "$(cat "$_AX_GUARD_DIR/$i.path")" = "$path" ]; then
      return 0
    fi
    i=$((i + 1))
  done
  cp -p "$path" "$_AX_GUARD_DIR/$_AX_GUARD_COUNT.orig"
  printf '%s' "$path" >"$_AX_GUARD_DIR/$_AX_GUARD_COUNT.path"
  _AX_GUARD_COUNT=$((_AX_GUARD_COUNT + 1))
}

ax_restore_guarded() {
  local i=0 path
  while [ "$i" -lt "$_AX_GUARD_COUNT" ]; do
    path="$(cat "$_AX_GUARD_DIR/$i.path")"
    if ! cmp -s "$_AX_GUARD_DIR/$i.orig" "$path"; then
      cp "$_AX_GUARD_DIR/$i.orig" "$path" || echo "ERROR: could not restore $path" >&2
      echo "restored $path" >&2
    fi
    i=$((i + 1))
  done
}

_ax_guard_on_exit() {
  local status=$?
  trap - EXIT INT TERM HUP
  if declare -F ax_guard_cleanup >/dev/null; then
    ax_guard_cleanup || true
  fi
  ax_restore_guarded
  rm -rf "$_AX_GUARD_DIR"
  exit "$status"
}

trap _ax_guard_on_exit EXIT
trap 'exit 130' INT
trap 'exit 143' TERM
trap 'exit 129' HUP
