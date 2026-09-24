# Sourced by the gauntlets: fingerprints of the user's global agent configs.
# Layer 7 compares the output before and after a run; any difference fails it.
# A missing file prints "absent", so creating one counts as a change.
# ~/.claude.json is rewritten by Claude Code all the time, so only its ax entry counts.
# Every step returns nonzero on failure: `set -e` does not reach into `$( … )`.
user_agent_files() {
  local f h
  for f in "$HOME/.cursor/mcp.json" "$HOME/.cursor/hooks.json" "$HOME/.claude/settings.json" \
    "$HOME/.codex/config.toml" "$HOME/.gemini/settings.json"; do
    if [ -e "$f" ]; then
      h="$(shasum <"$f")" || return 1
      echo "$h $f"
    else
      echo "absent $f"
    fi
  done
  f="$HOME/.claude.json"
  if [ -e "$f" ]; then
    node -e 'const c=JSON.parse(require("fs").readFileSync(process.argv[1],"utf8"));console.log("claude.json ax="+JSON.stringify((c.mcpServers||{}).ax??null))' "$f" || return 1
  else
    echo "absent $f"
  fi
}
