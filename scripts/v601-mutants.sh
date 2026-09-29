#!/usr/bin/env bash
# Each mutant must turn its Unix test red. Fails if a mutant cannot be applied or survives.
set -euo pipefail
cd "$(dirname "$0")/.."

run_mutant() {
	local file=$1 from=$2 to=$3 pkg=$4 test=$5
	git diff --quiet -- "$file" || { echo "ERROR: $file has uncommitted changes" >&2; exit 2; }
	FROM="$from" TO="$to" python3 - "$file" <<'EOF'
import os, sys
p = sys.argv[1]
s = open(p).read()
if s.count(os.environ["FROM"]) != 1:
    sys.exit(f"ERROR: mutant target not found exactly once in {p}")
open(p, "w").write(s.replace(os.environ["FROM"], os.environ["TO"]))
EOF
	git diff --quiet -- "$file" && { echo "ERROR: mutant did not change $file" >&2; exit 2; }
	local status=0
	env -u CARGO_TARGET_DIR cargo test -q -p "$pkg" --lib "$test" >/dev/null 2>&1 || status=$?
	git checkout -q -- "$file"
	if [ "$status" -eq 0 ]; then
		echo "SURVIVED: $pkg $test ($from -> $to)"
		exit 1
	fi
	echo "killed: $pkg $test"
}

run_mutant crates/ax-quality/src/bootstrap.rs \
	'    let folder_name = project_root
        .file_name()' \
	'    let folder_name = project_root
        .parent().and_then(|p| p.file_name())' \
	ax-quality resolves_placeholder_to_folder_name_unix

run_mutant crates/ax-quality/src/bootstrap.rs \
	'folder_slug == workspace_key' 'folder_slug != workspace_key' \
	ax-quality legacy_prefix_from_workspace_folder_unix

run_mutant crates/ax-usage/src/savings.rs \
	'parent_name.is_some() && parent_name == file_stem' 'parent_name.is_some()' \
	ax-usage cursor_transcript_path_filter_unix

echo "all mutants killed"
