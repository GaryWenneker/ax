#!/usr/bin/env bash
# After a local ax-cli build, put `ax` on PATH.
#
# macOS: never cargo-install or copy a Mach-O into ~/.local/bin or ~/.cargo/bin
# (those paths are SIGKILL'd on some machines). Build into target-dev and install
# a POSIX shim at ~/.local/bin/ax that execs the checkout binary.
# Linux: cargo install to ~/.cargo/bin (unchanged).
set -euo pipefail
root="$(cd "$(dirname "$0")/.." && pwd)"
cd "$root"

kill_all_ax() {
	echo "Stopping ax daemon (if reachable)..."
	if command -v ax >/dev/null 2>&1; then
		ax daemon stop 2>/dev/null || true
		sleep 0.4
	fi

	echo "Killing all ax processes..."
	if command -v pkill >/dev/null 2>&1; then
		pkill -x ax 2>/dev/null || true
		pkill -f '[/]ax serve' 2>/dev/null || true
		pkill -f '[/]ax daemon' 2>/dev/null || true
		pkill -f '[/]ax watch' 2>/dev/null || true
		pkill -f '[/]ax sync' 2>/dev/null || true
	fi
	sleep 0.6

	if command -v pgrep >/dev/null 2>&1 && pgrep -x ax >/dev/null 2>&1; then
		echo "Warning: ax process(es) still running:" >&2
		pgrep -xa ax >&2 || true
	fi
}

relink_macos_macho() {
	# Overwriting a Mach-O in place (cargo, cp onto existing file) makes this Mac
	# SIGKILL it. Unlink first so the path gets a new inode.
	local bin="$1"
	local tmp="${bin}.new.$$"
	cp "$bin" "$tmp"
	chmod 0755 "$tmp"
	rm -f "$bin"
	mv "$tmp" "$bin"
	if ! "$bin" --version >/dev/null 2>&1; then
		echo "relink failed: $bin still not executable (SIGKILL?)" >&2
		exit 1
	fi
}

install_macos_path_shim() {
	local dest="$1"
	local shim_path="${HOME}/.local/bin/ax"
	if [[ ! -x "$dest" ]]; then
		echo "missing executable: $dest" >&2
		exit 1
	fi
	mkdir -p "$(dirname "$shim_path")"
	# Atomic replace so a half-written shim is never left executable.
	local tmp
	tmp="$(mktemp "${shim_path}.XXXXXX")"
	cat >"$tmp" <<EOF
#!/bin/sh
# POSIX PATH shim (not a Mach-O). Installed by scripts/reinstall-cli.sh.
# Latest build: cargo build --release -p ax-cli in the ax checkout.
exec "$dest" "\$@"
EOF
	chmod 0755 "$tmp"
	mv -f "$tmp" "$shim_path"
	if file "$shim_path" | grep -q 'Mach-O'; then
		echo "refusing: $shim_path is a Mach-O (SIGKILL on this Mac)" >&2
		exit 1
	fi
	echo "PATH shim: $shim_path -> $dest"
}

uname_s="$(uname -s)"
if [[ "$uname_s" == "Darwin" ]]; then
	# Do not pkill ax: MCP and `ax web` keep the old inode until restarted.
	# Replacing ~/.local/bin with a shim does not require stopping processes.
	echo "Building ax-cli (release, onnx) into target-dev..."
	cargo build --release -p ax-cli --features onnx
	bin="$root/target-dev/release/ax"
	relink_macos_macho "$bin"
	install_macos_path_shim "$bin"
	echo "$("$bin" --version)"
	echo "Direct: $bin"
	echo "PATH: $HOME/.local/bin/ax (hash -r if this shell cached the old binary)"
else
	kill_all_ax
	echo "Installing ax-cli to cargo bin..."
	cargo install --path crates/ax-cli --force --features onnx
	bin="$(command -v ax)"
	echo "$(ax --version)"
	echo "Installed: $bin"
fi
