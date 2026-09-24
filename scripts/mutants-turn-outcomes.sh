#!/usr/bin/env bash
# Manual mutation run for docs/specs/turn-memory-outcomes.md (outcomes, turn history, ax_history,
# prune backups). Wiring without a unit test — the `response` phase dispatch in `turn_hook::run` and
# `ax history` — is covered by the real-execution layer of scripts/gauntlet-turn-outcomes.sh.
# Each mutant must apply exactly once (else the run fails) and must make its crate's tests fail.
# Files are restored from the git index after each mutant; stage the change under test first.
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
cd "$ROOT"

TURN=crates/ax-cli/src/commands/turn_hook.rs
HIST=crates/ax-memory/src/history.rs
TURNS=crates/ax-memory/src/turns.rs
STORE=crates/ax-memory/src/store.rs
TOOLS=crates/ax-mcp/src/tools.rs
FILTER=crates/ax-mcp/src/tool_filter.rs
HOOKS=crates/ax-installer/src/hooks.rs

for f in "$TURN" "$HIST" "$TURNS" "$STORE" "$TOOLS" "$FILTER" "$HOOKS"; do
  if ! git diff --quiet -- "$f"; then
    echo "mutants: $f has unstaged changes; stage them first" >&2
    exit 1
  fi
done

test_crate() { # crate -> exit status of its tests (ax-cli: the binary's unit tests)
  if [ "$1" = ax-cli ]; then
    env -u CARGO_TARGET_DIR cargo test -q -p ax-cli --bin ax >/dev/null 2>&1
  elif [ "$1" = ax-mcp ]; then
    # tests/new_tools_smoke.rs needs the repo's own .ax/ and fails in a fresh worktree.
    env -u CARGO_TARGET_DIR cargo test -q -p ax-mcp --lib --test catalog_payload_size >/dev/null 2>&1
  else
    env -u CARGO_TARGET_DIR cargo test -q -p "$1" >/dev/null 2>&1
  fi
}

for crate in ax-cli ax-memory ax-mcp ax-installer; do
  if ! test_crate "$crate"; then
    echo "mutants: baseline tests of $crate fail on the unmutated tree; a kill would mean nothing" >&2
    exit 1
  fi
done

killed=0
total=0

# ax's post-checkout hook fails in a tree without .ax/; a file restore needs no hooks.
restore() { git -c core.hooksPath=/dev/null checkout -q -- "$1"; }

# mutant <id> <crate> <file> <perl-substitution>; the substitution must change the file.
mutant() {
  local id="$1" crate="$2" file="$3" expr="$4"
  total=$((total + 1))
  local before after
  before="$(shasum "$file" | cut -d' ' -f1)"
  perl -0pi -e "$expr" "$file"
  after="$(shasum "$file" | cut -d' ' -f1)"
  if [ "$before" = "$after" ]; then
    restore "$file"
    echo "mutants: $id did not apply to $file" >&2
    exit 1
  fi
  if test_crate "$crate"; then
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

# Outcome capture (ax-cli turn hook)
mutant O1 ax-cli "$TURN" 's/    if input\.event\.as_deref\(\) == Some\("SubagentStop"\) \{\n        return;\n    \}\n//'
mutant O2 ax-cli "$TURN" 's/text\("text"\)\.or_else\(\|\| text\("last_assistant_message"\)\)/text("text")/'
mutant O3 ax-cli "$TURN" 's/reply\.map\(outcome_text\)\.or\(snapshot\.outcome\)/snapshot.outcome.or(reply.map(outcome_text))/'
mutant O4 ax-cli "$TURN" 's/let redacted = ax_memory::redact_secrets\(reply\);/let redacted = reply.to_string();/'
mutant O5 ax-cli "$TURN" 's/const OUTCOME_CHARS: usize = 20_000;/const OUTCOME_CHARS: usize = 30_000;/'
mutant O6 ax-cli "$TURN" 's/        outcome: None,\n    \};/        outcome: read_snapshot(root, \&input.conversation).and_then(|s| s.outcome),\n    };/'
mutant O7 ax-cli "$TURN" 's/outcome\.filter\(\|o\| !o\.is_empty\(\)\)/outcome/'
mutant O9 ax-cli "$TURN" 's/ax_memory::redact_secrets\(&body\)\.replace\(ax_memory::OUTCOME_MARKER, "\\n\\nOutcome - "\)/ax_memory::redact_secrets(\&body)/'
mutant O8 ax-cli "$TURN" 's/format!\("\{\}\[truncated\]", clip\(&redacted, OUTCOME_CHARS\)\)/clip(\&redacted, OUTCOME_CHARS)/'

# Prune with backup (ax-memory)
mutant P1 ax-memory "$TURNS" 's/pub const TURN_RETENTION_DAYS: i64 = 90;/pub const TURN_RETENTION_DAYS: i64 = 30;/'
mutant P2 ax-memory "$TURNS" 's/    backup_turns\(backup_dir, now_ms, &expired\)\?;\n//'
mutant P3 ax-memory "$TURNS" 's/    backup_turns\(backup_dir, now_ms, &expired\)\?;/    let _ = backup_turns(backup_dir, now_ms, \&expired);/'
mutant P4 ax-memory "$TURNS" 's/\.append\(true\)/.write(true).truncate(true)/'
mutant P5 ax-memory "$TURNS" 's/    if expired\.is_empty\(\) \{\n        return Ok\(0\);\n    \}\n//'
mutant P6 ax-memory "$TURNS" 's/Some\(outcome\)\.filter\(\|o\| !o\.is_empty\(\)\)/Some(outcome)/'
mutant P7 ax-memory "$STORE" 's/WHERE kind = \? AND created_at < \?/WHERE (kind = ? OR 1) AND created_at < ?/'
mutant P8 ax-memory "$TURNS" 's/    tx\.commit\(\)\.await\.map_err\(db_err\)\?;\n//'

# Related turns and the preflight block (ax-memory)
mutant R1 ax-memory "$HIST" 's/row\.files\.iter\(\)\.any\(\|f\| files\.contains\(f\)\)\n(\s*)\|\|/false\n$1||/'
mutant R2 ax-memory "$HIST" 's/const MIN_SHARED_WORDS: usize = 2;/const MIN_SHARED_WORDS: usize = 1;/'
mutant R3 ax-memory "$STORE" 's/WHERE kind = \? AND enabled = 1 AND/WHERE kind = ? AND (enabled = 1 OR 1) AND/'
mutant R4 ax-memory "$HIST" 's/        \.take\(limit\)\n//'
mutant R9 ax-memory "$HIST" 's/let candidates = limit\.saturating_add\(recalled\.len\(\)\);/let candidates = limit;/'
mutant R10 ax-memory "$STORE" 's/WHERE f\.value IN \(SELECT value FROM json_each\(\?\)\)/WHERE (0 AND f.value IN (SELECT value FROM json_each(?)))/'
mutant R5 ax-memory "$HIST" 's/        if len > max_chars \{/        if false {/'
mutant R6 ax-memory "$HIST" 's/const BLOCK_OUTCOME_CHARS: usize = 120;/const BLOCK_OUTCOME_CHARS: usize = 1_000;/'
mutant R7 ax-memory "$HIST" 's/const BLOCK_FILES: usize = 3;/const BLOCK_FILES: usize = 4;/'
mutant R8 ax-memory "$HIST" 's/    "wanneer heb ik",\n//'

# History query (ax-memory)
mutant H1 ax-memory "$HIST" 's/"SELECT path FROM files WHERE path = \? OR path LIKE \? ESCAPE/"SELECT path FROM files WHERE path = ? AND path LIKE ? ESCAPE/'
mutant H2 ax-memory "$HIST" 's/    if paths\.is_empty\(\) && !query\.contains\(char::is_whitespace\) \{/    if false {/'
mutant H3 ax-memory "$HIST" 's/        found\.extend\(git_commits\(root, &pathspecs, query\.since_ms, limit\)\.await\);\n//'
mutant H11 ax-memory "$HIST" 's/\n\s*\.filter\(\|entry\| since_ms\.is_none_or\(\|since\| entry\.at_ms >= since\)\)//'
mutant H5 ax-memory "$STORE" 's/\.bind\(since\.unwrap_or\(i64::MIN\)\)/.bind(i64::MIN)/'
mutant H10 ax-memory "$STORE" 's/format!\("%\/\{escaped\}"\)/format!("{escaped}")/'
mutant H6 ax-memory "$HIST" 's/shared_words\(&words, row\) >= 1\)/shared_words(\&words, row) >= 0)/'
mutant H7 ax-memory "$HIST" 's/clip\(outcome, outcome_chars\)/outcome.clone()/'
mutant H8 ax-memory "$HIST" 's/        \.filter\(\|row\| row\.kind == TURN_KIND\)\n        \.map\(\|row\| turn_entry\(&row\)\)\)/        .map(|row| turn_entry(\&row)))/'
mutant H9 ax-memory "$HIST" 's/entries\.sort_by_key\(\|e\| std::cmp::Reverse\(e\.at_ms\)\);/entries.sort_by_key(|e| e.at_ms);/'

# MCP (ax-mcp)
mutant C1 ax-mcp "$TOOLS" 's/        inject\.push_str\(&turn_block\);\n//'
mutant C2 ax-mcp "$TOOLS" 's/tokio::fs::canonicalize\(path\)\.await/tokio::fs::canonicalize("\/nonexistent-ax-mutant").await/'
mutant C3 ax-mcp "$TOOLS" 's/    if ax_memory::is_history_question\(&prompt\) \{/    if true {/'
mutant C4 ax-mcp "$TOOLS" 's/Some\(date\) => Some\(ax_memory::parse_since\(date\)\.ok_or\("since must be YYYY-MM-DD"\)\?\),/Some(_) => None,/'
mutant C5 ax-mcp "$TOOLS" 's/const HISTORY_OUTCOME_CHARS: usize = 600;/const HISTORY_OUTCOME_CHARS: usize = 6_000;/'
mutant C6 ax-mcp "$FILTER" 's/    "ax_recall",\n    "ax_history",\n/    "ax_recall",\n/'

# Installer (ax-installer)
mutant I1 ax-installer "$HOOKS" 's/        \("afterAgentResponse", "response"\),\n//'

echo "mutants: $killed/$total killed"
[ "$killed" -eq "$total" ]
