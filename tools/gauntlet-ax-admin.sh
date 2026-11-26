#!/usr/bin/env bash
# Gauntlet for ax-admin and the remote MCP admin extension
# (docs/specs/ax-admin/SPEC.md). One command reruns every layer:
#
#   tools/gauntlet-ax-admin.sh              # all layers except mutation
#   tools/gauntlet-ax-admin.sh --mutation   # also the hand-written mutants (about 35 minutes)
#
# Every layer fails the run when it fails. Nothing here is allowed to be skipped
# silently: a tool that is missing is an error, not a pass.
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
cd "$ROOT"
MUTATION=0
[[ "${1:-}" == "--mutation" ]] && MUTATION=1
UI="crates/ax-admin/web-ui"
# The headless build goes to its own directory so the normal debug `ax` stays as it is.
HEADLESS_DIR="$ROOT/target-dev/headless"

step() { printf '\n== %s\n' "$*"; }

# Runs a command once, prints the lines matching the pattern, keeps its exit code.
show() {
  local pattern="$1" log code
  shift
  log="$(mktemp)"
  set +e
  "$@" >"$log" 2>&1
  code=$?
  set -e
  grep -E "$pattern" "$log" || true
  [[ $code -eq 0 ]] || tail -40 "$log"
  rm -f "$log"
  return $code
}
RESULT='^test result|FAILED|^error'

step "source state"
git rev-parse HEAD
git status --short | sed 's/^/  /'
rustc --version
cargo --version
node --version

step "L1 UI: install, types, unit tests, production build"
(cd "$UI" && npm ci --no-audit --no-fund >/dev/null && npm run typecheck >/dev/null)
echo "types ok (application, tests and browser story)"
(cd "$UI" && show '^ℹ (tests|pass|fail)' npm test)
(cd "$UI" && show 'built in|error' npx vite build)

step "L2 Rust tests: ax-admin, ax-remote-client"
AX_SKIP_WEB_BUILD=1 show "$RESULT" cargo test -p ax-admin -p ax-remote-client

step "L3 Rust tests: remote MCP (new admin tests and the existing suite)"
show "$RESULT" cargo test -p ax-mcp --test remote_admin --test remote_mcp

step "L4 Rust tests: full ax-mcp and ax-usage against the recorded baseline"
# On the base commit two ax-mcp tests fail (see EVIDENCE). Zero NEW failures is the bar.
FULL_LOG="$ROOT/target-dev/ax-admin-full-tests.log"
set +e
cargo test -p ax-mcp -p ax-usage --no-fail-fast >"$FULL_LOG" 2>&1
set -e
# A build that fails runs no tests: that must not look like "no failures".
if grep -qE '^error(\[E[0-9]+\])?: ' "$FULL_LOG" && grep -q 'could not compile' "$FULL_LOG"; then
  grep -E '^error' "$FULL_LOG" | head -20; echo "FAIL: the test build did not compile"; exit 1
fi
BINARIES="$(grep -c '^test result:' "$FULL_LOG" || true)"
[[ "$BINARIES" -ge 12 ]] || { echo "FAIL: only $BINARIES test binaries reported a result"; exit 1; }
PASSED="$(grep '^test result:' "$FULL_LOG" | sed -E 's/.* ([0-9]+) passed.*/\1/' | paste -sd+ - | bc)"
grep -E '^test .* FAILED$' "$FULL_LOG" | sort -u > "$ROOT/target-dev/ax-admin-failed.txt" || true
cat "$ROOT/target-dev/ax-admin-failed.txt"
cat > "$ROOT/target-dev/ax-admin-baseline.txt" <<'BASELINE'
test preflight_returns_exact_indexed_manifest_line ... FAILED
test server::policy_integration::unchanged_catalog_and_memory_titles_are_sent_once_per_session ... FAILED
BASELINE
UNEXPECTED="$(grep -vxFf "$ROOT/target-dev/ax-admin-baseline.txt" "$ROOT/target-dev/ax-admin-failed.txt" || true)"
[[ -z "$UNEXPECTED" ]] || { echo "NEW failures:"; echo "$UNEXPECTED"; exit 1; }
echo "$BINARIES test binaries, $PASSED tests passed, $(wc -l < "$ROOT/target-dev/ax-admin-failed.txt" | tr -d ' ') failed (all in the baseline), no new failures"

step "L5 type check of ax-web and ax-cli after the client move (N2)"
AX_SKIP_WEB_BUILD=1 show '^error|Finished' cargo check -p ax-web -p ax-cli
AX_SKIP_WEB_BUILD=1 show "$RESULT" cargo test -p ax-web remote

step "L6 Clippy, warnings are errors"
AX_SKIP_WEB_BUILD=1 show '^error|Finished' cargo clippy -p ax-admin -p ax-remote-client -p ax-mcp -p ax-usage --all-targets -- -D warnings

step "L7 D1: ax-admin does not link the indexing engine"
TREE="$(cargo tree -p ax-admin -e normal --prefix none | sort -u)"
for crate in ax-extraction ax-db ax-core ax-mcp tree-sitter; do
  if printf '%s\n' "$TREE" | grep -qE "^${crate}(-[a-z]+)? v"; then echo "FAIL: ax-admin links $crate"; exit 1; fi
done
# Negative control: the same check must find these crates where they are linked.
CONTROL="$(cargo tree -p ax-mcp -e normal --prefix none | sort -u)"
for crate in ax-extraction ax-db tree-sitter; do
  printf '%s\n' "$CONTROL" | grep -qE "^${crate}(-[a-z]+)? v" || { echo "FAIL: control did not find $crate in ax-mcp"; exit 1; }
done
echo "ax-admin: $(printf '%s\n' "$TREE" | wc -l | tr -d ' ') crates, none of the engine (control found them in ax-mcp)"

step "L8 E8: headless build of ax with the admin extension"
CARGO_TARGET_DIR="$HEADLESS_DIR" show '^error|Finished' cargo build --locked -p ax-cli --no-default-features --features "minimal-grammars mimalloc"
HEADLESS_AX="$HEADLESS_DIR/debug/ax"
"$HEADLESS_AX" web 2>&1 | grep -q "not included in this headless ax build" || { echo "FAIL: not a headless build"; exit 1; }
echo "headless ax: $("$HEADLESS_AX" version | sed 's/\x1b\[[0-9;]*m//g')"

step "L9 ax-admin binary with its embedded UI"
show '^error|Finished' cargo build -p ax-admin
ADMIN_BIN="$ROOT/target-dev/debug/ax-admin"
"$ADMIN_BIN" version

step "L10 real run: browser story against the headless ax (A1-A8, E6, E9)"
show '^[[:space:]]+[✓✘-]|passed|failed|e2e ok|FAIL' tools/ax-admin-e2e.sh "$HEADLESS_AX" "$ADMIN_BIN"

step "L11 D2/D3: installers"
bash scripts/test-install-admin.sh "$ADMIN_BIN"
sh -n install-admin.sh
if command -v pwsh >/dev/null 2>&1; then
  bash scripts/test-install-admin-ps1.sh "$ADMIN_BIN"
else
  echo "FAIL: pwsh is needed for the install-admin.ps1 check"; exit 1
fi
cmp install-admin.sh site/public/install-admin.sh
cmp install-admin.ps1 site/public/install-admin.ps1
echo "site/public copies match"

step "L12 changed files are UTF-8 without BOM"
for f in $(git diff --name-only main...HEAD; git ls-files --others --exclude-standard); do
  [[ -f "$f" ]] || continue
  case "$f" in *.png|*.lock|*package-lock.json) continue ;; esac
  head -c 3 "$f" | xxd -p | grep -qiE '^(efbbbf|fffe|feff)' && { echo "FAIL: BOM in $f"; exit 1; }
  iconv -f UTF-8 -t UTF-8 "$f" >/dev/null 2>&1 || { echo "FAIL: not UTF-8: $f"; exit 1; }
done
echo "encoding ok"

step "L13 documentation site tests"
(cd site && show '^ℹ (tests|pass|fail)' npm test)

if [[ "$MUTATION" == 1 ]]; then
  step "L14 mutation: self-test, then every mutant"
  python3 tools/ax_admin_mutants.py --self-test
  python3 tools/ax_admin_mutants.py
else
  step "L14 mutation: not run (pass --mutation)"
fi

printf '\nGAUNTLET PASSED\n'
