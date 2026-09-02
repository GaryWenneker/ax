#!/usr/bin/env bash
# Fail-closed checks for the macOS PATH shim (SPEC macos-dev-ax-path.md).
set -euo pipefail
root="$(cd "$(dirname "$0")/.." && pwd)"
fail() { echo "FAIL: $*" >&2; exit 1; }

shim="$HOME/.local/bin/ax"
bin="$root/target-dev/release/ax"
reinstall="$root/scripts/reinstall-cli.sh"
mcp="$HOME/.cursor/mcp.json"

[[ -x "$bin" ]] || fail "missing $bin"
[[ -e "$shim" ]] || fail "missing PATH shim $shim"
[[ -x "$shim" ]] || fail "shim not executable: $shim"

ft="$(file -b "$shim")"
echo "$ft" | grep -q 'Mach-O' && fail "shim is Mach-O: $ft"
echo "$ft" | grep -Eq 'text|script|POSIX' || fail "shim not a script: $ft"

grep -q "$bin" "$shim" || fail "shim does not exec $bin"
grep -q '^exec ' "$shim" || fail "shim has no exec"

# Negative control: a Mach-O at the shim path would match this grep.
if echo 'Mach-O 64-bit executable arm64' | grep -q 'Mach-O'; then
	:
else
	fail "negative control for Mach-O grep is broken"
fi

[[ -x "$reinstall" ]] || fail "missing $reinstall"
grep -q 'relink_macos_macho' "$reinstall" || fail "reinstall-cli.sh missing Mach-O inode relink"
if grep -n 'Darwin' "$reinstall" | grep -q .; then
	:
else
	fail "reinstall-cli.sh has no Darwin branch"
fi
# cargo install must not run on the Darwin path (would copy Mach-O to cargo bin).
awk '
  /uname_s/ { d=1 }
  d && /else/ { d=0 }
  d && /cargo install/ { bad=1 }
  END { exit bad ? 1 : 0 }
' "$reinstall" || fail "Darwin branch still cargo-installs"

[[ -x "$bin" ]] || fail "target-dev ax missing"
direct="$("$bin" --version)"
via="$(cd /tmp && "$shim" --version)"
[[ "$direct" == "$via" ]] || fail "version mismatch direct='$direct' shim='$via'"

# command -v from another cwd (PATH must include ~/.local/bin)
if [[ ":$PATH:" != *":$HOME/.local/bin:"* ]]; then
	fail "~/.local/bin not on PATH (got PATH=$PATH)"
fi
hashed="$(hash -r 2>/dev/null || true; cd / && command -v ax)"
[[ "$hashed" == "$shim" ]] || fail "command -v ax is $hashed not $shim"

[[ -f "$mcp" ]] || fail "missing $mcp"
grep -q "$bin" "$mcp" || fail "mcp.json command is not $bin"

echo "OK: shim=$shim file='$ft'"
echo "OK: $direct"
echo "OK: MCP command points at target-dev"
exit 0
