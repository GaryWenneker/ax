#!/usr/bin/env bash
# Manual mutation run for docs/specs/install-daemon-hygiene.md. Each mutant must apply exactly once
# (else the run fails) and must make the tests of its group fail. Files are restored from the git
# index after each mutant; stage the change under test first.
# Mutants in the daemon wiring (spawning, identity fields, the exe watch) are killed by the
# real-process tests in crates/ax-cli/tests/daemon_lifecycle.rs, so the `mcp` group runs those too.
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
cd "$ROOT"

CMDS=crates/ax-cli/src/commands/mod.rs
SYNC=crates/ax-cli/src/commands/sync.rs
SHIP=crates/ax-cli/src/commands/ship.rs
INIT=crates/ax-cli/src/commands/init.rs
SHARE=crates/ax-policy/src/agents_share.rs
AXCMD=crates/ax-installer/src/ax_command.rs
TARGETS=crates/ax-installer/src/targets.rs
EXE=crates/ax-mcp/src/exe_identity.rs
DAEMON=crates/ax-mcp/src/daemon.rs
PROXY=crates/ax-mcp/src/proxy.rs
PUMP=crates/ax-mcp/src/proxy_pump.rs

for f in "$CMDS" "$SYNC" "$SHIP" "$INIT" "$SHARE" "$AXCMD" "$TARGETS" "$EXE" "$DAEMON" "$PROXY" "$PUMP"; do
  if ! git diff --quiet -- "$f"; then
    echo "mutants: $f has unstaged changes; stage them first" >&2
    exit 1
  fi
done

cargo_() { env -u CARGO_TARGET_DIR cargo "$@"; }
test_group() { # group -> exit status of the tests that guard it
  case "$1" in
    cli) cargo_ test -q -p ax-cli --test hooks_without_ax --test init_gitignore >/dev/null 2>&1 ;;
    policy) cargo_ test -q -p ax-policy --lib agents_share >/dev/null 2>&1 ;;
    installer) cargo_ test -q -p ax-installer >/dev/null 2>&1 ;;
    mcp)
      cargo_ test -q -p ax-mcp --lib >/dev/null 2>&1 &&
        cargo_ test -q -p ax-cli --test daemon_lifecycle >/dev/null 2>&1
      ;;
    *) echo "mutants: unknown group $1" >&2; exit 1 ;;
  esac
}

CHECK_ONLY="${MUTANTS_CHECK_ONLY:-0}" # 1: only check that each pattern applies once; not evidence
for group in cli policy installer mcp; do
  [ "$CHECK_ONLY" = 1 ] && break
  if ! test_group "$group"; then
    echo "mutants: baseline tests of $group fail on the unmutated tree; a kill would mean nothing" >&2
    exit 1
  fi
done

killed=0
total=0

# ax's post-checkout hook may run an older ax; a file restore needs no hooks.
restore() { git -c core.hooksPath=/dev/null checkout -q -- "$1"; }

# mutant <id> <group> <file> <perl-substitution>; the substitution must change the file exactly once.
ONLY="${MUTANTS_ONLY:-}" # e.g. "C2 E10": run just these; a partial run is not evidence
mutant() {
  local id="$1" group="$2" file="$3" expr="$4"
  if [ -n "$ONLY" ] && [[ " $ONLY " != *" $id "* ]]; then
    return
  fi
  total=$((total + 1))
  local count
  count="$(perl -0ne "\$n = (${expr}g); print \$n || 0" "$file")"
  if [ "$count" != 1 ]; then
    echo "mutants: $id matches $count times in $file, expected exactly 1" >&2
    exit 1
  fi
  if [ "$CHECK_ONLY" = 1 ]; then
    echo "applies  $id"
    return
  fi
  perl -0pi -e "$expr" "$file"
  if git diff --quiet -- "$file"; then
    restore "$file"
    echo "mutants: $id did not change $file" >&2
    exit 1
  fi
  if test_group "$group"; then
    echo "SURVIVED $id ($file)"
  else
    echo "killed   $id"
    killed=$((killed + 1))
  fi
  restore "$file"
  if [ -n "$(git diff -- "$file")" ]; then
    echo "mutants: $file not restored" >&2
    exit 1
  fi
}

# A. Quiet hooks in a tree without .ax/
mutant A1 cli "$CMDS" 's/    quiet && !ax_context::directory::is_initialized\(root\)/    false/'
mutant A2 cli "$SHIP" 's/if evaluate && quiet_and_uninitialized\(&root, quiet\)/if false \&\& quiet_and_uninitialized(\&root, quiet)/'
mutant A3 cli "$CMDS" 's/    quiet && !ax_context::directory::is_initialized\(root\)/    !ax_context::directory::is_initialized(root)/'
mutant A4 cli "$SYNC" 's/    if quiet_and_uninitialized\(&root, quiet\) \{\n        return Ok\(\(\)\);\n    \}\n//'

# B. .ax/.gitignore
mutant B1 cli "$INIT" 's/    ax_policy::ensure_ax_share_gitignore\(&root\)\.map_err\(\|e\| format!\("\.ax\/\.gitignore: \{e\}"\)\)\?;\n//'
mutant B2 policy "$SHARE" 's/&\["\*", "!\.gitignore", "!policy\/", "!policy\/\*\*"\]/\&["*", "!.gitignore", "!policy\/**"]/'
mutant B3 policy "$SHARE" 's/content = format!\("\{\}\\n\{content\}", missing\.join\("\\n"\)\);/content = format!("{content}\\n{}", missing.join("\\n"));/'
mutant B4 policy "$SHARE" 's/\.filter\(\|line\| !content\.lines\(\)\.any\(\|l\| l\.trim\(\) == \*line\)\)/.filter(|_| true)/'

# C. The installer's ax path and the Claude hook
mutant C1 installer "$AXCMD" 's/    match path_env\.and_then\(first_on_path\) \{/    match None::<PathBuf> {/'
mutant C2 installer "$AXCMD" 's/        \.filter\(\|dir\| dir\.is_absolute\(\)\)\n//'
mutant C3 installer "$AXCMD" 's/Some\(running\) if !same_file\(&self\.path, running\) =>/Some(running) if false =>/'
mutant C4 installer "$AXCMD" 's/        if !self\.on_path \{\n            return Some/        if self.on_path \&\& false {\n            return Some/'
mutant C5 installer "$TARGETS" 's/        Some\(entry\) => entry\["command"\] = Value::String\(hook_cmd\),/        Some(_) => groups.push(serde_json::json!({ "hooks": [{ "type": "command", "command": hook_cmd }] })),/'
mutant C6 installer "$TARGETS" 's/    if existed && json_equal\(&before, value\) \{/    if false {/'

# D. Proxy reconnect
mutant D1 mcp "$PUMP" 's/    for id in pending\.drain\(\.\.\) \{/    for id in pending.drain(..).take(0) {/'
mutant D2 mcp "$PUMP" 's/"error": \{ "code": -32000, "message": DAEMON_RESTARTED \}/"error": { "code": -32603, "message": DAEMON_RESTARTED }/'
mutant D3 mcp "$PUMP" 's/        \*unsent = Some\(line\);/        let _ = line;/'
mutant D4 mcp "$PUMP" 's/msg\.get\("id"\)\.filter\(\|id\| !id\.is_null\(\)\)\.cloned\(\)/Some(msg.get("id").cloned().unwrap_or(Value::Null))/'
mutant D5 mcp "$PUMP" 's/            pending\.retain\(\|p\| p != id\);/            let _ = id;/'
mutant D6 mcp "$PUMP" 's/        if now >= give_up \{\n            break;\n        \}\n//'
mutant D7 mcp "$PUMP" 's/    let _ = daemon_tx\.shutdown\(\)\.await;\n//'
mutant D8 mcp "$PUMP" 's/Ok\(\(\)\) \| Err\(Stop::ClientLeft\) => Ok\(\(\)\),/Ok(()) => Ok(()),\n        Err(Stop::ClientLeft) => Err("client left".into()),/'
mutant D9 mcp "$PUMP" 's/if msg\.get\("type"\)\.and_then\(\|t\| t\.as_str\(\)\) == Some\("hello"\) && msg\.get\("jsonrpc"\)\.is_none\(\)/if false/'
mutant D10 mcp "$PROXY" 's/attempt >= FIRST_SPAWNING_ATTEMPT/false/'
mutant D11 mcp "$PUMP" 's/if let Some\(session\) = reconnect\(attempt\)\.await \{/if let Some(session) = None::<DaemonSession> { let _ = \&mut reconnect;/'

# E. Binary identity and newer-wins attach
mutant E1 mcp "$EXE" 's/\(Some\(m\), Some\(d\)\) if m\.mtime_ms > d\.mtime_ms => Attach::RestartOnMine/(Some(m), Some(d)) if m.mtime_ms < d.mtime_ms => Attach::RestartOnMine/'
mutant E2 mcp "$EXE" 's/\(Some\(m\), Some\(d\)\) if m == d => Attach::Same,/(Some(m), Some(d)) if m.path == d.path => Attach::Same,/'
mutant E3 mcp "$EXE" 's/\(_, None\) if my_version != daemon_version => Attach::RestartOnMine,\n//'
mutant E4 mcp "$EXE" 's/Self::of\(Path::new\(&self\.path\)\)\.as_ref\(\) != Some\(self\)/Self::of(Path::new(\&self.path)).is_none()/'
mutant E5 mcp "$DAEMON" 's/if !me\.is_stopping\(\) && exe\.replaced_on_disk\(\)/if !me.is_stopping() \&\& false/'
mutant E6 mcp "$DAEMON" 's/exe: daemon_exe\(\),\n    \};/exe: None,\n    };/'
mutant E7 mcp "$DAEMON" 's/exe: daemon_exe\(\),\n    \}\n\}/exe: None,\n    }\n}/'
mutant E8 mcp "$PROXY" 's/Attach::RestartOnMine if may_spawn =>/Attach::RestartOnMine if false =>/'
mutant E9 mcp "$EXE" 's/match text\.strip_suffix\(" \(deleted\)"\)/match None::<\&str>/'
mutant E10 mcp "$EXE" 's/        \.unwrap_or\(DEFAULT_EXE_CHECK_MS\)/        .unwrap_or(0)/'

if [ "$CHECK_ONLY" = 1 ]; then
  echo "mutants: all $total patterns apply exactly once (check only, no tests run: not evidence)"
  exit 1
fi
echo "mutants: $killed/$total killed"
if [ -n "$ONLY" ]; then
  echo "PARTIAL RUN ($ONLY): not evidence"
  exit 1
fi
[ "$killed" -eq "$total" ]
