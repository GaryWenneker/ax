#!/usr/bin/env bash
# Manual mutants for choosing IDEs in ax init (docs/specs/init-ide-selection.md and
# docs/specs/ide-detection-and-removal.md). Each
# mutant must change the file (proved by cmp) and must make its suite fail.
set -euo pipefail
root="$(cd "$(dirname "$0")/.." && pwd)"
killed=0
total=0

choice_suite() { (cd "$root" && cargo test -q -p ax-installer --lib ide_choice); }
config_suite() { (cd "$root" && cargo test -q -p ax-policy --lib project_ides); }
init_suite() { (cd "$root" && cargo test -q -p ax-cli --test init_ides); }
detect_suite() { (cd "$root" && cargo test -q -p ax-installer --lib detect); }
menu_suite() { (cd "$root" && cargo test -q -p ax-cli --bin ax stack_menu_text); }

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


ch="$root/crates/ax-installer/src/ide_choice.rs"
mutate choice_suite "$ch" "        Some(list) => list.to_vec()," "        Some(list) if !list.is_empty() => list.to_vec(),
        Some(_) => detected.to_vec(),"
mutate choice_suite "$ch" "        if !chosen.contains(&id) {" "        if true {"
mutate choice_suite "$ch" "        if !is_known(&id) {" "        if false {"
mutate choice_suite "$ch" "        .filter(|id| is_known(id) && !chosen.contains(id))" "        .filter(|id| !chosen.contains(id))"
mutate choice_suite "$ch" "    editors.chain(agents).collect()" "    agents.chain(editors).collect()"
cf="$root/crates/ax-policy/src/config.rs"
mutate config_suite "$cf" "    let list = root.get(\"agents\")?.get(\"ides\")?.as_array()?;" "    let list = root.get(\"agents\")?.get(\"ides\")?.as_array().filter(|l| !l.is_empty())?;"
mutate config_suite "$cf" "    agents[\"ides\"] = serde_json::json!(ides);" "    *agents = serde_json::json!({ \"ides\": ides });"
it="$root/crates/ax-cli/src/commands/init.rs"
mutate init_suite "$it" "        ax_policy::write_project_ides(root, &choice.chosen)?;" ""
mutate init_suite "$it" "        return Ok(IdeChoice { chosen: defaults, asked: false });" "        return Ok(IdeChoice { chosen: defaults, asked: true });"
mutate init_suite "$it" "            .filter(|s| s.configured)" "            .filter(|_| true)"
mutate init_suite "$it" "        let dropped = ax_installer::ides_to_remove(&configured, &choice.chosen);" "        let dropped = ax_installer::ides_to_remove(&[], &choice.chosen);"
mutate init_suite "$it" "            for report in ax_installer::uninstall_targets(root, &dropped)? {" "            for report in ax_installer::uninstall_targets(root, &[])? {"
mutate init_suite "$it" "    if choice.chosen.is_empty() {" "    if false {"
mutate init_suite "$it" "            eprintln!(\"{}\", dim(format!(\"Ignoring unknown saved IDE(s): {}\", unknown.join(\", \"))));" ""
mutate init_suite "$it" "        .filter(|id| ax_installer::is_detected(id))" ""
mutate init_suite "$it" "        let chosen = ax_installer::parse_ide_choice(&answer, &defaults)?;" "        let chosen = ax_installer::parse_ide_choice(&answer, &defaults).unwrap_or_default();"
mutate init_suite "$it" "                    .filter(|p| seen.insert(p.clone()))" "                    .filter(|p| { seen.insert(p.clone()); true })"
mutate menu_suite "$it" "            last_group = &item.group;" ""
# docs/specs/claude-disconnect-sticks.md (D1, D3)
tg="$root/crates/ax-installer/src/targets.rs"
mutate init_suite "$tg" "    for path in [home.join(\".claude.json\"), project_root.join(\".mcp.json\")] {" "    for path in [home.join(\".claude.json\")] {"
mutate choice_suite "$ch" "        .filter(|id| saved.is_none_or(|list| list.contains(id)))" "        .filter(|_| true)"
mutate choice_suite "$ch" "saved.is_none_or(|list| list.contains(id))" "saved.is_some_and(|list| list.contains(id))"
# docs/specs/ide-detection-and-removal.md (F2)
dt="$root/crates/ax-installer/src/detect.rs"
mutate detect_suite "$dt" "[PathBuf::from(\"/Applications\"), places.home.join(\"Applications\")]" "[PathBuf::from(\"/Applications\")]"
mutate detect_suite "$dt" "[PathBuf::from(\"/Applications\"), places.home.join(\"Applications\")]" "[places.home.join(\"Applications\")]"
mutate detect_suite "$dt" "local.iter().chain(global.iter())" "local.iter()"
mutate detect_suite "$dt" "        Os::Linux => bins.iter().any(|b| probe.on_path(b)) || " "        Os::Linux => "
mutate detect_suite "$dt" "|| dirs.iter().any(|d| probe.exists(Path::new(d)))," ","
mutate detect_suite "$dt" "[\".vscode\", \".cursor\"]" "[\".vscode\"]"
mutate detect_suite "$dt" "&[\"zed\", \"zeditor\"]" "&[\"zed\"]"
mutate detect_suite "$dt" ".any(|n| n.starts_with(\"continue.continue-\"))" ".any(|n| !n.is_empty())"
mutate detect_suite "$dt" "        _ => return None," "        _ => (\"Cursor.app\", \"cursor/Cursor.exe\", &[\"cursor\"], &[]),"

echo "mutants killed: $killed/$total"
[ "$killed" -eq "$total" ]
