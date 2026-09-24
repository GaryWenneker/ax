#!/usr/bin/env bash
# Negative controls for scripts/lib/user-agent-files.sh (layer 7 of the gauntlets), in a temp HOME.
# Each control must come out as stated, or the run fails. The real HOME is never touched.
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
. "$ROOT/scripts/lib/user-agent-files.sh"
HOME="$(mktemp -d /tmp/ax-f1-control.XXXX)"
export HOME
trap 'rm -rf "$HOME"' EXIT
failures=0
check() { # label, then a test command
  local label="$1"; shift
  if "$@"; then echo "ok    $label"; else echo "FAIL  $label"; failures=$((failures + 1)); fi
}
differs() { [ "$1" != "$2" ]; }
same() { [ "$1" = "$2" ]; }

mkdir -p "$HOME/.cursor"
echo '{"mcpServers":{"ax":{"command":"/a/ax"}},"other":1}' >"$HOME/.claude.json"
a="$(user_agent_files)"
echo '{}' >"$HOME/.cursor/mcp.json"
b="$(user_agent_files)"
check "a created ~/.cursor/mcp.json is a change" differs "$a" "$b"
echo '{"x":1}' >"$HOME/.cursor/mcp.json"
c="$(user_agent_files)"
check "an edited ~/.cursor/mcp.json is a change" differs "$b" "$c"
echo '{"mcpServers":{"ax":{"command":"/a/ax"}},"other":2}' >"$HOME/.claude.json"
d="$(user_agent_files)"
check "other keys in ~/.claude.json are ignored" same "$c" "$d"
echo '{"mcpServers":{"ax":{"command":"/tmp/x/ax"}},"other":2}' >"$HOME/.claude.json"
e="$(user_agent_files)"
check "the ax entry in ~/.claude.json is a change" differs "$d" "$e"
echo 'not json' >"$HOME/.claude.json"
unreadable_fails() { ! user_agent_files >/dev/null 2>&1; }
check "an unreadable ~/.claude.json fails closed" unreadable_fails

[ "$failures" -eq 0 ] || { echo "controls: $failures failed" >&2; exit 1; }
echo "controls: all passed"
