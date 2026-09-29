#!/usr/bin/env bash
# Real execution for docs/specs/obsidian-links.md: a release `ax` binary against a fresh temp
# project with HOME and AX_GLOBAL_DB isolated, driven through /dav, /api/links and MCP stdio.
# Every check prints OK or FAIL; any FAIL exits 1. Usage: scripts/links-real.sh [path-to-ax]
set -euo pipefail
AX="${1:-$(cd "$(dirname "$0")/.." && pwd)/target-dev/release/ax}"
[ -x "$AX" ] || { echo "no ax binary at $AX"; exit 1; }
PORT=7093
B="http://127.0.0.1:$PORT"
T="$(mktemp -d /tmp/ax-links-real.XXXXXX)"
export HOME="$T/home" AX_GLOBAL_DB="$T/global.db"
mkdir -p "$HOME" "$T/proj"
WEB_PID=""
cleanup() { if [ -n "$WEB_PID" ]; then kill "$WEB_PID" 2>/dev/null || true; fi; }
trap cleanup EXIT

fails=0
check() { # name, expected, actual
  if [ "$2" = "$3" ]; then echo "OK   $1"; else echo "FAIL $1 (expected '$2', got '$3')"; fails=$((fails + 1)); fi
}
code() { curl -s -o /dev/null -w '%{http_code}' "$@"; }
put() { printf '%s' "$2" | code -T - "$B/dav/$1"; }

preflight() { # prompt [inject budget] -> full JSON-RPC result on stdout
  env ${2:+AX_POLICY_MAX_CHARS=$2} python3 - "$AX" "$T/proj" "$1" <<'EOF'
import json, subprocess, sys
ax, root, prompt = sys.argv[1:4]
p = subprocess.Popen([ax, "serve", "--mcp", "--path", root], stdin=subprocess.PIPE,
                     stdout=subprocess.PIPE, stderr=subprocess.DEVNULL, text=True)
def send(msg):
    p.stdin.write(json.dumps(msg) + "\n"); p.stdin.flush()
send({"jsonrpc": "2.0", "id": 1, "method": "initialize", "params": {
    "protocolVersion": "2024-11-05", "capabilities": {},
    "clientInfo": {"name": "links-real", "version": "1"}}})
send({"jsonrpc": "2.0", "method": "notifications/initialized"})
send({"jsonrpc": "2.0", "id": 2, "method": "tools/call",
      "params": {"name": "ax_preflight", "arguments": {"prompt": prompt}}})
for line in p.stdout:
    try:
        msg = json.loads(line)
    except ValueError:
        continue
    if msg.get("id") == 2:
        print(json.dumps(msg))
        break
else:
    sys.exit("mcp closed without a preflight reply")
p.kill()
EOF
}

echo "== setup in $T"
"$AX" --version
(cd "$T/proj" && git init -q && "$AX" init . </dev/null >"$T/init.log" 2>&1) || { tail -20 "$T/init.log"; exit 1; }
(cd "$T/proj" && "$AX" web --port "$PORT" . >"$T/web.log" 2>&1) &
WEB_PID=$!
for _ in $(seq 1 120); do curl -s -o /dev/null "$B/api/version" && break; sleep 0.5; done
check "ax web is up" 200 "$(code "$B/api/version")"

echo "== V1: app.json defaults"
check "V1 new notes default to memories/" memories \
  "$(curl -s "$B/dav/.obsidian/app.json" | python3 -c 'import sys,json;print(json.load(sys.stdin)["newFileFolderPath"])')"

echo "== P1: preflight follows a link one hop"
check "skill page created" 201 "$(put skills/linked-skill.md $'---\nname: linked-skill\ndescription: Zebra onboarding steps\n---\n\nLINKED-SKILL-BODY-7')"
check "rule without link created" 201 "$(put rules/link-rule.md $'---\nid: link-rule\nlevel: WARNING\nalwaysApply: true\n---\n\nNo link yet.')"
control="$(preflight "control: fix the build" 200000)"
check "control: the reply is a full preflight that delivers link-rule" 1 \
  "$(grep -c '\[WARNING\] link-rule' <<<"$control" || true)"
check "control: skill not delivered before the link exists" 0 \
  "$(grep -c 'LINKED-SKILL-BODY-7\|Skills omitted[^"]*linked-skill' <<<"$control" || true)"
check "rule now links the skill" 204 "$(put rules/link-rule.md $'---\nid: link-rule\nlevel: WARNING\nalwaysApply: true\n---\n\nFollow [[linked-skill]] first.')"
preflight "default budget: fix the build" >"$T/preflight-default.json"
check "P1 default budget: linked skill body in the inject or named in the omitted-skills line" 1 \
  "$(grep -c 'LINKED-SKILL-BODY-7\|Skills omitted[^"]*linked-skill' "$T/preflight-default.json" || true)"
check "P1 room in the budget: linked skill body in the inject" 1 \
  "$(preflight "large budget: fix the build" 200000 | grep -c LINKED-SKILL-BODY-7 || true)"

echo "== A1/A2: /api/links"
check "memory page links the rule" 201 "$(put "memories/Link%20note.md" 'Relates to [[link-rule]].')"
links="$(curl -s "$B/api/links?kind=rule&id=link-rule")"
check "A1 outgoing link resolves to the skill" "skill:linked-skill" \
  "$(python3 -c 'import sys,json;o=json.load(sys.stdin)["outgoing"][0]["resolved"];print(o["kind"]+":"+o["id"])' <<<"$links")"
check "A2 backlink from the memory" True \
  "$(python3 -c 'import sys,json;print(any(b["kind"]=="memory" for b in json.load(sys.stdin)["backlinks"]))' <<<"$links")"
check "A3 unknown item is 404" 404 "$(code "$B/api/links?kind=rule&id=nope")"

echo "== G: global/ folders"
check "G6 root lists global/" 1 \
  "$(curl -s -X PROPFIND -H 'Depth: 1' "$B/dav/" | grep -c '/dav/global/</D:href>' || true)"
check "G1 new global skill saved" 201 "$(put global/skills/g-skill.md $'---\nname: g-skill\ndescription: global one\n---\n\nGLOBAL-BODY-1')"
check "G2 edit a global skill" 204 "$(put global/skills/g-skill.md $'---\nname: g-skill\ndescription: global one\n---\n\nGLOBAL-BODY-2')"
check "G2 global.db holds the edit" 1 \
  "$(python3 -c 'import sqlite3,sys;c=sqlite3.connect(sys.argv[1]);print(sum("GLOBAL-BODY-2" in str(r) for r in c.execute("select * from global_policy_skills")))' "$AX_GLOBAL_DB")"
check "G5 DELETE saved global page is 403" 403 "$(code -X DELETE "$B/dav/global/skills/g-skill.md")"
check "G6 DELETE global/ is 403" 403 "$(code -X DELETE "$B/dav/global/")"

echo "== N1: no note files on disk"
check "no vault files written into the project" 0 \
  "$(find "$T/proj" \( -name 'Link note*' -o -name 'app.json' -o -name 'g-skill*' \) | wc -l | tr -d ' ')"

echo "real execution: $fails failure(s)"
[ "$fails" -eq 0 ]
