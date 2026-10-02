#!/usr/bin/env bash
# Session context layer gauntlet (docs/specs/session-context-layer.md). One entry point for every
# layer in docs/specs/session-context-layer-EVIDENCE.md. Fails closed: any layer error stops the run.
#
#   scripts/session-layer-gauntlet.sh             all layers
#   LAYERS="cli-tests hook" scripts/session-layer-gauntlet.sh
#
# Env: BASE (default f9f6362, the commit before the spec), REPEAT (default 3), plus the
#      context-cache-gauntlet.sh variables for the shared layers.
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
cd "$ROOT"
export CARGO_TARGET_DIR="${CARGO_TARGET_DIR:-$ROOT/target-dev}"
export PYTHONDONTWRITEBYTECODE=1
export BASE="${BASE:-f9f6362}"
export REPEAT="${REPEAT:-3}"
LAYERS="${LAYERS:-shared cli-tests cli-clippy mutants manual-mutants session hook}"
OUT="$ROOT/target-dev/session-layer-gauntlet"
BIN="$CARGO_TARGET_DIR/debug/ax"
mkdir -p "$OUT"

layer() { printf '\n=== %s ===\n' "$1"; }

run_shared() {
  layer "shared: versions, tests x$REPEAT, clippy, changed-line coverage, audit (base $BASE)"
  LAYERS="versions tests clippy coverage audit" scripts/context-cache-gauntlet.sh
}

run_cli_tests() {
  layer "ax-cli turn_hook tests (x$REPEAT)"
  for i in $(seq 1 "$REPEAT"); do
    cargo test -p ax-cli turn_hook 2>&1 | tee "$OUT/cli-tests-$i.log" | grep -E '^test result' \
      | awk '{p+=$4; f+=$6} END {print "run '"$i"': passed=" p " failed=" f; if (f>0 || p==0) exit 1}'
  done
}

run_cli_clippy() {
  layer "ax-cli clippy: only the known baseline error (upgrade.rs unused import, from bcd87a0)"
  cargo clippy -p ax-cli --all-targets -- -D warnings >"$OUT/cli-clippy.log" 2>&1 && { echo "clippy clean"; return; }
  python3 - "$OUT/cli-clippy.log" <<'PY'
import re, sys
log = open(sys.argv[1], encoding="utf-8").read()
errors = re.findall(r"^error(?:\[\w+\])?: (.+)\n\s*--> ([^:\n]+)", log, re.M)
if not errors:
    sys.exit("clippy failed without a parseable error:\n" + log[-3000:])
unexpected = [e for e in errors if not (e[1].endswith("commands/upgrade.rs") and "unused import" in e[0])]
for msg, path in errors:
    print(("  baseline " if (msg, path) not in unexpected else "  NEW      ") + f"{path}: {msg}")
sys.exit(1 if unexpected else 0)
PY
}

run_mutants() {
  layer "cargo-mutants on the new session code"
  cargo mutants --package ax-mcp --file crates/ax-mcp/src/chat_session.rs \
    --output "$OUT/chat_session" --no-shuffle -- --lib
  cargo mutants --package ax-usage --file crates/ax-usage/src/working_context.rs \
    --output "$OUT/working_context" --no-shuffle -- --lib
  cargo mutants --package ax-usage --file crates/ax-usage/src/cursor_state.rs \
    -F 'session_if_recent|read_recent_cursor_session' --output "$OUT/cursor_state" --no-shuffle -- --lib
  cargo mutants --package ax-usage --file crates/ax-usage/src/reuse_cache.rs \
    -F 'session_from_args|canonical_args' --output "$OUT/reuse_cache" --no-shuffle -- --lib
}

run_manual_mutants() {
  layer "manual mutants (resolution, preflight, nudge, bounds)"
  python3 - "$ROOT" <<'PY'
import subprocess, sys
root = sys.argv[1]
MUTANTS = [
    ("preflight reuses the connection's chat", "crates/ax-mcp/src/chat_session.rs",
     "(CallKind::Tool, Some(id)) => id.to_string(),", "(_, Some(id)) => id.to_string(),",
     ["-p", "ax-mcp", "chats_on_one_daemon_without_a_hook_file_stay_apart"]),
    ("the session argument is ignored", "crates/ax-mcp/src/server.rs",
     "        ax_usage::session_from_args(args),\n", "        None,\n",
     ["-p", "ax-mcp", "the_session_argument_beats_the_hook_file"]),
    ("preflight ignores known_context", "crates/ax-mcp/src/tools.rs",
     'let known = params.get("known_context").and_then(Value::as_str);',
     'let known: Option<&str> = None;',
     ["-p", "ax-mcp", "preflight_sends_the_notes_once_per_change"]),
    ("preflight never nudges", "crates/ax-mcp/src/tools.rs",
     "if let Some(nudge) = ax_usage::session_nudge(turns, stale) {",
     "if let Some(nudge) = ax_usage::session_nudge(turns, stale).filter(|_| false) {",
     ["-p", "ax-mcp", "preflight_nudges_after_five_quiet_turns_and_for_stale_notes"]),
    ("a write does not restart the turn count", "crates/ax-mcp/src/server.rs",
     "        engine.turns().on_write(&conversation);\n", "",
     ["-p", "ax-mcp", "preflight_nudges_after_five_quiet_turns_and_for_stale_notes"]),
    ("preflight hides the graph version", "crates/ax-mcp/src/tools.rs",
     "inject.push_str(&chat_line(&conversation, graph.as_deref()));",
     "inject.push_str(&chat_line(&conversation, graph.as_deref().filter(|_| false)));",
     ["-p", "ax-mcp", "preflight_shows_the_graph_version_and_it_moves_with_the_index"]),
    ("writes never evict", "crates/ax-usage/src/working_context.rs",
     "            evict(pool, root, &key).await?;\n", "",
     ["-p", "ax-usage", "working_context"]),
    ("writes without an index are accepted", "crates/ax-usage/src/working_context.rs",
     'if fingerprint.is_empty() && matches!(action, "add" | "update" | "compact") {',
     'if false && matches!(action, "add" | "update" | "compact") {',
     ["-p", "ax-usage", "writes_need_a_readable_index_and_leave_the_snapshot_unchanged"]),
]
killed = 0
for label, path, old, new, test in MUTANTS:
    full = f"{root}/{path}"
    src = open(full, encoding="utf-8").read()
    if src.count(old) != 1:
        sys.exit(f"mutant '{label}': expected exactly one match in {path}, found {src.count(old)}")
    open(full, "w", encoding="utf-8").write(src.replace(old, new))
    try:
        if open(full, encoding="utf-8").read() == src:
            sys.exit(f"mutant '{label}' was not applied")
        done = subprocess.run(["cargo", "test", *test], cwd=root, capture_output=True, text=True)
        ran = any(l.startswith("test result") and " 0 passed; 0 failed" not in l for l in done.stdout.splitlines())
        if "error[E" in done.stderr:
            sys.exit(f"mutant '{label}' did not compile:\n{done.stderr[-2000:]}")
        if done.returncode == 0:
            print(f"SURVIVED  {label}")
        elif not ran:
            sys.exit(f"mutant '{label}': no test ran\n{done.stdout[-2000:]}")
        else:
            killed += 1
            print(f"killed    {label}")
    finally:
        open(full, "w", encoding="utf-8").write(src)
    if subprocess.run(["git", "diff", "--quiet", "--", path], cwd=root).returncode != 0:
        sys.exit(f"restore failed for {path}")
print(f"manual mutants killed: {killed}/{len(MUTANTS)}")
if killed != len(MUTANTS):
    sys.exit(1)
PY
}

run_session() {
  layer "L5/L6/L5S/L6S real-binary sessions (x$REPEAT) + negative control"
  cargo build -p ax-cli
  for i in $(seq 1 "$REPEAT"); do
    AX_BIN="$BIN" python3 scripts/bench-agent-efficiency/reuse_session.py --out "$OUT/session-$i.json" >/dev/null
    python3 -c 'import json,sys; d=json.load(open(sys.argv[1])); s=d["L5S"]; print(f"run {sys.argv[2]}: L5 hits {d["L5"]["hits"]}/{d["L5"]["repeats"]}; L5S hits {s["hits"]}, not stored {s["not_stored"]}, preflight {sum(s["preflight_tokens_per_turn_without"])}->{sum(s["preflight_tokens_per_turn_with_known_context"])} (saved {s["known_context_saved_tokens"]}); L6S stale block {d["L6S"]["stale_block_tokens"]}, confirmed {d["L6S"]["confirmed_preflight_tokens"]}")' "$OUT/session-$i.json" "$i"
  done
  if AX_BIN="$BIN" python3 scripts/bench-agent-efficiency/reuse_session.py --control-no-edit --out "$OUT/session-control.json" >/dev/null; then
    echo "negative control passed: L6 cannot detect a stale hit"; exit 1
  fi
  echo "negative control failed as required:"
  python3 -c 'import json,sys; [print("  ", f) for f in json.load(open(sys.argv[1]))["failures"]]' "$OUT/session-control.json"
}

run_hook() {
  layer "turn-hook start real execution + rebuilt mutant"
  cargo build -p ax-cli
  AX_BIN="$BIN" scripts/turn-hook-session-check.sh
  local src=crates/ax-cli/src/commands/turn_hook.rs
  local write='let _ = ax_usage::write_active_cursor_session(&chat);'
  [ "$(grep -cF "$write" "$src")" = 1 ] || { echo "hook mutant: anchor not found"; exit 1; }
  cp "$src" "$OUT/turn_hook.rs.orig"
  trap 'cp "$OUT/turn_hook.rs.orig" crates/ax-cli/src/commands/turn_hook.rs' EXIT
  python3 -c 'import sys; p,a=sys.argv[1],sys.argv[2]; s=open(p).read(); open(p,"w").write(s.replace(a,"let _ = &chat;"))' "$src" "$write"
  cargo build -p ax-cli
  if AX_BIN="$BIN" scripts/turn-hook-session-check.sh >"$OUT/hook-mutant.log" 2>&1; then
    echo "mutant 'turn hook never writes' survived"; exit 1
  fi
  cp "$OUT/turn_hook.rs.orig" "$src"; trap - EXIT
  git diff --quiet -- "$src" || { echo "restore failed for $src"; exit 1; }
  echo "mutant 'turn hook never writes' killed: $(tail -n1 "$OUT/hook-mutant.log")"
  cargo build -p ax-cli
}

for l in $LAYERS; do
  "run_${l//-/_}"
done
printf '\nGAUNTLET PASSED: %s\n' "$LAYERS"
