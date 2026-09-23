#!/usr/bin/env bash
# Manual mutation run for the client-name cleanup (docs catalog + share URL).
# Each mutant must apply exactly once (else the run fails) and must make its crate's tests fail.
# Files are restored from the git index after each mutant; stage the change under test first.
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
cd "$ROOT"

for f in crates/ax-docs-catalog/src/config.rs crates/ax-docs-catalog/src/scan.rs \
         crates/ax-docs-catalog/src/memories.rs crates/ax-docs-catalog/src/lib.rs \
         crates/ax-share/src/config.rs crates/ax-share/src/providers/onedrive.rs; do
  if ! git diff --quiet -- "$f"; then
    echo "mutants: $f has unstaged changes; stage them first" >&2
    exit 1
  fi
done

for crate in ax-docs-catalog ax-share; do
  if ! env -u CARGO_TARGET_DIR cargo test -q -p "$crate" >/dev/null 2>&1; then
    echo "mutants: baseline tests of $crate fail on the unmutated tree; a kill would mean nothing" >&2
    exit 1
  fi
done

killed=0
total=0

# mutant <id> <crate> <file> <perl-substitution>
mutant() {
  local id="$1" crate="$2" file="$3" expr="$4"
  total=$((total + 1))
  local before after
  before="$(shasum "$file" | cut -d' ' -f1)"
  perl -0pi -e "$expr" "$file"
  after="$(shasum "$file" | cut -d' ' -f1)"
  if [ "$before" = "$after" ]; then
    git checkout -q -- "$file"
    echo "mutants: $id did not apply to $file" >&2
    exit 1
  fi
  if env -u CARGO_TARGET_DIR cargo test -q -p "$crate" >/dev/null 2>&1; then
    echo "SURVIVED $id ($file)"
  else
    echo "killed   $id"
    killed=$((killed + 1))
  fi
  git checkout -q -- "$file"
  if [ -n "$(git diff -- "$file")" ]; then
    echo "mutants: $file not restored" >&2
    exit 1
  fi
}

DC=ax-docs-catalog
SH=ax-share
mutant M1  $DC crates/ax-docs-catalog/src/config.rs   's/wiki_remote: j\.wiki_remote\.clone\(\),/wiki_remote: j.wiki_remote.clone().or(Some("https:\/\/example.invalid\/w.git".into())),/'
mutant M2  $DC crates/ax-docs-catalog/src/config.rs   's/\n\s*\.or_else\(\|\| j\.wiki_remote\.clone\(\)\)//'
mutant M3  $DC crates/ax-docs-catalog/src/config.rs   's/\.wiki_root_url\n(\s*)\.clone\(\)\n(\s*)\.or_else\(\|\| j\.wiki_remote\.clone\(\)\)/.wiki_remote\n$1.clone()\n$2.or_else(|| j.wiki_root_url.clone())/'
mutant M4  $DC crates/ax-docs-catalog/src/config.rs   's/unwrap_or\(folder_name\)/unwrap_or_else(|| "project".into())/'
mutant M5  $DC crates/ax-docs-catalog/src/scan.rs     's/if lines\.get\(i \+ 1\)\.is_some_and\(\|next\| is_separator_row\(next\)\) \{/if false {/'
mutant M6  $DC crates/ax-docs-catalog/src/scan.rs     's/ \|\| skip\.iter\(\)\.any\(\|s\| s == &name\)//'
mutant M7  $DC crates/ax-docs-catalog/src/scan.rs     's/let Ok\(content\) = std::fs::read_to_string\(path\) else \{\n        return vec!\[\];/let Ok(content) = std::fs::read_to_string(path) else {\n        return vec!["x".into()];/'
mutant M8  $DC crates/ax-docs-catalog/src/memories.rs 's/"documentation-catalog"\.into\(\),\n(\s*)"wiki"\.into\(\),\n(\s*)"team"/"documentation-catalog".into(),\n$1"azdo-wiki".into(),\n$2"team"/'
mutant M9  $DC crates/ax-docs-catalog/src/memories.rs 's/None => "ax docs-catalog sync"\.into\(\),/None => "ax docs-catalog sync (skill: docs)".into(),/'
mutant M10 $DC crates/ax-docs-catalog/src/memories.rs 's/"Wiki - integrations"/"Wiki - Integraties"/'
mutant M11 $DC crates/ax-docs-catalog/src/lib.rs      's/"not-configured"\.into\(\)/"skipped".into()/'
mutant M12 $DC crates/ax-docs-catalog/src/lib.rs      's/    pub integration_pages: usize,/    #[serde(rename = "legacyPages")]\n    pub integration_pages: usize,/'
mutant M13 $SH crates/ax-share/src/config.rs          's/pub const DEFAULT_ONEDRIVE_SHARE_URL: &str = "";/pub const DEFAULT_ONEDRIVE_SHARE_URL: &str = "https:\/\/example.invalid\/share";/'
mutant M14 $SH crates/ax-share/src/providers/onedrive.rs 's/return Err\("onedrive\.shareUrl is not set in ax\.json"\.into\(\)\);/return Err("OneDrive share URL is not configured".into());/'
mutant M15 $DC crates/ax-docs-catalog/src/memories.rs 's/if data\.wiki_root_url\.is_empty\(\) \{/if data.wiki_root_url.is_empty() && false {/'

echo "mutants: $killed/$total killed"
[ "$killed" -eq "$total" ]
