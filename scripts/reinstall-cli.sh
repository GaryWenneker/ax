#!/usr/bin/env bash
# Reinstall ax CLI to ~/.cargo/bin after a local build.
# Kills every running ax instance first so cargo install can replace the binary.
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

kill_all_ax

echo "Installing ax-cli to cargo bin..."
cargo install --path crates/ax-cli --force

bin="$(command -v ax)"
echo "$(ax --version)"
echo "Installed: $bin"
