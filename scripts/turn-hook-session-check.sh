#!/usr/bin/env bash
# Real execution of `ax turn-hook start` against a temp home: a Cursor-style stdin payload must
# write the chat id to <home>/.ax/active-cursor-session, and blank ids must write nothing.
# Fails closed; never touches the real ~/.ax.
# Usage: AX_BIN=/path/to/ax scripts/turn-hook-session-check.sh
set -euo pipefail

bin="${AX_BIN:?set AX_BIN to the ax binary}"
real="$HOME/.ax/active-cursor-session"

state_of() {
  if [ ! -e "$1" ]; then echo absent
  elif [ "$(uname)" = Darwin ]; then stat -f '%m %z' "$1"
  else stat -c '%Y %s' "$1"
  fi
}
before="$(state_of "$real")"

tmp="$(mktemp -d)"
trap 'rm -rf "$tmp"' EXIT

hook() {
  local home="$1" payload="$2"
  mkdir -p "$home/.ax"
  (cd "$tmp" && printf '%s' "$payload" | env AX_HOME_DIR="$home" HOME="$home" \
    AX_USAGE_DB="$home/usage.db" AX_GLOBAL_DB="$home/global.db" "$bin" turn-hook start >/dev/null)
}

hook "$tmp/a" '{"session_id":"  chat-from-cursor  ","conversation_id":"other","prompt":"hi"}'
got="$(head -n1 "$tmp/a/.ax/active-cursor-session")"
[ "$got" = "chat-from-cursor" ] || { echo "FAIL: session_id case wrote '$got'"; exit 1; }

hook "$tmp/b" '{"conversation_id":"conv-42","prompt":"hi"}'
got="$(head -n1 "$tmp/b/.ax/active-cursor-session")"
[ "$got" = "conv-42" ] || { echo "FAIL: conversation_id fallback wrote '$got'"; exit 1; }

hook "$tmp/c" '{"session_id":"   ","prompt":"hi"}'
[ ! -e "$tmp/c/.ax/active-cursor-session" ] || { echo "FAIL: a blank id wrote the hook file"; exit 1; }

after="$(state_of "$real")"
[ "$before" = "$after" ] || { echo "FAIL: the real $real changed ($before -> $after)"; exit 1; }

echo "OK: turn-hook start wrote session_id, fell back to conversation_id, skipped a blank id; real hook file unchanged ($after)"
