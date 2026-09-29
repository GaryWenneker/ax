#!/usr/bin/env bash
# Manual mutants for vault folders (config, DAV folder area, folder sync). Each
# mutant must change the file (proved by cmp) and must make its suite fail.
set -euo pipefail
root="$(cd "$(dirname "$0")/.." && pwd)"
killed=0
total=0

sync_suite() { (cd "$root" && cargo test -q -p ax-memory --test folder_sync); }
web_suite() { (cd "$root" && cargo test -q -p ax-web --lib dav::folders && cargo test -q -p ax-web --test vault_folders); }
ts_suite() { (cd "$root/crates/ax-web/web-ui" && node --test src/vaultMount.test.ts src/memoryCategory.test.ts); }

mutate() { # suite, file, python-literal old, python-literal new
  local suite="$1" file="$2" old="$3" new="$4"
  total=$((total + 1))
  cp "$file" "$file.orig"
  python3 - "$file" "$old" "$new" <<'PY'
import sys
p, old, new = sys.argv[1:]
s = open(p).read()
if old not in s:
    sys.exit(f"mutant pattern not found in {p}: {old}")
open(p, "w").write(s.replace(old, new, 1))
PY
  if cmp -s "$file" "$file.orig"; then
    mv "$file.orig" "$file"; echo "NOT APPLIED: $old"; exit 1
  fi
  if "$suite" >/dev/null 2>&1; then
    echo "SURVIVED: $file :: $old -> $new"
  else
    killed=$((killed + 1)); echo "killed: $file :: $old -> $new"
  fi
  mv "$file.orig" "$file"
}


fs="$root/crates/ax-web/src/dav/folders.rs"
mutate web_suite "$fs" "    if checked.starts_with(&base) {" "    if true {"
mutate web_suite "$fs" "        if part == \".\" || part == \"..\" ||" "        if part == \".\" ||"
mutate web_suite "$fs" "        if meta.file_type().is_symlink() {
            continue;
        }" ""
cfg="$root/crates/ax-web/src/vault_folders.rs"
mutate web_suite "$cfg" "        if !seen.insert(folder.name.to_lowercase()) {" "        if false {"
mutate web_suite "$cfg" "        if !folder.path.is_absolute() {" "        if false {"
mutate web_suite "$cfg" "    if write && hub.readonly {" "    if false {"
mutate web_suite "$cfg" "    (!crate::dav::mount::allowed(headers, false)).then(" "    false.then("
mutate web_suite "$cfg" "    for folder in list.iter().filter(|f| f.index && !unchanged(f)) {" "    for folder in list.iter().filter(|f| f.index) {"
mutate web_suite "$cfg" "        if !list.iter().any(|f| f.name == old.name && f.index) {" "        if false {"
mutate web_suite "$cfg" "        if let Some(folder) = find(&root, &name).filter(|f| f.index) {" "        if let Some(folder) = None::<VaultFolder> {"
dfs="$root/crates/ax-web/src/dav/fs.rs"
mutate web_suite "$dfs" "                out.insert(FOLDERS_DIR.to_string(), Meta::dir(0));" ""
mod="$root/crates/ax-web/src/dav/mod.rs"
mutate web_suite "$mod" " || matches!(pages::classify(&p), Target::Folder(_, rest) if rest.is_empty())" ""
mutate web_suite "$mod" "    if touches_folder && !mount::allowed(req.headers(), false) {" "    if false {"
mutate web_suite "$mod" "is_folder(req.uri().path()) || destination.as_deref().is_some_and(is_folder);" "is_folder(req.uri().path());"
sync="$root/crates/ax-memory/src/folder_sync.rs"
mutate sync_suite "$sync" "    name.starts_with('.') || name == \"node_modules\" || name.starts_with(\"target\")" "    name == \"node_modules\""
mutate sync_suite "$sync" "            if meta.is_dir() && !skipped_dir(&name) {" "            if meta.is_dir() {"
mutate sync_suite "$sync" "        if size > MAX_FOLDER_FILE_BYTES || files.len() >= MAX_FOLDER_FILES {" "        if files.len() >= MAX_FOLDER_FILES {"
mutate sync_suite "$sync" "            Some((t, b)) if t == title && b == body => {}" "            Some((_t, _b)) if false => {}"
mutate sync_suite "$sync" "    for id in stale.keys() {" "    for id in Vec::<String>::new().iter() {"
mutate sync_suite "$sync" "    kind == crate::turns::TURN_KIND || kind == DOC_KIND" "    kind == crate::turns::TURN_KIND"
mutate sync_suite "$sync" "    for id in existing(pool, folder).await?.keys() {" "    for id in existing(pool, \"\").await?.keys() {"
ts="$root/crates/ax-web/web-ui/src/vaultMount.ts"
mutate ts_suite "$ts" "  if (!f.index) return 'Not indexed';" ""
cat="$root/crates/ax-web/web-ui/src/memoryCategory.ts"
mutate ts_suite "$cat" "  doc: 'doc',
" ""

echo "mutants killed: $killed/$total"
[ "$killed" -eq "$total" ]
