#!/usr/bin/env bash
# Each mutant must turn its Unix test red (scripts/lib/mutate.sh). Fails if a
# mutant cannot be applied, survives, or does not build.
set -euo pipefail
root="$(cd "$(dirname "$0")/.." && pwd)"
source "$root/scripts/lib/mutate.sh"
cd "$root"

lib_test() { env -u CARGO_TARGET_DIR cargo test -q -p "$1" --lib "$2"; }
folder_name_suite() { lib_test ax-quality resolves_placeholder_to_folder_name_unix; }
legacy_prefix_suite() { lib_test ax-quality legacy_prefix_from_workspace_folder_unix; }
transcript_filter_suite() { lib_test ax-usage cursor_transcript_path_filter_unix; }

ax_mutate folder_name_suite crates/ax-quality/src/bootstrap.rs \
	'    let folder_name = project_root
        .file_name()' \
	'    let folder_name = project_root
        .parent().and_then(|p| p.file_name())'

ax_mutate legacy_prefix_suite crates/ax-quality/src/bootstrap.rs \
	'folder_slug == workspace_key' 'folder_slug != workspace_key'

ax_mutate transcript_filter_suite crates/ax-usage/src/savings.rs \
	'parent_name.is_some() && parent_name == file_stem' 'parent_name.is_some()'

ax_mutants_summary
