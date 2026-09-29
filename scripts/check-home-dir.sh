#!/usr/bin/env bash
# Every home-directory lookup goes through ax_utils::paths::home_dir, which honours AX_HOME_DIR.
# Only catches the `dirs::home_dir` spelling, not other ways to find a home dir.
set -euo pipefail
cd "$(dirname "$0")/.."

status=0
found=$(grep -rn --include='*.rs' 'dirs::home_dir' crates/*/src) || status=$?
if [ "$status" -gt 1 ]; then
	echo "ERROR: grep failed (exit $status)" >&2
	exit 2
fi

hits=()
while IFS= read -r line; do
	[ -z "$line" ] && continue
	case $line in
		crates/ax-utils/src/paths.rs:*) ;;
		*) hits+=("$line") ;;
	esac
done <<< "$found"

if [ "${#hits[@]}" -gt 0 ]; then
	echo "Use ax_utils::paths::home_dir() instead of dirs::home_dir():" >&2
	printf '%s\n' "${hits[@]}" >&2
	exit 1
fi
echo "ok: no direct dirs::home_dir calls"
