#!/usr/bin/env bash
# Manual mutants for the folder picker and IDE install (docs/specs/folder-picker-and-ide-install.md). Each
# mutant must change the file and must make a test in its suite fail
# (scripts/lib/mutate.sh).
set -euo pipefail
root="$(cd "$(dirname "$0")/.." && pwd)"
source "$root/scripts/lib/mutate.sh"

picker_suite() { (cd "$root" && cargo test -q -p ax-web --lib folder_picker && cargo test -q -p ax-web --test folder_picker); }
agent_suite() { (cd "$root" && cargo test -q -p ax-web --test agent_install_guard); }
ts_suite() { (cd "$root/crates/ax-web/web-ui" && node --test src/vaultMount.test.ts src/ideInstall.test.ts); }

pk="$root/crates/ax-web/src/folder_picker.rs"
ax_mutate picker_suite "$pk" "    if !crate::dav::mount::allowed(&headers, hub.readonly, hub.port) {" "    if false {"
ax_mutate picker_suite "$pk" "    if OPEN.swap(true, Ordering::SeqCst) {" "    if false {"
ax_mutate picker_suite "$pk" "        if !out.status.success() {" "        if false {"
ax_mutate picker_suite "$pk" "            continue;
        };" "            return Ok(None);
        };"
ax_mutate picker_suite "$pk" "    let is_root = line == \"/\" || (line.len() == 3 && line.ends_with(\":\\\\\"));" "    let is_root = line == \"/\";"
ax_mutate picker_suite "$pk" "    let path = if is_root {
        line
    } else {
        line.trim_end_matches(['/', '\\\\'])
    };" "    let path = line;"
ax_mutate picker_suite "$pk" "\"-STA\"," ""
ax_mutate picker_suite "$pk" "            own(
                \"kdialog\",
                &[\"--getexistingdirectory\", \".\", \"--title\", PROMPT],
            )," ""
ag="$root/crates/ax-web/src/agent.rs"
ax_mutate agent_suite "$ag" "    } else if !crate::dav::mount::allowed(headers, false, hub.port) {" "    } else if false {"
ax_mutate agent_suite "$ag" "    let error = if hub.readonly {" "    let error = if false {"
ax_mutate agent_suite "$ag" "            \"results\": reports.iter().map(report_json).collect::<Vec<_>>()," ""
ide="$root/crates/ax-web/web-ui/src/ideInstall.ts"
ax_mutate ts_suite "$ide" "  if (t.configured) return 'connected';" ""
ax_mutate ts_suite "$ide" "  return targets.filter((t) => ideState(t) === 'found').map((t) => t.id);" "  return targets.filter((t) => t.detected).map((t) => t.id);"
ax_mutate ts_suite "$ide" "      const files = r.files.length ? r.files.join(', ') : 'already up to date';" "      const files = r.files.join(', ');"
vm="$root/crates/ax-web/web-ui/src/vaultMount.ts"
ax_mutate ts_suite "$vm" "  if (/^[A-Za-z]:$/.test(last)) return '';" ""
ax_mutate ts_suite "$vm" "  const name = last.replace(/[^A-Za-z0-9 _-]/g, '-').slice(0, 32).trim();" "  const name = last.slice(0, 32).trim();"

ax_mutants_summary
