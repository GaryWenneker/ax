#!/usr/bin/env bash
# Real run of ax-admin against a real headless Ax, driven by a browser.
#
#   tools/ax-admin-e2e.sh AX_BINARY AX_ADMIN_BINARY [--serve]
#
# Creates two throwaway projects in a temp directory, serves them with
# `ax mcp remote serve`, starts ax-admin against an empty configuration and runs
# the Playwright story in crates/ax-admin/web-ui/e2e. With --serve it prints the
# addresses and keys and keeps both servers running until Ctrl+C.
#
# Nothing outside the temp directory is touched: HOME, the Ax usage database
# and the ax-admin configuration all point into it, and keys stay in memory.
set -euo pipefail

AX_BIN="$(cd "$(dirname "${1:?path to the ax binary}")" && pwd)/$(basename "$1")"
ADMIN_BIN="$(cd "$(dirname "${2:?path to the ax-admin binary}")" && pwd)/$(basename "$2")"
MODE="${3:-test}"
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
[[ -x "$AX_BIN" && -x "$ADMIN_BIN" ]] || { echo "binaries not found" >&2; exit 1; }

WORK="$(mktemp -d "${TMPDIR:-/tmp}/ax-admin-e2e.XXXXXX")"
WORK="$(cd "$WORK" && pwd -P)"
PIDS=()
cleanup() {
  for pid in "${PIDS[@]:-}"; do [[ -n "$pid" ]] && kill "$pid" 2>/dev/null || true; done
  wait 2>/dev/null || true
  rm -rf "$WORK"
}
trap cleanup EXIT

export HOME="$WORK/home"
export AX_USAGE_DB="$WORK/home/usage.db"
export AX_TELEMETRY=0
unset AX_MCP_TOOLS
mkdir -p "$HOME"

# Only od and sha256sum (or shasum on macOS) are needed: minimal images lack uuidgen.
random_hex() { od -An -tx1 -N16 /dev/urandom | tr -d ' \n'; }
if command -v sha256sum >/dev/null 2>&1; then
  sha() { printf '%s' "$1" | sha256sum | cut -d' ' -f1; }
else
  sha() { printf '%s' "$1" | shasum -a 256 | cut -d' ' -f1; }
fi
CONTEXT_KEY="e2e-context-$(random_hex)"
ADMIN_KEY="e2e-admin-$(random_hex)"
BETA_KEY="e2e-beta-$(random_hex)"

for project in alpha beta; do
  mkdir -p "$WORK/$project"
  printf 'pub fn remote_fixture() -> u32 { 1 }\n' > "$WORK/$project/lib.rs"
  (cd "$WORK/$project" && "$AX_BIN" init . >/dev/null 2>&1 && "$AX_BIN" index . >/dev/null 2>&1)
  [[ -f "$WORK/$project/.ax/ax.db" ]] || { echo "ax init failed for $project" >&2; exit 1; }
done

cat > "$WORK/remote-mcp.json" <<JSON
{
  "public_url": "https://ax.example.test",
  "oauth": {"built_in": true, "issuer": "https://ax.example.test/oauth", "jwks_uri": "https://ax.example.test/oauth/jwks"},
  "projects": [
    {"id": "alpha", "root": "$WORK/alpha", "subjects": ["owner"]},
    {"id": "beta", "root": "$WORK/beta", "subjects": ["owner"]}
  ],
  "api_keys": [
    {"id": "context", "sha256": "$(sha "$CONTEXT_KEY")", "projects": ["alpha"]},
    {"id": "admin", "sha256": "$(sha "$ADMIN_KEY")", "projects": ["alpha"], "write": true, "admin": true},
    {"id": "beta", "sha256": "$(sha "$BETA_KEY")", "projects": ["beta"]}
  ]
}
JSON

wait_for() { # file pattern
  for _ in $(seq 1 150); do
    grep -Eo "$2" "$1" 2>/dev/null | head -1 && return 0
    sleep 0.2
  done
  echo "timed out waiting for $2 in $1:" >&2; cat "$1" >&2; return 1
}

"$AX_BIN" mcp remote serve --config "$WORK/remote-mcp.json" --port 0 > "$WORK/remote.log" 2>&1 &
PIDS+=($!)
REMOTE="$(wait_for "$WORK/remote.log" 'http://127\.0\.0\.1:[0-9]+')"

AX_ADMIN_HOME="$WORK/admin-home" AX_ADMIN_SECRET_STORE=memory \
  "$ADMIN_BIN" --port 0 --no-open > "$WORK/admin.log" 2>&1 &
PIDS+=($!)
ADMIN="$(wait_for "$WORK/admin.log" 'http://127\.0\.0\.1:[0-9]+')"

export AX_ADMIN_URL="$ADMIN"
export AX_E2E_ALPHA_URL="$REMOTE/projects/alpha/mcp"
export AX_E2E_BETA_URL="$REMOTE/projects/beta/mcp"
export AX_E2E_ALPHA_DIR="$WORK/alpha"
export AX_E2E_CONTEXT_KEY="$CONTEXT_KEY" AX_E2E_ADMIN_KEY="$ADMIN_KEY" AX_E2E_BETA_KEY="$BETA_KEY"

if [[ "$MODE" == "--serve" ]]; then
  cat <<INFO
ax-admin:        $ADMIN
alpha (context): $AX_E2E_ALPHA_URL   key: $CONTEXT_KEY
alpha (admin):   $AX_E2E_ALPHA_URL   key: $ADMIN_KEY
beta (context):  $AX_E2E_BETA_URL   key: $BETA_KEY
Ctrl+C stops both servers and deletes $WORK
INFO
  wait
  exit 0
fi

(cd "$ROOT/crates/ax-admin/web-ui" && npx playwright test)

# A9: keys never reach the configuration file or either process log.
for secret in "$CONTEXT_KEY" "$ADMIN_KEY" "$BETA_KEY"; do
  if grep -rF -- "$secret" "$WORK/admin-home" "$WORK/admin.log" 2>/dev/null; then
    echo "FAIL: a key was written by ax-admin" >&2; exit 1
  fi
done
echo "e2e ok: no key in ax-admin's files or log"
