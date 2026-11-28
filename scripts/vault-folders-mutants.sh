#!/usr/bin/env bash
# Manual mutants for vault folders (config, DAV folder area, folder sync). Each
# mutant must change the file and must make a test in its suite fail
# (scripts/lib/mutate.sh).
set -euo pipefail
root="$(cd "$(dirname "$0")/.." && pwd)"
source "$root/scripts/lib/mutate.sh"

sync_suite() { (cd "$root" && cargo test -q -p ax-memory --test folder_sync); }
web_suite() { (cd "$root" && cargo test -q -p ax-web --lib dav::folders && cargo test -q -p ax-web --test vault_folders); }
ts_suite() { (cd "$root/crates/ax-web/web-ui" && node --test src/vaultMount.test.ts src/memoryCategory.test.ts); }

fs="$root/crates/ax-web/src/dav/folders.rs"
ax_mutate web_suite "$fs" "    if checked.starts_with(&base) {" "    if true {"
ax_mutate web_suite "$fs" "        if part == \".\" || part == \"..\" ||" "        if part == \".\" ||"
ax_mutate web_suite "$fs" "        if meta.file_type().is_symlink() {
            continue;
        }" ""
cfg="$root/crates/ax-web/src/vault_folders.rs"
ax_mutate web_suite "$cfg" "        if !seen.insert(folder.name.to_lowercase()) {" "        if false {"
ax_mutate web_suite "$cfg" "        if !folder.path.is_absolute() {" "        if false {"
ax_mutate web_suite "$cfg" "    if write && hub.readonly {" "    if false {"
ax_mutate web_suite "$cfg" "    (!crate::dav::mount::allowed(headers, false, hub.port)).then(" "    false.then("
ax_mutate web_suite "$cfg" "    for folder in list.iter().filter(|f| f.index && !unchanged(f)) {" "    for folder in list.iter().filter(|f| f.index) {"
ax_mutate web_suite "$cfg" "        if !list.iter().any(|f| f.name == old.name && f.index) {" "        if false {"
ax_mutate web_suite "$cfg" "        if let Some(folder) = find(&root, &name).filter(|f| f.index) {" "        if let Some(folder) = None::<VaultFolder> {"
dfs="$root/crates/ax-web/src/dav/fs.rs"
ax_mutate web_suite "$dfs" "                out.insert(FOLDERS_DIR.to_string(), Meta::dir(0));" ""
mod="$root/crates/ax-web/src/dav/mod.rs"
ax_mutate web_suite "$mod" " || matches!(pages::classify(&p), Target::Folder(_, rest) if rest.is_empty())" ""
ax_mutate web_suite "$mod" "    if touches_folder && !mount::allowed(req.headers(), false, port) {" "    if false {"
ax_mutate web_suite "$mod" "is_folder(req.uri().path()) || destination.as_deref().is_some_and(is_folder);" "is_folder(req.uri().path());"
sync="$root/crates/ax-memory/src/folder_sync.rs"
ax_mutate sync_suite "$sync" "    name.starts_with('.') || name == \"node_modules\" || name.starts_with(\"target\")" "    name == \"node_modules\""
ax_mutate sync_suite "$sync" "            if meta.is_dir() && !skipped_dir(&name) {" "            if meta.is_dir() {"
ax_mutate sync_suite "$sync" "        if size > MAX_FOLDER_FILE_BYTES || read.files.len() >= MAX_FOLDER_FILES {" "        if read.files.len() >= MAX_FOLDER_FILES {"
ax_mutate sync_suite "$sync" "            Some((t, b)) if t == title && b == body => {}" "            Some((_t, _b)) if false => {}"
ax_mutate sync_suite "$sync" "    for id in stale.keys() {" "    for id in Vec::<String>::new().iter() {"
ax_mutate sync_suite "$sync" "    kind == crate::turns::TURN_KIND || kind == DOC_KIND" "    kind == crate::turns::TURN_KIND"
ax_mutate sync_suite "$sync" "    for id in existing(pool, folder).await?.keys() {" "    for id in existing(pool, \"\").await?.keys() {"
ts="$root/crates/ax-web/web-ui/src/vaultMount.ts"
ax_mutate ts_suite "$ts" "  if (!f.index) return 'Not indexed';" ""
cat="$root/crates/ax-web/web-ui/src/memoryCategory.ts"
ax_mutate ts_suite "$cat" "  doc: 'doc',
" ""

ax_mutants_summary
